use config::{Config, ConfigError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaikoConfig {
    pub window: WindowConfig,
    pub volume: VolumeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fps: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub song: f32,
    pub se: f32,
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
            volume: VolumeConfig {
                song: 100.0,
                se: 100.0,
            },
        }
    }
}

pub fn get_config() -> Result<TaikoConfig, ConfigError> {
    Config::builder()
        .add_source(Config::try_from(&TaikoConfig::default())?)
        .add_source(config::File::with_name("config.toml").required(false))
        .build()?
        .try_deserialize::<TaikoConfig>()
}
