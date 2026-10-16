use anyhow::Result;

use serde::{
    Deserialize,
    Serialize,
};

use std::fs;

use ssh_pub::config::Server;


#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize
)]
pub struct DbTypeConfig {

    pub rules:Vec<DbRule>,

}


#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize
)]
pub struct DbRule {

    pub source_db:String,

    pub target_db:String,

    pub components:Vec<String>,

}




//
// 读取 db_type.json
//
pub fn load_config(
    file:&str
)->Result<DbTypeConfig>{


    let content =
        fs::read_to_string(
            file
        )?;


    let config:
        DbTypeConfig =
        serde_json::from_str(
            &content
        )?;


    Ok(config)

}






//
// 根据 Excel 中的 source_db / target_db
// 获取数据库类型支持的组件
//
pub fn get_components(

    server:&Server,

    config:&DbTypeConfig

)->Vec<String>{


    for rule in &config.rules {


        if rule.source_db
            .eq_ignore_ascii_case(
                &server.source_db
            )
            &&
           rule.target_db
            .eq_ignore_ascii_case(
                &server.target_db
            )
        {


            return rule
                .components
                .clone();

        }

    }



    Vec::new()

}
