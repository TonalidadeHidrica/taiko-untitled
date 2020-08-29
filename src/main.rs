use sdl2;
use sdl2::event::Event;
use sdl2::video::WindowBuildError;
use std::time::Duration;

use config::{Config, ConfigError};
use sdl2::image::LoadTexture;
use sdl2::keyboard::Keycode;
use sdl2::mixer::{Channel, Chunk, AUDIO_S16LSB, DEFAULT_CHANNELS};
use sdl2::{mixer, IntegerOrSdlError};
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
    SdlCanvasError(IntegerOrSdlError),
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

    fn new_sdl_canvas_error<S>(message: S, canvas_error: IntegerOrSdlError) -> TaikoError
    where
        S: ToString,
    {
        TaikoError {
            message: message.to_string(),
            cause: TaikoErrorCause::SdlCanvasError(canvas_error),
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
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize SDL context", s))?;
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
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize event pump for SDL", s))?;

    let mut canvas = window
        .into_canvas()
        .build()
        .map_err(|e| TaikoError::new_sdl_canvas_error("Failed to create SDL canvas", e))?;
    let texture_creator = canvas.texture_creator();
    let background_texture = texture_creator
        .load_texture("assets/img/game_bg.png")
        .map_err(|s| TaikoError::new_sdl_error("Failed to load background texture", s))?;

    // let _audio = sdl_context
    //     .audio()
    //     .map_err(|s| TaikoError::new_sdl_error("Failed to initialize audio subsystem of SDL", s))?;
    mixer::open_audio(44100, AUDIO_S16LSB, DEFAULT_CHANNELS, 256)
        .map_err(|s| TaikoError::new_sdl_error("Failed to open audio stream", s))?;
    mixer::allocate_channels(128);

    let sound_don = Chunk::from_file("assets/snd/dong.ogg")
        .map_err(|s| TaikoError::new_sdl_error("Failed to load 'don' sound", s))?;
    let sound_ka = Chunk::from_file("assets/snd/ka.ogg")
        .map_err(|s| TaikoError::new_sdl_error("Failed to load 'ka' sound", s))?;

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    repeat: false,
                    keycode: Some(keycode),
                    ..
                } => {
                    if let Some(sound) = match keycode {
                        Keycode::X | Keycode::Slash => Some(&sound_don),
                        Keycode::Z | Keycode::Underscore => Some(&sound_ka),
                        _ => None,
                    } {
                        Channel::all().play(&sound, 0).map_err(|s| {
                            TaikoError::new_sdl_error("Failed to play sound effect", s)
                        })?;
                    };
                }
                _ => {}
            }
        }
        canvas
            .copy(&background_texture, None, None)
            .map_err(|s| TaikoError::new_sdl_error("Failed to draw background", s))?;
        canvas.present();
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
    }

    Ok(())
}
