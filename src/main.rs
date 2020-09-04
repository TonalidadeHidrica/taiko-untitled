use itertools::Itertools;
use sdl2::event::{Event, EventType};
use sdl2::keyboard::Keycode;
use sdl2::mixer;
use sdl2::mixer::{Channel, Music, AUDIO_S16LSB, DEFAULT_CHANNELS};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::convert::TryFrom;
use std::ffi::c_void;
use std::time::Duration;
use taiko_untitled::assets::Assets;
use taiko_untitled::errors::{
    new_config_error, new_sdl_canvas_error, new_sdl_error, new_sdl_window_error, new_tja_error,
    TaikoError,
};
use taiko_untitled::tja;
use taiko_untitled::tja::{load_tja_from_file, Bpm, Song};

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

    // TODO SDL_mixer dependent
    // let _audio = sdl_context
    //     .audio()
    //     .map_err(|s| new_sdl_error("Failed to initialize audio subsystem of SDL", s))?;
    mixer::open_audio(44100, AUDIO_S16LSB, DEFAULT_CHANNELS, 256)
        .map_err(|s| new_sdl_error("Failed to open audio stream", s))?;
    mixer::allocate_channels(128);

    let audio_manager = taiko_untitled::audio::AudioManager::new();

    let mut assets = Assets::new(&texture_creator, &audio_manager)?;
    {
        let volume = (128.0 * config.volume.se / 100.0) as i32;
        assets.chunks.sound_don.set_volume(volume);
        assets.chunks.sound_ka.set_volume(volume);
        let volume = (128.0 * config.volume.song / 100.0) as i32;
        Music::set_volume(volume);
    }

    let song = if let [_, tja_file_name, ..] = &std::env::args().collect_vec()[..] {
        Some(
            load_tja_from_file(tja_file_name)
                .map_err(|e| new_tja_error("Failed to load tja file", e))?,
        )
    } else {
        None
    };

    unsafe {
        // variable `assets` is valid while this main function exists on the stack trace.
        sdl2_sys::SDL_AddEventWatch(Some(callback), &mut assets as *mut _ as *mut c_void);
    }

    let mut auto = false;
    let mut auto_last_played = f64::NEG_INFINITY;
    let mut renda_last_played = f64::NEG_INFINITY;

    if let Some(song_wave_path) = song.as_ref().and_then(|song| song.wave.as_ref()) {
        audio_manager.load_music(song_wave_path);
    }

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    repeat: false,
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    Keycode::Space => {
                        audio_manager.play();
                    }
                    Keycode::F1 => {
                        auto = !auto;
                        dbg!(auto);
                        auto_last_played =
                            audio_manager.music_position().unwrap_or(f64::NEG_INFINITY);
                    }
                    // Keycode::Slash => audio_manager.add_play(don_sound.new_source()),
                    Keycode::X | Keycode::Slash => {
                        audio_manager.add_play(assets.chunks.sound_don_buffered.new_source())
                    }
                    Keycode::Z | Keycode::Underscore => {
                        audio_manager.add_play(assets.chunks.sound_ka_buffered.new_source())
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if let (
            Some(Song {
                score: Some(score), ..
            }),
            Some(music_position),
            true,
        ) = (&song, audio_manager.music_position(), &auto)
        {
            for note in score.notes.iter() {
                match &note.content {
                    tja::NoteContent::Normal { time, color, size } => {
                        if !(auto_last_played < *time && *time <= music_position) {
                            continue;
                        }
                        let chunk = match color {
                            tja::NoteColor::Don => &assets.chunks.sound_don,
                            tja::NoteColor::Ka => &assets.chunks.sound_ka,
                        };
                        let count = match size {
                            tja::NoteSize::Small => 1,
                            tja::NoteSize::Large => 2,
                        };
                        for _ in 0..count {
                            Channel::all()
                                .play(chunk, 0)
                                .map_err(|e| new_sdl_error("Failed to play wave file", e))?;
                        }
                    }
                    tja::NoteContent::Renda {
                        start_time,
                        end_time,
                        ..
                    } => {
                        if *end_time <= auto_last_played || music_position < *start_time {
                            continue;
                        }
                        if music_position - renda_last_played > 1.0 / 20.0 {
                            Channel::all()
                                .play(&assets.chunks.sound_don, 0)
                                .map_err(|e| new_sdl_error("Failed to play wave file", e))?;
                            renda_last_played = music_position;
                        }
                    }
                }
            }
            auto_last_played = music_position;
        }

        canvas
            .copy(
                &assets.textures.background,
                None,
                Some(Rect::new(0, 0, 1920, 1080)),
            )
            .map_err(|s| new_sdl_error("Failed to draw background", s))?;

        if let (
            Some(Song {
                score: Some(score), ..
            }),
            Some(music_position),
        ) = (&song, audio_manager.music_position())
        {
            canvas.set_clip_rect(Rect::new(498, 288, 1422, 195));

            let rects = score
                .bar_lines
                .iter()
                .filter_map(|bar_line| {
                    if bar_line.visible {
                        let x = get_x(music_position, bar_line.time, &bar_line.scroll_speed) as i32;
                        if 0 <= x && x <= 2000 {
                            // TODO magic number depending on 1920
                            return Some(Rect::new(x + 96, 288, 3, 195));
                        }
                    }
                    None
                })
                .collect_vec();
            canvas.set_draw_color(Color::RGB(200, 200, 200));
            canvas
                .fill_rects(&rects[..])
                .map_err(|e| new_sdl_error("Failed to draw bar lines", e))?;

            for note in score.notes.iter().rev() {
                match &note.content {
                    tja::NoteContent::Normal { time, color, size } => {
                        let x = get_x(music_position, *time, &note.scroll_speed);
                        let texture = match color {
                            tja::NoteColor::Don => match size {
                                tja::NoteSize::Small => &assets.textures.note_don,
                                tja::NoteSize::Large => &assets.textures.note_don_large,
                            },
                            tja::NoteColor::Ka => match size {
                                tja::NoteSize::Small => &assets.textures.note_ka,
                                tja::NoteSize::Large => &assets.textures.note_ka_large,
                            },
                        };
                        canvas
                            .copy(texture, None, Rect::new(x as i32, 288, 195, 195))
                            .map_err(|e| new_sdl_error("Failed to draw a note", e))?;
                    }
                    tja::NoteContent::Renda {
                        start_time,
                        end_time,
                        kind: tja::RendaKind::Unlimited { size },
                    } => {
                        let (texture_left, texture_right) = match size {
                            tja::NoteSize::Small => {
                                (&assets.textures.renda_left, &assets.textures.renda_right)
                            }
                            tja::NoteSize::Large => {
                                (&assets.textures.renda_left, &assets.textures.renda_right)
                            }
                        };
                        let xs = get_x(music_position, *start_time, &note.scroll_speed) as i32;
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
                    tja::NoteContent::Renda {
                        start_time,
                        end_time,
                        kind: tja::RendaKind::Quota { .. },
                    } => {
                        let x = get_x(
                            music_position,
                            num::clamp(music_position, *start_time, *end_time),
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
                }
            }

            canvas.set_clip_rect(None);
        }

        canvas.present();
        if !config.window.vsync {
            std::thread::sleep(Duration::from_secs_f64(1.0 / config.window.fps));
        }
    }

    unsafe {
        sdl2_sys::SDL_DelEventWatch(Some(callback), &mut assets as *mut _ as *mut c_void);
    }

    Ok(())
}

fn get_x(music_position: f64, time: f64, scroll_speed: &Bpm) -> f64 {
    let diff = time - music_position;
    520.0 + 1422.0 / 4.0 * diff / scroll_speed.get_beat_duration()
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
        // `user_data` originates from `assets` variable in the `main` function stack frame,
        // which should be valid until the hook is removed.
        let chunks = unsafe { &(*(user_data as *mut Assets)).chunks };
        if let Some(sound) = match keycode {
            // Keycode::X | Keycode::Slash => Some(&chunks.sound_don),
            // Keycode::Z | Keycode::Underscore => Some(&chunks.sound_ka),
            _ => None,
        } {
            Channel::all().play(&sound, 0).ok();
        }
    }
    0
}
