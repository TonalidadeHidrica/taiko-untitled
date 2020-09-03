use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use rodio::source::UniformSourceIterator;
use rodio::Source;
use std::io::BufReader;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::{Duration, Instant};

pub struct AudioManager {
    _stream: Stream,
    sender_to_audio: Sender<MessageToAudio>,
    playback_position: Arc<Mutex<Option<PlaybackPosition>>>,
}

struct PlaybackPosition {
    instant: Instant,
    music_position: f64,
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
        let playback_position = Arc::new(Mutex::new(None));

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
                let decoder = UniformSourceIterator::<_, f32>::new(
                    decoder,
                    stream_config.channels,
                    stream_config.sample_rate.0,
                );
                Some(decoder)
            }
            _ => None,
        };

        let state = AudioThreadState::new(
            stream_config.clone(),
            music,
            receiver_to_audio,
            Arc::downgrade(&playback_position.clone()),
        );
        let stream = device
            .build_output_stream(&stream_config, state.data_callback(), |err| {
                eprintln!("an error occurred on stream: {:?}", err)
            })
            .unwrap();
        stream.play().unwrap();

        AudioManager {
            _stream: stream,
            sender_to_audio,
            playback_position,
        }
    }

    pub fn play(&self) {
        // TODO error propagation
        self.sender_to_audio.send(MessageToAudio::Play).unwrap();
    }

    pub fn music_position(&self) -> Option<f64> {
        let playback_position = self.playback_position.lock().unwrap();
        playback_position.as_ref().map(|playback_position| {
            let now = Instant::now();
            let diff = if now > playback_position.instant {
                (now - playback_position.instant).as_secs_f64()
            } else {
                -(playback_position.instant - now).as_secs_f64()
            };
            diff + playback_position.music_position
        })
    }
}

struct AudioThreadState<T, I> {
    stream_config: StreamConfig,
    music: Option<I>,
    receiver_to_audio: mpsc::Receiver<MessageToAudio>,
    playing: bool,
    played_sample_count: usize,
    playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
    _marker: PhantomData<fn() -> T>,
}

impl<S, I> AudioThreadState<S, I>
where
    S: rodio::Sample,
    I: Iterator<Item = S>,
{
    pub fn new(
        stream_config: StreamConfig,
        music: Option<I>,
        receiver_to_audio: mpsc::Receiver<MessageToAudio>,
        playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
    ) -> Self {
        AudioThreadState {
            stream_config,
            music,
            receiver_to_audio,
            playing: false,
            played_sample_count: 0,
            playback_position_ptr,
            _marker: PhantomData,
        }
    }

    fn data_callback(mut self) -> impl FnMut(&mut [S], &cpal::OutputCallbackInfo) {
        move |output, callback_info| {
            self.playing = self.playing || self.receiver_to_audio.try_iter().count() > 0;

            if self.playing {
                let timestamp = callback_info.timestamp();
                let instant = Instant::now()
                    + timestamp
                        .playback
                        .duration_since(&timestamp.callback)
                        .unwrap_or_else(|| Duration::from_nanos(0));
                let music_position =
                    self.played_sample_count as f64 / self.stream_config.sample_rate.0 as f64;

                if let Some(playback_position) = self.playback_position_ptr.upgrade() {
                    let mut playback_position = playback_position.lock().unwrap();
                    *playback_position = Some(PlaybackPosition {
                        instant,
                        music_position,
                    });
                }

                self.played_sample_count += output.len() / (self.stream_config.channels as usize);
            }

            for out in output.into_iter() {
                let next = if let (Some(ref mut music), true) = (&mut self.music, self.playing) {
                    music.next()
                } else {
                    None
                };
                *out = next.unwrap_or_else(S::zero_value);
            }
        }
    }
}
