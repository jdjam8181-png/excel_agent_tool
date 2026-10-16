use anyhow::Result;

use ssh2::Session;

use std::io::Read;

use std::net::TcpStream;

use std::time::Duration;

use crate::config::Server;

use crate::group::HostGroup;



pub struct SshClient {


    session:Session,


}




impl SshClient {



    //
    // Server连接
    //
    pub fn connect(

        server:&Server

    )->Result<Self>{


        Self::connect_info(

            &server.host,

            server.port,

            &server.username,

            &server.password

        )

    }





    //
    // HostGroup连接
    //
    // 一个IP只连接一次
    //
    pub fn connect_hostgroup(

        host:&HostGroup

    )->Result<Self>{


        Self::connect_info(

            &host.host,

            host.port,

            &host.username,

            &host.password

        )

    }







    //
    // SSH公共连接
    //
    fn connect_info(

        host:&str,

        port:u16,

        username:&str,

        password:&str

    )->Result<Self>{



        let tcp =


            TcpStream::connect_timeout(

                &format!(

                    "{}:{}",

                    host,

                    port

                )

                .parse()?,

                Duration::from_secs(10)

            )?;





        let mut session =

            Session::new()?;




        session.set_tcp_stream(

            tcp

        );



        session.handshake()?;




        session.userauth_password(

            username,

            password

        )?;





        if !session.authenticated(){


            anyhow::bail!(

                "SSH authentication failed {}:{}",

                host,

                port

            );


        }





        Ok(

            SshClient {

                session

            }

        )


    }









    //
    // 执行命令
    //
    // 返回stdout+stderr
    //
    pub fn exec(

        &self,

        cmd:&str

    )->Result<String>{



        let mut channel =


            self.session

            .channel_session()?;




        channel.exec(

            cmd

        )?;





        let mut stdout =

            String::new();



        channel.read_to_string(

            &mut stdout

        )?;





        let mut stderr =

            String::new();



        channel.stderr()

            .read_to_string(

                &mut stderr

            )?;





        channel.wait_close()?;




        if !stderr.is_empty(){


            stdout.push_str(

                "\nSTDERR:\n"

            );


            stdout.push_str(

                &stderr

            );


        }





        Ok(stdout)

    }









    //
    // 执行命令返回状态码
    //
    // 0成功
    //
    // 非0失败
    //
    pub fn exec_status(

        &self,

        cmd:&str

    )->Result<i32>{



        let mut channel =


            self.session

            .channel_session()?;



        channel.exec(

            cmd

        )?;





        channel.wait_close()?;




        let exit =

            channel.exit_status()?;




        Ok(exit)

    }









    //
    // 提供scp.rs使用
    //
    pub(crate) fn session(

        &self

    )->&Session{


        &self.session


    }



}
