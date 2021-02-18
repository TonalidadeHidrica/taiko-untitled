use std::path::PathBuf;
use std::time::Duration;

use sdl2::event::Event;
use sdl2::render::WindowCanvas;
use sdl2::EventPump;
use sdl2::EventSubsystem;
use sdl2::TimerSubsystem;

use crate::assets::Assets;
use crate::audio::AudioManager;
use crate::config::TaikoConfig;
use crate::errors::to_sdl_error;
use crate::errors::TaikoError;
use crate::errors::TaikoErrorCause;
use crate::game_graphics::draw_background;
use crate::mode::GameMode;
use crate::tja::Song;

pub fn pause(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_subsystem: &EventSubsystem,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager,
    assets: &mut Assets,
    tja_file_name: PathBuf,
    song: Song,
) -> Result<GameMode, TaikoError> {
    let score = song.score.as_ref().ok_or_else(|| TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    })?;

    audio_manager.pause()?;

    loop {
        if let Some(res) = pause_loop(config, canvas, event_pump, assets)? {
            break Ok(res);
        }
    }
}

fn pause_loop(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    assets: &mut Assets,
) -> Result<Option<GameMode>, TaikoError> {
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(GameMode::Exit)),
            e => {
                dbg!(e);
            }
        }
    }

    draw_background(canvas, assets).map_err(to_sdl_error("While drawing background"))?;

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}
