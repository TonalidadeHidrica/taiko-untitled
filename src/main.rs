use config::Config;
use ffmpeg4::codec::decoder;
use ffmpeg4::format::context;
use ffmpeg4::util::{frame, media};
use ffmpeg4::{format, Packet};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::{Point, Rect};
use std::cmp::max;
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Instant;
use taiko_untitled::ffmpeg_utils::get_sdl_pix_fmt_and_blendmode;

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

struct VideoReader<'a> {
    // input_context: &'a context::Input,
    frame: frame::Video,
    packet_iterator: Box<dyn Iterator<Item = Packet> + 'a>,
    decoder: decoder::Video,
}

impl<'a> VideoReader<'a> {
    fn new(input_context: &'a mut context::Input) -> Result<VideoReader<'a>, MainErr> {
        let stream = input_context
            .streams()
            .best(media::Type::Video)
            .ok_or("No video stream found")?;
        let stream_index = stream.index();

        let mut decoder = stream.codec().decoder().video()?;
        decoder.set_parameters(stream.parameters())?;

        let packet_iterator = input_context
            .packets()
            .filter(move |(x, _)| x.index() == stream_index)
            .map(|p| p.1);

        Ok(VideoReader {
            // input_context,
            decoder,
            frame: frame::Video::empty(),
            packet_iterator: Box::new(packet_iterator),
        })
    }

    fn next_frame(&mut self) -> Result<Option<&frame::Video>, MainErr> {
        if let Some(packet) = self.packet_iterator.next() {
            if self.decoder.decode(&packet, &mut self.frame)? {
                return Ok(Some(&self.frame));
            }
        }
        Ok(None)
    }
}

fn main() -> Result<(), MainErr> {
    let width = 1280u32;
    let height = 720u32;

    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;
    let video_path = config.get::<PathBuf>("video")?;
    let font_path = config.get::<PathBuf>("font")?;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Main Window", width, height)
        .build()
        .map_err(|x| x.to_string())?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;
    let mouse_util = sdl_context.mouse();
    let ttf_context = sdl2::ttf::init()?;

    let texture_creator = canvas.texture_creator();
    let mut texture =
        texture_creator.create_texture_streaming(Some(PixelFormatEnum::IYUV), width, height)?;

    let font = ttf_context.load_font(font_path, 24)?;

    let mut input_context = format::input(&video_path)?;
    let mut video_reader = VideoReader::new(&mut input_context)?;

    let mut do_play = false;
    let mut zoom_proportion = 1;
    let mut focus_x = 0;
    let mut focus_y = 0;

    let start = Instant::now();

    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    Keycode::Space => do_play = !do_play,
                    Keycode::Z => zoom_proportion += 1,
                    Keycode::X => zoom_proportion = max(1, zoom_proportion - 1),
                    Keycode::M => mouse_util.show_cursor(!mouse_util.is_cursor_showing()),
                    _ => {}
                },
                Event::MouseMotion { x, y, .. } => {
                    focus_x = x;
                    focus_y = y;
                }
                _ => {}
            }
        }

        if do_play {
            if let Some(frame) = video_reader.next_frame()? {
                let (_, format) = get_sdl_pix_fmt_and_blendmode(frame.format());
                assert!(format == PixelFormatEnum::IYUV && frame.stride(0) > 0);
                texture.update_yuv(
                    None,
                    frame.data(0),
                    frame.stride(0),
                    frame.data(1),
                    frame.stride(1),
                    frame.data(2),
                    frame.stride(2),
                )?;
            }
        }

        canvas.copy(
            &texture,
            None,
            Some(Rect::new(
                focus_x * (1 - zoom_proportion as i32),
                focus_y * (1 - zoom_proportion as i32),
                width * zoom_proportion,
                height * zoom_proportion,
            )),
        )?;

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

        let text_surface = font
            .render(&format!("({}, {})", focus_x, focus_y))
            .solid(Color::GREEN)?;
        let width = text_surface.width();
        let height = text_surface.height();
        let text_texture = texture_creator.create_texture_from_surface(text_surface)?;
        canvas.copy(&text_texture, None, Some(Rect::new(0, 0, width, height)))?;

        canvas.present();

        // std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
    }

    Ok(())
}
