use anyhow::{bail, Result};
use clap::Parser;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::io::Write;

use ssh_pub::config::Server;
use ssh_pub::excel::read_excel_detailed;
use ssh_pub::shell;
use ssh_pub::ssh::SshClient;

mod db_type;
mod logger;

#[derive(Parser, Debug)]
#[command(
    name = "stop_tool",
    version = "0.1.0",
    about = "DataXone Agent Stop Tool"
)]
struct Args {
    /// Excel文件
    #[arg(short, long)]
    excel: String,

    /// 停止组件范围
    ///
    /// all/ds/dt/dy
    #[arg(short, long, default_value = "all")]
    r#type: String,

    /// 最大并发数
    ///
    /// 1-10
    #[arg(short, long, default_value = "5")]
    parallel: usize,

    /// 数据库类型配置文件
    #[arg(long, default_value = "stop_tool/config/db_type.json")]
    db_type: String,

    /// SSH 阻塞操作超时(秒), 0 = 不限制
    ///
    /// 停服脚本卡住时不会无限等待
    #[arg(long, default_value = "300")]
    timeout: u64,
}

//
// 单台结果
//
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopResult {
    Success,

    Failed,

    Skip,
}

impl StopResult {
    fn as_str(&self) -> &'static str {
        match self {
            StopResult::Success => "SUCCESS",

            StopResult::Failed => "FAILED",

            StopResult::Skip => "SKIP",
        }
    }
}

#[derive(Debug, Clone)]
struct ServerReport {
    host: String,

    port: u16,

    result: StopResult,

    reason: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    //
    // 并发限制
    //
    if args.parallel == 0 || args.parallel > 10 {
        bail!("parallel must be between 1 and 10");
    }

    //
    // 初始化日志
    //
    let logger = logger::Logger::new()?;

    logger.println("STOP TOOL START");

    logger.println(&format!(
        "EXCEL:{} TYPE:{} PARALLEL:{} TIMEOUT:{}s",
        args.excel, args.r#type, args.parallel, args.timeout
    ));

    //
    // 读取数据库类型JSON
    //
    // 兼容"仓库根目录"与"工具目录"两种启动位置
    //
    let db_type_path = resolve_config(&args.db_type);

    logger.println(&format!("DB_TYPE CONFIG:{}", db_type_path));

    let db_config = db_type::load_config(&db_type_path)?;

    //
    // 读取Excel
    //
    let data = read_excel_detailed(&args.excel)?;

    for w in &data.warnings {
        logger.println(&format!("EXCEL WARN {}", w));

        eprintln!("[WARN] {}", w);
    }

    let servers = data.servers;

    if servers.is_empty() {
        bail!("Excel 没有可用数据行: {}", args.excel);
    }

    println!("Excel:{}", args.excel);

    println!("TYPE:{}", args.r#type);

    println!("PARALLEL:{}", args.parallel);

    println!("SERVER COUNT:{}", servers.len());

    logger.println(&format!("SERVER COUNT:{}", servers.len()));

    //
    // SSH 阻塞超时
    //
    // 命令行用秒, libssh2 需要毫秒; 0 = 不限制
    //
    let timeout_ms = args.timeout.saturating_mul(1000).min(u32::MAX as u64) as u32;

    let user_type = args.r#type.clone();

    let pool = ThreadPoolBuilder::new()
        .num_threads(args.parallel)
        .build()?;

    //
    // 并发停服并回收真实结果
    //
    let reports: Vec<ServerReport> = pool.install(|| {
        servers
            .par_iter()
            .map(|server| stop_server(server, &user_type, &db_config, &logger, timeout_ms))
            .collect()
    });

    //
    // 汇总
    //
    let total = reports.len();

    let success_count = reports
        .iter()
        .filter(|r| r.result == StopResult::Success)
        .count();

    let failed_count = reports
        .iter()
        .filter(|r| r.result == StopResult::Failed)
        .count();

    let skip_count = reports
        .iter()
        .filter(|r| r.result == StopResult::Skip)
        .count();

    println!();

    println!("==============================");

    println!("TOTAL   :{}", total);

    println!("SUCCESS :{}", success_count);

    println!("FAILED  :{}", failed_count);

    println!("SKIP    :{}", skip_count);

    println!("==============================");

    //
    // 失败明细: 便于直接定位是哪台机器哪个组件
    //
    if failed_count > 0 {
        println!();

        println!("失败明细:");

        for r in reports.iter().filter(|r| r.result == StopResult::Failed) {
            println!("  {}:{}  {}", r.host, r.port, r.reason);
        }
    }

    logger.println(&format!(
        "TOTAL:{} SUCCESS:{} FAILED:{} SKIP:{}",
        total, success_count, failed_count, skip_count
    ));

    logger.println("STOP TOOL FINISH");

    //
    // 有失败就返回非 0
    //
    if failed_count > 0 {
        std::io::stdout().flush().ok();

        std::process::exit(1);
    }

    Ok(())
}

//
// 配置文件定位
//
// 默认值是 stop_tool/config/db_type.json(相对仓库根目录),
// 若按工具目录启动则应使用 config/db_type.json。
//
fn resolve_config(path: &str) -> String {
    if std::path::Path::new(path).exists() {
        return path.to_string();
    }

    if let Some(name) = std::path::Path::new(path).file_name() {
        let alt = format!("config/{}", name.to_string_lossy());

        if std::path::Path::new(&alt).exists() {
            return alt;
        }
    }

    path.to_string()
}

//
// 停一台服务器
//
fn stop_server(
    server: &Server,

    user_type: &str,

    db_config: &db_type::DbTypeConfig,

    logger: &logger::Logger,

    timeout_ms: u32,
) -> ServerReport {
    let make = |result: StopResult, reason: String| ServerReport {
        host: server.host.clone(),

        port: server.port,

        result,

        reason,
    };

    logger.println(&format!("{}:{} START", server.host, server.port));

    //
    // 1. 根据 source_db + target_db 获取数据库支持组件
    //
    let support_components = db_type::get_components(server, db_config);

    if support_components.is_empty() {
        println!(
            "STOP_RESULT|{}|{}|SKIP|UNSUPPORTED_DB_TYPE",
            server.host, server.port
        );

        logger.println(&format!(
            "{} SKIP UNSUPPORTED_DB_TYPE {} -> {}",
            server.host, server.source_db, server.target_db
        ));

        return make(
            StopResult::Skip,
            format!(
                "db_type.json 中没有匹配规则 {} -> {}",
                server.source_db, server.target_db
            ),
        );
    }

    //
    // 2. 用户 type 过滤
    //
    let execute_components = filter_components(support_components, user_type);

    if execute_components.is_empty() {
        println!(
            "STOP_RESULT|{}|{}|SKIP|NO_COMPONENT_MATCH",
            server.host, server.port
        );

        logger.println(&format!(
            "{} SKIP NO_COMPONENT_MATCH TYPE={}",
            server.host, user_type
        ));

        return make(
            StopResult::Skip,
            format!("该数据库类型不包含组件 {}", user_type),
        );
    }

    //
    // 3. SSH连接
    //
    // 一次连接, 后续 ds/dt/dy 全部复用
    //
    let client = match SshClient::connect(server) {
        Ok(client) => {
            logger.println(&format!(
                "{}:{} CONNECT SUCCESS",
                server.host, server.port
            ));

            client
        }

        Err(e) => {
            println!(
                "STOP_RESULT|{}|{}|FAILED|CONNECT_FAILED",
                server.host, server.port
            );

            logger.println(&format!(
                "{}:{} CONNECT FAILED:{}",
                server.host, server.port, e
            ));

            return make(StopResult::Failed, format!("SSH 连接失败: {}", e));
        }
    };

    client.set_timeout(timeout_ms);

    //
    // 4. 执行组件
    //
    let mut stopped = 0usize;

    let mut skipped: Vec<String> = Vec::new();

    let mut failed: Vec<String> = Vec::new();

    for component in execute_components {
        let component_dir = match build_component_dir(server, &component) {
            Ok(v) => v,

            Err(e) => {
                failed.push(format!("{}: {}", component, e));

                continue;
            }
        };

        //
        // 预检: 目录不存在就没必要执行, 也不该报成功
        //
        let probe = client.run(&format!(
            "if [ -d {} ]; then echo YES; fi",
            shell::quote(&component_dir)
        ));

        match probe {
            Ok(out) if out.stdout.contains("YES") => {}

            Ok(_) => {
                skipped.push(format!("{} 目录不存在 {}", component, component_dir));

                logger.println(&format!(
                    "{} {} SKIP DIR NOT FOUND {}",
                    server.host, component, component_dir
                ));

                continue;
            }

            Err(e) => {
                failed.push(format!("{} 目录检查失败: {}", component, e));

                continue;
            }
        }

        let stop_cmd = format!("cd {} && ./stop_flow.sh", shell::quote(&component_dir));

        logger.println(&format!("{} EXEC:{}", server.host, stop_cmd));

        //
        // 关键修复:
        //
        // 旧实现只判断 exec 是否 Err,
        // 而远端 stop_flow.sh 退出码非 0 时 exec 依然返回 Ok,
        // 于是"没停掉"也会被报成 SUCCESS。
        //
        match client.run(&stop_cmd) {
            Ok(out) if out.success() => {
                stopped += 1;

                logger.println(&format!("{} STOP {} SUCCESS", server.host, component));
            }

            Ok(out) => {
                let detail = out.combined();

                failed.push(format!(
                    "{} 停止失败 exit={} {}",
                    component,
                    out.status,
                    detail.trim()
                ));

                logger.println(&format!(
                    "{} STOP {} FAILED exit={} {}",
                    server.host,
                    component,
                    out.status,
                    detail.trim()
                ));
            }

            Err(e) => {
                failed.push(format!("{} 执行异常: {}", component, e));

                logger.println(&format!(
                    "{} STOP {} FAILED:{}",
                    server.host, component, e
                ));
            }
        }
    }

    //
    // 结果判定
    //
    let (result, reason) = if !failed.is_empty() {
        (StopResult::Failed, failed.join("; "))
    } else if stopped > 0 {
        (
            StopResult::Success,
            format!("已停止 {} 个组件", stopped),
        )
    } else {
        (StopResult::Skip, skipped.join("; "))
    };

    println!(
        "STOP_RESULT|{}|{}|{}|{}",
        server.host,
        server.port,
        result.as_str(),
        reason.replace('|', " ")
    );

    logger.println(&format!(
        "{} STOP {} {}",
        server.host,
        result.as_str(),
        reason
    ));

    make(result, reason)
}

//
// 用户type过滤
//
// 数据库支持组件 ∩ 用户选择组件
//
fn filter_components(components: Vec<String>, user_type: &str) -> Vec<String> {
    //
    // all = 数据库实际支持的全部组件
    //
    if user_type.eq_ignore_ascii_case("all") {
        return components;
    }

    //
    // ds / dt / dy
    //
    components
        .into_iter()
        .filter(|component| component.eq_ignore_ascii_case(user_type))
        .collect()
}

//
// 组件目录
//
// <agent_base>/<service_name>/<component_dir>
//
// ds:
// ds_service_name
//
// dt:
// dt_service_name
//
// dy:
// dt_service_name_y
//
fn build_component_dir(server: &Server, component: &str) -> Result<String> {
    if server.service_name.trim().is_empty() {
        bail!("服务名为空");
    }

    if server.service_name.contains('/') {
        bail!("服务名含非法字符 /: {}", server.service_name);
    }

    if component.trim().is_empty() || component.contains('/') {
        bail!("组件名非法: {}", component);
    }

    if server.agent_base.trim().is_empty() {
        bail!("部署目录为空");
    }

    let component_dir = match component {
        //
        // ds
        //
        "ds" => format!("ds_{}", server.service_name),

        //
        // dt
        //
        "dt" => format!("dt_{}", server.service_name),

        //
        // dy
        //
        // 注意: dy组件实际目录叫 dt_service_name_y
        //
        "dy" => format!("dt_{}_y", server.service_name),

        //
        // JSON未来新增组件
        //
        _ => format!("{}_{}", component, server.service_name),
    };

    let dir = format!(
        "{}/{}/{}",
        server.agent_base.trim_end_matches('/'),
        server.service_name,
        component_dir
    );

    //
    // 进入 shell 命令前做绝对路径校验
    //
    shell::validate_remote_path(&dir, "组件目录")?;

    Ok(dir)
}
