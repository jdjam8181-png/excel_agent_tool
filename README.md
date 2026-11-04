# excel_agent_tool

A batch file replacement tool written in Rust.

按 Excel 清单批量升级远端程序文件（支持预检、校验与回退）。

> 优化说明与行为变更见 [OPTIMIZATION.md](OPTIMIZATION.md)。

## 组成

| 模块 | 作用 |
|---|---|
| `scp_tool` | 主工具：升级 / 回退，SFTP 上传 + MD5 校验 + 自动备份 |
| `stop_tool` | 按数据库类型停组件（执行各组件的 `scripts/stop_flow.sh`） |
| `ssh_link` | SSH 连通性与远程命令探测 |
| `ssh_pub` | 公共库：ssh / scp / excel / md5 / group / logger / shell |

## 配置

- `agent.xlsx`：每行一台机器上的一个服务（**含明文口令，禁止提交到仓库**）
  列顺序：主机 / 端口 / 协议 / 用户名 / 密码 / 部署目录 / 服务名 / 源库 / 目标库
- `scp_tool/config/db_type.json`：`(源库, 目标库)` → 需要替换哪些组件、哪些文件
- `scp_tool/config/engine.json`：组件 + 架构 → 本地源目录 / 远端目标目录
  （支持 `{base_dir}`、`{service_name}` 占位符）
- `stop_tool/config/db_type.json`：`(源库, 目标库)` → 可停止的组件

## 使用

```bash
cargo build --release

# 预演：只预检并打印将要替换的文件，不做任何修改
./scp_tool --mode replace --excel agent.xlsx --arch x86 --dry-run

# 升级（结束时会打印回退命令与 task_id）
./scp_tool --mode replace --excel agent.xlsx --arch x86 --parallel 4

# 回退（必须带上一次升级的 task_id）
./scp_tool --mode rollback --excel agent.xlsx --arch x86 --task-id <task_id>

# 停服
./stop_tool -e agent.xlsx -t ds -p 5

# 连通性/命令探测
./ssh_link -e agent.xlsx -c 'hostname'
```

### 常用参数

| 参数 | 说明 |
|---|---|
| `--arch` | `x86` / `arm`（必填其一，默认 `x86`） |
| `--parallel` | 并发主机数，1-10 |
| `--dry-run` | 只预检与打印，不修改远端文件 |
| `--timeout` | SSH 阻塞操作超时（秒），`0` = 不限制 |
| `--task-id` | `rollback` 必填；`replace` 可指定，默认按时间生成 |

### 退出码

`0` = 全部成功；`1` = 存在失败主机/组件（便于 CI 与批量编排感知）。

## 运行痕迹

- `log/replace_*.out`、`log/rollback_*.out`：本次运行的逐文件明细
- `.replace_history`：每次运行的 `task_id` 与最终状态
- 远端备份：`<远端文件父目录>/backup/<目录名>_<task_id>/<文件名>`
