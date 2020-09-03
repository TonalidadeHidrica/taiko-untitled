use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use rodio::source::UniformSourceIterator;
use rodio::Source;
use std::io::BufReader;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::Sender;

pub struct AudioManager {
    _stream: Stream,
    sender_to_audio: Sender<MessageToAudio>,
}

enum MessageToAudio {
    Play,
}

impl AudioManager {
    pub fn new<P>(wave: Option<P>) -> AudioManager
    where
        P: AsRef<Path>,
    {
        let (sender_to_audio, receiver_to_audio) = mpsc::channel();

        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let mut supported_configs_range = device.supported_output_configs().unwrap();
        let supported_config = supported_configs_range
            .next()
            .unwrap()
            .with_max_sample_rate();
        let stream_config: StreamConfig = supported_config.into();
        dbg!(&stream_config);

        let music = match wave.as_ref() {
            Some(wave) => {
                let file = std::fs::File::open(wave).unwrap();
                let decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
                let decoder = decoder.convert_samples::<f32>();
                let decoder = UniformSourceIterator::new(
                    decoder,
                    stream_config.channels,
                    stream_config.sample_rate.0,
                );
                Some(decoder)
            }
            _ => None,
        };

        let state = AudioThreadState::new(music, receiver_to_audio);
        let stream = device
            .build_output_stream(&stream_config, state.data_callback(), |err| {
                eprintln!("an error occurred on stream: {:?}", err)
            })
            .unwrap();
        stream.play().unwrap();

        AudioManager {
            _stream: stream,
            sender_to_audio,
        }
    }

    pub fn play(&self) {
        // TODO error propagation
        self.sender_to_audio.send(MessageToAudio::Play).unwrap();
    }
}

struct AudioThreadState<I> {
    music: Option<I>,
    receiver_to_audio: mpsc::Receiver<MessageToAudio>,
    playing: bool,
}

impl<I> AudioThreadState<I>
where
    I: Iterator<Item = f32>,
{
    pub fn new(music: Option<I>, receiver_to_audio: mpsc::Receiver<MessageToAudio>) -> Self {
        AudioThreadState {
            music,
            receiver_to_audio,
            playing: false,
        }
    }

    fn data_callback(mut self) -> impl FnMut(&mut [f32], &cpal::OutputCallbackInfo) {
        // TODO `f32` actually depends on platforms
        move |output, _callback_info| {
            // let cpal::OutputStreamTimestamp { ref callback, ref playback } = callback_info.timestamp();
            self.playing = self.playing || self.receiver_to_audio.try_iter().count() > 0;
            for out in output {
                let next = if let (Some(ref mut music), true) = (&mut self.music, self.playing) {
                    music.next()
                } else {
                    None
                };
                *out = next.unwrap_or(0.0);
            }
        }
    }
}
