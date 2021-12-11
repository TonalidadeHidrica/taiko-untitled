use crate::errors::{CpalOrRodioError, TaikoError, TaikoErrorCause};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ChannelCount, SampleFormat, SampleRate, Stream, StreamConfig};
use itertools::Itertools;
use retain_mut::RetainMut;
use rodio::source::UniformSourceIterator;
use rodio::{Decoder, Source};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};
use universal_audio_decoder::{new_uniform_source_iterator, TrueUniformSourceIterator};

pub struct AudioManager<T> {
    pub stream_config: StreamConfig,
    sender_to_audio: Sender<MessageToAudio<T>>,
    drop_sender: Sender<()>,
    pub sound_effect_receiver: Receiver<T>,
    playback_position: Arc<Mutex<PlaybackPosition>>,
}

enum PlaybackPosition {
    NotStarted,
    Seeking {
        music_position: f64,
    },
    Paused {
        music_position: f64,
    },
    Playing {
        instant: Instant,
        music_position: f64,
        play_speed: f64,
    },
}

enum MessageToAudio<T> {
    Play,
    Pause,
    Seek(f64),
    LoadMusic(PathBuf),
    AddPlay(SoundBufferSource),

    SetMusicVolume(f32),
    SetPlaySpeed(f64),

    AddSchedules(Vec<SoundEffectSchedule<T>>),
    CleanSchedules,
    SwitchScheduled(bool),
}

impl<T: Send + 'static> AudioManager<T> {
    pub fn new() -> Result<AudioManager<T>, TaikoError> {
        let (sender_to_audio, receiver_to_audio) = mpsc::channel();
        let (stream_config_sender, stream_config_receiver) = mpsc::channel();
        let (drop_sender, drop_receiver) = mpsc::channel();
        let (sound_effect_sender, sound_effect_receiver) = mpsc::channel();
        let playback_position = Arc::new(Mutex::new(PlaybackPosition::NotStarted));

        let playback_position_ptr = Arc::downgrade(&playback_position);
        thread::spawn(move || {
            match stream_thread(
                receiver_to_audio,
                sound_effect_sender,
                playback_position_ptr,
            ) {
                Err(err) => {
                    if stream_config_sender.send(Err(err)).is_err() {
                        eprintln!("Failed to send error info to main thread.");
                    }
                }
                Ok((stream_config, _stream)) => {
                    if stream_config_sender.send(Ok(stream_config)).is_err() {
                        eprintln!("Failed to send stream config to main thread.");
                    }
                    // preserve stream until "drop" signal is sent from main thread
                    drop_receiver.recv().ok();
                }
            }
        });
        let stream_config = stream_config_receiver.recv().map_err(|_| TaikoError {
            message: "Audio device initialization thread has been stopped".to_string(),
            cause: TaikoErrorCause::None,
        })??;

        Ok(AudioManager {
            stream_config,
            sender_to_audio,
            drop_sender,
            sound_effect_receiver,
            playback_position,
        })
    }

    pub fn load_music<P>(&self, path: P) -> Result<(), TaikoError>
    where
        P: Into<PathBuf>,
    {
        self.sender_to_audio
            .send(MessageToAudio::LoadMusic(path.into()))
            .map_err(|_| TaikoError {
                message: "Failed to load music; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn play(&self) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::Play)
            .map_err(|_| TaikoError {
                message: "Failed to play music; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn pause(&self) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::Pause)
            .map_err(|_| TaikoError {
                message: "Failed to pause music; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn seek(&self, time: f64) -> Result<(), TaikoError> {
        {
            // TODO there should be a better way
            let mut playback_position = self.playback_position.lock().map_err(|_| TaikoError {
                message: "Failed to obtain music position; the audio stream has been panicked"
                    .to_string(),
                cause: TaikoErrorCause::None,
            })?;
            *playback_position = PlaybackPosition::Seeking {
                music_position: time,
            };
        }
        self.sender_to_audio
            .send(MessageToAudio::Seek(time))
            .map_err(|_| TaikoError {
                message: "Failed to seek; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn playing(&self) -> Result<bool, TaikoError> {
        let playback_position = self.playback_position.lock().map_err(|_| TaikoError {
            message: "Failed to obtain music position; the audio stream has been panicked"
                .to_string(),
            cause: TaikoErrorCause::None,
        })?;
        Ok(matches!(
            *playback_position,
            PlaybackPosition::Playing { .. }
        ))
    }

    pub fn set_music_volume(&self, volume: f32) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::SetMusicVolume(volume))
            .map_err(|_| TaikoError {
                message: "Failed to set music volume; the audio stream has been stopped"
                    .to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn set_play_speed(&self, speed: f64) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::SetPlaySpeed(speed))
            .map_err(|_| TaikoError {
                message: "Failed to set play speed; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn add_play(&self, buffer: &SoundBuffer) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::AddPlay(buffer.new_source()))
            .map_err(|_| TaikoError {
                message: "Failed to play a chunk; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn add_play_schedules(
        &self,
        schedules: Vec<SoundEffectSchedule<T>>,
    ) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::AddSchedules(schedules))
            .map_err(|_| TaikoError {
                message: "Failed to push schedules; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn clear_play_schedules(&self) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::CleanSchedules)
            .map_err(|_| TaikoError {
                message: "Failed to clear schedules; the audio stream has been stopped".to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    pub fn set_play_scheduled(&self, enabled: bool) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::SwitchScheduled(enabled))
            .map_err(|_| TaikoError {
                message: "Failed to switch scheduled play; the audio stream has been stopped"
                    .to_string(),
                cause: TaikoErrorCause::None,
            })
    }

    /// Returns error only if the audio stream has been pannicked.
    pub fn music_position(&self) -> Result<Option<f64>, TaikoError> {
        let playback_position = self.playback_position.lock().map_err(|_| TaikoError {
            message: "Failed to obtain music position; the audio stream has been panicked"
                .to_string(),
            cause: TaikoErrorCause::None,
        })?;
        use PlaybackPosition::*;
        let res = match *playback_position {
            Playing {
                music_position,
                instant,
                play_speed,
            } => {
                let now = Instant::now();
                let diff = if now > instant {
                    (now - instant).as_secs_f64() * play_speed
                } else {
                    -(instant - now).as_secs_f64() * play_speed
                };
                Some(music_position + diff)
            }
            Paused { music_position } | Seeking { music_position } => Some(music_position),
            NotStarted => None,
        };
        Ok(res)
    }
}

fn stream_thread<T: Send + 'static>(
    receiver_to_audio: Receiver<MessageToAudio<T>>,
    sound_effect_sender: Sender<T>,
    playback_position_ptr: Weak<Mutex<PlaybackPosition>>,
) -> Result<(StreamConfig, Stream), TaikoError> {
    let host = cpal::default_host();
    if let Ok(devices) = host.devices() {
        for device in devices {
            println!("Name: {:?}", device.name());
            println!("  Default config:");
            println!("    {:?}", device.default_output_config());
            println!("  List of supported configs:");
            if let Ok(configs) = device.supported_output_configs() {
                for config in configs {
                    println!("  - {:?}", config);
                }
            }
        }
    }
    let device = host.default_output_device().ok_or_else(|| TaikoError {
        message: "No default audio output device is available".to_string(),
        cause: TaikoErrorCause::None,
    })?;
    let mut supported_configs_range =
        device.supported_output_configs().map_err(|e| TaikoError {
            message: "Audio output device is no longer valid".to_string(),
            cause: TaikoErrorCause::CpalOrRodioError(
                CpalOrRodioError::SupportedStreamConfigsError(e),
            ),
        })?;
    let supported_config = supported_configs_range
        .next()
        .ok_or_else(|| TaikoError {
            message: "No audio configuration is available".to_string(),
            cause: TaikoErrorCause::None,
        })?
        .with_max_sample_rate();
    dbg!(&supported_config);
    let sample_format = supported_config.sample_format();
    let stream_config: StreamConfig = supported_config.into();
    dbg!(&stream_config);

    let state = AudioThreadState::new(
        stream_config.clone(),
        receiver_to_audio,
        sound_effect_sender,
        playback_position_ptr,
    );
    let error_callback = |err| eprintln!("an error occurred on stream: {:?}", err);
    let stream = match sample_format {
        SampleFormat::F32 => {
            device.build_output_stream(&stream_config, state.data_callback::<f32>(), error_callback)
        }
        SampleFormat::I16 => {
            device.build_output_stream(&stream_config, state.data_callback::<i16>(), error_callback)
        }
        SampleFormat::U16 => {
            device.build_output_stream(&stream_config, state.data_callback::<u16>(), error_callback)
        }
    };
    let stream = stream.map_err(|e| TaikoError {
        message: "Failed to build an audio output stream".to_string(),
        cause: TaikoErrorCause::CpalOrRodioError(CpalOrRodioError::BuildStreamError(e)),
    })?;
    stream.play().map_err(|e| TaikoError {
        message: "Failed to play the audio output stream".to_string(),
        cause: TaikoErrorCause::CpalOrRodioError(CpalOrRodioError::PlayStreamError(e)),
    })?;
    Ok((stream_config, stream))
}

impl<T> Drop for AudioManager<T> {
    fn drop(&mut self) {
        if self.drop_sender.send(()).is_err() {
            eprintln!("Failed to send drop signal to audio stream thread");
        }
    }
}

type MusicSource = TrueUniformSourceIterator<Decoder<BufReader<File>>>;

struct AudioThreadState<T> {
    stream_config: StreamConfig,

    music: Option<MusicSource>,
    sound_effects: Vec<SoundBufferSource>,

    sound_effect_schedules: VecDeque<SoundEffectSchedule<T>>,
    scheduled_play_enabled: bool,

    receiver_to_audio: Receiver<MessageToAudio<T>>,
    sound_effect_sender: Sender<T>,
    playing: bool,
    played_sample_count: usize,
    skip_sample_count: usize,
    playback_position_ptr: Weak<Mutex<PlaybackPosition>>,
    music_volume: f32,
    play_speed: f64,
}

pub struct SoundEffectSchedule<T> {
    pub timestamp: f64,
    pub source: SoundBufferSource,
    pub volume: f64,
    pub response: T,
}

impl<T> AudioThreadState<T> {
    pub fn new(
        stream_config: StreamConfig,
        receiver_to_audio: mpsc::Receiver<MessageToAudio<T>>,
        sound_effect_sender: Sender<T>,
        playback_position_ptr: Weak<Mutex<PlaybackPosition>>,
    ) -> Self {
        AudioThreadState {
            stream_config,
            music: None,
            sound_effects: Vec::new(),

            sound_effect_schedules: VecDeque::new(),
            scheduled_play_enabled: false,

            receiver_to_audio,
            sound_effect_sender,
            playing: false,
            played_sample_count: 0,
            skip_sample_count: 0,
            playback_position_ptr,
            music_volume: 1.0,
            play_speed: 1.0,
        }
    }

    fn data_callback<S>(mut self) -> impl FnMut(&mut [S], &cpal::OutputCallbackInfo)
    where
        S: rodio::Sample,
    {
        move |output, callback_info| {
            for message in self.receiver_to_audio.try_iter() {
                match message {
                    MessageToAudio::Play => self.playing = true,
                    MessageToAudio::Pause => {
                        self.playing = false;
                        self.update_pause_state();
                    }
                    MessageToAudio::Seek(time) => {
                        // TODO refactoring
                        if let Err(e) = if let (Some(music), false) = (&mut self.music, false) {
                            match music.seek(time.max(0.0)).map_err(|e| TaikoError {
                                message: e,
                                cause: TaikoErrorCause::None,
                            }) {
                                Ok(sample_count) => {
                                    self.skip_sample_count = (-time.min(0.0)
                                        * self.stream_config.sample_rate.0 as f64
                                        / self.play_speed)
                                        as usize
                                        * (self.stream_config.channels as usize);
                                    self.played_sample_count = sample_count as usize;
                                    self.update_pause_state();
                                    Ok(())
                                }
                                Err(e) => Err(e),
                            }
                        } else {
                            Err(TaikoError {
                                message: "Music is empty or playing".to_owned(),
                                cause: TaikoErrorCause::None,
                            })
                        } {
                            println!("Failed to seek: {:?}", e);
                        }
                    }
                    MessageToAudio::LoadMusic(path) => {
                        // TODO send error via another channel
                        self.music = Some(self.load_music(path).unwrap())
                    }
                    MessageToAudio::SetMusicVolume(volume) => self.music_volume = volume,
                    MessageToAudio::SetPlaySpeed(speed) => {
                        self.play_speed = speed;
                        if let Some(music) = &mut self.music {
                            music.set_output_sample_rate(
                                self.stream_config.sample_rate.0 as f64 / speed,
                            );
                        };
                    }
                    MessageToAudio::AddPlay(source) => {
                        self.sound_effects.push(source);
                    }
                    MessageToAudio::CleanSchedules => {
                        self.sound_effect_schedules.clear();
                    }
                    MessageToAudio::AddSchedules(mut schedules) => {
                        schedules.sort_unstable_by(|x, y| {
                            x.timestamp.partial_cmp(&y.timestamp).unwrap()
                        });
                        // TODO check for time rollback
                        self.sound_effect_schedules.extend(schedules.into_iter());
                    }
                    MessageToAudio::SwitchScheduled(enabled) => {
                        self.scheduled_play_enabled = enabled;
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

                let playing_sample_count = output.len() / (self.stream_config.channels as usize);

                let music_position_start = self.music_position_start() * self.play_speed;
                let music_position_end = (self.played_sample_count + playing_sample_count) as f64
                    / self.stream_config.sample_rate.0 as f64
                    * self.play_speed;

                if let Some(playback_position) = self.playback_position_ptr.upgrade() {
                    let mut playback_position = playback_position
                        .lock()
                        .map_err(|e| format!("The main thread has been panicked: {}", e))
                        .unwrap(); // Intentionally panic when error
                    *playback_position = PlaybackPosition::Playing {
                        instant,
                        music_position: music_position_start,
                        play_speed: self.play_speed,
                    };
                }

                while let Some(next) = self.sound_effect_schedules.front() {
                    if music_position_end <= next.timestamp {
                        break;
                    }
                    let next = self.sound_effect_schedules.pop_front().unwrap();
                    if next.timestamp < music_position_start || !self.scheduled_play_enabled {
                        continue;
                    }
                    let mut source = next.source;
                    source.wait = (self.stream_config.channels as f64
                        * (next.timestamp - music_position_start)
                        * self.stream_config.sample_rate.0 as f64
                        * self.play_speed) as usize;
                    self.sound_effects.push(source);
                    self.sound_effect_sender
                        .send(next.response)
                        .map_err(|e| format!("The main thread has been panicked: {}", e))
                        .unwrap(); // Intentionally panic when error
                }

                // TODO: SPAGHETTI CODE!
                self.played_sample_count += output.len().saturating_sub(self.skip_sample_count)
                    / (self.stream_config.channels as usize)
            }

            for out in output.iter_mut() {
                let mut next = match &mut self.music {
                    Some(music) if self.playing => {
                        if self.skip_sample_count > 0 {
                            self.skip_sample_count -= 1;
                            None
                        } else {
                            music.next().map(|a| a * self.music_volume)
                        }
                    }
                    _ => None,
                }
                .unwrap_or(0.0)
                .clamp(-4.0, 4.0); // Prevent too large sound

                self.sound_effects.retain_mut(|source| match source.next() {
                    Some(value) => {
                        next += value;
                        true
                    }
                    None => false,
                });
                *out = S::from(&next);
            }
        }
    }

    fn update_pause_state(&self) {
        if let Some(playback_position) = self.playback_position_ptr.upgrade() {
            let mut playback_position = playback_position
                .lock()
                .map_err(|e| format!("The main thread has been panicked: {}", e))
                .unwrap(); // Intentionally panic when error
            *playback_position = PlaybackPosition::Paused {
                music_position: self.music_position_start(),
            };
        }
    }

    fn music_position_start(&self) -> f64 {
        let sample_index = self.played_sample_count as isize
            - self.skip_sample_count as isize / self.stream_config.channels as isize;
        sample_index as f64 / self.stream_config.sample_rate.0 as f64
    }

    pub fn load_music(&self, wave: PathBuf) -> Result<MusicSource, TaikoError> {
        let file = std::fs::File::open(wave).map_err(|e| TaikoError {
            message: "Failed to open music file".to_string(),
            cause: TaikoErrorCause::AudioLoadError(e),
        })?;
        let decoder = rodio::Decoder::new(BufReader::new(file)).map_err(|e| TaikoError {
            message: "Failed to decode music".to_string(),
            cause: TaikoErrorCause::CpalOrRodioError(CpalOrRodioError::DecoderError(e)),
        })?;
        let ret = new_uniform_source_iterator(decoder, &self.stream_config);
        Ok(ret)
    }
}

#[derive(Clone)]
pub struct SoundBuffer {
    data: Arc<Vec<f32>>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    volume: f32,
}

impl SoundBuffer {
    pub fn load<P>(
        filename: P,
        channels: ChannelCount,
        sample_rate: SampleRate,
    ) -> Result<SoundBuffer, TaikoError>
    where
        P: AsRef<Path>,
    {
        let file = File::open(filename).map_err(|e| TaikoError {
            message: "Failed to open sound chunk file".to_string(),
            cause: TaikoErrorCause::AudioLoadError(e),
        })?;
        let decoder = rodio::Decoder::new(BufReader::new(file)).map_err(|e| TaikoError {
            message: "Failed to decode sound chunk file".to_string(),
            cause: TaikoErrorCause::CpalOrRodioError(CpalOrRodioError::DecoderError(e)),
        })?;
        let decoder = UniformSourceIterator::<_, f32>::new(decoder, channels, sample_rate.0);
        let decoded = decoder.collect_vec();
        Ok(SoundBuffer {
            data: Arc::new(decoded),
            channels,
            sample_rate,
            volume: 1.0,
        })
    }
    pub fn new_source(&self) -> SoundBufferSource {
        SoundBufferSource {
            sound_buffer: self.clone(),
            wait: 0,
            index: 0,
        }
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }
}

pub struct SoundBufferSource {
    sound_buffer: SoundBuffer,
    wait: usize,
    index: usize,
}

impl Iterator for SoundBufferSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.wait > 0 {
            self.wait -= 1;
            Some(0.0)
        } else {
            let ret = self
                .sound_buffer
                .data
                .get(self.index)
                .copied()
                .map(|a| a * self.sound_buffer.volume);
            self.index += 1;
            ret
        }
    }
}

impl Source for SoundBufferSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.sound_buffer.data.len() - self.index)
    }

    fn channels(&self) -> u16 {
        self.sound_buffer.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sound_buffer.sample_rate.0
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            (self.sound_buffer.data.len() as f64 / self.channels() as f64)
                / (1.0 / self.sample_rate() as f64),
        ))
    }
}
