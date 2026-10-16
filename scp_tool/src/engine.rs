use anyhow::{Result, anyhow};

use serde::Deserialize;

use serde_json::Value;

use std::collections::HashMap;

use ssh_pub::config::Server;

use crate::db_type::MatchedRule;

//
// 给 replace.rs 使用
//
#[derive(Debug, Clone)]
pub struct ReplaceTask {


    // 本地文件
    pub local_file:String,


    // 远程文件
    pub remote_file:String,


    // 服务名
    pub service_name:String,


    // 组件
    pub component:String,


    // bin/module
    pub file_type:String,


    // 文件名
    pub file_name:String,

}





//
// engine.json 根节点
//
#[derive(Debug, Deserialize)]
pub struct EngineConfig {


    pub engines:

    HashMap<String, Value>


}





//
// 普通engine
//
// hbase/kafka/oceanbase
//
#[derive(Debug, Deserialize)]
pub struct Engine {


    #[serde(flatten)]
    pub components:

    HashMap<String, Component>

}





//
// ds/dt/dy
//
#[derive(Debug, Deserialize)]
pub struct Component {


    #[serde(flatten)]
    pub arch:

    HashMap<String, Arch>

}





//
// x86/arm
//
#[derive(Debug, Deserialize)]
pub struct Arch {


    pub local:

    HashMap<String,String>,


    pub remote:

    HashMap<String,String>

}





//
// special程序
//
// xpmon/newxpmon
//
#[derive(Debug, Deserialize)]
pub struct SpecialService {


    #[serde(flatten)]
    pub arch:

    HashMap<String, SpecialArch>

}





#[derive(Debug, Deserialize)]
pub struct SpecialArch {


    pub local:

    HashMap<String,String>,


    pub remote:

    HashMap<String,String>

}





//
// 加载engine.json
//
pub fn load_engine(

    file:&str

)->Result<EngineConfig>{


    let content =

        std::fs::read_to_string(

            file

        )?;


    let config:

        EngineConfig =


        serde_json::from_str(

            &content

        )?;


    Ok(config)

}

//
//
// 根据db_type规则生成ReplaceTask
//
pub fn build_replace_tasks(

    engine:&EngineConfig,

    rule:&MatchedRule,

    server:&Server,

    arch:&str

)->Result<Vec<ReplaceTask>>{


    match rule.mode.as_str(){


        "special" => {


            build_special_tasks(

                engine,

                rule,

                arch

            )


        },


        _ => {


            build_normal_tasks(

                engine,

                rule,

                server,

                arch

            )


        }

    }

}








//
// 普通模式
//
// db_to_bigdata
// db_to_db
//
fn build_normal_tasks(

    engine:&EngineConfig,

    rule:&MatchedRule,

    server:&Server,

    arch:&str

)->Result<Vec<ReplaceTask>>{


    let mut tasks =

        Vec::new();





    //
    // source engine
    //
    let source_engine_value =


        engine.engines

        .get(

            &rule.source_db

        )

        .ok_or(


            anyhow!(

            "source engine not found:{}",

            rule.source_db

            )

        )?;





    let source_engine:

        Engine =


        serde_json::from_value(

            source_engine_value.clone()

        )?;





    //
    // target engine
    //
    let target_engine_value =


        engine.engines

        .get(

            &rule.target_db

        )

        .ok_or(


            anyhow!(

            "target engine not found:{}",

            rule.target_db

            )

        )?;





    let target_engine:

        Engine =


        serde_json::from_value(

            target_engine_value.clone()

        )?;







    //
    // source组件
    //
    // oceanbase
    //      |
    //      ds
    //
    for (component, files)

        in &rule.source_components

    {


        let source_cfg =


            source_engine.components

            .get(component)

            .ok_or(


                anyhow!(

                "source component not found:{}",

                component

                )

            )?;





        let source_arch_cfg =


            source_cfg.arch

            .get(arch)

            .ok_or(


                anyhow!(

                "source arch not found:{}",

                arch

                )

            )?;





        build_component_replace_tasks(

            &mut tasks,

            component,

            files,

            source_arch_cfg,

            source_arch_cfg,

            server

        )?;


    }







    //
    // target组件
    //
    // hbase
    //      |
    //      dt/dy
    //
    for (component, files)

        in &rule.target_components

    {


        let target_cfg =


            target_engine.components

            .get(component)

            .ok_or(


                anyhow!(

                "target component not found:{}",

                component

                )

            )?;





        let target_arch_cfg =


            target_cfg.arch

            .get(arch)

            .ok_or(


                anyhow!(

                "target arch not found:{}",

                arch

                )

            )?;





        build_component_replace_tasks(

            &mut tasks,

            component,

            files,

            target_arch_cfg,

            target_arch_cfg,

            server

        )?;


    }





    Ok(tasks)

}
//
//
// special模式
//
// special.xpmon.x86
//
fn build_special_tasks(

    engine:&EngineConfig,

    rule:&MatchedRule,

    arch:&str

)->Result<Vec<ReplaceTask>>{


    let mut tasks =

        Vec::new();




    let special_value =


        engine.engines

        .get("special")

        .ok_or(


            anyhow!(

            "special engine missing"

            )

        )?;





    let service_value =


        special_value

        .get(

            &rule.target_db

        )

        .ok_or(


            anyhow!(

            "special target not found:{}",

            rule.target_db

            )

        )?;





    let service:


        SpecialService =


        serde_json::from_value(

            service_value.clone()

        )?;





    let arch_cfg =


        service.arch

        .get(arch)

        .ok_or(


            anyhow!(

            "special arch not found:{}",

            arch

            )

        )?;







    for (component, files)

        in &rule.source_components

    {


        for file in &files.bin {


            let local_root =


                arch_cfg.local

                .get("bin")

                .ok_or(


                    anyhow!(

                    "special local bin missing"

                    )

                )?;





            let remote_root =


                arch_cfg.remote

                .get("bin")

                .ok_or(


                    anyhow!(

                    "special remote bin missing"

                    )

                )?;





            tasks.push(

                ReplaceTask {


                    local_file:

                    join_path(

                        local_root,

                        file

                    ),



                    remote_file:

                    join_path(

                        remote_root,

                        file

                    ),



                    service_name:

                    rule.target_db.clone(),



                    component:

                    component.clone(),



                    file_type:

                    "bin".to_string(),



                    file_name:

                    file.clone()


                }

            );


        }


    }



    Ok(tasks)

}








//
// 生成组件替换任务
//
// source local
//
// target remote
//
fn build_component_replace_tasks(

    tasks:&mut Vec<ReplaceTask>,

    component:&str,

    files:&crate::db_type::Component,
    local_cfg:&Arch,

    remote_cfg:&Arch,

    server:&Server

)->Result<()>{



    //
    // bin
    //
    build_file_tasks(

        tasks,

        component,

        "bin",

        &files.bin,

        local_cfg,

        remote_cfg,

        server

    )?;





    //
    // module
    //
    build_file_tasks(

        tasks,

        component,

        "module",

        &files.module,

        local_cfg,

        remote_cfg,

        server

    )?;





    Ok(())

}









//
// 生成文件替换任务
//
fn build_file_tasks(

    tasks:&mut Vec<ReplaceTask>,

    component:&str,

    file_type:&str,

    files:&Vec<String>,

    local_cfg:&Arch,

    remote_cfg:&Arch,

    server:&Server

)->Result<()>{


    if files.is_empty(){

        return Ok(());

    }





    let local_root =


        local_cfg.local

        .get(file_type)

        .ok_or(


            anyhow!(

            "local {} path missing component:{}",

            file_type,

            component

            )

        )?;





    let remote_root =


        remote_cfg.remote

        .get(file_type)

        .ok_or(


            anyhow!(

            "remote {} path missing component:{}",

            file_type,

            component

            )

        )?;





    let remote_root =


        expand_path(

            remote_root,

            server

        );





    for file in files {


        tasks.push(

            ReplaceTask {


                local_file:

                join_path(

                    local_root,

                    file

                ),



                remote_file:

                join_path(

                    &remote_root,

                    file

                ),



                service_name:

                server.service_name.clone(),



                component:

                component.to_string(),



                file_type:

                file_type.to_string(),



                file_name:

                file.clone()


            }

        );


    }



    Ok(())

}









//
// 替换远程路径变量
//
fn expand_path(

    path:&str,

    server:&Server

)->String{


    path

    .replace(

        "{base_dir}",

        &server.agent_base

    )

    .replace(

        "{service_name}",

        &server.service_name

    )


}









//
// 路径拼接
//
fn join_path(

    root:&str,

    file:&str

)->String{


    format!(

        "{}/{}",

        root.trim_end_matches('/'),

        file

    )


}
