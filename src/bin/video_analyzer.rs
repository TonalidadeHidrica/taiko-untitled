use config::Config;
use ffmpeg4::codec::decoder;
use ffmpeg4::format::context::input::PacketIter;
use ffmpeg4::sys::{av_seek_frame, AVSEEK_FLAG_BACKWARD};
use ffmpeg4::util::{frame, media};
use ffmpeg4::{format, Packet, Rational};
use itertools::Itertools;
use sdl2::event::Event;
use sdl2::image::LoadTexture;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::{Point, Rect};
use sdl2::render::Texture;
use std::cmp::max;
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Instant;
use taiko_untitled::assets::Assets;
use taiko_untitled::ffmpeg_utils::get_sdl_pix_fmt_and_blendmode;
use taiko_untitled::game::draw_game_notes;
use taiko_untitled::game_graphics::{draw_note, game_rect};
use taiko_untitled::game_manager::GameManager;
use taiko_untitled::structs::{NoteColor, NoteSize, SingleNoteKind};
use taiko_untitled::tja::load_tja_from_file;
use taiko_untitled::video_analyzer_assets::Textures;

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

// struct VideoReader<'a> {
//     // input_context: &'a context::Input,
//     frame: frame::Video,
//     packet_iterator: FilteredPacketIter<'a>,
//     decoder: decoder::Video,
//     stream_index: usize,
//     time_base: Rational,
// }

struct FilteredPacketIter<'a>(PacketIter<'a>, usize);
impl<'a> Iterator for FilteredPacketIter<'a> {
    type Item = Packet;
    fn next(&mut self) -> Option<Self::Item> {
        for (stream, packet) in &mut self.0 {
            if stream.index() == self.1 {
                return Some(packet);
            }
        }
        None
    }
}

// impl<'a> VideoReader<'a> {
//     fn new(input_context: &'a mut context::Input) -> Result<VideoReader<'a>, MainErr> {
//         let stream = input_context
//             .streams()
//             .best(media::Type::Video)
//             .ok_or("No video stream found")?;
//         let stream_index = stream.index();
//
//         let time_base = stream.time_base();
//
//         let mut decoder = stream.codec().decoder().video()?;
//         decoder.set_parameters(stream.parameters())?;
//
//         let packet_iterator = FilteredPacketIter(input_context.packets(), stream_index);
//
//         Ok(VideoReader {
//             // input_context,
//             decoder,
//             frame: frame::Video::empty(),
//             packet_iterator,
//             time_base,
//             stream_index,
//         })
//     }
//
//     fn next_frame(&mut self) -> Result<Option<&frame::Video>, MainErr> {
//         if let Some(packet) = self.packet_iterator.next() {
//             if self.decoder.decode(&packet, &mut self.frame)? {
//                 return Ok(Some(&self.frame));
//             }
//         }
//         Ok(None)
//     }
//
//     fn seek(
//         stream_index: usize,
//         time_base: Rational,
//         input_context: &'a mut context::Input,
//         time: i32,
//     ) -> Result<VideoReader<'a>, MainErr> {
//         let timestamp = Rational::new(time, 1) / time_base;
//         let timestamp = f64::from(timestamp).trunc() as _;
//         unsafe {
//             if av_seek_frame(
//                 input_context.as_mut_ptr(),
//                 stream_index as _,
//                 timestamp,
//                 AVSEEK_FLAG_BACKWARD,
//             ) < 0
//             {
//                 return Err(MainErr(String::from("Failed to seek")));
//             }
//         }
//         VideoReader::new(input_context)
//     }
// }

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

    let score = config
        .get::<PathBuf>("score")
        .ok()
        .map(|f| load_tja_from_file(&f))
        .transpose()
        .map_err(|e| MainErr(format!("{:?}", e)))?
        .map(|song| {
            song.score
                .ok_or_else(|| MainErr("Score not found in the tja file".into()))
        })
        .transpose()?
        .map(|score| GameManager::new(&score).score);

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
    let mut video_texture =
        texture_creator.create_texture_streaming(Some(PixelFormatEnum::IYUV), width, height)?;
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
    let (mut texture_x, mut texture_y) = (500, 288);
    let frame_id = -1; // TODO: remove this variable
    let mut texture_width = notes_texture.as_ref().map_or(1, |t| t.query().width);
    let mut draw_gauge = false;
    let mut score_time_delta = config.get_float("score_time_delta").unwrap_or(0.0);
    let mut show_score = true;
    let mut note_kind = None;
    let mut note_x = 500;

    let mut pts = 0;

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
                        Keycode::Space => do_play = !do_play,
                        Keycode::Z => zoom_proportion += 1,
                        Keycode::X => zoom_proportion = max(1, zoom_proportion - 1),
                        Keycode::M => mouse_util.show_cursor(!mouse_util.is_cursor_showing()),
                        Keycode::F => fixed = !fixed,
                        Keycode::S if alt => speed_up = !speed_up,
                        Keycode::C => cursor_mode = !cursor_mode,
                        Keycode::G => draw_gauge = !draw_gauge,
                        Keycode::Q => texture_width = max(1, texture_width - 1),
                        Keycode::W => texture_width += 1,
                        Keycode::L => {
                            config.refresh()?;
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
                                _ if shift => 10,
                                _ => 1,
                            };
                            if alt {
                                texture_x += sign * amount;
                            } else {
                                note_x += sign * amount;
                            }
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
                            if next_frame(&mut packet_iterator, &mut decoder, &mut frame)? {
                                update_frame_to_texture(&frame, &mut video_texture)?;
                                if let Some(t) = frame.pts() {
                                    pts = t;
                                }
                            }
                        }
                        Keycode::J | Keycode::K => {
                            score_time_delta += match keycode {
                                Keycode::J => -1.,
                                _ => 1.,
                            } * match () {
                                _ if alt => 1.,
                                _ if shift => 0.01,
                                _ => 0.0001,
                            };
                        }
                        Keycode::PageDown => {
                            packet_iterator = seek(
                                SeekTarget::Timestamp(pts.saturating_sub(1)),
                                time_base,
                                &mut input_context,
                                stream_index,
                                &mut decoder,
                                &mut frame,
                                &mut video_texture,
                                &mut pts,
                            )?;
                        }
                        Keycode::PageUp => {
                            let timestamp_delta = Rational::new(10, 1) / time_base;
                            let target_timestamp =
                                pts + (timestamp_delta.0 as f64 / timestamp_delta.1 as f64) as i64;
                            packet_iterator = seek(
                                SeekTarget::Timestamp(target_timestamp),
                                time_base,
                                &mut input_context,
                                stream_index,
                                &mut decoder,
                                &mut frame,
                                &mut video_texture,
                                &mut pts,
                            )?;
                        }
                        Keycode::Num2 if shift => show_score = !show_score,
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
                            println!("{:.3} {}", time, note_x);
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
            if speed_up {
                for _ in 0..5 {
                    // TODO: do we really have to decode the frame?
                    next_frame(&mut packet_iterator, &mut decoder, &mut frame)?;
                }
            }
            // TODO: duplicate
            if next_frame(&mut packet_iterator, &mut decoder, &mut frame)? {
                update_frame_to_texture(&frame, &mut video_texture)?;
                if let Some(t) = frame.pts() {
                    pts = t;
                }
            }
        }

        canvas.copy(&video_texture, None, affine(0, 0, width, height))?;

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
            if score.is_some() && show_score || note_kind.is_some() {
                canvas.set_draw_color((28, 28, 28));
                canvas.fill_rect(rect)?;
            }
            if let Some(note_kind) = note_kind {
                draw_note(&mut canvas, &game_assets, &note_kind, note_x, game_rect().y)
                    .map_err(debug_to_err())?;
            }
            if let (Some(score), true) = (&score, show_score) {
                let time = f64::from(Rational::new(pts as i32, 1) * time_base) + score_time_delta;
                draw_game_notes(&mut canvas, &game_assets, time, score)
                    .map_err(|e| MainErr(format!("{:?}", e)))?;
            }
        }
        canvas.set_clip_rect(None);

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
            format!("score_time_delta = {:.4}", score_time_delta),
            // format!("YUV = {:?}",
            //     current_frame.and_then(|frame|
            //         (0..3).map(|i|
            //             current_frame.data(i)[focus_x + focus_y * (width as i32)]
            //         ).collect_vec()
            // ),
        ];
        let mut current_top = 0;
        for info in &infos {
            let text_surface = font.render(info).solid(Color::GREEN)?;
            let text_width = text_surface.width();
            let text_height = text_surface.height();
            let text_texture = texture_creator.create_texture_from_surface(text_surface)?;
            // canvas.copy(
            //     &text_texture,
            //     None,
            //     Some(Rect::new(0, 0, text_width, text_height)),
            // )?;
            canvas.copy(
                &text_texture,
                None,
                Some(Rect::new(
                    (width - text_width) as i32,
                    current_top,
                    text_width,
                    text_height,
                )),
            )?;
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

#[allow(clippy::too_many_arguments)]
fn seek<'a>(
    seek_target: SeekTarget,
    time_base: Rational,
    input_context: &'a mut format::context::Input,
    stream_index: usize,
    decoder: &mut decoder::Video,
    frame: &mut frame::Video,
    video_texture: &mut Texture,
    pts: &mut i64,
) -> Result<FilteredPacketIter<'a>, MainErr> {
    let timestamp = match seek_target {
        SeekTarget::Milliseconds(time_ms) => {
            let timestamp = Rational::new(time_ms, 1000) / time_base;
            f64::from(timestamp).trunc() as _
        }
        SeekTarget::Timestamp(t) => t,
    };
    let res = unsafe {
        av_seek_frame(
            input_context.as_mut_ptr(),
            stream_index as _,
            timestamp,
            AVSEEK_FLAG_BACKWARD,
        )
    };
    if res < 0 {
        return Err(MainErr(String::from("Failed to seek")));
    }
    let mut packet_iterator = FilteredPacketIter(input_context.packets(), stream_index);
    decoder.flush();
    if next_frame(&mut packet_iterator, decoder, frame)? {
        update_frame_to_texture(&*frame, video_texture)?;
        if let Some(t) = frame.pts() {
            *pts = t;
        }
    }
    Ok(packet_iterator)
}

fn next_frame(
    packet_iterator: &mut FilteredPacketIter,
    decoder: &mut decoder::Video,
    frame: &mut frame::Video,
) -> Result<bool, MainErr> {
    // We assume that a frame is always decoded.
    for packet in packet_iterator.by_ref() {
        if decoder.decode(&packet, frame)? {
            return Ok(true);
        }
    }
    Ok(false)
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
