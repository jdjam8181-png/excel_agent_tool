use anyhow::{bail, Result};

//
// 远端命令拼接工具
//
// 背景:
//
// 远端路径来自 Excel(agent_base/service_name) 与 engine.json,
// 直接 format! 拼进 shell 命令时,
// 空格/分号/引号会被 shell 解释,
// 轻则命令错乱, 重则命令注入。
//
// 所有进入远端命令的动态字符串都必须经过 quote()。
//

//
// 包装为 POSIX 单引号字面量
//
// 单引号内除单引号本身外全部为字面量,
// 因此把 ' 替换为 '\'' 即可安全转义。
//
// 示例:
//
//   /data/a b/bin  ->  '/data/a b/bin'
//   /data/it's     ->  '/data/it'\''s'
//
pub fn quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);

    out.push('\'');

    for ch in arg.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }

    out.push('\'');

    out
}

//
// 校验远端路径
//
// 升级场景下路径必须为绝对路径,
// 相对路径会随登录目录变化, 结果不可预期。
//
pub fn validate_remote_path(path: &str, what: &str) -> Result<()> {
    if path.trim().is_empty() {
        bail!("{} 为空", what);
    }

    if !path.starts_with('/') {
        bail!("{} 必须是绝对路径: {}", what, path);
    }

    if path.contains('\n') || path.contains('\r') {
        bail!("{} 含非法换行符: {:?}", what, path);
    }

    Ok(())
}

//
// 取远端文件的父目录
//
// /app/ds/bin/mdsd -> /app/ds/bin
//
pub fn parent_dir(path: &str) -> Option<String> {
    let p = std::path::Path::new(path);

    p.parent()
        .map(|v| v.to_string_lossy().to_string())
        .filter(|v| !v.is_empty())
}

//
// 取父目录名
//
// /app/ds/bin -> bin
// /           -> root   (避免 unwrap panic)
//
pub fn parent_dir_name(path: &str) -> String {
    let p = std::path::Path::new(path);

    p.parent()
        .and_then(|v| v.file_name())
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| "root".to_string())
}
