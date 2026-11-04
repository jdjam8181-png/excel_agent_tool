use anyhow::Result;
use clap::Parser;

use ssh_pub::{excel, ssh::SshClient};

#[derive(Parser, Debug)]
#[command(name = "ssh_link", about = "SSH connection test tool")]
struct Args {
    /// Excel文件
    #[arg(short, long)]
    excel: String,

    /// 远程执行命令
    #[arg(short, long)]
    cmd: String,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    /*
       人看的信息走stderr
       不影响shell解析
    */
    eprintln!("Excel:{}", args.excel);

    eprintln!("CMD:{}", args.cmd);

    let servers = excel::read_excel(&args.excel)?;

    eprintln!("server count:{}", servers.len());

    let mut failed = false;

    for server in servers {
        eprintln!("check {}:{}", server.host, server.port);

        /*
           SSH连接
        */
        let client = match SshClient::connect(&server) {
            Ok(client) => client,

            Err(e) => {
                failed = true;

                println!(
                    "SSH_RESULT|{}|{}|FAILED|{}",
                    server.host,
                    server.port,
                    e.to_string().replace('\n', " ")
                );

                continue;
            }
        };

        /*
           执行远程命令

           修复: 旧实现只要 channel 建立成功就报 SUCCESS,
           远端命令退出码非 0 也照样是 SUCCESS。
           这里以真实退出码判定成败, 并把 exit 带回输出。
        */
        match client.run(&args.cmd) {
            Ok(out) => {
                let msg = out.combined().trim().replace('\n', " ");

                if out.success() {
                    println!(
                        "SSH_RESULT|{}|{}|SUCCESS|exit=0 {}",
                        server.host, server.port, msg
                    );
                } else {
                    failed = true;

                    println!(
                        "SSH_RESULT|{}|{}|FAILED|exit={} {}",
                        server.host, server.port, out.status, msg
                    );
                }
            }

            Err(e) => {
                failed = true;

                println!(
                    "SSH_RESULT|{}|{}|FAILED|{}",
                    server.host,
                    server.port,
                    e.to_string().replace('\n', " ")
                );
            }
        }
    }

    if failed {
        std::process::exit(1);
    }

    Ok(())
}
