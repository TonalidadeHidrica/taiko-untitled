use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use itertools::Itertools;
use std::cmp::{max, min};
use std::fs::File;
use std::io;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug)]
pub enum TjaError {
    IoError(io::Error),
    DecodingError(DecodingError),
}

#[derive(Debug)]
pub enum DecodingError {
    AnotherEncodingWasUsed {
        detected: &'static Encoding,
        used: &'static Encoding,
    },
    MalformedByteSequenceFound(&'static Encoding),
}

impl From<io::Error> for TjaError {
    fn from(e: Error) -> Self {
        Self::IoError(e)
    }
}

#[derive(Debug)]
pub struct Song {
    pub title: Option<String>,
    pub subtitle: Option<Subtitle>,
    pub bpm: f64,
    pub wave: Option<PathBuf>,
    pub offset: f64,
    pub song_volume: u32,
    pub se_volume: u32,

    pub score: Option<Score>, // will later be Vec<Score>
}

#[derive(Default, Debug)]
pub struct Score {
    pub notes: Vec<Note>,
    pub bar_lines: Vec<BarLine>,
}

#[derive(Debug)]
pub struct Note {
    pub scroll_speed: Bpm,
    pub content: NoteContent,
}

#[derive(Debug)]
pub enum NoteContent {
    Normal {
        color: NoteColor,
        size: NoteSize,
        time: f64,
    },
    Renda {
        kind: RendaKind,
        start_time: f64,
        end_time: f64,
    },
}

#[derive(Debug)]
pub enum NoteColor {
    Don,
    Ka,
}

#[derive(Debug)]
pub enum NoteSize {
    Small,
    Large,
}

#[derive(Debug)]
pub enum RendaKind {
    Unlimited { size: NoteSize },
    Quota { kind: QuotaRendaKind, quota: u64 },
}

#[derive(Debug)]
pub enum QuotaRendaKind {
    Balloon,
    Potato,
}

#[derive(Debug)]
pub struct BarLine {
    time: f64,
    scroll_speed: Bpm,
    visible: bool,
}

impl Default for Song {
    fn default() -> Self {
        let (title, subtitle, wave, offset, score) = Default::default();
        Self {
            title,
            subtitle,
            bpm: 120.0,
            wave,
            offset,
            song_volume: 100, // default value is not asserted to be true
            se_volume: 100,   // default value is not asserted to be true
            score,
        }
    }
}

#[derive(Debug)]
pub struct Subtitle {
    text: String,
    style: SubtitleStyle,
}

#[derive(Debug)]
pub enum SubtitleStyle {
    Unspecified,
    Suppress,
    Show,
}

pub fn load_tja_from_file<P: AsRef<Path>>(path: P) -> Result<Song, TjaError> {
    let path = path.as_ref();
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    let _ = file.read_to_end(&mut buf)?;

    let mut detector = EncodingDetector::new();
    detector.feed(&buf, true);
    let encoding = detector.guess(None, true);

    let (source, actual_encoding, replacement) = encoding.decode(&buf);
    if encoding != actual_encoding {
        Err(TjaError::DecodingError(
            DecodingError::AnotherEncodingWasUsed {
                detected: encoding,
                used: actual_encoding,
            },
        ))
    } else if replacement {
        Err(TjaError::DecodingError(
            DecodingError::MalformedByteSequenceFound(encoding),
        ))
    } else {
        let mut song = load_tja_from_str(source.to_string())?;
        if let Some(wave) = song.wave {
            song.wave = Some(path.with_file_name(wave));
        }
        Ok(song)
    }
}

#[derive(Debug)]
struct SongContext {
    player: Player,
    score: Score,
    elements: Vec<TjaElement>,
    time: f64,
    measure: Measure,
    bpm: Bpm,
    hs: f64,
    bar_line: bool,
    gogo: bool,
}

#[derive(Debug)]
struct Measure(f64, f64);
impl Default for Measure {
    fn default() -> Self {
        Measure(4.0, 4.0)
    }
}
impl Measure {
    pub fn get_beat_count(&self) -> f64 {
        self.0 / self.1 * 4.0
    }
}

#[derive(Debug)]
pub struct Bpm(f64);
impl Bpm {
    pub fn get_beat_duration(&self) -> f64 {
        60.0 / self.0
    }
}

impl SongContext {
    fn new(song: &Song) -> SongContext {
        let (player, elements, measure) = Default::default();
        SongContext {
            player,
            score: Score::default(),
            elements,
            time: -song.offset,
            measure,
            bpm: Bpm(song.bpm),
            hs: 1.0,
            bar_line: true,
            gogo: false,
        }
    }
    fn terminate_measure(&mut self) {
        let notes_count = self
            .elements
            .iter()
            .filter(|x| matches!(x, TjaElement::NoteChar(..)))
            .count();
        if notes_count == 0 {
            self.elements.push(TjaElement::NoteChar('0'));
        }
        let notes_count = max(1, notes_count) as f64;
        let mut first_note = true;
        for element in self.elements.iter() {
            match element {
                TjaElement::NoteChar(c) => {
                    if let Some(note) = match c {
                        '0' => None,
                        '1' => Some(self.note(false, false)),
                        '2' => Some(self.note(true, false)),
                        '3' => Some(self.note(false, true)),
                        '4' => Some(self.note(true, true)),
                        _ => None,
                    } {
                        self.score.notes.push(note);
                    }
                    if first_note {
                        first_note = false;
                        self.score.bar_lines.push(BarLine {
                            scroll_speed: self.scroll_speed(),
                            time: self.time,
                            visible: self.bar_line,
                        });
                    }
                    self.time +=
                        self.measure.get_beat_count() * self.bpm.get_beat_duration() / notes_count;
                }
                TjaElement::BpmChange(bpm) => self.bpm = Bpm(*bpm),
                TjaElement::Gogo(gogo) => self.gogo = *gogo,
                TjaElement::Measure(a, b) => self.measure = Measure(*a, *b),
                TjaElement::Scroll(scroll) => self.hs = *scroll,
                TjaElement::Delay(delay) => self.time += delay,
                TjaElement::BarLine(bar) => self.bar_line = *bar,
            }
        }
        self.elements.clear();
    }
    fn scroll_speed(&self) -> Bpm {
        Bpm(self.bpm.0 * self.hs)
    }
    fn note(&self, ka: bool, large: bool) -> Note {
        Note {
            scroll_speed: self.scroll_speed(),
            content: NoteContent::Normal {
                color: match ka {
                    false => NoteColor::Don,
                    true => NoteColor::Ka,
                },
                size: match large {
                    false => NoteSize::Small,
                    true => NoteSize::Large,
                },
                time: self.time,
            },
        }
    }
}

#[derive(Debug)]
enum TjaElement {
    NoteChar(char),
    BpmChange(f64),
    Gogo(bool),
    Measure(f64, f64),
    Scroll(f64),
    Delay(f64),
    BarLine(bool),
}

#[derive(Debug)]
pub enum Player {
    Single,
    Double1P,
    Double2P,
}

impl Default for Player {
    fn default() -> Self {
        Self::Single
    }
}

pub fn load_tja_from_str(source: String) -> Result<Song, TjaError> {
    let mut song = Song::default();
    let mut song_context: Option<SongContext> = None;
    'lines: for line in source.lines() {
        if let Some(ref mut context) = song_context {
            // TODO check if this parser is compatible
            let line = line
                .split("//")
                .next()
                .expect("Unexpected: split() must have one element");
            if line.starts_with("#END") {
                if let Some(context) = song_context.take() {
                    song.score = Some(context.score);
                    break 'lines;
                }
            } else if let Some(bpm) = take_remaining("#BPMCHANGE", line) {
                if let Some(bpm) = bpm.parse_first() {
                    context.elements.push(TjaElement::BpmChange(bpm));
                } else {
                    eprintln!("Parse error: {}", line);
                }
            } else if line.starts_with("#GOGOSTART") {
                context.elements.push(TjaElement::Gogo(true));
            } else if line.starts_with("#GOGOEND") {
                context.elements.push(TjaElement::Gogo(false));
            } else if let Some(measure) = take_remaining("#MEASURE", line) {
                if let [x, y] = &measure.split('/').collect_vec()[..] {
                    if let (Some(x), Some(y)) = (x.parse_first(), y.parse_first()) {
                        context.elements.push(TjaElement::Measure(x, y));
                    }
                }
            } else if let Some(scroll) = take_remaining("#SCROLL", line) {
                if let Some(scroll) = scroll.parse_first() {
                    context.elements.push(TjaElement::Scroll(scroll));
                } else {
                    println!("Ignored: {}", line);
                }
            } else if let Some(delay) = take_remaining("#DELAY", line) {
                if let Some(delay) = delay.parse_first() {
                    eprintln!("Delay is deprecated, so it may not work properly.");
                    context.elements.push(TjaElement::Delay(delay));
                }
            } else if let Some(_) = take_remaining("#BRANCHSTART", line) {
                eprintln!("branches are not implemented");
            } else if ["#SECTION", "#N", "#E", "#M", "#LEVELHOLD"]
                .iter()
                .any(|s| line.starts_with(s))
            {
                eprintln!("branches are not implemented");
            } else if line.starts_with("#BARLINEON") {
                context.elements.push(TjaElement::BarLine(true));
            } else if line.starts_with("#BARLINEOFF") {
                context.elements.push(TjaElement::BarLine(false));
            } else {
                if line.starts_with('#') {
                    eprintln!(
                        "Command {} is not recognized. Parsing as score instead.",
                        line
                    );
                }
                let mut split = line.split(',');
                let line = split
                    .next()
                    .expect("split() returns always at least one element");
                context
                    .elements
                    .extend(line.chars().filter_map(|c| match c {
                        '0'..='9' => Some(TjaElement::NoteChar(c)),
                        _ => None,
                    }));
                if split.next().is_some() {
                    context.terminate_measure();
                }
            }
        } else {
            if let Some(remaining) = take_remaining("#START", line) {
                let mut song_context_new = SongContext::new(&song);
                song_context_new.player = match remaining
                    .chars()
                    .skip_while(|c| *c != 'P' || *c != 'p')
                    .nth(1)
                {
                    Some('1') => Player::Double1P,
                    Some('2') => Player::Double2P,
                    _ => Player::Single,
                };
                song_context = Some(song_context_new);
            } else if let Some(title) = take_remaining("TITLE:", line) {
                // TODO warnings on override
                song.title = Some(title.to_string());
            } else if let Some(subtitle) = take_remaining("SUBTITLE:", line) {
                if let Some(subtitle) = take_remaining("--", subtitle) {
                    song.subtitle = Some(Subtitle {
                        text: subtitle.to_string(),
                        style: SubtitleStyle::Suppress,
                    });
                } else if let Some(subtitle) = take_remaining("++", subtitle) {
                    song.subtitle = Some(Subtitle {
                        text: subtitle.to_string(),
                        style: SubtitleStyle::Show,
                    })
                } else {
                    song.subtitle = Some(Subtitle {
                        text: subtitle.to_string(),
                        style: SubtitleStyle::Unspecified,
                    })
                }
            } else if let Some(bpm) = take_remaining("BPM:", line) {
                // TODO error warnings and wider accepted format
                if let Some(bpm) = bpm.parse_first() {
                    if bpm > 0.0 {
                        song.bpm = bpm;
                    }
                }
            } else if let Some(wave) = take_remaining("WAVE:", line) {
                song.wave = Some(Path::new(wave).to_path_buf());
            } else if let Some(offset) = take_remaining("OFFSET:", line) {
                if let Some(offset) = offset.parse_first() {
                    song.offset = offset;
                }
            } else if let Some(_balloon) = take_remaining("BALLOON:", line) {
                eprintln!("Warning: BALLOON not implemented");
            } else if let Some(song_volume) = take_remaining("SONGVOL:", line) {
                if let Some(song_volume) = song_volume.parse_first() {
                    song.song_volume = min(song_volume, 5000);
                }
            } else if let Some(se_volume) = take_remaining("SEVOL:", line) {
                if let Some(se_volume) = se_volume.parse_first() {
                    song.se_volume = min(se_volume, 5000);
                }
            } else if let Some(_) = take_remaining("SCOREINIT:", line) {
                eprintln!("Warning: SCOREINIT not implemented")
            } else if let Some(_) = take_remaining("SCOREDIFF:", line) {
                eprintln!("Warning: SCOREDIFF not implemented")
            } else if let Some(_) = take_remaining("COURSE:", line) {
                eprintln!("Warning: COURSE not implemented")
            } else if let Some(_) = take_remaining("STYLE:", line) {
                eprintln!("Warning: STYLE not implemented")
            } else if let Some(_) = take_remaining("GAME:", line) {
                eprintln!("Warning: GAME not implemented")
            } else if let Some(_) = take_remaining("LIFE:", line) {
                eprintln!("Warning: LIFE not implemented")
            } else if let Some(_) = take_remaining("DEMOSTART:", line) {
                eprintln!("Warning: DEMOSTART not implemented")
            } else if let Some(_) = take_remaining("SIDE:", line) {
                eprintln!("Warning: SIDE not implemented")
            } else if let Some(_) = take_remaining("SCOREMODE:", line) {
                eprintln!("Warning: SCOREMODE not implemented")
            } else if let Some(_) = take_remaining("TOTAL:", line) {
                eprintln!("Warning: TOTAL not implemented")
            } else {
                let mut split = line.split(':');
                let key = split.next().expect("Split has always at least one element");
                let value = split.next();
                if value.is_some() {
                    eprintln!("Unknown key: {}", key);
                }
            }
        }
    }
    Ok(song)
}

fn take_remaining<'a>(key: &'static str, string: &'a str) -> Option<&'a str> {
    if string.starts_with(key) {
        Some(&string[key.len()..])
    } else {
        None
    }
}

trait ParseFirst<V> {
    fn parse_first(self: Self) -> Option<V>;
}

impl<V> ParseFirst<V> for &str
where
    V: FromStr,
{
    fn parse_first(self) -> Option<V> {
        self.trim().parse().ok()
    }
}
