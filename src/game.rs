use crate::assets::Assets;
use crate::audio::{AudioManager, SoundBuffer, SoundEffectSchedule};
use crate::config::TaikoConfig;
use crate::errors::{new_sdl_error, new_tja_error, to_sdl_error, TaikoError, TaikoErrorCause};
use crate::game_graphics::{
    draw_background, draw_bar_lines, draw_branch_overlay, draw_combo, draw_flying_notes,
    draw_gauge, draw_judge_strs, draw_notes,
};
use crate::game_manager::{GameManager, OfGameState};
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
use sdl2::event::{Event, EventType};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use sdl2::{EventPump, TimerSubsystem};
use std::convert::{TryFrom, TryInto};
use std::ffi::c_void;
use std::iter;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::time::Duration;

type ScoreOfGameState = TypedScore<OfGameState>;

pub fn game<P>(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager,
    assets: &mut Assets,
    tja_file_name: P,
) -> Result<(), TaikoError>
where
    P: AsRef<Path>,
{
    let song = load_tja_from_file(tja_file_name)
        .map_err(|e| new_tja_error("Failed to load tja file", e))?;
    let score = song.score.as_ref().ok_or_else(|| TaikoError {
        message: "There is no score in the tja file".to_owned(),
        cause: TaikoErrorCause::None,
    })?;

    setup_audio_manager(&audio_manager, &assets, &score, song.wave.clone())?;

    let mut game_manager = GameManager::new(&score);

    let mut event_callback_tuple = (
        audio_manager,
        &assets.chunks.sound_don.clone(),
        &assets.chunks.sound_ka.clone(),
    );
    unsafe {
        // variables `audio_manager` and `assets` are valid
        // while this main function exists on the stack.
        sdl2_sys::SDL_AddEventWatch(
            Some(callback),
            &mut event_callback_tuple as *mut _ as *mut c_void,
        );
    }

    let ret = loop {
        if let Some(res) = game_loop(
            config,
            canvas,
            event_pump,
            timer_subsystem,
            audio_manager,
            assets,
            &score,
            &mut game_manager,
        )? {
            #[allow(clippy::unit_arg)]
            break Ok(res);
        }
    };

    unsafe {
        sdl2_sys::SDL_DelEventWatch(
            Some(callback),
            &mut event_callback_tuple as *mut _ as *mut c_void,
        );
    }

    ret
}

// TODO too many parameters
#[allow(clippy::too_many_arguments)]
fn game_loop(
    config: &TaikoConfig,
    canvas: &mut WindowCanvas,
    event_pump: &mut EventPump,
    timer_subsystem: &mut TimerSubsystem,
    audio_manager: &AudioManager,
    assets: &mut Assets,
    score: &Score,
    game_manager: &mut GameManager,
) -> Result<Option<()>, TaikoError> {
    let music_position = audio_manager.music_position()?;
    let sdl_timestamp = timer_subsystem.ticks();

    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(Some(())),
            Event::KeyDown {
                repeat: false,
                keycode: Some(keycode),
                timestamp,
                keymod,
                ..
            } => match keycode {
                Keycode::Z
                | Keycode::X
                | Keycode::Slash
                | Keycode::Underscore
                | Keycode::Backslash
                    if audio_manager.playing()? =>
                {
                    process_key_event(
                        keycode,
                        game_manager,
                        music_position,
                        timestamp,
                        sdl_timestamp,
                    );
                }
                Keycode::Space => {
                    if keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
                        audio_manager.pause()?;
                    } else {
                        audio_manager.play()?;
                    }
                }
                Keycode::F1 => {
                    let auto = game_manager.switch_auto();
                    audio_manager.set_play_scheduled(auto)?;
                }
                Keycode::PageUp => {
                    if let Some(music_position) = music_position {
                        audio_manager.seek(music_position + 1.0)?;
                    }
                }
                Keycode::PageDown => {
                    if let Some(music_position) = music_position {
                        audio_manager.seek(music_position - 1.0)?;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(m) = music_position {
        game_manager.hit(None, m);
    }

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
        let score_rect = Rect::new(498, 288, 1422, 195);
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

fn setup_audio_manager(
    audio_manager: &AudioManager,
    assets: &Assets,
    score: &Score,
    wave: Option<PathBuf>,
) -> Result<(), TaikoError>
where
{
    if let Some(song_wave_path) = wave {
        audio_manager.load_music(song_wave_path)?;
    }
    audio_manager.add_play_schedules(generate_schedules(&score, assets))?;
    audio_manager.play()?;
    Ok(())
}

fn generate_schedules(score: &Score, assets: &Assets) -> Vec<SoundEffectSchedule> {
    let mut schedules = Vec::new();
    for note in &score.notes {
        match &note.content {
            NoteContent::Single(single_note) => {
                let chunk = match single_note.kind.color {
                    NoteColor::Don => &assets.chunks.sound_don,
                    NoteColor::Ka => &assets.chunks.sound_ka,
                };
                let count = match single_note.kind.size {
                    NoteSize::Small => 1,
                    NoteSize::Large => 2,
                };
                schedules.extend(
                    iter::repeat_with(|| SoundEffectSchedule {
                        timestamp: note.time,
                        source: chunk.new_source(),
                    })
                    .take(count),
                );
            }
            NoteContent::Renda(RendaContent { end_time, .. }) => {
                schedules.extend(
                    iterate(note.time, |&x| x + 1.0 / 20.0)
                        .take_while(|t| t < end_time)
                        .map(|t| SoundEffectSchedule {
                            timestamp: t,
                            source: assets.chunks.sound_don.new_source(),
                        }),
                );
            }
        }
    }
    schedules
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
            }) => renda.info.finished.then(|| {
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

extern "C" fn callback(user_data: *mut c_void, event: *mut sdl2_sys::SDL_Event) -> i32 {
    let raw = unsafe { *event };
    let raw_type = unsafe { raw.type_ };

    // The following conversion is copied from `sdl2::event::Event:from_ll`.
    // Why can't I reuse it?  Because it's currently a private function.

    // if event type has not been defined, treat it as a UserEvent
    let event_type: EventType = EventType::try_from(raw_type as u32).unwrap_or(EventType::User);
    if let Some(keycode) = unsafe {
        match event_type {
            EventType::KeyDown => {
                let event = raw.key;
                Keycode::from_i32(event.keysym.sym as i32).filter(|_| event.repeat == 0)
            }
            _ => None,
        }
    } {
        // `user_data` originates from `audio_manager` and `assets` variables
        // in the `main` function stack, which should be valid until the hook is removed.
        let (audio_manager, sound_don, sound_ka) = unsafe {
            // &(*(user_data as *mut Assets)).chunks
            *(user_data as *mut (&AudioManager, &SoundBuffer, &SoundBuffer))
        };
        match keycode {
            Keycode::X | Keycode::Slash => {
                // TODO send error to main thread
                let _ = audio_manager.add_play(sound_don);
            }
            Keycode::Z | Keycode::Underscore | Keycode::Backslash => {
                // TODO send error to main thread
                let _ = audio_manager.add_play(sound_ka);
            }
            _ => {}
        }
    }
    0
}
