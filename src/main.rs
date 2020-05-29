use config::Config;
use ffmpeg4::codec::decoder;
use ffmpeg4::format::context;
use ffmpeg4::util::{frame, media};
use ffmpeg4::{format, Packet};
use sdl2::event::Event;
use sdl2::pixels::PixelFormatEnum;
use std::fmt::Debug;
use std::path::PathBuf;
use taiko_untitled::ffmpeg_utils::get_sdl_pix_fmt_and_blendmode;
use sdl2::keyboard::Keycode;

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
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;
    let video_path = config.get::<PathBuf>("video")?;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Main Window", 1280, 720)
        .build()
        .map_err(|x| x.to_string())?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|x| x.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;

    let texture_creator = canvas.texture_creator();
    let mut texture =
        texture_creator.create_texture_streaming(Some(PixelFormatEnum::IYUV), 1280, 720)?;

    let mut input_context = format::input(&video_path)?;
    let mut video_reader = VideoReader::new(&mut input_context)?;

    let mut do_play = false;

    'main: loop {
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
                canvas.copy(&texture, None, None)?;
            }
        }

        canvas.present();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown { keycode: Some(Keycode::Space), .. } => do_play = !do_play,
                _ => {}
            }
        }

        // std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
    }

    Ok(())
}
