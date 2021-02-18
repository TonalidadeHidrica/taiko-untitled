use std::path::PathBuf;
use std::time::Duration;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
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
use crate::game_graphics::draw_bar_lines;
use crate::game_graphics::draw_notes;
use crate::game_graphics::game_rect;
use crate::mode::GameMode;
use crate::structs::just::Score;
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
    mut time: f64,
) -> Result<GameMode, TaikoError> {
    let score = song.score.as_ref().ok_or_else(|| TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    })?;

    audio_manager.pause()?;

    loop {
        if let Some(res) = pause_loop(config, canvas, event_pump, assets, &mut time, score)? {
            break Ok(res);
        }
    }
}

fn pause_loop(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    assets: &mut Assets,
    music_position: &mut f64,
    score: &Score,
) -> Result<Option<GameMode>, TaikoError> {
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(GameMode::Exit)),
            Event::KeyDown { keycode: Some(keycode), .. } => match keycode {
                Keycode::Space => {
                    return Ok(Some(GameMode::Play { music_position: Some(*music_position) }))
                }
                Keycode::Left => { *music_position -= 1.0 },
                Keycode::Right => { *music_position += 1.0 },
                _ => {},
            }
            _ => {}
        }
    }

    draw_background(canvas, assets).map_err(to_sdl_error("While drawing background"))?;
    let rect = game_rect();
    canvas.set_clip_rect(rect);
    {
        draw_bar_lines(canvas, *music_position, score.bar_lines.iter())?;
        draw_notes(canvas, assets, *music_position, score.notes.iter().rev())?;
    }
    canvas.set_clip_rect(None);

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}
