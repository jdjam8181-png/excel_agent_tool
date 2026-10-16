use anyhow::{Result, anyhow};

use md5::{
    Md5,
    Digest,
};

use std::fs::File;

use std::io::Read;

use crate::ssh::SshClient;





//
// 计算本地文件MD5
//
pub fn calc_file_md5(

    file:&str

)->Result<String>{


    let mut file =

        File::open(file)?;



    let mut hasher =

        Md5::new();



    let mut buffer =

        [0u8;8192];




    loop {


        let n =

            file.read(

                &mut buffer

            )?;



        if n == 0 {


            break;


        }



        hasher.update(

            &buffer[..n]

        );


    }




    let result =

        hasher.finalize();




    Ok(

        hex::encode(

            result

        )

    )


}









//
// replace.rs调用
//
// 本地文件MD5
//
pub fn local_md5(

    file:&str

)->Result<String>{


    calc_file_md5(

        file

    )


}









//
// 远程文件MD5
//
// 执行:
// md5sum remote_file
//
// 返回:
// d41d8cd98f00...
//
pub fn remote_md5(

    ssh:&SshClient,

    file:&str

)->Result<String>{



    let output =

        ssh.exec(

            &format!(

                "md5sum {}",

                file

            )

        )?;



    parse_remote_md5(

        &output

    )

    .ok_or(

        anyhow!(

            "remote md5 parse failed:{}",

            file

        )

    )


}









//
// 比较MD5
//
pub fn verify_md5(

    local_md5:&str,

    remote_md5:&str

)->bool{


    local_md5

        .trim()

        .eq_ignore_ascii_case(

            remote_md5.trim()

        )


}









//
// 解析远程md5sum输出
//
// 格式:
//
// d41d8cd98f00  /path/file
//
pub fn parse_remote_md5(

    output:&str

)->Option<String>{


    output

    .split_whitespace()

    .next()

    .map(

        |v|

        v.to_string()

    )

}
