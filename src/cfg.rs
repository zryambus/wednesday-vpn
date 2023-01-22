use std::sync::Arc;

use config::{Config, File, Environment};
use serde::Deserialize;
use anyhow::Result;

#[derive(Deserialize)]
pub struct Cfg {
    pub private_key: String,
    pub public_key: String,

    pub endpoint: String,
    pub port: u16,

    pub bot_name: String,
    pub bot_token: String,
    pub admin_id: i64,

    pub post_up: String,
    pub pre_down: String,
}

pub type CfgPtr = Arc<Cfg>;

pub fn get_config() -> Result<Cfg> {
    let settings = Config::builder()
        .add_source(File::with_name("config"))
        .add_source(Environment::with_prefix("APP"))
        .build()?;
    Ok(settings.try_deserialize()?)
}