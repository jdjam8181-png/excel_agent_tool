use anyhow::{anyhow, bail, Result};

use ssh_pub::config::Server;
use ssh_pub::group::HostGroup;
use ssh_pub::logger::Logger;
use ssh_pub::md5;
use ssh_pub::shell;
use ssh_pub::ssh::SshClient;

use crate::db_type::{match_rule, DbTypeConfig};
use crate::engine::{build_replace_tasks, EngineConfig, ReplaceTask};
use crate::run::{FileStat, HostReport, RunOptions};

//
// host级回退
//
// 一个host一个SSH连接
//
pub fn rollback_host(
    host: &HostGroup,

    db_config: &DbTypeConfig,

    engine: &EngineConfig,

    logger: &Logger,

    opts: &RunOptions,
) -> HostReport {
    let mut report = HostReport::new(&host.host);

    logger.println(&format!("ROLLBACK HOST START {}", host.host));

    //
    // SSH一次
    //
    let ssh = match SshClient::connect_hostgroup(host) {
        Ok(v) => v,

        Err(e) => {
            logger.println(&format!("ROLLBACK SSH FAILED {} {}", host.host, e));

            report.push_error(format!("SSH 连接失败: {}", e));

            return report;
        }
    };

    ssh.set_timeout(opts.timeout_ms);

    report.ssh_ok = true;

    //
    // 多service
    //
    for service in &host.services {
        let result = rollback_service(
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
                    "ROLLBACK SERVICE SUCCESS {} {}",
                    service.host, service.service_name
                ));
            }

            Err(e) => {
                report.services_failed += 1;

                report.push_error(format!("{}: {}", service.service_name, e));

                logger.println(&format!(
                    "ROLLBACK SERVICE FAILED {} {} {}",
                    service.host, service.service_name, e
                ));
            }
        }
    }

    logger.println(&format!(
        "ROLLBACK HOST FINISH {} services_ok={} services_failed={} files_ok={} files_failed={} files_skipped={}",
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
// 单service回退
//
fn rollback_service(
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
            "没有生成任何回退任务 (source_db={} target_db={})",
            server.source_db,
            server.target_db
        );
    }

    //
    // 先探明备份情况, 再决定是否动手
    //
    // 旧实现遇到"没有备份"就直接 return Ok(()),
    // 于是一个什么都没做的回退被报成 ROLLBACK SUCCESS。
    //
    let mut planned: Vec<(ReplaceTask, String)> = Vec::new();

    for task in tasks {
        let backup_file = build_backup_file(&task.remote_file, &opts.task_id)?;

        let probe = ssh.run(&format!(
            "if [ -f {} ]; then echo EXISTS; fi",
            shell::quote(&backup_file)
        ))?;

        if probe.stdout.contains("EXISTS") {
            planned.push((task, backup_file));
        } else {
            logger.println(&format!(
                "ROLLBACK SKIP {} 未找到备份 {}",
                task.remote_file, backup_file
            ));

            stat.files_skipped += 1;
        }
    }

    //
    // 一个备份都没有 -> task_id 大概率传错了, 必须报错而不是静默成功
    //
    if planned.is_empty() {
        bail!(
            "task_id={} 在 {} 上没有任何备份, 未执行回退",
            opts.task_id,
            server.service_name
        );
    }

    if opts.dry_run {
        for (task, backup_file) in &planned {
            logger.println(&format!(
                "DRY-RUN WOULD ROLLBACK {} <- {}",
                task.remote_file, backup_file
            ));

            stat.files_skipped += 1;
        }

        return Ok(());
    }

    for (task, backup_file) in &planned {
        match rollback_file(ssh, task, backup_file, logger) {
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
// 单文件回退
//
fn rollback_file(
    ssh: &SshClient,

    task: &ReplaceTask,

    backup_file: &str,

    logger: &Logger,
) -> Result<()> {
    logger.println(&format!(
        "ROLLBACK FILE START {} <- {}",
        task.remote_file, backup_file
    ));

    //
    // 恢复
    //
    // cp -p 保留备份文件原本的权限与时间戳
    //
    let cmd = format!(
        "cp -p {src} {dst}",
        src = shell::quote(backup_file),
        dst = shell::quote(&task.remote_file),
    );

    ssh.run_ok(&cmd)
        .map_err(|e| anyhow!("恢复失败 {} : {}", task.remote_file, e))?;

    //
    // MD5校验
    //
    let backup_md5 = md5::remote_md5(ssh, backup_file)?;

    let remote_md5 = md5::remote_md5(ssh, &task.remote_file)?;

    if !md5::verify_md5(&backup_md5, &remote_md5) {
        return Err(anyhow!(
            "回退校验失败 {} backup={} remote={}",
            task.remote_file,
            backup_md5,
            remote_md5
        ));
    }

    logger.println(&format!("ROLLBACK SUCCESS {}", task.remote_file));

    Ok(())
}

//
// 根据原文件生成backup文件
//
// 原:
//
// /app/ds/bin/mdsd
//
// backup:
//
// /app/ds/bin/backup/bin_<task_id>/mdsd
//
// 同样不再 unwrap, 避免根目录文件导致 panic。
//
fn build_backup_file(file: &str, task_id: &str) -> Result<String> {
    let parent = shell::parent_dir(file).ok_or_else(|| anyhow!("无法解析父目录: {}", file))?;

    let dirname = shell::parent_dir_name(file);

    let name = std::path::Path::new(file)
        .file_name()
        .ok_or_else(|| anyhow!("无法解析文件名: {}", file))?
        .to_string_lossy()
        .to_string();

    Ok(format!(
        "{}/backup/{}_{}/{}",
        parent.trim_end_matches('/'),
        dirname,
        task_id,
        name
    ))
}
