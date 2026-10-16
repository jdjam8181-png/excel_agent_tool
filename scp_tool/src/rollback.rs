use anyhow::{Result, anyhow};

use ssh_pub::group::HostGroup;

use ssh_pub::ssh::SshClient;

use ssh_pub::logger::Logger;

use ssh_pub::md5;

use crate::engine::{EngineConfig, ReplaceTask, build_replace_tasks};

use crate::db_type::{DbTypeConfig, match_rule};

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

    arch: &str,

    task_id: &str,
) -> Result<()> {
    logger.println(&format!("ROLLBACK HOST START {}", host.host));

    //
    // SSH一次
    //
    let ssh = match SshClient::connect_hostgroup(host) {
        Ok(v) => v,

        Err(e) => {
            logger.println(&format!("ROLLBACK SSH FAILED {} {}", host.host, e));

            return Err(e);
        }
    };

    //
    // 多service
    //
    for service in &host.services {
        match rollback_service(&ssh, service, db_config, engine, logger, arch, task_id) {
            Ok(_) => {
                logger.println(&format!(
                    "ROLLBACK SERVICE SUCCESS {} {}",
                    service.host, service.service_name
                ));
            }

            Err(e) => {
                logger.println(&format!(
                    "ROLLBACK SERVICE FAILED {} {} {}",
                    service.host, service.service_name, e
                ));

                //
                // 当前service失败
                // 继续其他service
                //
                continue;
            }
        }
    }

    logger.println(&format!("ROLLBACK HOST FINISH {}", host.host));

    Ok(())
}

//
// 单service回退
//
fn rollback_service(
    ssh: &SshClient,

    server: &ssh_pub::config::Server,

    db_config: &DbTypeConfig,

    engine: &EngineConfig,

    logger: &Logger,

    arch: &str,

    task_id: &str,
) -> Result<()> {
    let rule = match_rule(db_config, server)?;

    let tasks = build_replace_tasks(engine, &rule, server, arch)?;

    if tasks.is_empty() {
        return Err(anyhow!("no rollback task generated"));
    }

    for task in tasks {
        match rollback_file(ssh, &task, task_id, logger) {
            Ok(_) => {}

            Err(e) => {
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

    task_id: &str,

    logger: &Logger,
) -> Result<()> {
    logger.println(&format!("ROLLBACK FILE START {}", task.remote_file));

    //
    // 找backup文件
    //
    let backup_file = build_backup_file(&task.remote_file, task_id);

    //
    // 检查backup
    //
    let check = ssh.exec(&format!("test -f '{}' && echo EXISTS", backup_file))?;

    if !check.contains("EXISTS") {
        logger.println(&format!(
            "ROLLBACK SKIP {} BACKUP NOT FOUND {}",
            task.remote_file, backup_file
        ));

        //
        // 新增文件
        // 不影响其他文件
        //
        return Ok(());
    }

    //
    // 恢复
    //
    let result = ssh.exec(&format!("cp '{}' '{}'", backup_file, task.remote_file));

    if let Err(e) = result {
        logger.println(&format!("ROLLBACK CP FAILED {} {}", task.remote_file, e));

        return Err(e);
    }

    logger.println(&format!("ROLLBACK COPY SUCCESS {}", task.remote_file));

    //
    // MD5校验
    //
    let backup_md5 = md5::remote_md5(ssh, &backup_file)?;

    let remote_md5 = md5::remote_md5(ssh, &task.remote_file)?;

    if !md5::verify_md5(&backup_md5, &remote_md5) {
        logger.println(&format!(
            "ROLLBACK MD5 FAILED {} backup={} remote={}",
            task.remote_file, backup_md5, remote_md5
        ));

        return Err(anyhow!("rollback md5 failed"));
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
//
//
// backup:
//
// /app/ds/bin/backup/bin_taskid/mdsd
//
fn build_backup_file(file: &str, task_id: &str) -> String {
    let path = std::path::Path::new(file);

    let parent = path.parent().unwrap();

    let dirname = parent.file_name().unwrap().to_string_lossy();

    let backup_dir = format!("{}_{}", dirname, task_id);

    parent
        .join("backup")
        .join(backup_dir)
        .join(path.file_name().unwrap())
        .to_string_lossy()
        .to_string()
}
