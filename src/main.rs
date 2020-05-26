use config::Config;
use stainless_ffmpeg::format_context::FormatContext;
use stainless_ffmpeg::stream::Stream;
use std::path::PathBuf;
use stainless_ffmpeg_sys::AVMediaType::AVMEDIA_TYPE_VIDEO;
use stainless_ffmpeg::video_decoder::VideoDecoder;

fn main() -> Result<(), String> {
    let mut image_paths = Config::default();
    let image_paths = image_paths
        .merge(config::File::with_name("config.toml"))
        .map_err(|x| x.to_string())?;
    let image_path = image_paths
        .get::<PathBuf>("video")
        .map_err(|x| x.to_string())?;

    let mut format_context = FormatContext::new(image_path.to_str().unwrap())?;
    format_context.open_input()?;
    let video_stream_idx: isize = (0..format_context.get_nb_streams() as isize)
        .filter(|x| format_context.get_stream_type(*x) == AVMEDIA_TYPE_VIDEO)
        .next().ok_or("No video stream found")?;
    // let video_stream = Stream::new(format_context.get_stream(video_stream_idx));
    let video_decoder = VideoDecoder::new(String::new(), &format_context, video_stream_idx)?;
    while let Ok(packet) = format_context.next_packet() {
        if packet.get_stream_index() != video_stream_idx {
            continue
        }
        let frame = video_decoder.decode(&packet)?;
        println!("Frame: pts={}", frame.get_pts());
    }

    Ok(())
}
