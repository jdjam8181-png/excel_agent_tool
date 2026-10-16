use serde::{Serialize, Deserialize};


#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Server {

    pub host:String,

    pub port:u16,

    pub protocol:String,

    pub username:String,

    pub password:String,

    pub agent_base:String,

    pub service_name:String,

    pub source_db:String,

    pub target_db:String,
}
