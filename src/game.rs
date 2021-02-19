use crate::assets::Assets;
use crate::audio::{AudioManager, SoundEffectSchedule};
use crate::config::TaikoConfig;
use crate::errors::{new_sdl_error, new_tja_error, to_sdl_error, TaikoError, TaikoErrorCause};
use crate::game_graphics::game_rect;
use crate::game_graphics::{
    draw_background, draw_bar_lines, draw_branch_overlay, draw_combo, draw_flying_notes,
    draw_gauge, draw_judge_strs, draw_notes,
};
use crate::game_manager::{GameManager, OfGameState};
use crate::mode::GameMode;
use crate::pause::pause;
use crate::pause::PauseBreak;
use crate::structs::SingleNoteKind;
use crate::structs::{
    just,
    just::Score,
    typed::{Branch, NoteContent, RendaContent, RendaKind, Score as TypedScore},
    BarLine, BranchType, NoteColor, NoteSize,
};
use crate::tja::load_tja_from_file;
use crate::utils::to_digits;
use itertools::{iterate, Itertools};
use num::clamp;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::render::WindowCanvas;
use sdl2::{EventPump, EventSubsystem, TimerSubsystem};
use std::convert::TryInto;
use std::iter::Peekable;
use std::path::Path;
use std::time::Duration;

type ScoreOfGameState = TypedScore<OfGameState>;

enum GameBreak {
    Pause(f64),
    Escape,
    Exit,
}

pub fn game<P>(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_subsystem: &EventSubsystem,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager<AutoEvent>,
    assets: &mut Assets,
    tja_file_name: P,
) -> Result<GameMode, TaikoError>
where
    P: AsRef<Path>,
{
    let song = load_tja_from_file(&tja_file_name)
        .map_err(|e| new_tja_error("Failed to load tja file", e))?;
    let score = song.score.as_ref().ok_or_else(|| TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    })?;

    if let Some(song_wave_path) = &song.wave {
        audio_manager.load_music(song_wave_path)?;
    }
    let mut time = 0.0;

    loop {
        match pause(
            config,
            canvas,
            event_pump,
            audio_manager,
            assets,
            &tja_file_name,
            &song,
            time,
        )? {
            PauseBreak::Exit => break Ok(GameMode::Exit),
            PauseBreak::Play(request_time) => time = request_time,
        }
        match play(
            config,
            canvas,
            event_subsystem,
            event_pump,
            timer_subsystem,
            audio_manager,
            assets,
            &score,
            time,
        )? {
            GameBreak::Exit => break Ok(GameMode::Exit),
            GameBreak::Escape => {}
            GameBreak::Pause(request_time) => time = request_time,
        }
    }
}

fn play(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_subsystem: &EventSubsystem,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager<AutoEvent>,
    assets: &mut Assets,
    score: &Score,
    start_time: f64,
) -> Result<GameBreak, TaikoError> {
    let mut game_manager = GameManager::new(&score);
    let _sound_effect_event_watch = setup_sound_effect(event_subsystem, audio_manager, assets);

    audio_manager.seek(start_time)?;
    let mut auto_sent_pointer = 0;
    audio_manager.clear_play_schedules()?;
    audio_manager.add_play_schedules(generate_audio_schedules(
        assets,
        &game_manager.score,
        &mut auto_sent_pointer,
    ))?;
    audio_manager.play()?;

    // TODO Gotta wait until seek completes and it starts to play

    loop {
        if let Some(res) = game_loop(
            config,
            canvas,
            event_pump,
            timer_subsystem,
            audio_manager,
            assets,
            &score,
            &mut game_manager,
            &mut auto_sent_pointer,
        )? {
            break Ok(res);
        }
    }
}

// TODO too many parameters
#[allow(clippy::too_many_arguments)]
fn game_loop(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager<AutoEvent>,
    assets: &mut Assets,
    score: &Score,
    game_manager: &mut GameManager,
    auto_sent_pointer: &mut usize,
) -> Result<Option<GameBreak>, TaikoError> {
    let music_position = audio_manager.music_position()?;
    let sdl_timestamp = timer_subsystem.ticks();

    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(GameBreak::Exit)),
            Event::KeyDown {
                repeat: false,
                keycode: Some(keycode),
                timestamp,
                keymod,
                ..
            } => match keycode {
                Keycode::Q => return Ok(Some(GameBreak::Escape)),
                Keycode::Z
                | Keycode::X
                | Keycode::Slash
                | Keycode::Underscore
                | Keycode::Backslash => {
                    if !game_manager.auto() {
                        process_key_event(
                            keycode,
                            game_manager,
                            music_position,
                            timestamp,
                            sdl_timestamp,
                        );
                    }
                }
                Keycode::Space => {
                    if keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
                        return Ok(Some(GameBreak::Pause(music_position.unwrap_or(0.0))));
                    }
                }
                Keycode::F1 => {
                    let auto = game_manager.switch_auto();
                    audio_manager.set_play_scheduled(auto)?;
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(m) = music_position {
        game_manager.hit(None, m);
    }

    audio_manager.add_play_schedules(generate_audio_schedules(
        assets,
        &game_manager.score,
        auto_sent_pointer,
    ))?;

    draw_game_to_canvas(canvas, assets, score, game_manager, music_position)?;

    canvas.present();
    if !config.window.vsync {
        std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
    }

    Ok(None)
}

fn draw_game_to_canvas(
    canvas: &mut WindowCanvas,
    assets: &mut Assets,
    score: &Score,
    game_manager: &mut GameManager,
    music_position: Option<f64>,
) -> Result<(), TaikoError> {
    draw_background(canvas, assets).map_err(to_sdl_error("While drawing background"))?;

    let gauge = game_manager.game_state.gauge;
    let gauge = clamp(gauge, 0.0, 10000.0) as u32 / 200;
    draw_gauge(canvas, assets, gauge, 39, 50).map_err(|e| new_sdl_error("Failed to drawr", e))?;

    if let Some(music_position) = music_position {
        let score_rect = game_rect();
        canvas.set_clip_rect(score_rect);
        {
            draw_branch_overlay(
                canvas,
                music_position,
                score_rect,
                &game_manager.animation_state.branch_state,
            )?;

            let bar_lines =
                BarLineIterator::new(game_manager.score.branches.iter(), score.bar_lines.iter());
            draw_bar_lines(canvas, music_position, bar_lines)?;

            draw_game_notes(canvas, assets, music_position, &game_manager.score)?;
        }
        canvas.set_clip_rect(None);

        let flying_notes = game_manager
            .flying_notes(|note| note.time <= music_position - 0.5) // TODO incomplete refactor
            .rev();
        draw_flying_notes(canvas, assets, music_position, flying_notes)?;

        let judge_strs = game_manager
            .judge_strs(|judge| (music_position - judge.time) * 60.0 >= 18.0)
            .rev();
        draw_judge_strs(canvas, assets, music_position, judge_strs)?;

        let combo = game_manager.game_state.combo;
        if let Some(textures) = match () {
            _ if combo < 10 => None,
            _ if combo < 50 => Some(&assets.textures.combo_nummber_white),
            _ if combo < 100 => Some(&assets.textures.combo_nummber_silver),
            _ => Some(&assets.textures.combo_nummber_gold),
        } {
            let digits = to_digits(
                combo
                    .max(0)
                    .try_into()
                    .expect("i64 cannot be converted to u64 only if it's negative"),
            );
            let time = music_position - game_manager.animation_state.last_combo_update;
            draw_combo(canvas, textures, time, digits)?;
        }
    }
    Ok(())
}

fn setup_sound_effect<'e, 'au, 'at>(
    event_subsystem: &'e EventSubsystem,
    audio_manager: &'au AudioManager<AutoEvent>,
    assets: &'at Assets,
) -> impl Drop + 'au {
    let sound_don = assets.chunks.sound_don.clone();
    let sound_ka = assets.chunks.sound_ka.clone();
    event_subsystem.add_event_watch(move |event| {
        if let Event::KeyDown {
            keycode: Some(keycode),
            repeat: false,
            ..
        } = event
        {
            match keycode {
                Keycode::X | Keycode::Slash => {
                    // TODO send error to main thread
                    let _ = audio_manager.add_play(&sound_don);
                }
                Keycode::Z | Keycode::Underscore | Keycode::Backslash => {
                    // TODO send error to main thread
                    let _ = audio_manager.add_play(&sound_ka);
                }
                _ => {}
            }
        }
    })
}

struct BarLineIterator<'a, Branches, BarLines>
where
    Branches: Iterator<Item = &'a Branch<OfGameState>>,
    BarLines: Iterator<Item = &'a BarLine>,
{
    branches: Peekable<Branches>,
    bar_lines: BarLines,
    current_branch: BranchType,
}

impl<'a, Branches, BarLines> BarLineIterator<'a, Branches, BarLines>
where
    Branches: Iterator<Item = &'a Branch<OfGameState>>,
    BarLines: Iterator<Item = &'a BarLine>,
{
    fn new(branches: Branches, bar_lines: BarLines) -> Self {
        let branches = branches.peekable();
        let current_branch = BranchType::Normal;
        Self {
            branches,
            bar_lines,
            current_branch,
        }
    }
}

impl<'a, Branches, BarLines> Iterator for BarLineIterator<'a, Branches, BarLines>
where
    Branches: Iterator<Item = &'a Branch<OfGameState>>,
    BarLines: Iterator<Item = &'a BarLine>,
{
    type Item = &'a BarLine;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(bar_line) = self.bar_lines.next() {
            while let Some(branch) = self.branches.peek() {
                if branch.switch_time <= bar_line.time {
                    if let Some(branch) = branch.info.determined_branch {
                        self.current_branch = branch;
                    }
                    self.branches.next();
                } else {
                    break;
                }
            }
            if bar_line.branch.map_or(true, |b| b == self.current_branch) && bar_line.visible {
                return Some(bar_line);
            }
        }
        None
    }
}

/// Draw notes
pub fn draw_game_notes(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    music_position: f64,
    score: &ScoreOfGameState,
) -> Result<(), TaikoError> {
    let mut branches = score.branches.iter().rev().peekable();
    let notes = score.notes.iter().rev();

    // Filter by branch
    let notes = notes.filter(move |note| {
        branches
            .peeking_take_while(|t| note.time < t.switch_time || t.info.determined_branch.is_none())
            .for_each(|_| {});
        let branch = branches
            .peek()
            .and_then(|b| b.info.determined_branch)
            .unwrap_or(BranchType::Normal);
        note.branch.map_or(true, |b| b == branch)
    });

    // Filter by disappearance
    let notes = notes.filter_map(|note| {
        let content = match &note.content {
            NoteContent::Single(single_note) => single_note
                .info
                .visible()
                .then(|| NoteContent::Single(single_note.clone_with_default())),
            NoteContent::Renda(RendaContent {
                kind: RendaKind::Unlimited(renda),
                end_time,
                ..
            }) => Some(NoteContent::Renda(RendaContent {
                kind: RendaKind::Unlimited(renda.clone_with_default()),
                end_time: *end_time,
                info: (),
            })),
            NoteContent::Renda(RendaContent {
                kind: RendaKind::Quota(renda),
                end_time,
                ..
            }) => (!renda.info.finished).then(|| {
                NoteContent::Renda(RendaContent {
                    kind: RendaKind::Quota(renda.clone_with_default()),
                    end_time: *end_time,
                    info: (),
                })
            }),
        };
        content.map(|content| just::Note {
            scroll_speed: note.scroll_speed,
            time: note.time,
            branch: note.branch,
            info: (),
            content,
        })
    });

    draw_notes(canvas, assets, music_position, notes)
}

fn process_key_event(
    keycode: Keycode,
    game_manager: &mut GameManager,
    music_position: Option<f64>,
    timestamp: u32,
    sdl_timestamp: u32,
) {
    let color = match keycode {
        Keycode::X | Keycode::Slash => NoteColor::Don,
        Keycode::Z | Keycode::Underscore | Keycode::Backslash => NoteColor::Ka,
        _ => unreachable!(),
    };
    if let Some(music_position) = music_position {
        game_manager.hit(
            Some(color),
            music_position + (timestamp - sdl_timestamp) as f64 / 1000.0,
        );
    }
}

fn generate_audio_schedules(
    assets: &Assets,
    score: &ScoreOfGameState,
    auto_sent_pointer: &mut usize,
) -> Vec<SoundEffectSchedule<AutoEvent>> {
    let mut schedules = Vec::new();
    let mut current_branch = BranchType::Normal;
    let mut branches = score.branches.iter().peekable();
    while *auto_sent_pointer < score.notes.len() {
        let note = &score.notes[*auto_sent_pointer];
        if let Some(branch) = branches
            .peeking_take_while(|b| b.switch_time <= note.time)
            .last()
        {
            match branch.info.determined_branch {
                Some(branch) => current_branch = branch,
                None => break,
            }
        }
        *auto_sent_pointer += 1;
        if note.branch.map_or(false, |b| b != current_branch) {
            continue;
        }
        match &note.content {
            NoteContent::Single(single_note) => {
                let chunk = match single_note.kind.color {
                    NoteColor::Don => &assets.chunks.sound_don,
                    NoteColor::Ka => &assets.chunks.sound_ka,
                };
                let volume = match single_note.kind.size {
                    NoteSize::Small => 1.0,
                    NoteSize::Large => 2.0,
                };
                schedules.push(SoundEffectSchedule {
                    timestamp: note.time,
                    source: chunk.new_source(),
                    volume,
                    response: AutoEvent {
                        time: note.time,
                        kind: single_note.kind,
                    },
                });
            }
            NoteContent::Renda(RendaContent { end_time, .. }) => {
                schedules.extend(
                    iterate(note.time, |&x| x + 1.0 / 20.0)
                        .take_while(|t| t < end_time)
                        .map(|t| SoundEffectSchedule {
                            timestamp: t,
                            source: assets.chunks.sound_don.new_source(),
                            volume: 1.0,
                            response: AutoEvent {
                                time: t,
                                kind: SingleNoteKind {
                                    color: NoteColor::Don,
                                    size: NoteSize::Small,
                                },
                            },
                        }),
                );
            }
        }
    }
    if schedules.len() > 0 {
        println!(
            "Sent schedules: {:?}",
            schedules.iter().map(|x| &x.response).collect_vec()
        );
    }
    schedules
}

#[derive(Debug)]
pub struct AutoEvent {
    pub time: f64,
    pub kind: SingleNoteKind,
}
