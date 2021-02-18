use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use itertools::iterate;
use itertools::Itertools;
use ordered_float::OrderedFloat;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::WindowCanvas;
use sdl2::EventPump;

use crate::assets::Assets;
use crate::audio::AudioManager;
use crate::config::TaikoConfig;
use crate::errors::to_sdl_error;
use crate::errors::TaikoError;
use crate::errors::TaikoErrorCause;
use crate::game_graphics::draw_background;
use crate::game_graphics::draw_bar_lines;
use crate::game_graphics::draw_branch_overlay;
use crate::game_graphics::draw_notes;
use crate::game_graphics::game_rect;
use crate::game_graphics::BranchAnimationState;
use crate::mode::GameMode;
use crate::structs::just::Score;
use crate::structs::BranchType;
use crate::tja::Song;
use crate::value_with_update_time::EasingF64;
use crate::value_with_update_time::EasingF64Impl;
use crate::value_with_update_time::ValueWithUpdateTime;

struct PausedScore<'a> {
    score: &'a Score,
    measure_scroll_points: BTreeSet<OrderedFloat<f64>>,
    beat_scroll_points: BTreeSet<OrderedFloat<f64>>,
}

impl<'a> PausedScore<'a> {
    fn new(score: &'a Score) -> Self {
        let measure_scroll_points = score.bar_lines.iter().map(|b| b.time.into()).collect();
        let beat_scroll_points = score
            .bar_lines
            .iter()
            .tuple_windows()
            .flat_map(|(a, b)| {
                iterate(a.time, move |x| x + a.scroll_speed.beat_duration())
                    .take_while(move |&x| x < b.time - 1e-3)
            })
            .map(Into::into)
            .collect();
        PausedScore {
            score,
            measure_scroll_points,
            beat_scroll_points,
        }
    }
}

pub fn pause(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    audio_manager: &AudioManager,
    assets: &mut Assets,
    _tja_file_name: PathBuf,
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
    let mut branch = ValueWithUpdateTime::new(BranchAnimationState::new(BranchType::Normal));

    loop {
        if let Some(res) = pause_loop(
            config,
            canvas,
            event_pump,
            assets,
            &score,
            &mut music_position,
            &mut branch,
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
    score: &PausedScore,
    music_position: &mut E,
    branch: &mut ValueWithUpdateTime<BranchAnimationState>,
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
                Keycode::PageDown => music_position.set_with(|x| {
                    score
                        .measure_scroll_points
                        .range(..OrderedFloat::from(x - 1e-3))
                        .next_back()
                        .map_or(x, |x| **x)
                }),
                Keycode::PageUp => music_position.set_with(|x| {
                    score
                        .measure_scroll_points
                        .range(OrderedFloat::from(x + 1e-3)..)
                        .next()
                        .map_or(x, |x| **x)
                }),
                Keycode::Left => music_position.set_with(|x| {
                    score
                        .beat_scroll_points
                        .range(..OrderedFloat::from(x - 1e-3))
                        .next_back()
                        .map_or(x, |x| **x)
                }),
                Keycode::Right => music_position.set_with(|x| {
                    score
                        .beat_scroll_points
                        .range(OrderedFloat::from(x + 1e-3)..)
                        .next()
                        .map_or(x, |x| **x)
                }),
                Keycode::Up => branch.update(|b| b.set(b.get().saturating_next(), 0.0)),
                Keycode::Down => branch.update(|b| b.set(b.get().saturating_prev(), 0.0)),
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
        draw_branch_overlay(
            canvas,
            branch.duration_since_update().as_secs_f64(),
            rect,
            &branch.get(),
        )?;

        let bar_lines = score
            .score
            .bar_lines
            .iter()
            .filter(|x| branch.get().get().matches(x.branch));
        draw_bar_lines(canvas, display_position, bar_lines)?;

        let notes = score
            .score
            .notes
            .iter()
            .rev()
            .filter(|x| branch.get().get().matches(x.branch));
        draw_notes(canvas, assets, display_position, notes)?;
    }
    canvas.set_clip_rect(None);

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}
