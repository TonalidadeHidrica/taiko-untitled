use crate::tja::TjaError;
use config::ConfigError;
use sdl2::video::WindowBuildError;
use sdl2::IntegerOrSdlError;

#[derive(Debug)]
pub struct TaikoError {
    pub message: String,
    pub cause: TaikoErrorCause,
}

#[derive(Debug)]
pub enum TaikoErrorCause {
    None,
    SdlError(String),
    SdlWindowError(WindowBuildError),
    SdlCanvasError(IntegerOrSdlError),
    ConfigError(ConfigError),
    InvalidResourceError,
    TjaLoadError(TjaError),
}

pub fn new_sdl_error<S>(message: S, sdl_message: String) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::SdlError(sdl_message),
    }
}

pub fn new_sdl_window_error<S>(message: S, window_build_error: WindowBuildError) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::SdlWindowError(window_build_error),
    }
}

pub fn new_sdl_canvas_error<S>(message: S, canvas_error: IntegerOrSdlError) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::SdlCanvasError(canvas_error),
    }
}

pub fn new_config_error<S>(message: S, config_error: ConfigError) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::ConfigError(config_error),
    }
}

pub fn new_tja_error<S>(message: S, tja_error: TjaError) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::TjaLoadError(tja_error),
    }
}
