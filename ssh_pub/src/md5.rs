use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use std::fs::File;
use std::io::Read;

use crate::shell;
use crate::ssh::SshClient;

//
// 计算本地文件MD5
//
pub fn calc_file_md5(file: &str) -> Result<String> {
    let mut file = File::open(file)?;

    let mut hasher = Md5::new();

    let mut buffer = [0u8; 8192];

    loop {
        let n = file.read(&mut buffer)?;

        if n == 0 {
            break;
        }

        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();

    Ok(hex::encode(result))
}

//
// replace.rs调用
//
// 本地文件MD5
//
pub fn local_md5(file: &str) -> Result<String> {
    calc_file_md5(file)
}

//
// 远程文件MD5
//
// 执行:
// md5sum -- 'remote_file'
//
// 返回:
// d41d8cd98f00...
//
// 修复点:
//
// 1) 路径经过 shell 转义
//    旧实现 format!("md5sum {}", file) 不加引号,
//    路径含空格/分号时命令会被 shell 拆开甚至注入。
//
// 2) 使用 run_ok, 退出码非 0 直接报错
//    旧实现即使 md5sum 失败, 也可能从 stderr 里"猜"出一个字符串当成 md5。
//
// 3) 只解析 stdout
//    旧实现把 stderr 也并进 stdout 再取第一个 token, 容易误判。
//
pub fn remote_md5(ssh: &SshClient, file: &str) -> Result<String> {
    let stdout = ssh.run_ok(&format!("md5sum -- {}", shell::quote(file)))?;

    parse_remote_md5(&stdout).ok_or_else(|| anyhow!("remote md5 parse failed:{}", file))
}

//
// 比较MD5
//
pub fn verify_md5(local_md5: &str, remote_md5: &str) -> bool {
    local_md5
        .trim()
        .eq_ignore_ascii_case(remote_md5.trim())
}

//
// 解析远程md5sum输出
//
// 格式:
//
// d41d8cd98f00  /path/file
//
pub fn parse_remote_md5(output: &str) -> Option<String> {
    output.split_whitespace().next().map(|v| v.to_string())
}
