use anyhow::{Result, anyhow};

use calamine::{
    open_workbook,
    Reader,
    Xlsx,
};

use crate::config::Server;



pub fn read_excel(

    file:&str

)->Result<Vec<Server>>{


    let mut workbook:
        Xlsx<_> =
        open_workbook(file)?;



    let sheet_name =
        workbook
        .sheet_names()
        .first()
        .ok_or(
            anyhow!("Excel sheet empty")
        )?
        .clone();



    let range =
        workbook
        .worksheet_range(
            &sheet_name
        )?;



    let mut servers =
        Vec::new();



    for (index,row)
        in range.rows()
        .enumerate()
    {


        // 跳过标题

        if index == 0 {

            continue;

        }



        // Excel必须9列

        if row.len() < 9 {

            continue;

        }



        let host =
            row[0]
            .to_string()
            .trim()
            .to_string();



        let port =
            row[1]
            .to_string()
            .trim()
            .parse::<u16>()
            .unwrap_or(22);



        let protocol =
            row[2]
            .to_string()
            .trim()
            .to_lowercase();



        let username =
            row[3]
            .to_string()
            .trim()
            .to_string();



        let password =
            row[4]
            .to_string()
            .trim()
            .to_string();



        let agent_base =
            row[5]
            .to_string()
            .trim()
            .to_string();



        let service_name =
            row[6]
            .to_string()
            .trim()
            .to_string();



        let source_db =
            row[7]
            .to_string()
            .trim()
            .to_lowercase();



        let target_db =
            row[8]
            .to_string()
            .trim()
            .to_lowercase();



        servers.push(

            Server {

                host,

                port,

                protocol,

                username,

                password,

                agent_base,

                service_name,

                source_db,

                target_db,

            }

        );


    }



    Ok(servers)

}
