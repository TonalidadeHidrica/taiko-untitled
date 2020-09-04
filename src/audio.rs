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
use retain_mut::RetainMut;

pub struct AudioManager {
    _stream: Stream,
    stream_config: StreamConfig,
    sender_to_audio: Sender<MessageToAudio>,
    playback_position: Arc<Mutex<Option<PlaybackPosition>>>,
}

struct PlaybackPosition {
    instant: Instant,
    music_position: f64,
}

enum MessageToAudio {
    Play,
    AddPlay(Box<dyn Source<Item=f32> + Send>),
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
            .build_output_stream(&stream_config, state.data_callback::<f32>(), |err| {
                eprintln!("an error occurred on stream: {:?}", err)
            })
            .unwrap();
        stream.play().unwrap();

        AudioManager {
            _stream: stream,
            stream_config,
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

    pub fn add_play<S>(&self, source: S)
    where
        S: Source + 'static + Send,
        <S as Iterator>::Item: rodio::Sample + Send,
    {
        let source = UniformSourceIterator::<_, f32>::new(
            source,
            self.stream_config.channels,
            self.stream_config.sample_rate.0,
        );
        self.sender_to_audio.send(MessageToAudio::AddPlay(Box::new(source))).unwrap();
    }
}

struct AudioThreadState<I> {
    stream_config: StreamConfig,
    music: Option<I>,
    sound_effects: Vec<Box<dyn Source<Item=f32> + Send>>,
    receiver_to_audio: mpsc::Receiver<MessageToAudio>,
    playing: bool,
    played_sample_count: usize,
    playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
    // _marker: PhantomData<fn() -> T>,
}

impl<I> AudioThreadState<I>
where
    I: Iterator<Item = f32>,
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
            sound_effects: Vec::new(),
            receiver_to_audio,
            playing: false,
            played_sample_count: 0,
            playback_position_ptr,
            // _marker: PhantomData,
        }
    }

    fn data_callback<S>(mut self) -> impl FnMut(&mut [S], &cpal::OutputCallbackInfo)
    where
        S: rodio::Sample,
    {
        move |output, callback_info| {
            let start = Instant::now();

            for message in self.receiver_to_audio.try_iter() {
                match message {
                    MessageToAudio::Play => self.playing = true,
                    MessageToAudio::AddPlay(source) => {
                        self.sound_effects.push(source);
                    }
                }
            }

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

            let mut first = true;
            for out in output.into_iter() {
                let mut next = if let (Some(ref mut music), true) = (&mut self.music, self.playing) {
                    music.next()
                } else {
                    None
                }.unwrap_or(0.0);

                self.sound_effects.retain_mut(|source| {
                    let start = if first {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    let ret = match source.next() {
                        Some(value) => {
                            next += value;
                            true
                        }
                        None => false
                    };
                    if let Some(start) = start {
                        // dbg!(Instant::now() - start);
                    }
                    ret
                });
                *out = S::from(&next);

                first = false;
            }

            // dbg!(self.sound_effects.len());
            // dbg!(Instant::now() - start);
        }
    }
}
