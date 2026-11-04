use anyhow::{anyhow, bail, Result};

use ssh_pub::config::Server;
use ssh_pub::group::HostGroup;
use ssh_pub::logger::Logger;
use ssh_pub::md5;
use ssh_pub::scp::ScpClient;
use ssh_pub::shell;
use ssh_pub::ssh::SshClient;

use crate::db_type::{match_rule, DbTypeConfig};
use crate::engine::{build_replace_tasks, EngineConfig, ReplaceTask};
use crate::run::{FileStat, HostReport, RunOptions};

//
// host级替换
//
// 一个host一个SSH连接
//
pub fn replace_host(
    host: &HostGroup,

    db_config: &DbTypeConfig,

    engine: &EngineConfig,

    logger: &Logger,

    opts: &RunOptions,
) -> HostReport {
    let mut report = HostReport::new(&host.host);

    logger.println(&format!("HOST START {}", host.host));

    //
    // SSH连接一次
    //
    let ssh = match SshClient::connect_hostgroup(host) {
        Ok(v) => v,

        Err(e) => {
            logger.println(&format!("SSH FAILED {} {}", host.host, e));

            report.push_error(format!("SSH 连接失败: {}", e));

            return report;
        }
    };

    //
    // 防止某个 worker 永久卡死
    //
    ssh.set_timeout(opts.timeout_ms);

    report.ssh_ok = true;

    //
    // 一个host多个service
    //
    for service in &host.services {
        let result = replace_service(
            &ssh,
            service,
            db_config,
            engine,
            logger,
            opts,
            &mut report.files,
        );

        match result {
            Ok(_) => {
                report.services_ok += 1;

                logger.println(&format!(
                    "SERVICE SUCCESS {} {}",
                    service.host, service.service_name
                ));
            }

            Err(e) => {
                report.services_failed += 1;

                report.push_error(format!("{}: {}", service.service_name, e));

                logger.println(&format!(
                    "SERVICE FAILED {} {} {}",
                    service.host, service.service_name, e
                ));
            }
        }
    }

    logger.println(&format!(
        "HOST FINISH {} services_ok={} services_failed={} files_ok={} files_failed={} files_skipped={}",
        host.host,
        report.services_ok,
        report.services_failed,
        report.files.files_ok,
        report.files.files_failed,
        report.files.files_skipped
    ));

    report
}

//
// 单service替换
//
// 统计通过 stat 回传, 保证"部分成功后失败"也能被准确计数。
//
fn replace_service(
    ssh: &SshClient,

    server: &Server,

    db_config: &DbTypeConfig,

    engine: &EngineConfig,

    logger: &Logger,

    opts: &RunOptions,

    stat: &mut FileStat,
) -> Result<()> {
    let rule = match_rule(db_config, server)?;

    let tasks = build_replace_tasks(engine, &rule, server, opts.arch.as_str())?;

    if tasks.is_empty() {
        bail!(
            "没有生成任何替换任务 (source_db={} target_db={})",
            server.source_db,
            server.target_db
        );
    }

    logger.println(&format!(
        "SERVICE {} TASK_ID {} FILES {}",
        server.service_name,
        opts.task_id,
        tasks.len()
    ));

    //
    // 预检
    //
    // 任何一项不满足就整个 service 不做修改。
    //
    // 旧实现是"边检查边替换",
    // 很可能上传了 2 个文件之后第 3 个失败,
    // 留下一个半新半旧的组件 —— 这是批量升级最忌讳的状态。
    //
    preflight(ssh, &tasks, logger)?;

    if opts.dry_run {
        for task in &tasks {
            logger.println(&format!(
                "DRY-RUN WOULD REPLACE {} <- {} (file_type={} mode={:o})",
                task.remote_file,
                task.local_file,
                task.file_type,
                default_mode(&task.file_type)
            ));

            stat.files_skipped += 1;
        }

        return Ok(());
    }

    //
    // 逐文件替换
    //
    // 单个文件失败即停止该 service,
    // 避免同一组件内出现版本混杂。
    //
    for task in &tasks {
        match replace_file(ssh, task, &opts.task_id, logger) {
            Ok(_) => {
                stat.files_ok += 1;
            }

            Err(e) => {
                stat.files_failed += 1;

                return Err(e);
            }
        }
    }

    Ok(())
}

//
// 预检
//
// 1) 远端路径合法(绝对路径)
//
// 2) 本地源文件存在且非空
//
// 3) 远端父目录存在
//
fn preflight(ssh: &SshClient, tasks: &[ReplaceTask], logger: &Logger) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();

    for task in tasks {
        shell::validate_remote_path(&task.remote_file, "远端文件")?;

        match std::fs::metadata(&task.local_file) {
            Ok(m) if m.is_file() && m.len() > 0 => {}

            Ok(m) if m.is_file() => {
                problems.push(format!("本地文件为空: {}", task.local_file));
            }

            Ok(_) => {
                problems.push(format!("本地路径不是普通文件: {}", task.local_file));
            }

            Err(e) => {
                problems.push(format!("本地文件不可读: {} ({})", task.local_file, e));
            }
        }
    }

    //
    // 远端父目录去重后再检查
    //
    let scp = ScpClient::new(ssh);

    let mut parents: Vec<String> = Vec::new();

    for task in tasks {
        if let Some(p) = shell::parent_dir(&task.remote_file) {
            if !parents.iter().any(|v| v == &p) {
                parents.push(p);
            }
        }
    }

    for p in &parents {
        match scp.remote_exists(p) {
            Ok(true) => {}

            Ok(false) => {
                problems.push(format!("远端目录不存在: {}", p));
            }

            Err(e) => {
                problems.push(format!("远端目录检查失败: {} ({})", p, e));
            }
        }
    }

    if problems.is_empty() {
        logger.println(&format!(
            "PREFLIGHT OK files={} dirs={}",
            tasks.len(),
            parents.len()
        ));

        return Ok(());
    }

    bail!(
        "预检失败, 未做任何修改:\n  - {}",
        problems.join("\n  - ")
    )
}

//
// 单文件替换
//
fn replace_file(ssh: &SshClient, task: &ReplaceTask, task_id: &str, logger: &Logger) -> Result<()> {
    logger.println(&format!(
        "FILE START {} <- {}",
        task.remote_file, task.local_file
    ));

    let local_size = std::fs::metadata(&task.local_file)?.len();

    //
    // 远端是否已存在
    //
    // 用 if 包一层, 保证命令本身退出码为 0,
    // 这样"文件不存在"不会被误判成命令执行失败。
    //
    let probe = ssh.run(&format!(
        "if [ -f {} ]; then echo EXISTS; fi",
        shell::quote(&task.remote_file)
    ))?;

    let exists = probe.stdout.contains("EXISTS");

    //
    // 备份
    //
    // 备份失败必须中断, 不允许继续覆盖。
    //
    // 旧实现用 exec() 判断成功与否, 而 exec() 只反映"命令有没有发出去",
    // cp 因为没有权限而失败时依然会打印 BACKUP SUCCESS,
    // 结果是文件被覆盖、备份其实不存在、回退无路可走。
    //
    if exists {
        let backup_dir = build_backup_dir(&task.remote_file, task_id)?;

        let cmd = format!(
            "mkdir -p {dir} && cp -p {src} {dst}",
            dir = shell::quote(&backup_dir),
            src = shell::quote(&task.remote_file),
            dst = shell::quote(&format!("{}/", backup_dir)),
        );

        ssh.run_ok(&cmd)
            .map_err(|e| anyhow!("备份失败 {} -> {} : {}", task.remote_file, backup_dir, e))?;

        logger.println(&format!(
            "BACKUP SUCCESS {} -> {}",
            task.remote_file, backup_dir
        ));
    } else {
        logger.println(&format!("BACKUP SKIP 新增文件 {}", task.remote_file));
    }

    //
    // 上传(先写临时文件, 再原子替换)
    //
    let scp = ScpClient::new(ssh);

    let written = scp
        .upload_file(
            &task.local_file,
            &task.remote_file,
            default_mode(&task.file_type),
        )
        .map_err(|e| anyhow!("上传失败 {} : {}", task.remote_file, e))?;

    logger.println(&format!(
        "UPLOAD SUCCESS {} bytes={}",
        task.remote_file, written
    ));

    //
    // MD5校验
    //
    let local_md5 = md5::local_md5(&task.local_file)?;

    let remote_md5 = md5::remote_md5(ssh, &task.remote_file)?;

    if !md5::verify_md5(&local_md5, &remote_md5) {
        return Err(anyhow!(
            "MD5校验失败 {} local={} remote={}",
            task.remote_file,
            local_md5,
            remote_md5
        ));
    }

    logger.println(&format!(
        "MD5 SUCCESS {} size={}",
        task.remote_file, local_size
    ));

    Ok(())
}

//
// 生成备份目录
//
// 原文件:
//
// /app/ds/bin/mdsd
//
// 备份:
//
// /app/ds/bin/backup/bin_20260904_101500/mdsd
//
// 不再使用 parent.file_name().unwrap():
// 远端文件直接位于根目录时(如 /mdsd) file_name() 为 None, 旧实现会 panic。
//
fn build_backup_dir(file: &str, task_id: &str) -> Result<String> {
    let parent = shell::parent_dir(file).ok_or_else(|| anyhow!("无法解析父目录: {}", file))?;

    let dirname = shell::parent_dir_name(file);

    Ok(format!(
        "{}/backup/{}_{}",
        parent.trim_end_matches('/'),
        dirname,
        task_id
    ))
}

//
// 新增文件的默认权限
//
// 已有文件由 scp.rs 继承原权限,
// 这里只在"文件本来不存在"时生效。
//
fn default_mode(file_type: &str) -> i32 {
    match file_type {
        "bin" => 0o755,

        _ => 0o644,
    }
}
