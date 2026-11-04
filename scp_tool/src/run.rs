//
// 一次运行(整批)的公共参数
//
#[derive(Debug, Clone)]
pub struct RunOptions {
    //
    // x86 / arm
    //
    pub arch: String,

    //
    // 整批共用的 task_id
    //
    pub task_id: String,

    //
    // 只预演, 不修改任何远端文件
    //
    pub dry_run: bool,

    //
    // SSH 阻塞操作超时(libssh2 单位为毫秒), 0 = 不限制
    //
    pub timeout_ms: u32,
}

//
// 生成 task_id
//
// 关键修复:
//
// 整个批次共用一个 task_id。
//
// 旧实现在 replace_host 内部各自生成(毫秒时间戳),
// 一次升级多台机器会产生 N 个不同的 task_id,
// 而备份目录名里嵌的就是 task_id,
// rollback 却只接受一个 --task-id,
// 结果就是"整批升级之后无法整批回退"。
//
pub fn new_task_id() -> String {
    chrono::Local::now()
        .format("%Y%m%d_%H%M%S%.3f")
        .to_string()
}

//
// 文件级统计
//
#[derive(Debug, Default, Clone)]
pub struct FileStat {
    pub files_ok: usize,

    pub files_failed: usize,

    pub files_skipped: usize,
}

//
// 单主机结果
//
// 旧实现只看 replace_host 有没有返回 Err,
// 而它在 service 失败时只是 continue, 最后依然返回 Ok,
// main 就打印 "HOST SUCCESS" —— 全部失败也会报成功。
//
#[derive(Debug, Default, Clone)]
pub struct HostReport {
    pub host: String,

    //
    // SSH 是否连上
    //
    pub ssh_ok: bool,

    pub services_ok: usize,

    pub services_failed: usize,

    pub files: FileStat,

    //
    // 失败原因(限长, 避免日志爆炸)
    //
    pub errors: Vec<String>,
}

impl HostReport {
    pub fn new(host: &str) -> Self {
        HostReport {
            host: host.to_string(),

            ..Default::default()
        }
    }

    //
    // 该主机是否完全成功
    //
    pub fn ok(&self) -> bool {
        self.ssh_ok && self.services_failed == 0 && self.files.files_failed == 0
    }

    pub fn push_error(&mut self, msg: String) {
        if self.errors.len() < 20 {
            self.errors.push(msg);
        }
    }
}

//
// 整批汇总
//
#[derive(Debug, Default, Clone)]
pub struct RunSummary {
    pub hosts_total: usize,

    pub hosts_ok: usize,

    pub hosts_failed: usize,

    pub services_ok: usize,

    pub services_failed: usize,

    pub files_ok: usize,

    pub files_failed: usize,

    pub files_skipped: usize,
}

impl RunSummary {
    pub fn from_reports(reports: &[HostReport]) -> Self {
        let mut s = RunSummary {
            hosts_total: reports.len(),

            ..Default::default()
        };

        for r in reports {
            if r.ok() {
                s.hosts_ok += 1;
            } else {
                s.hosts_failed += 1;
            }

            s.services_ok += r.services_ok;

            s.services_failed += r.services_failed;

            s.files_ok += r.files.files_ok;

            s.files_failed += r.files.files_failed;

            s.files_skipped += r.files.files_skipped;
        }

        s
    }

    //
    // 是否整批成功 -> 决定进程退出码
    //
    pub fn success(&self) -> bool {
        self.hosts_failed == 0
    }
}
