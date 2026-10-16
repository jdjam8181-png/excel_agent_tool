use anyhow::{Result, anyhow};

use chrono::Local;

use std::fs::{OpenOptions, metadata};
use std::io::Write;

use ssh_pub::group::HostGroup;

use ssh_pub::ssh::SshClient;

use ssh_pub::scp::ScpClient;

use ssh_pub::logger::Logger;

use ssh_pub::md5;

use crate::engine::{EngineConfig, ReplaceTask, build_replace_tasks};

use crate::db_type::{DbTypeConfig, match_rule};

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

    arch: &str,
) -> Result<()> {
    logger.println(&format!("HOST START {}", host.host));

    //
    // 一个host一个版本
    //
    let task_id = generate_task_id();

    append_history(&task_id)?;

    logger.println(&format!("HOST TASK_ID {} {}", host.host, task_id));

    //
    // SSH连接一次
    //
    let ssh = match SshClient::connect_hostgroup(host) {
        Ok(v) => v,

        Err(e) => {
            logger.println(&format!("SSH FAILED {} {}", host.host, e));

            return Err(e);
        }
    };

    //
    // 一个host多个service
    //
    for service in &host.services {
        match replace_service(&ssh, service, db_config, engine, logger, arch, &task_id) {
            Ok(_) => {
                logger.println(&format!(
                    "SERVICE SUCCESS {} {}",
                    service.host, service.service_name
                ));
            }

            Err(e) => {
                logger.println(&format!(
                    "SERVICE FAILED {} {} {}",
                    service.host, service.service_name, e
                ));

                //
                // 当前service失败
                // 继续其它service
                //
                continue;
            }
        }
    }

    logger.println(&format!("HOST FINISH {}", host.host));

    Ok(())
}

//
// 单service替换
//
fn replace_service(
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
        return Err(anyhow!("no replace task generated"));
    }

    logger.println(&format!(
        "SERVICE {} TASK_ID {}",
        server.service_name, task_id
    ));

    for task in tasks {
        replace_file(ssh, &task, task_id, logger)?;
    }

    Ok(())
}

//
//
// 单文件替换
//
fn replace_file(ssh: &SshClient, task: &ReplaceTask, task_id: &str, logger: &Logger) -> Result<()> {
    logger.println(&format!("FILE START {}", task.remote_file));

  //
    // 检查本地文件
    //
    if !std::path::Path::new(
        &task.local_file
    ).exists() {
        logger.println(
            &format!(
                "LOCAL FILE NOT EXIST {}",
                task.local_file
            )
        );

        return Err(
            anyhow!(
                "LOCAL FILE NOT EXIST {}",
                task.local_file
            )
        );
    }


    //
    // 获取本地文件大小
    //
    let local_size =
        metadata(
            &task.local_file
        )?
        .len();

    logger.println(
        &format!(
            "LOCAL FILE CHECK SUCCESS {} SIZE={}",
            task.local_file,
            local_size
        )
    );

    //
    // 检查远程文件
    //
    let check = ssh.exec(&format!("test -f '{}' && echo EXISTS", task.remote_file))?;

    let exists = check.contains("EXISTS");

    //
    // 备份
    //
    if exists {
        let backup_dir = build_backup_dir(&task.remote_file, task_id);

        let backup_cmd = format!(
            "mkdir -p '{}' && cp '{}' '{}/'",
            backup_dir, task.remote_file, backup_dir
        );

        match ssh.exec(&backup_cmd) {
            Ok(_) => {
                logger.println(&format!("BACKUP SUCCESS {}", task.remote_file));
            }

            Err(e) => {
                logger.println(&format!("BACKUP FAILED {} {}", task.remote_file, e));

                return Err(e);
            }
        }
    } else {
        logger.println(&format!("BACKUP SKIP {}", task.remote_file));
    }

//
//
// SCP上传
//
let scp = ScpClient::new(ssh);


match scp.upload_file(

    &task.local_file,

    &task.remote_file,

    0o755

) {

    Ok(_) => {

        logger.println(
            &format!(
                "UPLOAD SUCCESS {}",
                task.remote_file
            )
        );

    }


    Err(e)=>{

        logger.println(
            &format!(
                "UPLOAD FAILED {} {}",
                task.remote_file,
                e
            )
        );

        return Err(e);

    }

}

    //
    // MD5校验
    //
    let local_md5 = md5::local_md5(&task.local_file)?;

    let remote_md5 = md5::remote_md5(ssh, &task.remote_file)?;

    if !md5::verify_md5(&local_md5, &remote_md5) {
        logger.println(&format!("MD5 FAILED {}", task.remote_file));

        return Err(anyhow!("MD5 CHECK FAILED {}", task.remote_file));
    }

    logger.println(&format!("MD5 SUCCESS {}", task.remote_file));

    Ok(())
}

//
// 生成task_id
//
fn generate_task_id() -> String {
    Local::now().format("%Y%m%d_%H%M%S%.3f").to_string()
}

//
// 写替换历史
//
// 文件:
//
// .replace_history
//
// 示例:
//
// VERSION=20260904_101500,TIME=2026-09-04 10:15:00,OPERATION=REPLACE,STATUS=RUNNING
//
fn append_history(task_id: &str) -> Result<()> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(".replace_history")?;

    writeln!(
        file,
        "VERSION={},TIME={},OPERATION=REPLACE,STATUS=RUNNING",
        task_id, now
    )?;

    Ok(())
}

//
// 生成备份目录
//
// 原文件:
//
// /app/ds/bin/mdsd
//
//
// 备份:
//
// /app/ds/bin/backup/bin_20260904_101500/mdsd
//
fn build_backup_dir(file: &str, task_id: &str) -> String {
    let path = std::path::Path::new(file);

    let parent = path.parent().unwrap_or(std::path::Path::new("/"));

    //
    // 当前目录
    //
    // bin
    //
    let dirname = parent.file_name().unwrap().to_string_lossy();

    //
    // bin_taskid
    //
    let backup_name = format!("{}_{}", dirname, task_id);

    //
    // /app/ds/bin/backup/bin_taskid
    //
    parent
        .join("backup")
        .join(backup_name)
        .to_string_lossy()
        .to_string()
}
