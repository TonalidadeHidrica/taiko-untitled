use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use ordered_float::OrderedFloat;
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
use crate::value_with_update_time::EasingF64;
use crate::value_with_update_time::EasingF64Impl;

struct PausedScore<'a> {
    score: &'a Score,
    scroll_points: BTreeSet<OrderedFloat<f64>>,
}

impl<'a> PausedScore<'a> {
    fn new(score: &'a Score) -> Self {
        let scroll_points = score.bar_lines.iter().map(|b| b.time.into()).collect();
        PausedScore {
            scroll_points,
            score,
        }
    }
}

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
    time: f64,
) -> Result<GameMode, TaikoError> {
    let score = song.score.as_ref().ok_or_else(|| TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    })?;
    let score = PausedScore::new(score);

    audio_manager.pause()?;

    let mut music_position = EasingF64Impl::new(time, Duration::from_millis(250), |x| {
        1.0 - (1.0 - x).powi(3)
    });

    loop {
        if let Some(res) = pause_loop(
            config,
            canvas,
            event_pump,
            assets,
            &mut music_position,
            &score,
        )? {
            break Ok(res);
        }
    }
}

fn pause_loop<E>(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    assets: &mut Assets,
    music_position: &mut E,
    score: &PausedScore,
) -> Result<Option<GameMode>, TaikoError>
where
    E: EasingF64,
{
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(GameMode::Exit)),
            Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::Space => {
                    return Ok(Some(GameMode::Play {
                        music_position: Some(music_position.get()),
                    }))
                }
                Keycode::Left => music_position.set_with(|x| {
                    score
                        .scroll_points
                        .range(..OrderedFloat::from(x - 1e-3))
                        .next_back()
                        .map_or(x, |x| **x)
                }),
                Keycode::Right => music_position.set_with(|x| {
                    score
                        .scroll_points
                        .range(OrderedFloat::from(x + 1e-3)..)
                        .next()
                        .map_or(x, |x| **x)
                }),
                _ => {}
            },
            _ => {}
        }
    }

    let display_position = music_position.get_eased();

    draw_background(canvas, assets).map_err(to_sdl_error("While drawing background"))?;
    let rect = game_rect();
    canvas.set_clip_rect(rect);
    {
        draw_bar_lines(canvas, display_position, score.score.bar_lines.iter())?;
        draw_notes(
            canvas,
            assets,
            display_position,
            score.score.notes.iter().rev(),
        )?;
    }
    canvas.set_clip_rect(None);

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}
