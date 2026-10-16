use anyhow::{
    Result,
    anyhow,
};

use clap::Parser;

use rayon::ThreadPoolBuilder;

use ssh_pub::{
    config::Server,
    excel,
    ssh::SshClient,
};

mod db_type;
mod logger;




#[derive(Parser, Debug)]
#[command(
    name="stop_tool",
    version="0.1.0",
    about="DataXone Agent Stop Tool"
)]
struct Args {


    /// Excel文件
    #[arg(
        short,
        long
    )]
    excel:String,



    /// 停止组件范围
    ///
    /// all/ds/dt/dy
    #[arg(
        short,
        long,
        default_value="all"
    )]
    r#type:String,



    /// 最大并发数
    ///
    /// 1-10
    #[arg(
        short,
        long,
        default_value="5"
    )]
    parallel:usize,



    /// 数据库类型配置文件
    #[arg(
        long,
        default_value="stop_tool/config/db_type.json"
    )]
    db_type:String,

}




fn main()->Result<()> {


    let args =
        Args::parse();



    //
    // 并发限制
    //
    if args.parallel == 0
        ||
       args.parallel > 10
    {

        return Err(
            anyhow!(
                "parallel must be between 1 and 10"
            )
        );

    }



    //
    // 初始化日志
    //
    let logger =
        logger::Logger::new()?;



    logger.println(
        "STOP TOOL START"
    );



    logger.println(
        &format!(
            "EXCEL:{} TYPE:{} PARALLEL:{}",
            args.excel,
            args.r#type,
            args.parallel
        )
    );



    //
    // 读取数据库类型JSON
    //
    let db_config =
        db_type::load_config(
            &args.db_type
        )?;



    //
    // 读取Excel
    //
    let servers =
        excel::read_excel(
            &args.excel
        )?;



    println!(
        "Excel:{}",
        args.excel
    );


    println!(
        "TYPE:{}",
        args.r#type
    );


    println!(
        "PARALLEL:{}",
        args.parallel
    );


    println!(
        "SERVER COUNT:{}",
        servers.len()
    );



    logger.println(
        &format!(
            "SERVER COUNT:{}",
            servers.len()
        )
    );



    //
    // 统计
    //
    let total =
        servers.len();



    let pool =
        ThreadPoolBuilder::new()
            .num_threads(
                args.parallel
            )
            .build()?;



    //
    // 原子统计
    //
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };



    use std::sync::Arc;



    let success =
        Arc::new(
            AtomicUsize::new(0)
        );



    let failed =
        Arc::new(
            AtomicUsize::new(0)
        );



    let skip =
        Arc::new(
            AtomicUsize::new(0)
        );



    //
    // 每个线程处理一台主机
    //
    pool.scope(
        |scope| {


            for server in servers {


                let logger =
                    logger.clone();


                let db_config =
                    db_config.clone();


                let user_type =
                    args.r#type.clone();


                let success =
                    success.clone();


                let failed =
                    failed.clone();


                let skip =
                    skip.clone();



                scope.spawn(
                    move |_| {


                        let result =
                            stop_server(

                                &server,

                                &user_type,

                                &db_config,

                                &logger

                            );



                        match result {


                            Ok(
                                StopResult::Success
                            ) => {

                                success.fetch_add(
                                    1,
                                    Ordering::Relaxed
                                );

                            }



                            Ok(
                                StopResult::Skip
                            ) => {

                                skip.fetch_add(
                                    1,
                                    Ordering::Relaxed
                                );

                            }



                            Ok(
                                StopResult::Failed
                            )
                            |
                            Err(_)
                            => {

                                failed.fetch_add(
                                    1,
                                    Ordering::Relaxed
                                );

                            }

                        }

                    }
                );

            }

        }
    );



    let success_count =
        success.load(
            Ordering::Relaxed
        );


    let failed_count =
        failed.load(
            Ordering::Relaxed
        );


    let skip_count =
        skip.load(
            Ordering::Relaxed
        );



    println!();

    println!(
        "=============================="
    );

    println!(
        "TOTAL   :{}",
        total
    );

    println!(
        "SUCCESS :{}",
        success_count
    );

    println!(
        "FAILED  :{}",
        failed_count
    );

    println!(
        "SKIP    :{}",
        skip_count
    );

    println!(
        "=============================="
    );



    logger.println(
        &format!(
            "TOTAL:{} SUCCESS:{} FAILED:{} SKIP:{}",
            total,
            success_count,
            failed_count,
            skip_count
        )
    );



    logger.println(
        "STOP TOOL FINISH"
    );



    Ok(())

}






#[derive(
    Debug,
    Clone,
    Copy
)]
enum StopResult {

    Success,

    Failed,

    Skip,

}






fn stop_server(

    server:&Server,

    user_type:&str,

    db_config:&db_type::DbTypeConfig,

    logger:&logger::Logger

)->Result<StopResult>{



    logger.println(
        &format!(
            "{}:{} START",
            server.host,
            server.port
        )
    );



    //
    // 1. 根据 source_db + target_db
    //    获取数据库支持组件
    //
    let support_components =
        db_type::get_components(
            server,
            db_config
        );



    //
    // 没有数据库匹配规则
    //
    if support_components.is_empty()
    {


        println!(
            "STOP_RESULT|{}|{}|SKIP|UNSUPPORTED_DB_TYPE",
            server.host,
            server.port
        );



        logger.println(
            &format!(
                "{} SKIP UNSUPPORTED_DB_TYPE {} -> {}",
                server.host,
                server.source_db,
                server.target_db
            )
        );



        return Ok(
            StopResult::Skip
        );

    }



    //
    // 2. 用户 type 过滤
    //
    let execute_components =
        filter_components(
            support_components,
            user_type
        );



    //
    // 没有交集
    //
    if execute_components.is_empty()
    {


        println!(
            "STOP_RESULT|{}|{}|SKIP|NO_COMPONENT_MATCH",
            server.host,
            server.port
        );



        logger.println(
            &format!(
                "{} SKIP NO_COMPONENT_MATCH TYPE={}",
                server.host,
                user_type
            )
        );



        return Ok(
            StopResult::Skip
        );

    }



    //
    // 3. SSH预检查
    //
    //
    // 注意：
    // 这里建立一次SSH连接。
    // 后续ds/dt/dy全部复用。
    //
    let client =
        match SshClient::connect(
            server
        )
        {

            Ok(client)=>{


                logger.println(
                    &format!(
                        "{}:{} CONNECT SUCCESS",
                        server.host,
                        server.port
                    )
                );


                client

            }



            Err(e)=>{


                println!(
                    "STOP_RESULT|{}|{}|FAILED|CONNECT_FAILED",
                    server.host,
                    server.port
                );


                logger.println(
                    &format!(
                        "{}:{} CONNECT FAILED:{}",
                        server.host,
                        server.port,
                        e
                    )
                );


                //
                // SSH失败：
                // 当前主机直接结束
                //
                return Ok(
                    StopResult::Failed
                );

            }

        };



    //
    // 4. 执行组件
    //
    for component in
        execute_components
    {


        logger.println(
            &format!(
                "{} STOP COMPONENT:{}",
                server.host,
                component
            )
        );



        println!(
            "STOP COMPONENT:{} {}",
            server.host,
            component
        );



        let stop_cmd =
            build_stop_cmd(
                server,
                &component
            );



        logger.println(
            &format!(
                "{} EXEC:{}",
                server.host,
                stop_cmd
            )
        );



        //
        // 使用同一个SSH连接
        //
        match client.exec(
            &stop_cmd
        )
        {

            Ok(_output)=>{


                println!(
                    "STOP_RESULT|{}|{}|SUCCESS|{}_STOP_SUCCESS",
                    server.host,
                    server.port,
                    component.to_uppercase()
                );


                logger.println(
                    &format!(
                        "{} STOP {} SUCCESS",
                        server.host,
                        component
                    )
                );

            }



            Err(e)=>{


                println!(
                    "STOP_RESULT|{}|{}|FAILED|STOP_FAILED",
                    server.host,
                    server.port
                );


                logger.println(
                    &format!(
                        "{} STOP {} FAILED:{}",
                        server.host,
                        component,
                        e
                    )
                );


                return Ok(
                    StopResult::Failed
                );

            }

        }

    }



    //
    // SshClient离开作用域
    // SSH连接自动释放
    //
    logger.println(
        &format!(
            "{} STOP SUCCESS",
            server.host
        )
    );



    Ok(
        StopResult::Success
    )

}






//
// 用户type过滤
//
// 数据库支持组件 ∩ 用户选择组件
//
fn filter_components(

    components:Vec<String>,

    user_type:&str

)->Vec<String>{



    //
    // all = 数据库实际支持的全部组件
    //
    if user_type
        .eq_ignore_ascii_case(
            "all"
        )
    {

        return components;

    }



    //
    // ds / dt / dy
    //
    components
        .into_iter()
        .filter(
            |component|
                component
                    .eq_ignore_ascii_case(
                        user_type
                    )
        )
        .collect()

}






//
// 根据组件生成停止脚本
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
fn build_stop_cmd(

    server:&Server,

    component:&str

)->String{



    let component_dir =
        match component
        {


            //
            // ds
            //
            "ds" => {

                format!(
                    "ds_{}",
                    server.service_name
                )

            }



            //
            // dt
            //
            "dt" => {

                format!(
                    "dt_{}",
                    server.service_name
                )

            }



            //
            // dy
            //
            // 注意：
            // dy组件实际目录叫
            // dt_service_name_y
            //
            "dy" => {

                format!(
                    "dt_{}_y",
                    server.service_name
                )

            }



            //
            // JSON未来新增组件
            //
            _ => {

                format!(
                    "{}_{}",
                    component,
                    server.service_name
                )

            }

        };



    format!(
        "cd {}/{}/{}/scripts && ./stop_flow.sh",
        server.agent_base,
        server.service_name,
        component_dir
    )

}
