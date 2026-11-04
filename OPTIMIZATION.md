# agent_tool 代码解析与优化说明

面向场景：**程序批量升级**（读 Excel 清单 → SSH 到目标机 → 替换程序文件 → 可回退）。

---

## 一、P0：先处理凭据泄露

`agent.xlsx` 内含明文 SSH 口令，且已提交到公开仓库。

| 项 | 值 |
|---|---|
| 仓库 | `https://github.com/jdjam8181-png/excel_agent_tool`（公网可访问） |
| 文件 | `agent.xlsx`，blob `e26049c703b8e18a6e471d6407ce8baf0e3c74cd` |
| 内容 | 主机 `192.168.58.131` / 用户 `dsg` / 明文口令（4 行数据） |

处置顺序：

1. **轮换 `dsg` 口令**（已公开，视同失陷；确认是否在别处复用）。
2. 改写历史并强推：`git filter-repo --path agent.xlsx --invert-paths`（或 BFG），再清 GitHub 缓存。改私有只是缓解。
3. 本次已把 `agent.xlsx` / `*.xlsx` / `.replace_history` 加入 `.gitignore`。

> 注意：`.gitignore` 里用了 `*.xlsx`。如果你以后要提交“不含口令的 Excel 模板”，请把模板改成 `.xlsx.example` 或改为按文件名精确忽略。

---

## 二、代码解析：批量升级的完整链路

```
agent.xlsx (每行=一台机器上的一个服务)
        │  read_excel  →  Server{host,port,user,pass,agent_base,service_name,source_db,target_db}
        ▼
   group_by_host()   按 host:port:user 聚合 → 一个 HostGroup 一条 SSH 连接, 下面挂多个 service
        ▼
   match_rule()      db_type.json: (source_db,target_db) → 用哪些组件(ds/dt/dy)
        ▼
   build_replace_tasks()
        │            engine.json: 组件+架构 → 本地源目录 / 远端目标目录
        │            {base_dir} {service_name} 占位符展开
        ▼            → Vec<ReplaceTask>{local_file, remote_file, file_type}
   preflight()       本地文件存在非空? 远端父目录存在?
        ▼
   每个文件:
     备份  mkdir -p <父目录>/backup/<目录名>_<task_id>/ && cp -p <远端文件> <备份目录>/
     ▼
     上传  SFTP 写 <remote>.<ts>.<pid>.tmp → 大小校验 → chmod → rename 覆盖
     ▼
     校验  md5sum 本地 vs 远端
        ▼
   rollback 用同一个 task_id 反查备份目录 → cp -p 还原 → md5 校验
```

关键设计点（保留不变）：`task_id` 同时是**备份目录名**和**回退凭据**；`engine.json` 用 `{base_dir}` / `{service_name}` 让同一份配置适配多套部署路径。

---

## 三、改了什么，为什么

### 3.1 致命：备份失败被当成成功（`ssh_pub/ssh.rs`, `scp_tool/src/replace.rs`）

`ssh2` 的 `channel.exec()` 成功只表示**命令已下发**，远端退出码非 0 时依然返回 `Ok`。
旧代码 `match ssh.exec(&backup_cmd) { Ok(_) => "BACKUP SUCCESS" }`，于是：

- `cp` 因权限不足失败 → 日志写 `BACKUP SUCCESS`
- 紧接着新文件覆盖上去 → **备份实际不存在**
- 事后 `rollback` 找不到备份 → 旧代码又静默 `return Ok(())` → 报 `ROLLBACK SUCCESS`

升级失败时无路可退，且每一步都显示成功。这是本次最严重的正确性问题。

修复：新增 `SshClient::run()`，同时返回 `stdout / stderr / 远端退出码`；关键步骤统一走 `run_ok()`，退出码非 0 直接报错。备份失败**立即中断该文件**，不再覆盖。

### 3.2 失败被吞掉，退出码永远 0（`scp_tool/src/replace.rs`, `main.rs`）

旧 `replace_host` 在 service 失败时只 `continue`，最后仍 `return Ok(())`；`main` 照打 `HOST SUCCESS`，且进程恒返回 0。

修复：`replace_host` 返回 `HostReport`（ssh/服务/文件三级计数 + 错误原因），`main` 汇总为 `RunSummary`：

- 有主机失败 → 进程退出码 1（便于 CI/编排感知）
- 结束打印汇总：主机/服务/文件 的成功、失败、跳过
- **直接打印回退命令**，含本次 `task_id`

### 3.3 整批升级无法整批回退（`scp_tool/src/run.rs`）

旧实现 `task_id` 在 `replace_host` 内部按毫秒时间戳生成 → 10 台机器产生 10 个 `task_id`，而备份目录名内嵌 `task_id`，`rollback` 只接受一个 `--task-id` → 只能一台台回退。

修复：`task_id` 在 `main` 生成一次，整批共用。`rollback` 现在**强制要求** `--task-id`（旧实现缺省空串，备份路径变成 `bin_`，永远找不到备份却报成功）。

### 3.4 半升级状态（`scp_tool/src/replace.rs`）

旧实现“边检查边替换”，可能在传完 2 个文件后第 3 个失败，留下半新半旧的组件。

修复：`preflight()` 先整体检查——远端路径为绝对路径、本地源文件存在且非空、远端父目录存在（父目录去重后检查）。任一项不满足 → **该 service 一个文件都不动**。service 内单个文件失败则停止该 service，避免版本混杂。

### 3.5 停服“假成功”（`stop_tool/src/main.rs`）

同类问题：旧代码 `match client.exec(&stop_cmd) { Ok(_) => SUCCESS }`，`stop_flow.sh` 退出码非 0 也报成功。

修复：以远端退出码判定；执行前 `test -d` 预检组件目录（不存在记 SKIP 而非 SUCCESS）；新增 `--timeout`（默认 300s，libssh2 毫秒语义）防止脚本挂死；有失败则退出码 1，并打印失败明细。原有 `STOP_RESULT|host|port|STATUS|reason` 行格式保留。

### 3.6 命令拼接未转义（`ssh_pub/src/shell.rs`，新增）

旧代码：
- `format!("md5sum {}", file)` —— 未加引号
- `format!("cd {}/{}/{}/scripts && ./stop_flow.sh", ...)` —— 未加引号

同一份代码在 `replace.rs` 里却是规范加引号的。路径来自 Excel 与 `engine.json`，含空格/分号时会命令错乱甚至注入。

修复：新增 `shell::quote()`（POSIX 单引号转义，`'` → `'\''`）与 `shell::validate_remote_path()`（要求绝对路径、拒绝换行）。所有进入远端命令的动态串统一转义；`md5sum -- 'path'` 用 `--` 兼容以 `-` 开头的路径。

### 3.7 上传非原子 + 不校验字节数（`ssh_pub/src/scp.rs`）

旧流程 `unlink(旧文件)` → `rename(tmp, 目标)`，存在“文件不存在”窗口；且不校验 `copy` 实际写入字节数，**网络中断导致的截断文件会被直接改名成正式文件**。

修复：
1. 校验写入字节数 == 本地文件大小，不等则删除临时文件并报错。
2. 优先直接 `rename` 覆盖（POSIX 原子语义），失败再退化为先删后改名（兼容要求目标不存在的 SFTP 服务端）。

### 3.8 权限被无条件改成 0755（同上）

旧代码所有文件 `chmod 0755`，`.jar` / `module` 也变可执行。

修复：上传时**继承远端已有文件权限**；仅当文件本不存在时按类型给默认值（`bin` → 0755，其余 → 0644）。

### 3.9 并发写坏历史文件（`scp_tool/src/history.rs`，新增）

旧代码在跑在 10 个 rayon 线程上的 `replace_host` 内 `OpenOptions::append` 写 `.replace_history`，无任何锁；且状态恒为 `STATUS=RUNNING`，从不更新。

修复：改为只在主线程写两次（开始 `RUNNING` / 结束 `SUCCESS|PARTIAL`），天然无并发问题，并追加 `DETAIL=` 记录计数。

> 格式变化：新增第 5 个字段 `DETAIL=`。若有脚本按 `,` 切分且严格要求 4 段，需同步调整。

### 3.10 panic 隐患（`replace.rs` / `rollback.rs`）

`parent.file_name().unwrap()` 在远端文件直接位于根目录（如 `/mdsd`）时为 `None` → 线程 panic。

修复：`shell::parent_dir()` / `parent_dir_name()`（根目录回退为 `root`），全链路不再 `unwrap`。

### 3.11 其它

| 项 | 说明 |
|---|---|
| Excel 静默跳行 | 新增 `read_excel_detailed()` 返回 warnings；列数不足/主机为空/服务名为空等会**明确告警**（控制台 + 日志），不再无声丢行 |
| `--dry-run` | 新增。完成预检并打印将要替换的文件，**不修改任何远端文件** |
| `--timeout` | 新增。`scp_tool` 默认 120s，`stop_tool` 默认 300s，`0` = 不限制 |
| `arch` 校验 | 仅允许 `x86` / `arm`，提前报错而非等到匹配任务时失败 |
| `ssh_link` | `SUCCESS` 改为以真实退出码判定，消息中带回 `exit=N` |
| `stop_tool` 配置查找 | 兼容“仓库根目录”与“工具目录”两种启动位置 |
| `stop_tool/config/db_type.json` | **修正拼写**：`ocenbase` → `oceanbase`，`habase` → `hbase` |

---

## 四、需要注意的行为变更

1. **`stop_tool/config/db_type.json` 的拼写修正会影响匹配结果。** 若你们的 Excel 里真的写的是 `ocenbase` / `habase`，请改回或两边统一——这决定了规则能否命中（不命中会 `SKIP|UNSUPPORTED_DB_TYPE`）。
2. **`scp_tool --mode rollback` 现在必须带 `--task-id`**，否则直接报错退出。
3. **退出码语义变了**：有失败返回 1。若上层脚本依赖“永远返回 0”，需同步调整。
4. **`replace` 的失败粒度**：预检失败则该 service 完全不动；service 内某文件失败则停止该 service 剩余文件。
5. **`xpmon` / `newxpmon` 在 `stop_tool` 的 `db_type.json` 里没有规则**，会 `SKIP`。Excel 里第 4 行是 `xpmon→xpmon`——是否需要补规则、以及 `dy` 目录命名（`dt_<service>_y`）是否符合实际，请确认后再上线。
6. `.replace_history` 新增 `DETAIL=` 字段。

---

## 五、编译与验证状态（重要）

**本机没有 Rust 工具链（`cargo` / `rustc` 均不存在），本次改动未经编译验证。** 这是唯一需要你们在做任何上线动作前补齐的步骤。

改动过程中已按真实 crate 文档核对过 API，并据此修正了一处我最初的错误假设：
`ssh2::Session::set_timeout` 的真实签名是 `set_timeout(&self, timeout_ms: u32)`（**毫秒，0 表示不限制**），不是 `Option<Duration>`。最终代码已按此实现。

请先编译：

```bash
cd agent_tool_20260831x1
cargo build --release          # 依赖 libssh2 / openssl, 与旧版本构建环境一致
cargo clippy --release         # 可选
```

若编译报错，最可能出在 `ssh_pub/src/scp.rs` 的 `FileStat.perm` 与 `Sftp::rename` 第三参数（这两处沿用了原代码的写法，属于已验证形式）以及 `ssh_pub/src/excel.rs` 的 `calamine::Data::Empty`。

---

## 六、建议的上线顺序

```bash
# 1) 预演: 只预检 + 打印将要替换的文件, 不改任何东西
./scp_tool --mode replace --excel agent.xlsx --arch x86 --dry-run

# 2) 正式升级(小并发先跑一批)
./scp_tool --mode replace --excel agent.xlsx --arch x86 --parallel 2

#    结束时会打印, 例如:
#    task_id   : 20261016_203443.512
#    回退命令  : ./scp_tool --mode rollback --excel <同一份Excel> --arch x86 --task-id 20261016_203443.512

# 3) 需要时回退(必须带 task_id)
./scp_tool --mode rollback --excel agent.xlsx --arch x86 --task-id 20261016_203443.512
```

停服同理：`./stop_tool -e agent.xlsx -t ds -p 5`。

---

## 七、后续可做（本次未动）

1. **`engine.rs` 与 `scp.rs` 的排版**保留原样（大量“一行一个 token”），功能正确，未做重排以免制造巨大 diff。建议后续统一跑一次 `cargo fmt`。
2. `ssh_pub/src/ssh.rs` 的 `run()` 先读完 stdout 再读 stderr。命令输出很小时无影响；若将来执行可能产生大量 stderr 的命令，建议改为非阻塞轮询，避免极端情况下的管道阻塞。
3. 口令仍在 Excel 里明文分发。中期建议改 SSH 密钥或凭据服务，Excel 只存主机与服务信息。
4. 可考虑加“升级后校验”（比对远端进程版本/文件 mtime）与灰度分批（按 Excel 增加批次列）。
