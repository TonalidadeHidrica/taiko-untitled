use config::Config;
use ffmpeg4::codec::decoder;
use ffmpeg4::sys::{av_seek_frame, AVSEEK_FLAG_BACKWARD};
use ffmpeg4::util::{frame, media};
use ffmpeg4::{format, Rational};
use itertools::Itertools;
use ordered_float::OrderedFloat;
use sdl2::event::Event;
use sdl2::image::LoadTexture;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::{Point, Rect};
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::ttf::Font;
use sdl2::video::WindowContext;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::iter::repeat_with;
use std::path::PathBuf;
use std::time::Instant;
use taiko_untitled::analyze::{detect_note_positions, integrate_some_fraction, DetectedNote};
use taiko_untitled::assets::Assets;
use taiko_untitled::ffmpeg_utils::{get_sdl_pix_fmt_and_blendmode, next_frame, FilteredPacketIter};
use taiko_untitled::game::draw_game_notes;
use taiko_untitled::game_graphics::{draw_note, game_rect};
use taiko_untitled::game_manager::{GameManager, Score};
use taiko_untitled::structs::{NoteColor, NoteSize, SingleNoteKind};
use taiko_untitled::tja::load_tja_from_file;
use taiko_untitled::video_analyzer_assets::{get_single_note_color, Textures};

#[derive(Debug)]
struct MainErr(String);

impl<T> From<T> for MainErr
where
    T: ToString,
{
    fn from(err: T) -> Self {
        MainErr(err.to_string())
    }
}
fn debug_to_err<T: std::fmt::Debug>() -> impl Fn(T) -> MainErr {
    |e| MainErr(format!("{:?}", e))
}

fn main() -> Result<(), MainErr> {
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;
    let width = config.get::<u32>("width")?;
    let height = config.get::<u32>("height")?;
    let hidpi_prop = config.get::<u32>("hidpi_prop").unwrap_or(1);
    let video_path = config.get::<PathBuf>("video")?;
    let font_path = config.get::<PathBuf>("font")?;
    let image_path = config.get::<PathBuf>("image").ok();
    let notes_path = config.get_str("notes_image").ok();
    let note_dimension =
        config
            .get_str("note_dimension")
            .ok()
            .and_then(|s| match s.split(' ').collect_vec()[..] {
                [a, b, c, d] => match (a.parse(), b.parse(), c.parse(), d.parse()) {
                    (Ok(a), Ok(b), Ok(c), Ok(d)) => Some(Rect::new(a, b, c, d)),
                    _ => None,
                },
                _ => None,
            });

    let mut score = get_scores(config);

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Main Window", width / hidpi_prop, height / hidpi_prop)
        .build()
        .map_err(|x| x.to_string())?;
    println!("{:?}, {:?}", window.size(), window.drawable_size());
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;
    let mouse_util = sdl_context.mouse();
    let ttf_context = sdl2::ttf::init()?;

    let texture_creator = canvas.texture_creator();
    let mut frame_buffer = RingBuffer::try_new::<MainErr, _>(15, || {
        Ok((
            texture_creator.create_texture_streaming(Some(PixelFormatEnum::IYUV), width, height)?,
            None,
        ))
    })?;
    let mut image_texture = match image_path {
        Some(ref image_path) => Some(texture_creator.load_texture(image_path)?),
        _ => None,
    };
    let mut notes_texture = match notes_path {
        Some(ref path) => Some(texture_creator.load_texture(path)?),
        _ => None,
    };

    let mut textures = Textures::new(&texture_creator)?;
    let font = ttf_context.load_font(font_path, 24)?;

    let audio_manager = taiko_untitled::audio::AudioManager::new().map_err(debug_to_err())?;
    let game_assets = Assets::new(&texture_creator, &audio_manager).map_err(debug_to_err())?;

    let mut input_context = format::input(&video_path)?;
    let stream = input_context
        .streams()
        .best(media::Type::Video)
        .ok_or("No video stream found")?;
    let stream_index = stream.index();
    let time_base = stream.time_base();
    let mut decoder = stream.codec().decoder().video()?;
    decoder.set_parameters(stream.parameters())?;
    let mut packet_iterator = FilteredPacketIter(input_context.packets(), stream_index);
    let mut frame = frame::Video::empty();

    let mut do_play = false;
    let mut zoom_proportion = 1;
    let mut focus_x = 0;
    let mut focus_y = 0;
    let mut fixed = false;
    let mut speed_up = false;
    let mut cursor_mode = false;
    let (texture_x, mut texture_y) = (500, 288);
    let frame_id = -1; // TODO: remove this variable
    let mut texture_width = notes_texture.as_ref().map_or(1, |t| t.query().width);
    let mut draw_gauge = false;
    let mut score_time_deltas = config
        .get::<ScoreTimeDeltas>("score_time_deltas")
        .unwrap_or_default();
    let mut score_time_delta = None;
    let mut show_score = true;
    let mut show_detected_notes = false;
    let mut note_kind = None;
    let mut note_x = 500;

    let mut pts: i64 = 0;

    let start = Instant::now();

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    keycode: Some(keycode),
                    keymod,
                    ..
                } => {
                    let alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
                    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    match keycode {
                        Keycode::Space => {
                            if shift {
                                score_time_delta = None;
                            } else {
                                do_play = !do_play;
                            }
                        }
                        Keycode::Z => zoom_proportion += 1,
                        Keycode::X => zoom_proportion = max(1, zoom_proportion - 1),
                        Keycode::M => mouse_util.show_cursor(!mouse_util.is_cursor_showing()),
                        Keycode::F => fixed = !fixed,
                        Keycode::S if alt => speed_up = !speed_up,
                        Keycode::C => cursor_mode = !cursor_mode,
                        Keycode::G => draw_gauge = !draw_gauge,
                        Keycode::Q if alt => texture_width = max(1, texture_width - 1),
                        Keycode::W if alt => texture_width += 1,
                        Keycode::L => {
                            if let Err(e) = config.refresh() {
                                println!("Failed to load the config file: {:?}", e);
                            }
                            let notes_path = config.get_str("notes_image").ok();
                            image_texture = match image_path {
                                Some(ref image_path) => {
                                    Some(texture_creator.load_texture(image_path)?)
                                }
                                _ => None,
                            };
                            notes_texture = match notes_path {
                                Some(ref path) => {
                                    let t = texture_creator.load_texture(path)?;
                                    texture_width = t.query().width;
                                    Some(t)
                                }
                                _ => None,
                            };
                            textures = Textures::new(&texture_creator)?;
                        }
                        Keycode::P => {
                            println!(
                                "{}\t{}\t{}\t{}",
                                frame_id, texture_x, texture_y, texture_width
                            );
                        }
                        Keycode::Left | Keycode::Right => {
                            let sign = match keycode {
                                Keycode::Right => 1,
                                _ => -1,
                            };
                            let amount = match () {
                                _ if alt => 100,
                                _ if shift => 10,
                                _ => 1,
                            };
                            // if alt {
                            //     texture_x += sign * amount;
                            // } else {
                            note_x += sign * amount;
                            // }
                        }
                        Keycode::Up | Keycode::Down => {
                            texture_y += match keycode {
                                Keycode::Down => 1,
                                _ => -1,
                            } * match () {
                                _ if shift => 10,
                                _ => 1,
                            }
                        }
                        Keycode::Period => {
                            if !frame_buffer.forward() {
                                frame_buffer.try_append_and_jump_there::<MainErr, _>(
                                    |(video_texture, pts)| {
                                        if next_frame(
                                            &mut packet_iterator,
                                            &mut decoder,
                                            &mut frame,
                                        )? {
                                            update_frame_to_texture(&frame, video_texture)?;
                                            *pts = frame.pts();
                                            Ok(true)
                                        } else {
                                            Ok(false)
                                        }
                                    },
                                )?;
                            }
                        }
                        Keycode::Comma => {
                            if !frame_buffer.backward() {
                                packet_iterator = seek(
                                    SeekTarget::Timestamp(pts.saturating_sub(1)),
                                    SeekMode::Precise,
                                    time_base,
                                    &mut input_context,
                                    stream_index,
                                    &mut decoder,
                                    &mut frame,
                                    &mut frame_buffer,
                                )?;
                            }
                        }
                        Keycode::J | Keycode::K => {
                            let d =
                                score_time_delta.get_or_insert_with(|| score_time_deltas.get(pts));
                            *d += match keycode {
                                Keycode::J => -1.,
                                _ => 1.,
                            } * match () {
                                _ if alt => 1.,
                                _ if shift => 0.01,
                                _ => 0.0001,
                            };
                        }
                        Keycode::PageUp | Keycode::PageDown => {
                            let sign = match keycode {
                                Keycode::PageUp => 1,
                                _ => -1,
                            };
                            let (timestamp_delta, seek_mode) = if shift {
                                let mode = if sign > 0 {
                                    SeekMode::NextKeyframe
                                } else {
                                    SeekMode::PreviousKeyframe
                                };
                                (1, mode)
                            } else {
                                let delta = Rational::new(2, 1) / time_base;
                                let delta = delta.0 as f64 / delta.1 as f64;
                                (delta as i64, SeekMode::Precise)
                            };
                            packet_iterator = seek(
                                SeekTarget::Timestamp(pts + sign * timestamp_delta),
                                seek_mode,
                                time_base,
                                &mut input_context,
                                stream_index,
                                &mut decoder,
                                &mut frame,
                                &mut frame_buffer,
                            )?;
                        }
                        Keycode::Num2 if shift => show_score = !show_score,
                        Keycode::Num3 if shift => show_detected_notes = !show_detected_notes,
                        Keycode::Num0 if alt => note_kind = None,
                        Keycode::Num1 if alt => {
                            note_kind = Some(SingleNoteKind {
                                color: NoteColor::Don,
                                size: NoteSize::Small,
                            })
                        }
                        Keycode::Num2 if alt => {
                            note_kind = Some(SingleNoteKind {
                                color: NoteColor::Ka,
                                size: NoteSize::Small,
                            })
                        }
                        Keycode::Num3 if alt => {
                            note_kind = Some(SingleNoteKind {
                                color: NoteColor::Don,
                                size: NoteSize::Large,
                            })
                        }
                        Keycode::Num4 if alt => {
                            note_kind = Some(SingleNoteKind {
                                color: NoteColor::Ka,
                                size: NoteSize::Large,
                            })
                        }
                        Keycode::Slash if shift => {
                            let time = f64::from(Rational::new(pts as i32, 1) * time_base);
                            println!("{:.10} {}", time, note_x);
                        }
                        Keycode::Q => {
                            if let Err(e) = config.refresh() {
                                println!("Failed to load the config file: {:?}", e);
                            }
                            score = get_scores(config);
                            match config.get("score_time_deltas") {
                                Ok(s) => score_time_deltas = s,
                                Err(e) => println!("Failed to update score_time_delta: {:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    if !fixed {
                        focus_x = x * (hidpi_prop as i32);
                        focus_y = y * (hidpi_prop as i32);
                    }
                }
                _ => {}
            }
        }

        let origin_x = focus_x * (1 - zoom_proportion as i32);
        let origin_y = focus_y * (1 - zoom_proportion as i32);
        let affine = |x, y, w, h| {
            Rect::new(
                origin_x + x * zoom_proportion as i32,
                origin_y + y * zoom_proportion as i32,
                w * zoom_proportion,
                h * zoom_proportion,
            )
        };

        if do_play {
            let times = if speed_up { 5 } else { 1 };
            for _ in 0..times {
                // TODO: duplicate
                if !frame_buffer.forward() {
                    frame_buffer.try_append_and_jump_there::<MainErr, _>(
                        |(video_texture, pts)| {
                            if next_frame(&mut packet_iterator, &mut decoder, &mut frame)? {
                                update_frame_to_texture(&frame, video_texture)?;
                                *pts = frame.pts();
                                Ok(true)
                            } else {
                                Ok(false)
                            }
                        },
                    )?;
                }
            }
        }

        if let Some((video_texture, new_pts)) = frame_buffer.current() {
            canvas.copy(video_texture, None, affine(0, 0, width, height))?;
            if let &Some(new_pts) = new_pts {
                pts = new_pts;
            }
        }

        let notes = detect_notes(&mut canvas, &texture_creator, &font, &frame, focus_y)?;

        if cursor_mode {
            canvas.set_draw_color(match (Instant::now() - start).as_millis() % 1000 {
                x if x < 500 => Color::WHITE,
                _ => Color::BLACK,
            });
            canvas.draw_line(
                Point::new(0, focus_y - 1),
                Point::new(width as i32, focus_y - 1),
            )?;
            canvas.draw_line(
                Point::new(0, focus_y + zoom_proportion as i32),
                Point::new(width as i32, focus_y + zoom_proportion as i32),
            )?;
            canvas.draw_line(
                Point::new(focus_x - 1, 0),
                Point::new(focus_x - 1, height as i32),
            )?;
            canvas.draw_line(
                Point::new(focus_x + zoom_proportion as i32, 0),
                Point::new(focus_x + zoom_proportion as i32, height as i32),
            )?;
        } else {
            canvas.set_clip_rect(Some(Rect::new(focus_x, focus_y, width, height)));
            if let Some(ref image_texture) = image_texture {
                canvas.copy(image_texture, None, affine(0, 0, width, height))?;
            }
            if let Some(ref notes_texture) = notes_texture {
                let dim = notes_texture.query();
                canvas.copy(
                    notes_texture,
                    note_dimension,
                    affine(texture_x, texture_y, texture_width, dim.height),
                )?;
            }

            if draw_gauge {
                // let gauge = game_manager.map_or(0.0, |g| g.game_state.gauge);
                let clear_count = 39;
                let all_count = 50;
                canvas.copy(&textures.gauge_left_base, None, affine(726, 204, 1920, 78))?;
                canvas.copy(
                    &textures.gauge_right_base,
                    None,
                    affine(726 + clear_count * 21, 204, 1920, 78),
                )?;
                let src = Rect::new(0, 0, 21 * clear_count as u32, 78);
                canvas.copy(
                    &textures.gauge_left_red,
                    src,
                    affine(738, 204, src.width(), src.height()),
                )?;
                let src = Rect::new(0, 0, 21 * (all_count - clear_count as u32) - 6, 78);
                canvas.copy(
                    &textures.gauge_right_yellow,
                    src,
                    affine(738 + clear_count * 21, 204, src.width(), src.height()),
                )?;
            }

            canvas.set_clip_rect(None);
        }

        let rect = game_rect();
        let (tx, ty) = (rect.x + rect.w as i32, rect.y + rect.h as i32);
        let (sx, sy) = (focus_x.clamp(rect.x, tx), focus_y.clamp(rect.y, ty));
        let rect = Rect::new(sx, sy, (tx - sx) as _, (ty - sy) as _);
        canvas.set_clip_rect(rect);
        {
            if score.is_some() && show_score || note_kind.is_some() || show_detected_notes {
                canvas.set_draw_color((28, 28, 28));
                canvas.fill_rect(rect)?;
            }
            if let Some(note_kind) = note_kind {
                draw_note(&mut canvas, &game_assets, &note_kind, note_x, game_rect().y)
                    .map_err(debug_to_err())?;
            }
            if let (Some(score), true) = (&score, show_score) {
                let delta = score_time_delta.unwrap_or_else(|| score_time_deltas.get(pts));
                let time = f64::from(Rational::new(pts as i32, 1) * time_base) + delta;
                draw_game_notes(&mut canvas, &game_assets, time, score, 0)
                    .map_err(|e| MainErr(format!("{:?}", e)))?;
            }
            if show_detected_notes {
                for note in notes {
                    let note_x = note.note_x();
                    draw_note(&mut canvas, &game_assets, &note.kind, note_x as _, 288)
                        .map_err(debug_to_err())?;
                }
            }
        }
        canvas.set_clip_rect(None);

        let integrations = (frame.planes() > 0).then(|| integrate_some_fraction(&frame));

        let infos = [
            format!("({}, {})", focus_x, focus_y),
            {
                let t = Rational::new(pts as i32, 1) * time_base;
                let ms = 1000 * t.0 as u64 / t.1 as u64;
                let min = ms / 1000 / 60;
                let sec = ms / 1000 % 60;
                let ms = ms % 1000;
                format!("{:02}:{:02}.{:03}", min, sec, ms)
            },
            format!("({})", pts),
            format!("delta configurated = {:.4?}", score_time_deltas.get(pts)),
            format!("delta overwritten = {:.4?}", score_time_delta),
            format!("note_x = {}", note_x),
            format!("top = {}", integrations.as_ref().map_or(0, |x| x.top_left)),
            format!("bottom = {}", integrations.as_ref().map_or(0, |x| x.bottom)),
        ];
        let mut current_top = 0;
        for info in &infos {
            let text_surface = font.render(info).solid(Color::GREEN)?;
            let text_width = text_surface.width();
            let text_height = text_surface.height();
            let text_texture = texture_creator.create_texture_from_surface(text_surface)?;
            let rect = Rect::new(
                (width - text_width) as i32,
                current_top,
                text_width,
                text_height,
            );
            canvas.set_draw_color(Color::BLACK);
            canvas.fill_rect(rect)?;
            canvas.copy(&text_texture, None, rect)?;
            current_top += (text_height as f64 * 1.2) as i32;
        }

        canvas.present();

        // std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
    }

    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum SeekTarget {
    Timestamp(i64),
    #[allow(unused)]
    Milliseconds(i32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum SeekMode {
    Precise,
    NextKeyframe,
    PreviousKeyframe,
}

#[allow(clippy::too_many_arguments)]
fn seek<'a>(
    seek_target: SeekTarget,
    seek_mode: SeekMode,
    time_base: Rational,
    input_context: &'a mut format::context::Input,
    stream_index: usize,
    decoder: &mut decoder::Video,
    frame: &mut frame::Video,
    frame_buffer: &mut RingBuffer<(Texture, Option<i64>)>,
) -> Result<FilteredPacketIter<'a>, MainErr> {
    let timestamp = match seek_target {
        SeekTarget::Milliseconds(time_ms) => {
            let timestamp = Rational::new(time_ms, 1000) / time_base;
            f64::from(timestamp).trunc() as _
        }
        SeekTarget::Timestamp(t) => t,
    };
    let direction = match seek_mode {
        SeekMode::Precise | SeekMode::PreviousKeyframe => AVSEEK_FLAG_BACKWARD,
        SeekMode::NextKeyframe => 0,
    };
    let res = unsafe {
        av_seek_frame(
            input_context.as_mut_ptr(),
            stream_index as _,
            timestamp,
            direction,
        )
    };
    if res < 0 {
        return Err(MainErr(String::from("Failed to seek")));
    }
    let mut packet_iterator = FilteredPacketIter(input_context.packets(), stream_index);
    decoder.flush();
    frame_buffer.clear();
    while let Some((_, pts)) =
        frame_buffer.try_append_and_jump_there::<MainErr, _>(|(video_texture, pts)| {
            if next_frame(&mut packet_iterator, decoder, frame)? {
                update_frame_to_texture(frame, video_texture)?;
                *pts = frame.pts();
                Ok(true)
            } else {
                Ok(false)
            }
        })?
    {
        if seek_mode != SeekMode::Precise {
            break;
        }
        if let &Some(pts) = pts {
            if timestamp < pts {
                // The last decoded frame exceeds the seek target,
                // so we should use the previous one
                frame_buffer.backward();
                break;
            }
        }
    }
    Ok(packet_iterator)
}

fn update_frame_to_texture(
    frame: &frame::Video,
    video_texture: &mut Texture,
) -> Result<(), MainErr> {
    let (_, format) = get_sdl_pix_fmt_and_blendmode(frame.format());
    assert!(format == PixelFormatEnum::IYUV && frame.stride(0) > 0);
    video_texture.update_yuv(
        None,
        frame.data(0),
        frame.stride(0),
        frame.data(1),
        frame.stride(1),
        frame.data(2),
        frame.stride(2),
    )?;
    Ok(())
}

struct RingBuffer<T> {
    elements: Vec<T>,
    start: usize,
    end: usize,
    cursor: usize,
}

impl<T> std::fmt::Debug for RingBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RingBuffer")
            .field("elements", &[..])
            .field("start", &self.start)
            .field("end", &self.end)
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl<T> RingBuffer<T> {
    fn try_new<E, F>(len: usize, gen: F) -> Result<Self, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        Ok(Self {
            elements: repeat_with(gen).take(len).collect::<Result<Vec<T>, E>>()?,
            start: 0,
            end: 0,
            cursor: 0,
        })
    }

    fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
        self.cursor = 0;
    }

    fn forward(&mut self) -> bool {
        let next = self.next_index(self.cursor);
        if self.valid_index(next) {
            self.cursor = next;
            true
        } else {
            false
        }
    }

    fn backward(&mut self) -> bool {
        let previous = self.previous_index(self.cursor);
        if self.valid_index(previous) {
            self.cursor = previous;
            true
        } else {
            false
        }
    }

    fn try_append_and_jump_there<E, F>(&mut self, f: F) -> Result<Option<&T>, E>
    where
        F: FnOnce(&mut T) -> Result<bool, E>,
    {
        if f(&mut self.elements[self.end])? {
            self.cursor = self.end;
            self.end = self.next_index(self.end);
            if self.end == self.start {
                self.start = self.next_index(self.start);
            }
            Ok(Some(&self.elements[self.cursor]))
        } else {
            Ok(None)
        }
    }

    fn current(&self) -> Option<&T> {
        self.valid_index(self.cursor)
            .then(|| &self.elements[self.cursor])
    }

    fn previous_index(&self, index: usize) -> usize {
        index.checked_sub(1).unwrap_or(self.elements.len() - 1)
    }

    fn next_index(&self, index: usize) -> usize {
        (index + 1) % self.elements.len()
    }

    fn valid_index(&self, index: usize) -> bool {
        if self.start <= self.end {
            (self.start..self.end).contains(&index)
        } else {
            (self.start..self.elements.len()).contains(&index) || (0..self.end).contains(&index)
        }
    }
}

fn get_scores(config: &Config) -> Option<Score> {
    let score_paths = match config.get::<Vec<PathBuf>>("scores") {
        Ok(v) => v,
        Err(e) => {
            println!("Could not get `scores` config: {:?}", e);
            vec![]
        }
    };
    let mut combined = Score {
        notes: vec![],
        bar_lines: vec![],
        branches: vec![],
        branch_events: vec![],
    };
    let mut score_added = false;
    for score_path in score_paths {
        let song = match load_tja_from_file(&score_path) {
            Ok(s) => s,
            Err(e) => {
                println!("Error when loading tja file: {:?}", e);
                continue;
            }
        };
        let score = match song.score {
            Some(s) => s,
            None => {
                println!("Score not found in: {:?}", score_path);
                continue;
            }
        };
        let score = GameManager::new(&score).score;
        combined.notes.extend(score.notes);
        combined.bar_lines.extend(score.bar_lines);
        combined.branches.extend(score.branches);
        combined.branch_events.extend(score.branch_events);
        score_added = true;
    }

    combined.notes.sort_by_key(|n| OrderedFloat::from(n.time));
    combined
        .bar_lines
        .sort_by_key(|n| OrderedFloat::from(n.time));
    combined
        .branches
        .sort_by_key(|n| OrderedFloat::from(n.switch_time));
    combined
        .branch_events
        .sort_by_key(|n| OrderedFloat::from(n.time));

    score_added.then(|| combined)
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct ScoreTimeDeltas(BTreeMap<i64, f64>);
impl ScoreTimeDeltas {
    fn get(&self, pts: i64) -> f64 {
        if let Some((_, &t)) = self.0.range(..=pts).last() {
            t
        } else if let Some((_, &t)) = self.0.range(..).next() {
            t
        } else {
            0.0
        }
    }
}

fn detect_notes(
    canvas: &mut WindowCanvas,
    texture_creator: &TextureCreator<WindowContext>,
    font: &Font,
    frame: &frame::Video,
    focus_y: i32,
) -> Result<Vec<DetectedNote>, MainErr> {
    if frame.planes() == 0 {
        return Ok(vec![]);
    }

    let y = if focus_y < 540 { 600 } else { 60 };
    canvas.set_draw_color(Color::BLACK);
    canvas.fill_rect(Rect::new(0, y, 1920, 256))?;
    canvas.set_draw_color(Color::WHITE);
    canvas.draw_line((0, y + 255 - 200), (1920, y + 255 - 200))?;

    let s = frame.stride(0);

    let result = detect_note_positions(frame);

    for note in &result.notes {
        let rect = Rect::new(
            note.left.2 as i32,
            y + 255 - 200 - 2,
            (note.right.1 as i32 - note.left.2 as i32) as u32,
            5,
        );
        canvas.set_draw_color(get_single_note_color(note.kind));
        canvas.fill_rect(rect)?;

        let text_surface = font
            .render(&note.note_x().to_string())
            .solid(Color::YELLOW)?;
        let (w, h) = (text_surface.width(), text_surface.height());
        let text_texture = texture_creator.create_texture_from_surface(text_surface)?;
        let rect = Rect::new(note.left.2 as i32, y + 255 - 200 - 5, w, h);
        canvas.copy(&text_texture, None, rect)?;
    }

    for ((i, color), rate) in (0..frame.planes())
        .take(3)
        .zip([Color::GREEN, Color::BLUE, Color::RED])
        .zip([1usize, 2, 2])
    {
        let k = (focus_y as usize / rate) * (s / rate);
        let data = &frame.data(i)[k..];
        let lines = (0..s / rate)
            .map(|i| Point::new((i * rate) as i32, y + 255 - data[i] as i32))
            .collect_vec();
        canvas.set_draw_color(color);
        canvas.draw_lines(&lines[..])?;
    }

    Ok(result.notes)
}
