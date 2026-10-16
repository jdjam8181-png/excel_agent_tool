use clap::Parser;
use anyhow::Result;

use ssh_pub::{
    excel,
    ssh::SshClient,
};


#[derive(Parser, Debug)]
#[command(
    name = "ssh_link",
    about = "SSH connection test tool"
)]
struct Args {

    /// Excel文件
    #[arg(
        short,
        long
    )]
    excel: String,


    /// 远程执行命令
    #[arg(
        short,
        long
    )]
    cmd: String,
}



fn main() -> Result<()> {

    env_logger::init();


    let args =
        Args::parse();


    /*
       人看的信息走stderr
       不影响shell解析
    */
    eprintln!(
        "Excel:{}",
        args.excel
    );


    eprintln!(
        "CMD:{}",
        args.cmd
    );


    let servers =
        excel::read_excel(
            &args.excel
        )?;


    eprintln!(
        "server count:{}",
        servers.len()
    );


    let mut failed = false;



    for server in servers {


        eprintln!(
            "check {}:{}",
            server.host,
            server.port
        );



        /*
           SSH连接
        */
        let client =
            match SshClient::connect(
                &server
            )
            {

                Ok(client)=>{
                    client
                }


                Err(e)=>{

                    failed=true;


                    println!(
                        "SSH_RESULT|{}|{}|FAILED|{}",
                        server.host,
                        server.port,
                        e
                    );


                    continue;
                }
            };



        /*
           执行远程命令
        */
        match client.exec(
            &args.cmd
        )
        {


            Ok(result)=>{


                let msg =
                    result
                    .trim()
                    .replace(
                        '\n',
                        " "
                    );


                println!(
                    "SSH_RESULT|{}|{}|SUCCESS|{}",
                    server.host,
                    server.port,
                    msg
                );

            }



            Err(e)=>{


                failed=true;


                println!(
                    "SSH_RESULT|{}|{}|FAILED|{}",
                    server.host,
                    server.port,
                    e
                );

            }

        }


    }



    if failed {

        std::process::exit(1);

    }


    Ok(())

}
