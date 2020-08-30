use sdl2::event::Event;

use std::time::Duration;

use sdl2::keyboard::Keycode;
use sdl2::mixer;
use sdl2::mixer::{Channel, AUDIO_S16LSB, DEFAULT_CHANNELS};
use sdl2::rect::Rect;

use itertools::Itertools;
use taiko_untitled::assets::Assets;
use taiko_untitled::errors::TaikoError;
use taiko_untitled::tja::load_tja_from_file;

fn main() -> Result<(), TaikoError> {
    let config = taiko_untitled::config::get_config()
        .map_err(|e| TaikoError::new_config_error("Failed to load configuration", e))?;

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
    match canvas.output_size() {
        Ok((width, height)) => {
            let scale = f32::min(width as f32 / 1920.0, height as f32 / 1080.0);
            if let Err(s) = canvas.set_scale(scale, scale) {
                eprintln!("Failed to scale the dimensions.  The drawing scale may not be valid.");
                eprintln!("Caused by: {}", s);
            }
        }
        Err(s) => {
            eprintln!("Failed to get the canvas dimension.  The drawing scale may not be valid.");
            eprintln!("Caused by: {}", s);
        }
    }
    let texture_creator = canvas.texture_creator();

    // let _audio = sdl_context
    //     .audio()
    //     .map_err(|s| TaikoError::new_sdl_error("Failed to initialize audio subsystem of SDL", s))?;
    mixer::open_audio(44100, AUDIO_S16LSB, DEFAULT_CHANNELS, 256)
        .map_err(|s| TaikoError::new_sdl_error("Failed to open audio stream", s))?;
    mixer::allocate_channels(128);

    let assets = Assets::new(&texture_creator)?;

    if let [_, tja_file_name, ..] = &std::env::args().collect_vec()[..] {
        load_tja_from_file(tja_file_name)
            .map_err(|e| {
                eprintln!("Failed to load tja file");
                eprintln!("Caused by: {:?}", e);
            })
            .ok();
    }

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
                        Keycode::X | Keycode::Slash => Some(&assets.chunks.sound_don),
                        Keycode::Z | Keycode::Underscore => Some(&assets.chunks.sound_ka),
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
            .copy(
                &assets.textures.background,
                None,
                Some(Rect::new(0, 0, 1920, 1080)),
            )
            .map_err(|s| TaikoError::new_sdl_error("Failed to draw background", s))?;
        canvas.present();
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
    }

    Ok(())
}
