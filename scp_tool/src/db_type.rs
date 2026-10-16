use anyhow::{Result, anyhow};

use serde::Deserialize;

use std::collections::HashMap;

use ssh_pub::config::Server;

//
// db_type.json 根节点
//
#[derive(Debug, Deserialize)]
pub struct DbTypeConfig {
    pub rules: Vec<Rule>,
}

//
// rule规则
//
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub mode: String,

    pub source_db: String,

    pub target_db: String,

    //
    // source_db对应组件
    //
    // 例如:
    //
    // oceanbase
    //     |
    //     ds
    //
    pub source_components: HashMap<String, Component>,

    //
    // target_db对应组件
    //
    // 例如:
    //
    // hbase
    //    |
    //    dt
    //    dy
    //
    pub target_components: HashMap<String, Component>,
}

//
// component
//
// ds/dt/dy/xpmon
//
// bin/module由json控制
//
#[derive(Debug, Clone, Deserialize)]
pub struct Component {
    #[serde(default)]
    pub bin: Vec<String>,

    #[serde(default)]
    pub module: Vec<String>,
}

//
// 匹配后的规则
//
// 提供给engine.rs
//
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub mode: String,

    pub source_db: String,

    pub target_db: String,

    //
    // 源端组件
    //
    pub source_components: HashMap<String, Component>,

    //
    // 目标端组件
    //
    pub target_components: HashMap<String, Component>,
}

//
// 加载db_type.json
//
pub fn load_db_type(file: &str) -> Result<DbTypeConfig> {
    let content = std::fs::read_to_string(file)?;

    let config: DbTypeConfig = serde_json::from_str(&content)?;

    Ok(config)
}

//
// 根据Excel匹配规则
//
// 精确匹配:
// source_db
// target_db
//
pub fn match_rule(config: &DbTypeConfig, server: &Server) -> Result<MatchedRule> {
    for rule in &config.rules {
        let source_match = rule.source_db == server.source_db;

        let target_match = rule.target_db == server.target_db;

        if source_match && target_match {
            return Ok(MatchedRule {
                mode: rule.mode.clone(),

                source_db: rule.source_db.clone(),

                target_db: rule.target_db.clone(),

                source_components: rule.source_components.clone(),

                target_components: rule.target_components.clone(),
            });
        }
    }

    Err(anyhow!(
        "No db_type rule match source_db={} target_db={}",
        server.source_db,
        server.target_db
    ))
}
