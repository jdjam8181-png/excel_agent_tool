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



    pub fn new()->Result<Self>{


        std::fs::create_dir_all(
            "log"
        )?;



        //
        // 每次启动生成一个独立日志
        //
        let filename =
            format!(
                "log/ca_{}.out",
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
                ),

            }
        )

    }




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

}
