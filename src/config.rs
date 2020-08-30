use config::{Config, ConfigError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaikoConfig {
    pub window: WindowConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fps: f64,
}

impl Default for TaikoConfig {
    fn default() -> Self {
        TaikoConfig {
            window: WindowConfig {
                width: 1920,
                height: 1080,
                vsync: false,
                fps: 60.0,
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
