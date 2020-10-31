use crate::structs::just::*;
use crate::structs::*;
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use enum_map::EnumMap;
use itertools::Itertools;
use ordered_float::OrderedFloat;
use std::cmp::{max, min};
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug)]
pub enum TjaError {
    IoError(io::Error),
    DecodingError(DecodingError),
    Unreachable(&'static str),
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
    pub balloons: Vec<u64>,

    pub score: Option<Score>, // will later be Vec<Score>
}

impl Default for Song {
    fn default() -> Self {
        let (title, subtitle, wave, offset, balloons, score) = Default::default();
        Self {
            title,
            subtitle,
            bpm: 120.0,
            wave,
            offset,
            song_volume: 100, // default value is not asserted to be true
            se_volume: 100,   // default value is not asserted to be true
            score,
            balloons,
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

#[derive(Clone, Debug)]
struct RendaBuffer(Bpm, f64, RendaContent);

#[derive(Debug)]
struct SongContext {
    player: Player,
    score: Score,

    // elements buffer in current measure
    elements: Vec<TjaElement>,

    branch_context: BranchContext,
    // Should be not overwritten while reading subsequent branches
    measure: Measure,
    bpm: Bpm,
    // Should be restored after branch ends
    parser_state: ParserState,

    balloons: VecDeque<u64>,
}

#[derive(Clone, Debug)]
struct ParserState {
    time: f64,
    // Independent
    hs: f64,
    bar_line: bool,
    gogo: bool,
    renda: Option<RendaBuffer>,
}

#[derive(Debug)]
enum BranchContext {
    Outside,
    Started,
    First(FirstBranchContext),
    Subsequent(SubsequentBranchContext),
    Duplicate(SubsequentBranchContext),
}

#[derive(Debug)]
struct FirstBranchContext {
    branch_type: BranchType,
    initial_state: ParserState,
    shared_elements: Vec<(usize, Vec<(usize, TjaElement)>)>,
}

#[derive(Debug)]
struct SubsequentBranchContext {
    branch_type: BranchType,
    initial_state: ParserState,
    end_state: ParserState,
    shared_elements: Vec<(usize, Vec<(usize, TjaElement)>)>,
    measure_index: usize,
    filled_branch: EnumMap<BranchType, bool>,
}

impl SongContext {
    fn new(song: &Song) -> SongContext {
        let (player, elements, measure) = Default::default();
        SongContext {
            player,
            score: Score::default(),
            elements,
            branch_context: BranchContext::Outside,
            measure,
            bpm: Bpm(song.bpm),
            parser_state: ParserState {
                hs: 1.0,
                bar_line: true,
                gogo: false,
                renda: None,
                time: -song.offset,
            },
            balloons: song.balloons.iter().copied().collect(),
        }
    }
    fn terminate_measure(&mut self, ignore_notes: bool) {
        // println!("    terminate_measure: {:?}", self.elements);

        let notes_count = self
            .elements
            .iter()
            .filter(|x| matches!(x, TjaElement::NoteChar(..)))
            .count();
        if notes_count == 0 {
            self.elements.push(TjaElement::NoteChar('0'));
        }
        let notes_count = max(1, notes_count);

        let (parse_notes, parse_tempo) = match &self.branch_context {
            BranchContext::Outside => (true, true),
            BranchContext::Started => {
                eprintln!(
                    "Warning: elements between #BRANCHSTART and first #N, #E or #M is deprecated."
                );
                eprintln!("The commands will be accepted, while the notes will be ignored.");
                (false, true)
            }
            BranchContext::First(..) => (true, true),
            BranchContext::Subsequent(context) => {
                if context.measure_index < context.shared_elements.len() {
                    (true, false)
                } else {
                    eprintln!("Warning: the number of measures in this branch exceeded that of the first one.");
                    eprintln!("The commands will be accepted, while the notes will be ignored.");
                    (false, true)
                }
            }
            BranchContext::Duplicate(_) => {
                self.elements.clear();
                return;
            }
        };
        let parse_notes = parse_notes && ignore_notes;

        if let BranchContext::First(context) = &mut self.branch_context {
            context.shared_elements.push((notes_count, Vec::new()));
        }

        let mut note_index = 0;
        let mut shared_elements_index = 0;
        for element in self.elements.iter() {
            if let BranchContext::Subsequent(context) = &mut self.branch_context {
                if let Some((total, shared_elements)) =
                    &context.shared_elements.get(context.measure_index)
                {
                    for (_, element) in
                        shared_elements[shared_elements_index..]
                            .iter()
                            .take_while(|(i, _)| {
                                // i / total <= note_index / notes_count
                                i.saturating_mul(notes_count) <= total.saturating_mul(note_index)
                            })
                    {
                        // TODO duplicate
                        match element {
                            TjaElement::BpmChange(bpm) => self.bpm = Bpm(*bpm),
                            TjaElement::Measure(a, b) => self.measure = Measure(*a, *b),
                            TjaElement::Delay(delay) => self.parser_state.time += delay,
                            _ => {}
                        }
                        shared_elements_index += 1;
                    }
                }
            }
            match element {
                TjaElement::NoteChar(c) if parse_notes => {
                    if let Some(note) = match c {
                        '0' => None,
                        '1' => Some(self.note(false, false)),
                        '2' => Some(self.note(true, false)),
                        '3' => Some(self.note(false, true)),
                        '4' => Some(self.note(true, true)),
                        '5' => {
                            self.parser_state.renda =
                                Some(self.renda(RendaKind::Unlimited(UnlimitedRenda {
                                    size: NoteSize::Small,
                                    info: (),
                                })));
                            None
                        }
                        '6' => {
                            self.parser_state.renda =
                                Some(self.renda(RendaKind::Unlimited(UnlimitedRenda {
                                    size: NoteSize::Large,
                                    info: (),
                                })));
                            None
                        }
                        '7' => {
                            let quota = self.balloons.pop_front().unwrap_or(5);
                            self.parser_state.renda =
                                Some(self.renda(RendaKind::Quota(QuotaRenda {
                                    kind: QuotaRendaKind::Balloon,
                                    quota,
                                    info: (),
                                })));
                            None
                        }
                        '8' => {
                            let branch = self.current_branch();
                            Self::terminate_renda(&mut self.parser_state, branch)
                        }
                        '9' => {
                            let quota = self.balloons.pop_front().unwrap_or(5);
                            self.parser_state.renda =
                                Some(self.renda(RendaKind::Quota(QuotaRenda {
                                    kind: QuotaRendaKind::Potato,
                                    quota,
                                    info: (),
                                })));
                            None
                        }
                        _ => {
                            unreachable!("NoteChar must contain characters between '0' and '9'",);
                        }
                    } {
                        self.score.notes.push(note);
                    }
                    if note_index == 0 {
                        self.score.bar_lines.push(BarLine {
                            scroll_speed: self.scroll_speed(),
                            time: self.parser_state.time,
                            visible: self.parser_state.bar_line,
                        });
                    }
                    note_index += 1;
                    self.parser_state.time += self.measure.get_beat_count()
                        * self.bpm.get_beat_duration()
                        / notes_count as f64;
                }
                TjaElement::BpmChange(bpm) if parse_tempo => self.bpm = Bpm(*bpm),
                TjaElement::Gogo(gogo) => self.parser_state.gogo = *gogo,
                TjaElement::Measure(a, b) if parse_tempo => self.measure = Measure(*a, *b),
                TjaElement::Scroll(scroll) => self.parser_state.hs = *scroll,
                TjaElement::Delay(delay) if parse_tempo => self.parser_state.time += delay,
                TjaElement::BarLine(bar) => self.parser_state.bar_line = *bar,
                _ => {
                    // Element was ignored due to illegal syntax in the tja file
                }
            }
            if let BranchContext::First(context) = &mut self.branch_context {
                if matches!(
                    element,
                    TjaElement::BpmChange(..) | TjaElement::Measure(..) | TjaElement::Delay(..)
                ) {
                    context
                        .shared_elements
                        .last_mut()
                        .unwrap()
                        .1
                        .push((note_index, element.clone()));
                }
            }
        }

        if let BranchContext::Subsequent(context) = &mut self.branch_context {
            context.measure_index += 1
        }

        self.elements.clear();
    }
    fn scroll_speed(&self) -> Bpm {
        Bpm(self.bpm.0 * self.parser_state.hs)
    }
    fn with_scroll_speed(&self, note_content: NoteContent) -> Note {
        Note {
            time: self.parser_state.time,
            scroll_speed: self.scroll_speed(),
            content: note_content,
            branch: self.current_branch(),
            info: (),
        }
    }
    fn note(&self, ka: bool, large: bool) -> Note {
        self.with_scroll_speed(NoteContent::Single(SingleNote {
            kind: SingleNoteKind {
                color: match ka {
                    false => NoteColor::Don,
                    true => NoteColor::Ka,
                },
                size: match large {
                    false => NoteSize::Small,
                    true => NoteSize::Large,
                },
            },
            info: (),
        }))
    }
    fn renda(&self, kind: RendaKind) -> RendaBuffer {
        RendaBuffer(
            self.scroll_speed(),
            self.parser_state.time,
            RendaContent {
                end_time: self.parser_state.time,
                kind,
                info: (),
            },
        )
    }
    fn terminate_renda(parser_state: &mut ParserState, branch: Option<BranchType>) -> Option<Note> {
        if let Some(RendaBuffer(scroll_speed, time, mut content)) = parser_state.renda.take() {
            content.end_time = parser_state.time;
            Some(Note {
                scroll_speed,
                time,
                content: NoteContent::Renda(content),
                branch,
                info: (),
            })
        } else {
            None
        }
    }

    fn current_branch(&self) -> Option<BranchType> {
        match &self.branch_context {
            BranchContext::Outside | BranchContext::Started => None,
            BranchContext::First(c) => Some(c.branch_type),
            BranchContext::Subsequent(c) | BranchContext::Duplicate(c) => Some(c.branch_type),
        }
    }

    fn parse_branch_condition(branch_condition: &str) -> Result<BranchCondition, ()> {
        #[derive(Debug)]
        enum T {
            R,
            S,
            P,
        }
        let (i, t) = branch_condition
            .find(&['r', 'R'][..])
            .map(|i| (i, T::R))
            .unwrap_or_else(|| {
                branch_condition
                    .find(&['s', 'S'][..])
                    .map(|i| (i, T::S))
                    .unwrap_or((0, T::P))
            });
        let branch_condition = &branch_condition[i..];
        let i = match branch_condition.find(',') {
            Some(i) => i + 1,
            None => return Err(()),
        };
        let ret = match &branch_condition[i..].splitn(2, ',').collect_vec()[..] {
            [_] => return Err(()),
            [x, y] => match t {
                T::R => {
                    BranchCondition::Renda(x.parse_first().ok_or(())?, y.parse_first().ok_or(())?)
                }
                T::S => {
                    BranchCondition::Score(x.parse_first().ok_or(())?, y.parse_first().ok_or(())?)
                }
                T::P => BranchCondition::Precision(
                    x.parse_first().ok_or(())?,
                    y.parse_first().ok_or(())?,
                ),
            },
            _ => unreachable!(),
        };
        Ok(ret)
    }

    fn branch_start(&mut self, branch_condition: &str) {
        self.terminate_measure(false);

        let condition = match Self::parse_branch_condition(branch_condition) {
            Ok(c) => c,
            Err(..) => {
                eprintln!("Invalid branch condition: {:?}", branch_condition);
                BranchCondition::Pass
            }
        };
        self.score.branches.push(Branch {
            time: self.parser_state.time,
            scroll_speed: self.scroll_speed(),
            condition,
            info: (),
        });

        if !matches!(self.branch_context, BranchContext::Outside) {
            eprintln!("#BRANCHSTART was found before branch ends.");
            self.branch_end();
        }
        self.branch_context = BranchContext::Started;
    }

    fn branch_switch(&mut self, branch_type: BranchType) {
        self.terminate_measure(false);

        let branch_context = std::mem::replace(&mut self.branch_context, BranchContext::Outside);
        self.branch_context = match branch_context {
            current @ BranchContext::Outside => {
                eprintln!(
                    "Cannot start branch {:?} outside #BRANCHSTART and END",
                    branch_type
                );
                current
            }
            BranchContext::Started => BranchContext::First(FirstBranchContext {
                branch_type,
                initial_state: self.parser_state.clone(),
                shared_elements: Vec::new(),
            }),
            BranchContext::First(context) => {
                let context = SubsequentBranchContext {
                    branch_type,
                    initial_state: context.initial_state,
                    end_state: self.parser_state.clone(),
                    shared_elements: context.shared_elements,
                    measure_index: 0,
                    filled_branch: EnumMap::new(),
                };
                self.branch_switch_subseqent(branch_type, context)
            }
            BranchContext::Subsequent(context) | BranchContext::Duplicate(context) => {
                self.branch_switch_subseqent(branch_type, context)
            }
        };
        // println!("{:?} {:?}", branch_type, &self.branch_context);
    }

    fn branch_switch_subseqent(
        &mut self,
        branch_type: BranchType,
        mut context: SubsequentBranchContext,
    ) -> BranchContext {
        // measure & bpm cannot be used
        self.parser_state = context.initial_state.clone();
        context.branch_type = branch_type;
        context.measure_index = 0;
        if std::mem::replace(&mut context.filled_branch[branch_type], true) {
            BranchContext::Duplicate(context)
        } else {
            BranchContext::Subsequent(context)
        }
    }

    fn branch_end(&mut self) {
        self.terminate_measure(false);

        match std::mem::replace(&mut self.branch_context, BranchContext::Outside) {
            BranchContext::Outside => {
                eprintln!("#BRANCHEND found before #BRANCHSTART");
            }
            BranchContext::Started => {
                eprintln!("Warning: None of #N, #E, #M was found between #BRANCHTSTART and END");
            }
            BranchContext::First(_) => {
                // No need to restore parser_state
            }
            BranchContext::Subsequent(context) | BranchContext::Duplicate(context) => {
                self.parser_state = context.end_state;
                // TODO should parser state be stored individually for different branches?
            }
        }
    }

    fn section(&mut self) {
        self.push_branch_event(BranchEventKind::Section);
    }

    fn level_hold(&mut self) {
        let branch_type = match &self.branch_context {
            BranchContext::Outside | BranchContext::Started => {
                eprintln!("#LEVELHOLD before #N, #E or #M is ignored.");
                return;
            },
            BranchContext::First(context) => context.branch_type,
            BranchContext::Subsequent(context) | BranchContext::Duplicate(context) => context.branch_type,
        };
        self.push_branch_event(BranchEventKind::LevelHold(branch_type));
    }

    fn push_branch_event(&mut self, kind: BranchEventKind) {
        self.score.branch_events.push(BranchEvent {
            time: self.parser_state.time,
            kind
        });
    }
}

#[derive(Clone, Debug)]
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
                if let Some(mut context) = song_context.take() {
                    context
                        .score
                        .notes
                        .sort_by_key(|e| OrderedFloat::from(e.time));
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
            } else if let Some(branch_condition) = take_remaining("#BRANCHSTART", line) {
                context.branch_start(branch_condition);
            } else if line.starts_with("#BRANCHEND") {
                context.branch_end();
            } else if line.starts_with("#N") {
                context.branch_switch(BranchType::Normal);
            } else if line.starts_with("#E") {
                context.branch_switch(BranchType::Expert);
            } else if line.starts_with("#M") {
                context.branch_switch(BranchType::Master);
            } else if line.starts_with("#SECTION") {
                context.section();
            } else if line.starts_with("#LEVELHOLD") {
                context.level_hold();
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
                    context.terminate_measure(true);
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
            } else if let Some(_level) = take_remaining("LEVEL:", line) {
                eprintln!("Warning: LEVEL not implemented");
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
            } else if let Some(balloon) = take_remaining("BALLOON:", line) {
                song.balloons = balloon
                    .split(',')
                    .filter_map(ParseFirst::parse_first)
                    .collect_vec();
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
