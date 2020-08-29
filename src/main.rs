use sdl2;
use sdl2::video::WindowBuildError;
use std::time::Duration;
use sdl2::event::Event;

#[derive(Debug)]
struct TaikoError {
    message: String,
    cause: TaikoErrorCause,
}

#[derive(Debug)]
enum TaikoErrorCause {
    SdlError(String),
    SdlWindowError(WindowBuildError),
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
}

fn main() -> Result<(), TaikoError> {
    let sdl_context = sdl2::init()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize SDL2 context", s))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize video subsystem of SDL", s))?;
    let window = video_subsystem
        .window("", 1920, 1080)
        .allow_highdpi()
        .build()
        .map_err(|x| TaikoError::new_sdl_window_error("Failed to create main window", x))?;
    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|s| TaikoError::new_sdl_error("Failed to initialize event pump for SDL2", s))?;

    dbg!(window.size(), window.drawable_size(), window.vulkan_drawable_size(), window.border_size());

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                _ => {},
            }
        }
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
    }

    Ok(())
}
