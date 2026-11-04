use anyhow::{anyhow, Result};
use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

use crate::config::Server;
use crate::group::HostGroup;

//
// 远端命令执行结果
//
// 关键点:
//
// 原实现只判断 exec 是否返回 Err,
// 而 ssh2 的 channel.exec 成功只代表"命令已下发",
// 远端退出码非 0 时 exec 依然返回 Ok。
//
// 因此:
//   备份失败 / md5sum 失败 / stop_flow.sh 失败
//   在旧代码里全部会被记录成 SUCCESS。
//
// 升级工具必须依赖真实退出码, 所以统一用 run()。
//
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    //
    // 远端退出码为 0
    //
    pub fn success(&self) -> bool {
        self.status == 0
    }

    //
    // stdout + stderr 合并, 用于日志展示
    //
    pub fn combined(&self) -> String {
        let out = self.stdout.trim_end();

        let err = self.stderr.trim_end();

        if err.is_empty() {
            out.to_string()
        } else {
            format!("{}\nSTDERR:\n{}", out, err)
        }
    }
}

pub struct SshClient {
    session: Session,
}

impl SshClient {
    //
    // Server连接
    //
    pub fn connect(server: &Server) -> Result<Self> {
        Self::connect_info(&server.host, server.port, &server.username, &server.password)
    }

    //
    // HostGroup连接
    //
    // 一个IP只连接一次
    //
    pub fn connect_hostgroup(host: &HostGroup) -> Result<Self> {
        Self::connect_info(&host.host, host.port, &host.username, &host.password)
    }

    //
    // SSH公共连接
    //
    fn connect_info(host: &str, port: u16, username: &str, password: &str) -> Result<Self> {
        //
        // 连接超时 10s
        //
        let tcp = TcpStream::connect_timeout(
            &format!("{}:{}", host, port).parse()?,
            Duration::from_secs(10),
        )?;

        let mut session = Session::new()?;

        session.set_tcp_stream(tcp);

        session.handshake()?;

        session.userauth_password(username, password)?;

        if !session.authenticated() {
            return Err(anyhow!(
                "SSH authentication failed {}:{}",
                host,
                port
            ));
        }

        Ok(SshClient { session })
    }

    //
    // 设置阻塞操作超时
    //
    // 批量升级时最怕某个 worker 卡死:
    // 没有超时的话, 一个挂住的连接会一直占着线程池。
    //
    // libssh2 语义: 单位毫秒, 0 = 不限制(保持旧行为)
    //
    pub fn set_timeout(&self, timeout_ms: u32) {
        self.session.set_timeout(timeout_ms);
    }

    //
    // 执行命令, 返回结构化结果
    //
    // 同时拿到 stdout / stderr / 远端退出码
    //
    pub fn run(&self, cmd: &str) -> Result<CommandOutput> {
        let mut channel = self.session.channel_session()?;

        channel.exec(cmd)?;

        let mut stdout = String::new();

        channel.read_to_string(&mut stdout)?;

        let mut stderr = String::new();

        channel.stderr().read_to_string(&mut stderr)?;

        channel.wait_close()?;

        let status = channel.exit_status()?;

        Ok(CommandOutput {
            status,
            stdout,
            stderr,
        })
    }

    //
    // 执行命令并要求退出码为 0
    //
    // 备份 / 校验 / 停服等关键步骤一律使用本方法,
    // 失败会返回 Err 并携带远端 stderr, 不再出现"假成功"。
    //
    pub fn run_ok(&self, cmd: &str) -> Result<String> {
        let out = self.run(cmd)?;

        if !out.success() {
            return Err(anyhow!(
                "远端命令失败 exit={} cmd={} stderr={}",
                out.status,
                cmd,
                out.stderr.trim()
            ));
        }

        Ok(out.stdout)
    }

    //
    // 执行命令
    //
    // 返回stdout+stderr
    //
    // 保留旧接口: ssh_link 等只做展示的场景继续可用
    //
    pub fn exec(&self, cmd: &str) -> Result<String> {
        Ok(self.run(cmd)?.combined())
    }

    //
    // 执行命令返回状态码
    //
    // 0成功
    //
    // 非0失败
    //
    pub fn exec_status(&self, cmd: &str) -> Result<i32> {
        Ok(self.run(cmd)?.status)
    }

    //
    // 提供scp.rs使用
    //
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }
}
