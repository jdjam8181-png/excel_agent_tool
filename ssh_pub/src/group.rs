use std::collections::HashMap;

use crate::config::Server;



//
// 一个服务器任务
//
// 一个host下面可能多个service
//
#[derive(Debug, Clone)]
pub struct HostGroup {


    //
    // 服务器地址
    //
    pub host:String,


    //
    // ssh端口
    //
    pub port:u16,


    //
    // 登录协议
    //
    pub protocol:String,


    //
    // 用户
    //
    pub username:String,


    //
    // 密码
    //
    pub password:String,


    //
    // 一个IP下面多个service
    //
    pub services:Vec<Server>,


}




//
// Excel读取结果
//
// Vec<Server>
//
// 转换
//
// HashMap<host, HostGroup>
//
pub fn group_by_host(

    servers:Vec<Server>

)->Vec<HostGroup>{



    let mut groups:

        HashMap<String,HostGroup> =

        HashMap::new();




    for server in servers {



        //
        // 分组key
        //
        //
        // 如果同一个IP
        //
        // 同一个账号
        //
        // 同一个端口
        //
        // 认为同一台机器
        //
        let key = format!(

            "{}:{}:{}",

            server.host,

            server.port,

            server.username

        );





        let entry =

            groups

            .entry(key)

            .or_insert_with(||{


                HostGroup {


                    host:
                    server.host.clone(),


                    port:
                    server.port,


                    protocol:
                    server.protocol.clone(),


                    username:
                    server.username.clone(),


                    password:
                    server.password.clone(),


                    services:
                    Vec::new(),

                }


            });






        //
        // 加入service
        //
        entry.services.push(

            server

        );

    }





    //
    // HashMap转Vec
    //
    // 方便线程池处理
    //
    groups

    .into_values()

    .collect()

}
