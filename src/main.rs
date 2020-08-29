use sdl2;
use sdl2::event::Event;
use sdl2::video::WindowBuildError;
use std::time::Duration;

use config::{Config, ConfigError};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct TaikoError {
    message: String,
    cause: TaikoErrorCause,
}

#[derive(Debug)]
enum TaikoErrorCause {
    SdlError(String),
    SdlWindowError(WindowBuildError),
    ConfigError(ConfigError),
}

impl TaikoError {
    fn new_sdl_error<S>(message: S, sdl_message: String) -> TaikoError
    where
        S: ToString,
    {
        TaikoError {
            message: message.to_string(),
            cause: TaikoErrorCause::SdlError(sdl_message),
        }
    }

    fn new_sdl_window_error<S>(message: S, window_build_error: WindowBuildError) -> TaikoError
    where
        S: ToString,
    {
        TaikoError {
            message: message.to_string(),
            cause: TaikoErrorCause::SdlWindowError(window_build_error),
        }
    }

    fn new_config_error<S>(message: S, config_error: ConfigError) -> TaikoError
    where
        S: ToString,
    {
        TaikoError {
            message: message.to_string(),
            cause: TaikoErrorCause::ConfigError(config_error),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TaikoConfig {
    window: WindowSizeConfig,
}
#[derive(Debug, Serialize, Deserialize)]
struct WindowSizeConfig {
    width: u32,
    height: u32,
}

fn main() -> Result<(), TaikoError> {
    let mut config = Config::new();
    config
        .merge(
            Config::try_from(&TaikoConfig {
                window: WindowSizeConfig {
                    width: 1920,
                    height: 1080,
                },
            })
            .map_err(|e| {
                TaikoError::new_config_error("Failed to load default configurations", e)
            })?,
        )
        .map_err(|e| TaikoError::new_config_error("Failed to merge default configurations", e))?;
    config
        .merge(config::File::with_name("config.toml").required(false))
        .map_err(|e| TaikoError::new_config_error("Failed to load local configurations", e))?;
    let config = config
        .try_into::<TaikoConfig>()
        .map_err(|e| TaikoError::new_config_error("Failed to parse configurations", e))?;

    let sdl_context = sdl2::init()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize SDL2 context", s))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize video subsystem of SDL", s))?;
    let window = video_subsystem
        .window("", config.window.width, config.window.height)
        .allow_highdpi()
        .build()
        .map_err(|x| TaikoError::new_sdl_window_error("Failed to create main window", x))?;
    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize event pump for SDL2", s))?;

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
    }

    Ok(())
}
