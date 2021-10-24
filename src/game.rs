use crate::assets::Assets;
use crate::audio::SoundBuffer;
use crate::audio::{AudioManager, SoundEffectSchedule};
use crate::config::TaikoConfig;
use crate::errors::no_score_in_tja;
use crate::errors::{new_sdl_error, new_tja_error, to_sdl_error, TaikoError};
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
use notify::RecursiveMode;
use notify::Watcher;
use num::clamp;
use sdl2::event::{Event, EventWatch, EventWatchCallback};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::render::WindowCanvas;
use sdl2::{EventPump, EventSubsystem, TimerSubsystem};
use std::convert::TryInto;
use std::iter::Peekable;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

type ScoreOfGameState = TypedScore<OfGameState>;

enum GameBreak {
    Pause(f64),
    Escape,
    Exit,
}

#[derive(Clone, Copy, Debug)]
pub struct GameUserState {
    pub time: f64,
    pub auto: bool,
    pub speed: f64,
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
    P: AsRef<Path> + std::fmt::Debug,
{
    let mut song = load_tja_from_file(&tja_file_name)
        .map_err(|e| new_tja_error("Failed to load tja file", e))?;

    if let Some(song_wave_path) = &song.wave {
        audio_manager.load_music(song_wave_path)?;
    }
    let mut game_user_state = GameUserState {
        time: 0.0,
        auto: false,
        speed: 1.0,
    };

    // File watcher
    let (file_change_sender, file_change_receiver) = mpsc::channel();
    let _watcher = match notify::watcher(file_change_sender, Duration::from_millis(500)) {
        Ok(mut watcher) => {
            if let Err(e) = watcher.watch(&tja_file_name, RecursiveMode::NonRecursive) {
                println!(
                    "Failed to create file watcher.  The file will not be reloaded automatically."
                );
                println!("Caused by: {:?}", e);
            } else {
                println!("Start watching {:?}", &tja_file_name);
            }
            Some(watcher)
        }
        Err(e) => {
            println!(
                "Failed to create file watcher.  The file will not be reloaded automatically."
            );
            println!("Caused by: {:?}", e);
            None
        }
    };

    'entireLoop: loop {
        loop {
            match pause(
                config,
                canvas,
                event_pump,
                audio_manager,
                assets,
                &file_change_receiver,
                &song,
                game_user_state,
            )? {
                PauseBreak::Exit => break 'entireLoop Ok(GameMode::Exit),
                PauseBreak::Play(new_state) => {
                    game_user_state = new_state;
                    break;
                }
                PauseBreak::Reload => {
                    match load_tja_from_file(&tja_file_name)
                        .map_err(|e| new_tja_error("Failed to load tja file", e))
                        .and_then(|song| match song.score {
                            Some(..) => Ok(song),
                            None => Err(no_score_in_tja()),
                        }) {
                        Ok(new_song) => song = new_song,
                        Err(e) => {
                            println!("Failed to load tja file: {:?}", e);
                        }
                    };
                }
            }
        }
        let score = song.score.as_ref().ok_or_else(no_score_in_tja)?;
        match play(
            config,
            canvas,
            event_subsystem,
            event_pump,
            timer_subsystem,
            audio_manager,
            assets,
            score,
            &mut game_user_state,
        )? {
            GameBreak::Exit => break Ok(GameMode::Exit),
            GameBreak::Escape => {}
            GameBreak::Pause(request_time) => game_user_state.time = request_time,
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
    game_user_state: &mut GameUserState,
) -> Result<GameBreak, TaikoError> {
    let mut game_manager = GameManager::new(&score);
    let mut sound_effect_event_watch = setup_sound_effect(event_subsystem, audio_manager, assets);
    sound_effect_event_watch.set_activated(!game_user_state.auto);

    audio_manager.sound_effect_receiver.try_iter().count(); // Consume all
    audio_manager.set_play_speed(game_user_state.speed)?;
    audio_manager.seek(game_user_state.time)?;
    let mut auto_sent_pointer = 0;
    audio_manager.clear_play_schedules()?;
    audio_manager.add_play_schedules(generate_audio_schedules(
        assets,
        &game_manager.score,
        &mut auto_sent_pointer,
    ))?;
    audio_manager.set_play_scheduled(game_user_state.auto)?;
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
            &mut sound_effect_event_watch,
            &mut auto_sent_pointer,
            &mut game_user_state.auto,
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
    sound_effect_event_watch: &mut EventWatch<SoundEffectCallback>,
    auto_sent_pointer: &mut usize,
    auto: &mut bool,
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
                | Keycode::Backslash
                | Keycode::A
                | Keycode::S
                | Keycode::Colon
                | Keycode::RightBracket => {
                    if !*auto {
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
                    *auto = !*auto;
                    audio_manager.set_play_scheduled(*auto)?;
                    sound_effect_event_watch.set_activated(!*auto);
                }
                _ => {}
            },
            _ => {}
        }
    }
    for response in audio_manager.sound_effect_receiver.try_iter() {
        game_manager.hit(Some(response.kind.color), response.time);
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

struct SoundEffectCallback<'a> {
    sound_don: SoundBuffer,
    sound_ka: SoundBuffer,
    audio_manager: &'a AudioManager<AutoEvent>,
}
impl<'a> EventWatchCallback for SoundEffectCallback<'a> {
    fn callback(&mut self, event: Event) {
        if let Event::KeyDown {
            keycode: Some(keycode),
            repeat: false,
            ..
        } = event
        {
            match keycode {
                Keycode::X | Keycode::Slash => {
                    // TODO send error to main thread
                    let _ = self.audio_manager.add_play(&self.sound_don);
                }
                Keycode::A | Keycode::Z | Keycode::Underscore | Keycode::Backslash => {
                    // TODO send error to main thread
                    let _ = self.audio_manager.add_play(&self.sound_ka);
                }
                _ => {}
            }
        }
    }
}

fn setup_sound_effect<'e, 'au, 'at>(
    event_subsystem: &'e EventSubsystem,
    audio_manager: &'au AudioManager<AutoEvent>,
    assets: &'at Assets,
) -> EventWatch<'au, SoundEffectCallback<'au>> {
    let sound_don = assets.chunks.sound_don.clone();
    let sound_ka = assets.chunks.sound_ka.clone();
    event_subsystem.add_event_watch(SoundEffectCallback {
        sound_don,
        sound_ka,
        audio_manager,
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
        Keycode::X | Keycode::Slash | Keycode::S | Keycode::Colon => NoteColor::Don,
        Keycode::Z
        | Keycode::Underscore
        | Keycode::Backslash
        | Keycode::A
        | Keycode::RightBracket => NoteColor::Ka,
        _ => unreachable!(),
    };
    if let Some(music_position) = music_position {
        game_manager.hit(
            Some(color),
            // TODO sometimes, timestamp is less than sdl timestamp
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
    schedules
}

#[derive(Debug)]
pub struct AutoEvent {
    pub time: f64,
    pub kind: SingleNoteKind,
}
