use anyhow::Result;

use chrono::Local;

use std::fs::{
    File,
    OpenOptions,
};

use std::io::Write;

use std::sync::{
    Arc,
    Mutex,
};





#[derive(Clone)]
pub struct Logger {


    file:Arc<Mutex<File>>,


}







impl Logger {



    //
    // 创建日志
    //
    // 每次运行生成独立日志
    //
    pub fn new(mode:&str)->Result<Self>{



        std::fs::create_dir_all(

            "log"

        )?;





        let filename =

            format!(

                "log/{}_{}.out",
                mode,
                Local::now()

                    .format(

                        "%Y%m%d_%H%M%S"

                    )

            );






        let file =

            OpenOptions::new()

            .create(true)

            .append(true)

            .open(

                filename

            )?;






        Ok(

            Logger {

                file:Arc::new(

                    Mutex::new(

                        file

                    )

                )

            }

        )


    }









    //
    // 写日志核心方法
    //
    pub fn println(

        &self,

        message:&str

    ){



        let now =

            Local::now()

            .format(

                "%Y-%m-%d %H:%M:%S"

            );





        let line =

            format!(

                "[{}] {}\n",

                now,

                message

            );





        if let Ok(

            mut file

        ) =

        self.file.lock()

        {


            let _ =

                file.write_all(

                    line.as_bytes()

                );



            let _ =

                file.flush();


        }


    }










    //
    // SSH连接成功
    //
    pub fn ssh_success(

        &self,

        host:&str,

        port:u16

    ){


        self.println(

            &format!(

                "SSH CONNECT SUCCESS host={} port={}",

                host,

                port

            )

        );


    }









    //
    // SSH连接失败
    //
    pub fn ssh_failed(

        &self,

        host:&str,

        port:u16,

        reason:&str

    ){


        self.println(

            &format!(

                "SSH CONNECT FAILED host={} port={} reason={}",

                host,

                port,

                reason

            )

        );


    }









    //
    // SCP上传开始
    //
    pub fn upload_start(

        &self,

        host:&str,

        local:&str,

        remote:&str

    ){


        self.println(

            &format!(

                "UPLOAD START host={} local={} remote={}",

                host,

                local,

                remote

            )

        );


    }









    //
    // SCP上传成功
    //
    pub fn upload_success(

        &self,

        host:&str,

        file:&str

    ){


        self.println(

            &format!(

                "UPLOAD SUCCESS host={} file={}",

                host,

                file

            )

        );


    }









    //
    // SCP上传失败
    //
    pub fn upload_failed(

        &self,

        host:&str,

        file:&str,

        reason:&str

    ){


        self.println(

            &format!(

                "UPLOAD FAILED host={} file={} reason={}",

                host,

                file,

                reason

            )

        );


    }









    //
    // MD5校验成功
    //
    pub fn md5_success(

        &self,

        host:&str,

        file:&str,

        md5:&str

    ){


        self.println(

            &format!(

                "MD5 SUCCESS host={} file={} md5={}",

                host,

                file,

                md5

            )

        );


    }









    //
    // MD5校验失败
    //
    pub fn md5_failed(

        &self,

        host:&str,

        file:&str,

        local_md5:&str,

        remote_md5:&str

    ){


        self.println(

            &format!(

                "MD5 FAILED host={} file={} local_md5={} remote_md5={}",

                host,

                file,

                local_md5,

                remote_md5

            )

        );


    }









    //
    // 替换成功
    //
    pub fn replace_success(

        &self,

        host:&str,

        file:&str

    ){


        self.println(

            &format!(

                "REPLACE SUCCESS host={} file={}",

                host,

                file

            )

        );


    }









    //
    // 替换失败
    //
    pub fn replace_failed(

        &self,

        host:&str,

        file:&str,

        reason:&str

    ){


        self.println(

            &format!(

                "REPLACE FAILED host={} file={} reason={}",

                host,

                file,

                reason

            )

        );


    }









    //
    // 备份成功
    //
    pub fn backup_success(

        &self,

        host:&str,

        file:&str

    ){


        self.println(

            &format!(

                "BACKUP SUCCESS host={} file={}",

                host,

                file

            )

        );


    }









    //
    // 创建目录
    //
    pub fn mkdir_success(

        &self,

        host:&str,

        path:&str

    ){


        self.println(

            &format!(

                "MKDIR SUCCESS host={} path={}",

                host,

                path

            )

        );


    }









    //
    // 通用错误
    //
    pub fn error(

        &self,

        message:&str

    ){


        self.println(

            &format!(

                "ERROR {}",

                message

            )

        );


    }






}
