use config::{Config, ConfigError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaikoConfig {
    pub window: WindowSizeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowSizeConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for TaikoConfig {
    fn default() -> Self {
        TaikoConfig {
            window: WindowSizeConfig {
                width: 1920,
                height: 1080,
            },
        }
    }
}

pub fn get_config() -> Result<TaikoConfig, ConfigError> {
    let mut config = Config::new();
    config.merge(Config::try_from(&TaikoConfig::default())?)?;
    config.merge(config::File::with_name("config.toml").required(false))?;
    let config = config.try_into::<TaikoConfig>()?;
    Ok(config)
}
