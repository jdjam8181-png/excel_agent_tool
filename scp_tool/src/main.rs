use anyhow::Result;

use clap::Parser;

use rayon::prelude::*;

use rayon::ThreadPoolBuilder;

use std::sync::Arc;

mod db_type;

mod engine;

mod replace;

mod rollback;

use ssh_pub::logger::Logger;

use ssh_pub::excel::read_excel;

use ssh_pub::group::group_by_host;

use crate::db_type::load_db_type;

use crate::engine::load_engine;

use crate::replace::replace_host;

use crate::rollback::rollback_host;

//
// 最大host并发
//
const MAX_PARALLEL: usize = 10;

//
// 参数
//
#[derive(Parser, Debug)]
#[command(name = "scp_tool")]
struct Args {
    //
    // 模式
    //
    // replace
    // rollback
    //
    #[arg(long, default_value = "replace")]
    mode: String,

    //
    // Excel
    //
    #[arg(long)]
    excel: String,

    //
    // 架构
    //
    #[arg(long, default_value = "x86")]
    arch: String,

    //
    // 并发
    //
    #[arg(long, default_value = "10")]
    parallel: usize,

    // 任务ID
    //
    // rollback使用
    //
    #[arg(long)]
    task_id: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let logger = Logger::new(&args.mode)?;

    let logger = Arc::new(logger);

    logger.println(&format!(
        "START mode={} excel={} arch={} parallel={} task_id={}",
        args.mode, args.excel, args.arch, args.parallel, args.task_id.clone().unwrap_or_default()
    ));

    //
    // 并发控制
    //
    let parallel = if args.parallel == 0 {
        10
    } else if args.parallel > MAX_PARALLEL {
        logger.println("parallel > 10,use 10");

        MAX_PARALLEL
    } else {
        args.parallel
    };

    //
    // Excel读取
    //
    let servers = read_excel(&args.excel)?;

    logger.println(&format!("EXCEL LOAD SUCCESS count={}", servers.len()));

    //
    // host分组
    //
    let hosts = group_by_host(servers);

    logger.println(&format!("HOST GROUP COUNT={}", hosts.len()));

    //
    // replace需要
    //
    let db_config = load_db_type("config/db_type.json")?;

    let engine = load_engine("config/engine.json")?;

    //
    // 线程池
    //
    let pool = ThreadPoolBuilder::new().num_threads(parallel).build()?;

    logger.println(&format!("THREAD POOL START parallel={}", parallel));

    match args.mode.as_str() {
        //
        // 文件替换
        //
        "replace" => {
            pool.install(|| {
                hosts.par_iter().for_each(|host| {
                    let result =
                        replace_host(host, &db_config, &engine, logger.as_ref(), &args.arch);

                    match result {
                        Ok(_) => {
                            logger.println(&format!("HOST SUCCESS {}", host.host));
                        }

                        Err(e) => {
                            logger.println(&format!("HOST FAILED {} {}", host.host, e));
                        }
                    }
                });
            });
        }

        //
        // 回退
        //
        "rollback" => {
            let task_id =
                args.task_id
                .clone()
                .unwrap_or_default();

            pool.install(|| {
                hosts.par_iter().for_each(|host| {
                    let result =
                        rollback_host(host, &db_config, &engine, logger.as_ref(), &args.arch, &task_id);

                    match result {
                        Ok(_) => {
                            logger.println(&format!("ROLLBACK SUCCESS {}", host.host));
                        }

                        Err(e) => {
                            logger.println(&format!("ROLLBACK FAILED {} {}", host.host, e));
                        }
                    }
                });
            });
        }

        _ => {
            anyhow::bail!("unsupported mode {},use replace or rollback", args.mode);
        }
    }

    logger.println("ALL TASK FINISH");

    Ok(())
}
