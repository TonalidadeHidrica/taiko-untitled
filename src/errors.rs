use crate::tja::TjaError;
use config::ConfigError;
use cpal::{BuildStreamError, PlayStreamError, SupportedStreamConfigsError};
use derive_more::From;
use rodio::decoder::DecoderError;
use sdl2::video::WindowBuildError;
use sdl2::IntegerOrSdlError;
use std::io;

#[derive(Debug)]
pub struct TaikoError {
    pub message: String,
    pub cause: TaikoErrorCause,
}

#[derive(Debug)]
pub enum TaikoErrorCause {
    None,
    SdlError(SdlError),
    SdlWindowError(WindowBuildError),
    SdlCanvasError(IntegerOrSdlError),
    ConfigError(ConfigError),
    AudioLoadError(io::Error),
    CpalOrRodioError(CpalOrRodioError),
    InvalidResourceError,
    TjaLoadError(TjaError),
}

#[derive(Debug, From)]
pub struct SdlError(#[allow(dead_code)] String);

#[derive(Debug)]
pub enum CpalOrRodioError {
    SupportedStreamConfigsError(SupportedStreamConfigsError),
    BuildStreamError(BuildStreamError),
    PlayStreamError(PlayStreamError),
    DecoderError(DecoderError),
}

pub fn new_sdl_error<S>(message: S, sdl_message: String) -> TaikoError
where
    S: ToString,
{
    TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::SdlError(sdl_message.into()),
    }
}

pub fn to_sdl_error<S>(message: S) -> impl FnOnce(SdlError) -> TaikoError
where
    S: ToString,
{
    move |sdl_error| TaikoError {
        message: message.to_string(),
        cause: TaikoErrorCause::SdlError(sdl_error),
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

pub fn no_score_in_tja() -> TaikoError {
    TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    }
}
