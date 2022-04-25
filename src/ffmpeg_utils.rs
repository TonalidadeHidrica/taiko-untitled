use ffmpeg4::{decoder, format::context::input::PacketIter, frame, Packet};

pub fn get_sdl_pix_fmt_and_blendmode(
    pixel_format: ffmpeg4::util::format::pixel::Pixel,
) -> (sdl2::render::BlendMode, sdl2::pixels::PixelFormatEnum) {
    use ffmpeg4::util::format::pixel::Pixel as F;
    use sdl2::pixels::PixelFormatEnum as S;
    use sdl2::render::BlendMode as B;

    let sdl_blendmode = match pixel_format {
        F::RGB32 | F::RGB32_1 | F::BGR32 | F::BGR32_1 => B::Blend,
        _ => B::None,
    };

    let sdl_pix_fmt = match pixel_format {
        F::RGB8 => S::RGB332,
        F::RGB444 => S::RGB444,
        F::RGB555 => S::RGB555,
        F::BGR555 => S::BGR555,
        F::RGB565 => S::RGB565,
        F::BGR565 => S::BGR565,
        F::RGB24 => S::RGB24,
        F::BGR24 => S::BGR24,
        F::ZRGB32 => S::RGB888,
        F::ZBGR32 => S::BGR888,
        F::RGBZ if cfg!(target_endian = "big") => S::RGBX8888,
        F::ZBGR if cfg!(target_endian = "little") => S::RGBX8888,
        F::BGRZ if cfg!(target_endian = "big") => S::BGRX8888,
        F::ZRGB if cfg!(target_endian = "little") => S::BGRX8888,
        F::RGB32 => S::ARGB8888,
        F::RGB32_1 => S::RGBA8888,
        F::BGR32 => S::ABGR8888,
        F::BGR32_1 => S::BGRA8888,
        F::YUV420P => S::IYUV,
        F::YUYV422 => S::YUY2,
        F::UYVY422 => S::UYVY,
        F::None => S::Unknown,
        _ => S::Unknown,
    };

    (sdl_blendmode, sdl_pix_fmt)
}

pub struct FilteredPacketIter<'a>(pub PacketIter<'a>, pub usize);
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

pub fn next_frame(
    packet_iterator: &mut FilteredPacketIter,
    decoder: &mut decoder::Video,
    frame: &mut frame::Video,
) -> Result<bool, ffmpeg4::Error> {
    // We assume that a frame is always decoded.
    for packet in packet_iterator.by_ref() {
        if decoder.decode(&packet, frame)? {
            return Ok(true);
        }
    }
    Ok(false)
}
