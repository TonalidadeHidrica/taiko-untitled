use rodio::Decoder;
use std::io::{Read, Seek};

pub type SeekResult = Result<(), String>;

pub trait Seekable {
    fn seek(&mut self, sample: u64) -> SeekResult;
}

impl<R: Read + Seek + Send + 'static> Seekable for Decoder<R> {
    fn seek(&mut self, sample: u64) -> SeekResult {
        Decoder::seek(self, sample).map_err(|()| "Oh no".to_owned())
    }
}
