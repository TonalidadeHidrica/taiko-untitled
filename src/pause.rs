use std::collections::BTreeSet;
use std::sync::mpsc::Receiver;
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
use crate::errors::no_score_in_tja;
use crate::errors::to_sdl_error;
use crate::errors::TaikoError;
use crate::game::AutoEvent;
use crate::game::GameUserState;
use crate::game_graphics::clear_background;
use crate::game_graphics::draw_background;
use crate::game_graphics::draw_bar_lines;
use crate::game_graphics::draw_branch_overlay;
use crate::game_graphics::draw_notes;
use crate::game_graphics::game_rect;
use crate::game_graphics::get_offsets_rev;
use crate::game_graphics::shift_rect;
use crate::game_graphics::BranchAnimationState;
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

pub enum PauseBreak {
    Play(GameUserState),
    Reload,
    Exit,
}

#[allow(clippy::too_many_arguments)]
pub fn pause(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    audio_manager: &AudioManager<AutoEvent>,
    assets: &mut Assets,
    file_change_receiver: &Receiver<notify::DebouncedEvent>,
    songs: &[Song],
    mut game_user_state: GameUserState,
) -> Result<PauseBreak, TaikoError> {
    let scores = songs
        .iter()
        .map(|song| {
            let score = song.score.as_ref().ok_or_else(no_score_in_tja)?;
            Ok(PausedScore::new(score))
        })
        .collect::<Result<Vec<_>, _>>()?;

    audio_manager.pause()?;

    let mut music_position =
        EasingF64Impl::new(game_user_state.time, Duration::from_millis(250), |x| {
            1.0 - (1.0 - x).powi(3)
        });
    let mut branch = ValueWithUpdateTime::new(BranchAnimationState::new(BranchType::Normal));

    loop {
        if let Some(res) = pause_loop(
            config,
            canvas,
            event_pump,
            assets,
            &scores,
            &mut music_position,
            &mut branch,
            &mut game_user_state,
        )? {
            break Ok(res);
        }

        if file_change_receiver.try_iter().count() > 0 {
            break Ok(PauseBreak::Reload);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_loop<E>(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    assets: &mut Assets,
    scores: &[PausedScore],
    music_position: &mut E,
    branch: &mut ValueWithUpdateTime<BranchAnimationState>,
    game_user_state: &mut GameUserState,
) -> Result<Option<PauseBreak>, TaikoError>
where
    E: EasingF64,
{
    assert!(!scores.is_empty());
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(PauseBreak::Exit)),
            Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::Space => {
                    game_user_state.time = music_position.get();
                    return Ok(Some(PauseBreak::Play(*game_user_state)));
                }
                Keycode::Q => {
                    return Ok(Some(PauseBreak::Reload));
                }
                Keycode::F1 => game_user_state.auto = !game_user_state.auto,
                Keycode::PageDown => music_position.set_with(|x| {
                    scores[0]
                        .measure_scroll_points
                        .range(..OrderedFloat::from(x - 1e-3))
                        .next_back()
                        .map_or(x, |x| **x)
                }),
                Keycode::PageUp => music_position.set_with(|x| {
                    scores[0]
                        .measure_scroll_points
                        .range(OrderedFloat::from(x + 1e-3)..)
                        .next()
                        .map_or(x, |x| **x)
                }),
                Keycode::Left => music_position.set_with(|x| {
                    scores[0]
                        .beat_scroll_points
                        .range(..OrderedFloat::from(x - 1e-3))
                        .next_back()
                        .map_or(x, |x| **x)
                }),
                Keycode::Right => music_position.set_with(|x| {
                    scores[0]
                        .beat_scroll_points
                        .range(OrderedFloat::from(x + 1e-3)..)
                        .next()
                        .map_or(x, |x| **x)
                }),
                Keycode::Up => branch.update(|b| b.set(b.get().saturating_next(), 0.0)),
                Keycode::Down => branch.update(|b| b.set(b.get().saturating_prev(), 0.0)),
                Keycode::Num1 => {
                    game_user_state.speed =
                        (game_user_state.speed / 2.0f64.powf(1. / 12.)).max(0.25)
                }
                Keycode::Num2 => {
                    game_user_state.speed = (game_user_state.speed * 2.0f64.powf(1. / 12.)).min(1.0)
                }
                _ => {}
            },
            _ => {}
        }
    }

    let display_position = music_position.get_eased();

    clear_background(canvas);

    for (score, offset_y) in scores.iter().rev().zip(get_offsets_rev(scores.len())) {
        draw_background(canvas, assets, offset_y)
            .map_err(to_sdl_error("While drawing background"))?;

        let rect = game_rect();
        canvas.set_clip_rect(shift_rect((0, offset_y), rect));
        {
            draw_branch_overlay(
                canvas,
                branch.duration_since_update().as_secs_f64(),
                rect,
                &branch.get(),
                offset_y,
            )?;

            let bar_lines = score
                .score
                .bar_lines
                .iter()
                .filter(|x| branch.get().get().matches(x.branch));
            draw_bar_lines(canvas, display_position, bar_lines, offset_y)?;

            let notes = score
                .score
                .notes
                .iter()
                .rev()
                .filter(|x| branch.get().get().matches(x.branch));
            draw_notes(canvas, assets, display_position, notes, offset_y)?;
        }
        canvas.set_clip_rect(None);
    }

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}
