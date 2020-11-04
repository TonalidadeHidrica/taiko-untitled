use crate::errors::{CpalOrRodioError, TaikoError, TaikoErrorCause};
use crate::rodio::{new_uniform_source_iterator, TrueUniformSourceIterator};
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

pub struct AudioManager {
    pub stream_config: StreamConfig,
    sender_to_audio: Sender<MessageToAudio>,
    drop_sender: Sender<()>,
    playback_position: Arc<Mutex<Option<PlaybackPosition>>>,
}

struct PlaybackPosition {
    instant: Instant,
    music_position: f64,
}

enum MessageToAudio {
    Play,
    LoadMusic(PathBuf),
    SetMusicVolume(f32),
    AddPlay(SoundBufferSource),
    AddSchedules(Vec<SoundEffectSchedule>),
    SwitchScheduled(bool),
}

impl AudioManager {
    pub fn new() -> Result<AudioManager, TaikoError> {
        let (sender_to_audio, receiver_to_audio) = mpsc::channel();
        let (stream_config_sender, stream_config_receiver) = mpsc::channel();
        let (drop_sender, drop_receiver) = mpsc::channel();
        let playback_position = Arc::new(Mutex::new(None));

        let playback_position_ptr = Arc::downgrade(&playback_position);
        thread::spawn(
            move || match stream_thread(receiver_to_audio, playback_position_ptr) {
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
            },
        );
        let stream_config = stream_config_receiver.recv().map_err(|_| TaikoError {
            message: "Audio device initialization thread has been stopped".to_string(),
            cause: TaikoErrorCause::None,
        })??;

        Ok(AudioManager {
            stream_config,
            sender_to_audio,
            drop_sender,
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

    pub fn set_music_volume(&self, volume: f32) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::SetMusicVolume(volume))
            .map_err(|_| TaikoError {
                message: "Failed to set music volume; the audio stream has been stopped"
                    .to_string(),
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
        schedules: Vec<SoundEffectSchedule>,
    ) -> Result<(), TaikoError> {
        self.sender_to_audio
            .send(MessageToAudio::AddSchedules(schedules))
            .map_err(|_| TaikoError {
                message: "Failed to push schedules; the audio stream has been stopped".to_string(),
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
        Ok(playback_position.as_ref().map(|playback_position| {
            let now = Instant::now();
            let diff = if now > playback_position.instant {
                (now - playback_position.instant).as_secs_f64()
            } else {
                -(playback_position.instant - now).as_secs_f64()
            };
            diff + playback_position.music_position
        }))
    }
}

fn stream_thread(
    receiver_to_audio: Receiver<MessageToAudio>,
    playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
) -> Result<(StreamConfig, Stream), TaikoError> {
    let host = cpal::default_host();
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

impl Drop for AudioManager {
    fn drop(&mut self) {
        if self.drop_sender.send(()).is_err() {
            eprintln!("Failed to send drop signal to audio stream thread");
        }
    }
}

type MusicSource = TrueUniformSourceIterator<Decoder<BufReader<File>>>;

struct AudioThreadState {
    stream_config: StreamConfig,

    music: Option<MusicSource>,
    sound_effects: Vec<SoundBufferSource>,

    sound_effect_schedules: VecDeque<SoundEffectSchedule>,
    scheduled_play_enabled: bool,

    receiver_to_audio: mpsc::Receiver<MessageToAudio>,
    playing: bool,
    played_sample_count: usize,
    playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
    music_volume: f32,
}

pub struct SoundEffectSchedule {
    pub timestamp: f64,
    pub source: SoundBufferSource,
}

impl AudioThreadState {
    pub fn new(
        stream_config: StreamConfig,
        receiver_to_audio: mpsc::Receiver<MessageToAudio>,
        playback_position_ptr: Weak<Mutex<Option<PlaybackPosition>>>,
    ) -> Self {
        AudioThreadState {
            stream_config,
            music: None,
            sound_effects: Vec::new(),

            sound_effect_schedules: VecDeque::new(),
            scheduled_play_enabled: false,

            receiver_to_audio,
            playing: false,
            played_sample_count: 0,
            playback_position_ptr,
            music_volume: 1.0,
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
                    MessageToAudio::LoadMusic(path) => {
                        // TODO send error via another channel
                        self.music = Some(self.load_music(path).unwrap())
                    }
                    MessageToAudio::SetMusicVolume(volume) => self.music_volume = volume,
                    MessageToAudio::AddPlay(source) => {
                        self.sound_effects.push(source);
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

                let music_position_start =
                    self.played_sample_count as f64 / self.stream_config.sample_rate.0 as f64;
                let music_position_end = (self.played_sample_count + playing_sample_count) as f64
                    / self.stream_config.sample_rate.0 as f64;

                if let Some(playback_position) = self.playback_position_ptr.upgrade() {
                    let mut playback_position = playback_position
                        .lock()
                        .map_err(|e| format!("The main thread has been panicked: {}", e))
                        .unwrap(); // Intentionally panic when error
                    *playback_position = Some(PlaybackPosition {
                        instant,
                        music_position: music_position_start,
                    });
                }

                while let Some(next) = self.sound_effect_schedules.front() {
                    if music_position_end <= next.timestamp {
                        break;
                    }
                    let next = self.sound_effect_schedules.pop_front().unwrap();
                    if !self.scheduled_play_enabled {
                        continue;
                    }
                    let mut source = next.source;
                    source.wait = (self.stream_config.channels as f64
                        * (next.timestamp - music_position_start)
                        * self.stream_config.sample_rate.0 as f64)
                        as usize;
                    self.sound_effects.push(source);
                }

                self.played_sample_count += playing_sample_count;
            }

            for out in output.iter_mut() {
                let mut next = if let (Some(ref mut music), true) = (&mut self.music, self.playing)
                {
                    music.next().map(|a| a * self.music_volume)
                } else {
                    None
                }
                .unwrap_or(0.0);

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
