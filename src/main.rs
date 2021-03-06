use enum_map::EnumMap;
use itertools::iterate;
use itertools::Itertools;
use num::clamp;
use sdl2::event::{Event, EventType};
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use std::convert::TryFrom;
use std::ffi::c_void;
use std::iter;
use std::time::Duration;
use taiko_untitled::assets::Assets;
use taiko_untitled::audio::{AudioManager, SoundBuffer, SoundEffectSchedule};
use taiko_untitled::errors::{
    new_config_error, new_sdl_canvas_error, new_sdl_error, new_sdl_window_error, new_tja_error,
    TaikoError,
};
use taiko_untitled::game::{GameManager, Judge};
use taiko_untitled::structs::{
    typed::{NoteContent, RendaContent, RendaKind},
    BarLineKind, Bpm, BranchType, NoteColor, NoteSize, SingleNoteKind,
};
use taiko_untitled::tja::{load_tja_from_file, Song};

fn main() -> Result<(), TaikoError> {
    let config = taiko_untitled::config::get_config()
        .map_err(|e| new_config_error("Failed to load configuration", e))?;

    let sdl_context =
        sdl2::init().map_err(|s| new_sdl_error("Failed to initialize SDL context", s))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|s| new_sdl_error("Failed to initialize video subsystem of SDL", s))?;
    let window = video_subsystem
        .window("", config.window.width, config.window.height)
        .allow_highdpi()
        .build()
        .map_err(|x| new_sdl_window_error("Failed to create main window", x))?;
    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|s| new_sdl_error("Failed to initialize event pump for SDL", s))?;

    let mut canvas = window.into_canvas();
    if config.window.vsync {
        canvas = canvas.present_vsync();
    }
    let mut canvas = canvas
        .build()
        .map_err(|e| new_sdl_canvas_error("Failed to create SDL canvas", e))?;
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

    let mut timer_subsystem = sdl_context
        .timer()
        .map_err(|s| new_sdl_error("Failed to initialize timer subsystem of SDL", s))?;

    let audio_manager = taiko_untitled::audio::AudioManager::new()?;

    let mut assets = Assets::new(&texture_creator, &audio_manager)?;
    {
        let volume = config.volume.se / 100.0;
        assets.chunks.sound_don.set_volume(volume);
        assets.chunks.sound_ka.set_volume(volume);
        let volume = config.volume.song / 100.0;
        audio_manager.set_music_volume(volume)?;
    }

    let song = if let [_, tja_file_name, ..] = &std::env::args().collect_vec()[..] {
        Some(
            load_tja_from_file(tja_file_name)
                .map_err(|e| new_tja_error("Failed to load tja file", e))?,
        )
    } else {
        None
    };

    let mut event_callback_tuple = (
        &audio_manager,
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

    let mut game_manager = song
        .as_ref()
        .and_then(|song| (&song.score).as_ref())
        .map(|score| GameManager::new(&score));

    if let Some(song_wave_path) = song.as_ref().and_then(|song| song.wave.as_ref()) {
        audio_manager.load_music(song_wave_path)?;
    }

    if let Some(Song {
        score: Some(score), ..
    }) = &song
    {
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
        audio_manager.add_play_schedules(schedules)?;
    }

    'main: loop {
        let music_position = audio_manager.music_position()?;
        let sdl_timestamp = timer_subsystem.ticks();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    repeat: false,
                    keycode: Some(keycode),
                    timestamp,
                    ..
                } => match keycode {
                    Keycode::Z
                    | Keycode::X
                    | Keycode::Slash
                    | Keycode::Underscore
                    | Keycode::Backslash => {
                        let color = match keycode {
                            Keycode::X | Keycode::Slash => NoteColor::Don,
                            Keycode::Z | Keycode::Underscore | Keycode::Backslash => NoteColor::Ka,
                            _ => unreachable!(),
                        };
                        if let (Some(game_manager), Some(music_position)) =
                            (game_manager.as_mut(), music_position)
                        {
                            game_manager.hit(
                                Some(color),
                                music_position + (timestamp - sdl_timestamp) as f64 / 1000.0,
                            );
                        }
                    }
                    Keycode::Space => {
                        audio_manager.play()?;
                    }
                    Keycode::F1 => {
                        if let Some(ref mut game_manager) = game_manager {
                            let auto = game_manager.switch_auto();
                            audio_manager.set_play_scheduled(auto)?;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        game_manager
            .as_mut()
            .map(|g| music_position.to_owned().map(|m| g.hit(None, m)));

        canvas.set_draw_color(Color::RGBA(20, 20, 20, 0));
        canvas.clear();
        canvas
            .copy(
                &assets.textures.background,
                None,
                Some(Rect::new(0, 0, 1920, 1080)),
            )
            .map_err(|s| new_sdl_error("Failed to draw background", s))?;

        let gauge = game_manager.as_ref().map_or(0.0, |g| g.game_state.gauge);
        let gauge = clamp(gauge, 0.0, 10000.0) as u32 / 200;
        draw_gauge(&mut canvas, &assets, gauge, 39, 50)
            .map_err(|e| new_sdl_error("Failed to drawr", e))?;

        if let (
            Some(Song {
                score: Some(score), ..
            }),
            Some(ref mut game_manager),
            Some(music_position),
        ) = (
            &song,
            game_manager.as_mut(),
            audio_manager.music_position()?,
        ) {
            let score_rect = Rect::new(498, 288, 1422, 195);
            // draw score
            canvas.set_clip_rect(score_rect);

            // Branch overleay effect
            // TODO color for master course is wrong
            canvas.set_blend_mode(sdl2::render::BlendMode::Add);
            let bs = &game_manager.animation_state.branch_state;
            canvas.set_draw_color(interpolate_color(
                branch_overlay_color(bs.branch_before),
                branch_overlay_color(bs.branch_after),
                clamp((music_position - bs.switch_time) * 60.0 / 20.0, 0.0, 1.0),
            ));
            canvas
                .fill_rect(score_rect)
                .map_err(|e| new_sdl_error("Failed to draw branch overlay", e))?;
            canvas.set_blend_mode(sdl2::render::BlendMode::None);

            // draw bar lines
            let mut branches = game_manager.score.branches.iter().peekable();
            let mut current_branch = BranchType::Normal;
            let mut bar_lines = EnumMap::<_, Vec<_>>::new();
            for bar_line in &score.bar_lines {
                while let Some(branch) = branches.peek() {
                    if branch.switch_time <= bar_line.time {
                        if let Some(branch) = branch.info.determined_branch {
                            current_branch = branch;
                        }
                        branches.next();
                    } else {
                        break;
                    }
                }
                if bar_line.branch.map_or(true, |b| b == current_branch) && bar_line.visible {
                    let x = get_x(music_position, bar_line.time, &bar_line.scroll_speed) as i32;
                    if 0 <= x && x <= 2000 {
                        bar_lines[bar_line.kind].push(Rect::new(x + 96, 288, 3, 195));
                    }
                }
            }
            for (kind, rects) in bar_lines {
                match kind {
                    BarLineKind::Normal => canvas.set_draw_color(Color::RGB(200, 200, 200)),
                    BarLineKind::Branch => canvas.set_draw_color(Color::RGB(0xf3, 0xff, 0x55)),
                };
                canvas
                    .fill_rects(&rects[..])
                    .map_err(|e| new_sdl_error("Failed to draw bar lines", e))?;
            }

            // draw notes
            let mut branches = game_manager.score.branches.iter().rev().peekable();
            for note in game_manager.score.notes.iter().rev() {
                branches
                    .peeking_take_while(|t| {
                        note.time < t.switch_time || t.info.determined_branch.is_none()
                    })
                    .for_each(|_| {});
                let branch = branches
                    .peek()
                    .and_then(|b| b.info.determined_branch)
                    .unwrap_or(BranchType::Normal);
                if note.branch.map_or(false, |b| b != branch) {
                    continue;
                }
                match &note.content {
                    NoteContent::Single(single_note) if single_note.info.visible() => {
                        let x = get_x(music_position, note.time, &note.scroll_speed);
                        draw_note(&mut canvas, &assets, &single_note.kind, x as i32, 288)?;
                    }
                    NoteContent::Renda(RendaContent {
                        end_time,
                        kind: RendaKind::Unlimited(renda),
                        ..
                    }) => {
                        let (texture_left, texture_right) = match renda.size {
                            NoteSize::Small => {
                                (&assets.textures.renda_left, &assets.textures.renda_right)
                            }
                            NoteSize::Large => (
                                &assets.textures.renda_large_left,
                                &assets.textures.renda_large_right,
                            ),
                        };
                        let xs = get_x(music_position, note.time, &note.scroll_speed) as i32;
                        let xt = get_x(music_position, *end_time, &note.scroll_speed) as i32;
                        canvas
                            .copy(
                                texture_right,
                                Rect::new(97, 0, 195 - 97, 195),
                                Rect::new(xt + 97, 288, 195 - 97, 195),
                            )
                            .map_err(|e| new_sdl_error("Failed to draw renda right", e))?;
                        canvas
                            .copy(
                                texture_right,
                                Rect::new(0, 0, 97, 195),
                                Rect::new(xs + 97, 288, (xt - xs) as u32, 195),
                            )
                            .map_err(|e| new_sdl_error("Failed to draw renda center", e))?;
                        canvas
                            .copy(texture_left, None, Rect::new(xs, 288, 195, 195))
                            .map_err(|e| new_sdl_error("Failed to draw renda left", e))?;
                    }
                    NoteContent::Renda(RendaContent {
                        end_time,
                        kind: RendaKind::Quota(renda),
                        ..
                    }) => {
                        if renda.info.finished {
                            continue;
                        }
                        let x = get_x(
                            music_position,
                            num::clamp(music_position, note.time, *end_time),
                            &note.scroll_speed,
                        ) as i32;
                        canvas
                            .copy(
                                &assets.textures.renda_left,
                                None,
                                Rect::new(x, 288, 195, 195),
                            )
                            .map_err(|e| new_sdl_error("Failed to draw renda left", e))?;
                    }
                    _ => {}
                }
            }

            canvas.set_clip_rect(None);

            // draw flying notes
            for note in game_manager
                .flying_notes(|note| note.time <= music_position - 0.5)
                .rev()
            {
                // ends in 0.5 seconds
                let t = (music_position - note.time) * 60.0;
                if t >= 0.5 {
                    // after 0.5 frames
                    let x = 521.428 + 19.4211 * t + 1.75748 * t * t - 0.035165 * t * t * t;
                    let y = 288.4 - 44.303 * t + 0.703272 * t * t + 0.0368848 * t * t * t
                        - 0.000542067 * t * t * t * t;
                    draw_note(&mut canvas, &assets, &note.kind, x as i32, y as i32)?;
                }
            }

            for judge in game_manager
                .judge_strs(|judge| (music_position - judge.time) * 60.0 >= 18.0)
                .rev()
            {
                // (552, 226)
                let (y, a) = match (music_position - judge.time) * 60.0 {
                    t if t < 1.0 => (226.0 - 20.0 * t, t),
                    t if t < 6.0 => (206.0 + 20.0 * (t - 1.0) / 5.0, 1.0),
                    t if t < 14.0 => (226.0, 1.0),
                    t => (226.0, (18.0 - t) / 4.0),
                };
                let texture = match judge.judge {
                    Judge::Good => &mut assets.textures.judge_text_good,
                    Judge::Ok => &mut assets.textures.judge_text_ok,
                    Judge::Bad => &mut assets.textures.judge_text_bad,
                };
                texture.set_alpha_mod((a * 255.0) as u8);
                canvas
                    .copy(texture, None, Some(Rect::new(552, y as i32, 135, 90)))
                    .map_err(|e| new_sdl_error("Failed to draw judge str", e))?;
            }

            let combo = game_manager.game_state.combo;
            if let Some(textures) = match () {
                _ if combo < 10 => None,
                _ if combo < 50 => Some(&assets.textures.combo_nummber_white),
                _ if combo < 100 => Some(&assets.textures.combo_nummber_silver),
                _ => Some(&assets.textures.combo_nummber_gold),
            } {
                let digits = combo
                    .to_string()
                    .chars()
                    .map(|c| c.to_digit(10).unwrap())
                    .collect_vec();
                let w = (52.0 * digits.len() as f64).min(44.0 * 4.0);
                let x = 399.0 - w / 2.0;
                let w = w / digits.len() as f64;
                let yd = match (music_position - game_manager.animation_state.last_combo_update)
                    * 60.0
                {
                    t if t < 2.0 => t * 7.5,
                    t if t < 9.0 => (9.0 - t) * 15.0 / 7.0,
                    _ => 0.0,
                };
                for (i, t) in digits.iter().map(|&i| &textures[i as usize]).enumerate() {
                    let x = x + w * i as f64 - w * 3.0 / 44.0;
                    let rect = Rect::new(
                        x as i32,
                        (334.0 - yd) as i32,
                        (w * 55.0 / 44.0) as u32,
                        (77.0 + yd) as u32,
                    );
                    canvas
                        .copy(t, None, rect)
                        .map_err(|e| new_sdl_error("Failed to draw combo number", e))?;
                }
            }
        }

        canvas.present();
        if !config.window.vsync {
            std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
        }
    }

    unsafe {
        sdl2_sys::SDL_DelEventWatch(
            Some(callback),
            &mut event_callback_tuple as *mut _ as *mut c_void,
        );
    }

    Ok(())
}

fn get_x(music_position: f64, time: f64, scroll_speed: &Bpm) -> f64 {
    let diff = time - music_position;
    520.0 + 1422.0 / 4.0 * diff / scroll_speed.beat_duration()
}

fn draw_note(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    kind: &SingleNoteKind,
    x: i32,
    y: i32,
) -> Result<(), TaikoError> {
    let texture = match kind.color {
        NoteColor::Don => match kind.size {
            NoteSize::Small => &assets.textures.note_don,
            NoteSize::Large => &assets.textures.note_don_large,
        },
        NoteColor::Ka => match kind.size {
            NoteSize::Small => &assets.textures.note_ka,
            NoteSize::Large => &assets.textures.note_ka_large,
        },
    };
    canvas
        .copy(texture, None, Rect::new(x, y, 195, 195))
        .map_err(|e| new_sdl_error("Failed to draw a note", e))
}

fn draw_gauge(
    canvas: &mut WindowCanvas,
    assets: &Assets,
    gauge: u32,
    clear_count: u32,
    all_count: u32,
) -> Result<(), String> {
    canvas.copy(
        &assets.textures.gauge_left_base,
        None,
        Rect::new(726, 204, 1920, 78),
    )?;
    canvas.copy(
        &assets.textures.gauge_right_base,
        None,
        Rect::new(726 + clear_count as i32 * 21, 204, 1920, 78),
    )?;

    let gauge_count = clamp(gauge, 0, clear_count);
    let src = Rect::new(0, 0, 21 * gauge_count, 78);
    canvas.copy(
        &assets.textures.gauge_left_red,
        src,
        Rect::new(738, 204, src.width(), src.height()),
    )?;

    let src = Rect::new(
        21 * gauge_count as i32,
        0,
        21 * (clear_count - gauge_count),
        78,
    );
    canvas.copy(
        &assets.textures.gauge_left_dark,
        src,
        Rect::new(738 + src.x(), 204, src.width(), src.height()),
    )?;

    let max_width = 21 * (all_count - clear_count) - 6;
    let gauge_count = clamp(gauge, clear_count, all_count);
    let src = Rect::new(0, 0, max_width.min(21 * (gauge_count - clear_count)), 78);
    canvas.copy(
        &assets.textures.gauge_right_yellow,
        src,
        Rect::new(
            738 + clear_count as i32 * 21,
            204,
            src.width(),
            src.height(),
        ),
    )?;

    let src = Rect::new(
        max_width.min(21 * (gauge_count - clear_count)) as i32,
        0,
        max_width.min(21 * (all_count - gauge_count)),
        78,
    );
    canvas.copy(
        &assets.textures.gauge_right_dark,
        src,
        Rect::new(
            738 + clear_count as i32 * 21 + src.x(),
            204,
            src.width(),
            src.height(),
        ),
    )?;

    canvas.copy(
        &assets.textures.gauge_soul,
        None,
        Rect::new(1799, 215, 71, 63),
    )?;
    Ok(())
}

fn branch_overlay_color(branch_type: BranchType) -> Color {
    match branch_type {
        BranchType::Normal => Color::RGB(0, 0, 0),
        BranchType::Expert => Color::RGB(8, 38, 55),
        BranchType::Master => Color::RGB(58, 0, 53),
    }
}

fn interpolate_color(color_zero: Color, color_one: Color, t: f64) -> Color {
    Color::RGBA(
        clamp(
            color_zero.r as f64 * (1.0 - t) + color_one.r as f64 * t,
            0.0,
            255.0,
        ) as u8,
        clamp(
            color_zero.g as f64 * (1.0 - t) + color_one.g as f64 * t,
            0.0,
            255.0,
        ) as u8,
        clamp(
            color_zero.b as f64 * (1.0 - t) + color_one.b as f64 * t,
            0.0,
            255.0,
        ) as u8,
        clamp(
            color_zero.a as f64 * (1.0 - t) + color_one.a as f64 * t,
            0.0,
            255.0,
        ) as u8,
    )
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
