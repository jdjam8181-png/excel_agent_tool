use anyhow::{anyhow, bail, Result};
use std::fs::File;
use std::io::copy;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ssh::SshClient;

//
// SFTP文件传输模块
//
// 负责:
// 文件上传
// 文件下载
//
// 升级语义要求:
//
// 1) 上传必须先写临时文件, 校验通过后再替换正式文件
//    -> 任何一步失败都不能破坏线上正在使用的旧文件
//
// 2) 替换动作尽量原子
//    -> 避免出现"文件不存在"的窗口
//
// 3) 保留原有权限
//    -> 旧实现无条件 chmod 0755, 会把 jar/module 也改成可执行
//

pub struct ScpClient<'a> {
    ssh: &'a SshClient,
}

impl<'a> ScpClient<'a> {
    //
    // 创建客户端
    //
    pub fn new(ssh: &'a SshClient) -> Self {
        ScpClient { ssh }
    }

    //
    // 远端路径是否存在
    //
    // 预检用: 父目录不存在就没必要开始替换
    //
    pub fn remote_exists(&self, path: &str) -> Result<bool> {
        let sftp = self.ssh.session().sftp()?;

        Ok(sftp.stat(Path::new(path)).is_ok())
    }

    //
    // 上传文件
    //
    // 流程:
    //
    // local
    //    |
    //    v
    // remote.<ts>.<pid>.tmp
    //    |
    //    +-- 大小校验(不一致直接失败, 绝不用残文件覆盖正式文件)
    //    |
    //    +-- chmod(继承原权限)
    //    |
    //    v
    // rename 原子替换
    //
    // 返回实际写入字节数
    //
    pub fn upload_file(&self, local: &str, remote: &str, default_mode: i32) -> Result<u64> {
        //
        // 本地文件
        //
        let mut local_file = File::open(local)?;

        let size = local_file.metadata()?.len();

        //
        // 创建sftp
        //
        let sftp = self
            .ssh
            .session()
            .sftp()
            .map_err(|e| anyhow!("CREATE SFTP FAILED {}", e))?;

        //
        // 检查目录
        //
        let parent = Path::new(remote)
            .parent()
            .ok_or_else(|| anyhow!("REMOTE PATH INVALID {}", remote))?;

        sftp.stat(parent)
            .map_err(|e| anyhow!("REMOTE DIR ERROR {:?} {}", parent, e))?;

        //
        // 继承原文件权限
        //
        // 文件不存在(新增)时使用调用方给的默认权限
        //
        let mode = match sftp.stat(Path::new(remote)) {
            Ok(st) => st.perm.unwrap_or(default_mode as u32) & 0o7777,

            Err(_) => default_mode as u32,
        };

        //
        // 临时文件
        //
        // 防止并发上传冲突
        //
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();

        let pid = std::process::id();

        let tmp = format!("{}.{}.{}.tmp", remote, timestamp, pid);

        //
        // 创建临时文件
        //
        let mut remote_file = sftp
            .create(Path::new(&tmp))
            .map_err(|e| anyhow!("CREATE TEMP FAILED {} {}", tmp, e))?;

        //
        // 写入
        //
        let write_size = copy(&mut local_file, &mut remote_file)?;

        //
        // 关闭文件
        //
        drop(remote_file);

        //
        // 大小校验
        //
        // 原实现不校验写入字节数,
        // 网络中断导致的截断文件会被直接改名成正式文件。
        //
        if write_size != size {
            let _ = sftp.unlink(Path::new(&tmp));

            bail!(
                "UPLOAD SIZE MISMATCH local={} expected={} actual={}",
                local,
                size,
                write_size
            );
        }

        //
        // 设置权限
        //
        let mut stat = sftp
            .stat(Path::new(&tmp))
            .map_err(|e| anyhow!("STAT TEMP FAILED {} {}", tmp, e))?;

        stat.perm = Some(mode);

        sftp.setstat(Path::new(&tmp), stat)
            .map_err(|e| anyhow!("CHMOD FAILED {} {}", tmp, e))?;

        //
        // 替换正式文件
        //
        // 优先直接 rename:
        //   POSIX 语义下 rename 覆盖目标是原子的,
        //   不会出现"旧文件已删/新文件未就位"的空窗。
        //
        // 部分 SFTP 服务端要求目标不存在才允许 rename,
        //   此时退化为先删后改名(保持与旧实现一致的行为)。
        //
        if sftp
            .rename(Path::new(&tmp), Path::new(remote), None)
            .is_err()
        {
            let _ = sftp.unlink(Path::new(remote));

            sftp.rename(Path::new(&tmp), Path::new(remote), None)
                .map_err(|e| anyhow!("RENAME FAILED {} -> {} {}", tmp, remote, e))?;
        }

        Ok(write_size)
    }

    //
    // 下载
    //
    pub fn download_file(&self, remote: &str, local: &str) -> Result<()> {
        let sftp = self.ssh.session().sftp()?;

        let mut remote_file = sftp.open(Path::new(remote))?;

        let mut local_file = File::create(local)?;

        copy(&mut remote_file, &mut local_file)?;

        local_file.sync_all()?;

        Ok(())
    }
}
