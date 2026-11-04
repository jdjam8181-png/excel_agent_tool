use anyhow::Result;
use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

//
// 替换/回退历史
//
// 旧实现的问题:
//
// 1) 在 replace_host 内部调用, 而 replace_host 跑在 10 个 rayon 线程上,
//    OpenOptions::append 没有任何锁保护 -> 多线程同时写同一个文件。
//
// 2) 只写 STATUS=RUNNING, 结束后从不更新,
//    历史里永远看不到这次升级到底成功还是失败。
//
// 现在:
//
// 只在主线程调用两次(开始 / 结束),
// 天然没有并发问题, 且能看到最终状态。
//
pub const HISTORY_FILE: &str = ".replace_history";

//
// 追加一条历史
//
// 格式(保持与旧文件兼容的前缀顺序):
//
// VERSION=<task_id>,TIME=<时间>,OPERATION=<REPLACE|ROLLBACK>,STATUS=<RUNNING|SUCCESS|PARTIAL>,DETAIL=...
//
pub fn record(operation: &str, task_id: &str, status: &str, detail: &str) -> Result<()> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(HISTORY_FILE)?;

    writeln!(
        file,
        "VERSION={},TIME={},OPERATION={},STATUS={},DETAIL={}",
        task_id, now, operation, status, detail
    )?;

    file.flush()?;

    Ok(())
}
