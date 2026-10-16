use anyhow::{Result, anyhow};
use std::fs::File;
use std::io::copy;
use std::path::Path;
use crate::ssh::SshClient;

//
// SFTP文件传输模块
//
// 负责:
// 文件上传
// 文件下载
//

pub struct ScpClient<'a> {
    ssh:&'a SshClient,
}


impl<'a> ScpClient<'a> {


    //
    // 创建客户端
    //
    pub fn new(
        ssh:&'a SshClient
    )->Self{

        ScpClient{
            ssh
        }
    }



    //
    // 上传文件
    //
    // 流程:
    //
    // local
    //
    //    |
    //
    //    v
    //
    // remote.tmp
    //
    //    |
    //
    // 删除旧文件
    //
    //    |
    //
    // rename
    //
    //    |
    //
    // remote
    //

    pub fn upload_file(
        &self,
        local:&str,
        remote:&str,
        mode:i32,
    )->Result<()> {


        println!(
            "SFTP START local={} remote={}",
            local,
            remote
        );


        //
        // 本地文件
        //
        let mut local_file =
            File::open(local)?;

        let size =
            local_file
            .metadata()?
            .len();


        println!(
            "LOCAL SIZE={}",
            size
        );



        //
        // 创建sftp
        //
        println!(
            "CREATE SFTP SESSION"
        );


        let sftp =
            self.ssh
            .session()
            .sftp()
            .map_err(|e|{
                anyhow!(
                    "CREATE SFTP FAILED {}",
                    e
                )
            })?;


        println!(
            "CREATE SFTP SUCCESS"
        );



        //
        // 检查目录
        //
        let parent =
            Path::new(remote)
            .parent()
            .ok_or(
                anyhow!(
                    "REMOTE PATH INVALID {}",
                    remote
                )
            )?;


        println!(
            "CHECK REMOTE DIR {:?}",
            parent
        );


        match sftp.stat(parent){

            Ok(_)=>{
                println!(
                    "REMOTE DIR OK"
                );
            }


            Err(e)=>{

                return Err(
                    anyhow!(
                        "REMOTE DIR ERROR {:?} {}",
                        parent,
                        e
                    )
                );

            }
        }



//
// 临时文件
//
// 防止并发上传冲突
//
use std::time::{
    SystemTime,
    UNIX_EPOCH
};

let timestamp =
    SystemTime::now()
    .duration_since(
        UNIX_EPOCH
    )?
    .as_millis();

let pid =
    std::process::id();

let tmp =
    format!(
        "{}.{}.{}.tmp",
        remote,
        timestamp,
        pid
    );

println!(
    "CREATE TEMP FILE {}",
    tmp
);


        //
        // 创建临时文件
        //
        let mut remote_file =
            sftp
            .create(
                Path::new(&tmp)
            )
            .map_err(|e|{

                anyhow!(
                    "CREATE TEMP FAILED {} {}",
                    tmp,
                    e
                )

            })?;


        println!(
            "CREATE TEMP SUCCESS"
        );



        //
        // 写入
        //
        let write_size =
            copy(
                &mut local_file,
                &mut remote_file
            )?;


        println!(
            "WRITE SUCCESS bytes={}",
            write_size
        );



        //
        // 关闭文件
        //
        drop(remote_file);



        //
        // 设置权限
        //
        let mut stat =
            sftp
            .stat(
                Path::new(&tmp)
            )?;


        stat.perm =
            Some(
                mode as u32
            );


        sftp
        .setstat(
            Path::new(&tmp),
            stat
        )?;


        println!(
            "CHMOD SUCCESS {}",
            tmp
        );



        //
        // 删除旧文件
        //
        println!(
            "REMOVE OLD FILE {}",
            remote
        );


match sftp.unlink(
    Path::new(remote)
){

    Ok(_)=>{

        println!(
            "REMOVE OLD SUCCESS {}",
            remote
        );

    }


    Err(e)=>{

        println!(
            "REMOVE OLD SKIP {} {}",
            remote,
            e
        );

    }

}


        //
        // tmp改正式文件
        //
        println!(
            "RENAME {} -> {}",
            tmp,
            remote
        );


        sftp
        .rename(
            Path::new(&tmp),
            Path::new(remote),
            None
        )
        .map_err(|e|{

            anyhow!(
                "RENAME FAILED {} -> {} {}",
                tmp,
                remote,
                e
            )

        })?;



        println!(
            "SFTP FINISH {}",
            remote
        );


        Ok(())
    }





    //
    // 下载
    //
    pub fn download_file(
        &self,
        remote:&str,
        local:&str
    )->Result<()> {


        let sftp =
            self.ssh
            .session()
            .sftp()?;


        let mut remote_file =
            sftp
            .open(
                Path::new(remote)
            )?;



        let mut local_file =
            File::create(
                local
            )?;



        copy(
            &mut remote_file,
            &mut local_file
        )?;


        local_file.sync_all()?;


        Ok(())

    }

}
