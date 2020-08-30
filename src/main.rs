use sdl2::event::Event;

use std::time::{Duration, Instant};

use sdl2::keyboard::Keycode;
use sdl2::mixer;
use sdl2::mixer::{Channel, Music, AUDIO_S16LSB, DEFAULT_CHANNELS};
use sdl2::rect::Rect;

use itertools::Itertools;
use taiko_untitled::assets::Assets;
use taiko_untitled::errors::{
    new_config_error, new_sdl_canvas_error, new_sdl_error, new_sdl_window_error, new_tja_error,
    TaikoError,
};
use taiko_untitled::tja;
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

    let mut canvas = window
        .into_canvas()
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

    // let _audio = sdl_context
    //     .audio()
    //     .map_err(|s| new_sdl_error("Failed to initialize audio subsystem of SDL", s))?;
    mixer::open_audio(44100, AUDIO_S16LSB, DEFAULT_CHANNELS, 256)
        .map_err(|s| new_sdl_error("Failed to open audio stream", s))?;
    mixer::allocate_channels(128);

    let assets = Assets::new(&texture_creator)?;

    let song = if let [_, tja_file_name, ..] = &std::env::args().collect_vec()[..] {
        Some(
            load_tja_from_file(tja_file_name)
                .map_err(|e| new_tja_error("Failed to load tja file", e))?,
        )
    } else {
        None
    };
    let music = match song {
        Some(Song {
                 wave: Some(ref wave),
                 ..
             }) => Some(
            Music::from_file(wave)
                .map_err(|s| new_sdl_error(format!("Failed to load wave file: {:?}", wave), s))?,
        ),
        _ => None,
    };

    let mut playback_start = None;

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    repeat: false,
                    keycode: Some(keycode),
                    ..
                } => {
                    if let Some(sound) = match keycode {
                        Keycode::X | Keycode::Slash => Some(&assets.chunks.sound_don),
                        Keycode::Z | Keycode::Underscore => Some(&assets.chunks.sound_ka),
                        _ => None,
                    } {
                        Channel::all()
                            .play(&sound, 0)
                            .map_err(|s| new_sdl_error("Failed to play sound effect", s))?;
                    } else {
                        match keycode {
                            Keycode::Space => {
                                if let Some(ref music) = music {
                                    if playback_start.is_none() {
                                        playback_start = Some(Instant::now());
                                        music.play(0).map_err(|s| {
                                            new_sdl_error("Failed to play wave file", s)
                                        })?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
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
            Some(playback_start),
        ) = (&song, &playback_start)
        {
            canvas.set_clip_rect(Rect::new(498, 288, 1422, 195));

            let now = Instant::now();
            for note in score.notes.iter().rev() {
                match &note.content {
                    tja::NoteContent::Normal { time, color, size } => {
                        let diff = time - (now - *playback_start).as_secs_f64();
                        let x = 520.0 + 1422.0 / 4.0 * diff / note.scroll_speed.get_beat_duration();
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
                    _ => {}
                }
            }

            canvas.set_clip_rect(None);
        }

        canvas.present();
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0));
    }

    Ok(())
}
