use anyhow::{bail, Result};
use clap::Parser;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::io::Write;

mod db_type;

mod engine;

mod history;

mod replace;

mod rollback;

mod run;

use ssh_pub::excel::read_excel_detailed;
use ssh_pub::group::group_by_host;
use ssh_pub::logger::Logger;

use crate::db_type::load_db_type;
use crate::engine::load_engine;
use crate::replace::replace_host;
use crate::rollback::rollback_host;
use crate::run::{new_task_id, HostReport, RunOptions, RunSummary};

//
// 最大host并发
//
const MAX_PARALLEL: usize = 10;

//
// 参数
//
#[derive(Parser, Debug)]
#[command(
    name = "scp_tool",
    about = "批量升级: 按 Excel 清单替换远端程序文件, 支持回退"
)]
struct Args {
    //
    // 模式
    //
    // replace  升级
    // rollback 回退
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

    //
    // task_id
    //
    // rollback 必填
    // replace 不填则按当前时间生成(整批共用一个)
    //
    #[arg(long)]
    task_id: Option<String>,

    //
    // 预演: 只做预检并打印将要执行的动作, 不修改任何远端文件
    //
    #[arg(long)]
    dry_run: bool,

    //
    // SSH 阻塞操作超时(秒), 0 = 不限制
    //
    // 防止单台机器卡住导致整个批次长时间挂起
    //
    #[arg(long, default_value = "120")]
    timeout: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mode = args.mode.to_lowercase();

    if mode != "replace" && mode != "rollback" {
        bail!("unsupported mode {}, use replace or rollback", args.mode);
    }

    if args.arch != "x86" && args.arch != "arm" {
        bail!("arch 只支持 x86 / arm, 当前: {}", args.arch);
    }

    //
    // 日志: 每次运行独立文件
    //
    let logger = Logger::new(&mode)?;

    //
    // 并发控制
    //
    let parallel = if args.parallel == 0 {
        10
    } else if args.parallel > MAX_PARALLEL {
        logger.println(&format!(
            "parallel {} 超过上限, 使用 {}",
            args.parallel, MAX_PARALLEL
        ));

        MAX_PARALLEL
    } else {
        args.parallel
    };

    //
    // SSH 阻塞超时
    //
    // 命令行用秒, libssh2 需要毫秒; 0 = 不限制
    //
    let timeout_ms = args.timeout.saturating_mul(1000).min(u32::MAX as u64) as u32;

    //
    // task_id
    //
    // rollback 必须显式指定:
    // 旧实现缺省为空字符串, 备份路径变成 bin_, 永远找不到备份,
    // 却依然报告回退成功 —— 这是最危险的一种"假成功"。
    //
    let explicit_task_id = args
        .task_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    if mode == "rollback" && explicit_task_id.is_none() {
        bail!("rollback 必须显式指定 --task-id (replace 结束时会在汇总里打印)");
    }

    let task_id = match &explicit_task_id {
        Some(v) => v.clone(),

        None => new_task_id(),
    };

    logger.println(&format!(
        "START mode={} excel={} arch={} parallel={} task_id={} dry_run={} timeout={}s",
        mode, args.excel, args.arch, parallel, task_id, args.dry_run, args.timeout
    ));

    //
    // Excel读取
    //
    let data = read_excel_detailed(&args.excel)?;

    for w in &data.warnings {
        logger.println(&format!("EXCEL WARN {}", w));

        eprintln!("[WARN] {}", w);
    }

    logger.println(&format!("EXCEL LOAD SUCCESS count={}", data.servers.len()));

    if data.servers.is_empty() {
        bail!("Excel 没有可用数据行: {}", args.excel);
    }

    //
    // host分组
    //
    let hosts = group_by_host(data.servers);

    logger.println(&format!("HOST GROUP COUNT={}", hosts.len()));

    //
    // 配置
    //
    let db_config = load_db_type("config/db_type.json")?;

    let engine = load_engine("config/engine.json")?;

    //
    // 运行历史
    //
    // 只在主线程写两次(开始/结束), 不再由 10 个线程并发追加。
    //
    let operation = if mode == "replace" { "REPLACE" } else { "ROLLBACK" };

    //
    // 历史写不进去不应该阻断升级本身, 只告警
    //
    if let Err(e) = history::record(
        operation,
        &task_id,
        "RUNNING",
        &format!(
            "excel={} arch={} hosts={} dry_run={}",
            args.excel,
            args.arch,
            hosts.len(),
            args.dry_run
        ),
    ) {
        logger.println(&format!("HISTORY WRITE FAILED {}", e));
    }

    let opts = RunOptions {
        arch: args.arch.clone(),

        task_id: task_id.clone(),

        dry_run: args.dry_run,

        timeout_ms,
    };

    //
    // 线程池
    //
    let pool = ThreadPoolBuilder::new().num_threads(parallel).build()?;

    logger.println(&format!("THREAD POOL START parallel={}", parallel));

    //
    // 并发执行并回收每台主机的真实结果
    //
    let reports: Vec<HostReport> = pool.install(|| {
        hosts
            .par_iter()
            .map(|host| {
                if mode == "replace" {
                    replace_host(host, &db_config, &engine, &logger, &opts)
                } else {
                    rollback_host(host, &db_config, &engine, &logger, &opts)
                }
            })
            .collect()
    });

    let summary = RunSummary::from_reports(&reports);

    //
    // 失败明细写进日志
    //
    for report in reports.iter().filter(|r| !r.ok()) {
        logger.println(&format!(
            "HOST FAILED {} ssh_ok={} services_failed={} files_failed={}",
            report.host, report.ssh_ok, report.services_failed, report.files.files_failed
        ));

        for e in &report.errors {
            logger.println(&format!("  ERROR {} {}", report.host, e));
        }
    }

    let status = if summary.success() {
        "SUCCESS"
    } else {
        "PARTIAL"
    };

    if let Err(e) = history::record(
        operation,
        &task_id,
        status,
        &format!(
            "hosts={} ok={} failed={} files_ok={} files_failed={} files_skipped={}",
            summary.hosts_total,
            summary.hosts_ok,
            summary.hosts_failed,
            summary.files_ok,
            summary.files_failed,
            summary.files_skipped
        ),
    ) {
        logger.println(&format!("HISTORY WRITE FAILED {}", e));
    }

    logger.println(&format!(
        "SUMMARY hosts={} ok={} failed={} services_ok={} services_failed={} files_ok={} files_failed={} files_skipped={}",
        summary.hosts_total,
        summary.hosts_ok,
        summary.hosts_failed,
        summary.services_ok,
        summary.services_failed,
        summary.files_ok,
        summary.files_failed,
        summary.files_skipped
    ));

    logger.println("ALL TASK FINISH");

    print_summary(&summary, &task_id, &mode, args.dry_run);

    //
    // 有主机失败就返回非 0, 便于 CI / 批量编排感知
    //
    // 旧实现无论失败多少台都返回 0。
    //
    if !summary.success() {
        std::io::stdout().flush().ok();

        std::process::exit(1);
    }

    Ok(())
}

//
// 控制台汇总
//
fn print_summary(summary: &RunSummary, task_id: &str, mode: &str, dry_run: bool) {
    println!();

    println!("==============================");

    println!(
        "模式      : {}{}",
        mode,
        if dry_run {
            " (DRY-RUN, 未修改任何文件)"
        } else {
            ""
        }
    );

    println!("task_id   : {}", task_id);

    println!(
        "主机      : {} 总数 / {} 成功 / {} 失败",
        summary.hosts_total, summary.hosts_ok, summary.hosts_failed
    );

    println!(
        "服务      : {} 成功 / {} 失败",
        summary.services_ok, summary.services_failed
    );

    println!(
        "文件      : {} 成功 / {} 失败 / {} 跳过",
        summary.files_ok, summary.files_failed, summary.files_skipped
    );

    println!("==============================");

    //
    // 直接把回退命令打出来, 省得升级完再翻日志找 task_id
    //
    if mode == "replace" && !dry_run {
        println!(
            "回退命令  : ./scp_tool --mode rollback --excel <同一份Excel> --arch <x86|arm> --task-id {}",
            task_id
        );
    }
}
