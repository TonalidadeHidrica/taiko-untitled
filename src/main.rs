use config::{Config, ConfigError};
use ffmpeg4::format;
use ffmpeg4::util::{frame, media};
use std::path::PathBuf;
use sdl2::pixels::PixelFormatEnum;
use taiko_untitled::ffmpeg_utils::get_sdl_pix_fmt_and_blendmode;

#[derive(Debug)]
enum MainErr {
    ConfigError(ConfigError),
    StringError(String),
    FFMpegError(ffmpeg4::Error),
}

impl From<&str> for MainErr {
    fn from(error: &str) -> Self {
        MainErr::StringError(error.into())
    }
}

impl From<ConfigError> for MainErr {
    fn from(error: ConfigError) -> Self {
        MainErr::ConfigError(error)
    }
}

impl From<ffmpeg4::Error> for MainErr {
    fn from(error: ffmpeg4::Error) -> Self {
        MainErr::FFMpegError(error)
    }
}

fn main() -> Result<(), MainErr> {
    let mut config = Config::default();
    let config = config.merge(config::File::with_name("config.toml"))?;
    let video_path = config.get::<PathBuf>("video")?;

    let mut input_context = format::input(&video_path)?;
    let stream = input_context
        .streams()
        .best(media::Type::Video)
        .ok_or("No video stream found")?;
    let stream_index = stream.index();

    let mut decoder = stream.codec().decoder().video()?;
    decoder.set_parameters(stream.parameters())?;

    let mut frame = frame::Video::empty();
    for (_, packet) in input_context
        .packets()
        .filter(|(x, _)| x.index() == stream_index)
    {
        if !decoder.decode(&packet, &mut frame)? {
            continue;
        }
        let (_, format) = get_sdl_pix_fmt_and_blendmode(frame.format());
        assert!(format == PixelFormatEnum::IYUV && frame.stride(0) > 0);
    }

    Ok(())
}
