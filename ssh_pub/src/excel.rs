use anyhow::{anyhow, Result};
use calamine::{open_workbook, Data, Reader, Xlsx};

use crate::config::Server;

//
// Excel 读取结果
//
// warnings 用于暴露"被静默跳过的行":
// 批量升级最怕 Excel 里少了一行却没人发现,
// 旧实现直接 continue, 调用方完全不知道。
//
pub struct ExcelData {
    pub servers: Vec<Server>,
    pub warnings: Vec<String>,
}

//
// 单元格是否为空
//
fn is_blank(cell: &Data) -> bool {
    match cell {
        Data::Empty => true,

        other => other.to_string().trim().is_empty(),
    }
}

//
// 读取Excel(带告警信息)
//
// 列约定(共9列):
//
// 0 主机  1 端口  2 协议  3 用户名  4 密码
// 5 部署目录  6 服务名  7 源库  8 目标库
//
pub fn read_excel_detailed(file: &str) -> Result<ExcelData> {
    let mut workbook: Xlsx<_> = open_workbook(file)?;

    let mut warnings = Vec::new();

    let sheet_name = workbook
        .sheet_names()
        .first()
        .ok_or_else(|| anyhow!("Excel sheet empty"))?
        .clone();

    let range = workbook.worksheet_range(&sheet_name)?;

    let mut servers = Vec::new();

    for (index, row) in range.rows().enumerate() {
        //
        // 跳过标题
        //
        if index == 0 {
            continue;
        }

        //
        // Excel 行号(给人看的)
        //
        let line_no = index + 1;

        //
        // 整行为空: 正常现象(尾部空行), 静默跳过
        //
        if row.iter().all(is_blank) {
            continue;
        }

        //
        // 列数不足
        //
        if row.len() < 9 {
            warnings.push(format!(
                "第 {} 行只有 {} 列(需要 9 列), 已跳过",
                line_no,
                row.len()
            ));

            continue;
        }

        let host = row[0].to_string().trim().to_string();

        if host.is_empty() {
            warnings.push(format!("第 {} 行主机为空, 已跳过", line_no));

            continue;
        }

        //
        // 端口
        //
        let raw_port = row[1].to_string().trim().to_string();

        let port = match raw_port.parse::<u16>() {
            Ok(v) => v,

            Err(_) => {
                warnings.push(format!(
                    "第 {} 行端口 {}(无法解析), 已按 22 处理",
                    line_no, raw_port
                ));

                22
            }
        };

        let protocol = row[2].to_string().trim().to_lowercase();

        let username = row[3].to_string().trim().to_string();

        let password = row[4].to_string().trim().to_string();

        let agent_base = row[5].to_string().trim().to_string();

        let service_name = row[6].to_string().trim().to_string();

        let source_db = row[7].to_string().trim().to_lowercase();

        let target_db = row[8].to_string().trim().to_lowercase();

        //
        // 关键字段校验
        //
        if username.is_empty() {
            warnings.push(format!("第 {} 行用户名为空, 已跳过", line_no));

            continue;
        }

        if password.is_empty() {
            warnings.push(format!("第 {} 行密码为空, 认证会失败({})", line_no, host));
        }

        if service_name.is_empty() {
            warnings.push(format!("第 {} 行服务名为空, 已跳过({})", line_no, host));

            continue;
        }

        if agent_base.is_empty() {
            warnings.push(format!(
                "第 {} 行部署目录为空({} {}), 替换会失败",
                line_no, host, service_name
            ));
        } else if !agent_base.starts_with('/') {
            warnings.push(format!(
                "第 {} 行部署目录不是绝对路径({} {} -> {}), 替换会失败",
                line_no, host, service_name, agent_base
            ));
        }

        if source_db.is_empty() || target_db.is_empty() {
            warnings.push(format!(
                "第 {} 行源库/目标库为空({} {} -> {:?}/{:?}), 规则匹配会失败",
                line_no, host, service_name, source_db, target_db
            ));
        }

        servers.push(Server {
            host,
            port,
            protocol,
            username,
            password,
            agent_base,
            service_name,
            source_db,
            target_db,
        });
    }

    Ok(ExcelData { servers, warnings })
}

//
// 兼容旧调用
//
pub fn read_excel(file: &str) -> Result<Vec<Server>> {
    Ok(read_excel_detailed(file)?.servers)
}
