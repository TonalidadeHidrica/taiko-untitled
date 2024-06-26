use crate::structs::just::*;
use crate::structs::*;
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use enum_map::EnumMap;
use itertools::Itertools;
use once_cell::sync::Lazy;
use ordered_float::OrderedFloat;
use regex::Regex;
use std::cmp::{max, min};
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};

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
    pub bpm: Bpm,
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
            bpm: Bpm(120.0),
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
    #[allow(dead_code)]
    text: String,
    #[allow(dead_code)]
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
struct ScoreParser<'a> {
    song: &'a Song,
    score: Score,

    // elements buffer in current measure
    elements: Vec<TjaElement>,

    branch_context: BranchContext,
    parser_state: ParserState,

    balloons: VecDeque<u64>,
}

#[derive(Clone, Debug)]
struct ParserState {
    time: f64,
    measure: Measure,
    bpm: Bpm,

    hs: f64,
    bar_line: bool,
    gogo: bool,
    renda: Option<RendaBuffer>,

    // Used to detemine the color of bar line (yellow or whilte)
    first_measure_in_branch: bool,
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

impl ScoreParser<'_> {
    fn new(song: &Song, _player: Player) -> ScoreParser {
        let (score, elements, measure) = Default::default();
        // TODO store player etc. to score
        ScoreParser {
            song,
            score,
            elements,
            branch_context: BranchContext::Outside,
            parser_state: ParserState {
                time: -song.offset,
                measure,
                bpm: song.bpm,

                hs: 1.0,
                bar_line: true,
                gogo: false,
                renda: None,

                first_measure_in_branch: false,
            },
            balloons: song.balloons.iter().copied().collect(),
        }
    }

    fn parse_lines<'a, I>(&mut self, lines: I) -> bool
    where
        I: Iterator<Item = &'a str>,
    {
        let mut ended_with_end = false;
        for line in lines {
            // TODO check if this parser is compatible
            let line = line
                .split("//")
                .next()
                .expect("Unexpected: split() must have one element");
            if line.starts_with("#END") {
                ended_with_end = true;
                break;
            } else if let Some(bpm) = line.strip_prefix("#BPMCHANGE") {
                if let Some(bpm) = bpm.parse_first() {
                    self.elements.push(TjaElement::BpmChange(bpm));
                } else {
                    eprintln!("Parse error: {}", line);
                }
            } else if line.starts_with("#GOGOSTART") {
                self.elements.push(TjaElement::Gogo(true));
            } else if line.starts_with("#GOGOEND") {
                self.elements.push(TjaElement::Gogo(false));
            } else if let Some(measure) = line.strip_prefix("#MEASURE") {
                if let [x, y] = &measure.split('/').collect_vec()[..] {
                    if let (Some(x), Some(y)) = (x.parse_first(), y.parse_first()) {
                        self.elements.push(TjaElement::Measure(x, y));
                    }
                }
            } else if let Some(scroll) = line.strip_prefix("#SCROLL") {
                if let Some(scroll) = scroll.parse_first() {
                    self.elements.push(TjaElement::Scroll(scroll));
                } else {
                    println!("Ignored: {}", line);
                }
            } else if let Some(delay) = line.strip_prefix("#DELAY") {
                if let Some(delay) = delay.parse_first() {
                    eprintln!("Delay is deprecated, so it may not work properly.");
                    self.elements.push(TjaElement::Delay(delay));
                }
            } else if let Some(branch_condition) = line.strip_prefix("#BRANCHSTART") {
                self.branch_start(branch_condition);
            } else if line.starts_with("#BRANCHEND") {
                self.branch_end(true);
            } else if line.starts_with("#N") {
                self.branch_switch(BranchType::Normal);
            } else if line.starts_with("#E") {
                self.branch_switch(BranchType::Expert);
            } else if line.starts_with("#M") {
                self.branch_switch(BranchType::Master);
            } else if line.starts_with("#SECTION") {
                self.section();
            } else if line.starts_with("#LEVELHOLD") {
                self.level_hold();
            } else if line.starts_with("#BARLINEON") {
                self.elements.push(TjaElement::BarLine(true));
            } else if line.starts_with("#BARLINEOFF") {
                self.elements.push(TjaElement::BarLine(false));
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
                self.elements.extend(line.chars().filter_map(|c| match c {
                    '0'..='9' => Some(TjaElement::NoteChar(c)),
                    _ => None,
                }));
                if split.next().is_some() {
                    self.terminate_measure(true);
                }
            }
        }
        self.score.notes.sort_by_key(|e| OrderedFloat::from(e.time));
        ended_with_end
    }

    fn terminate_measure(&mut self, ignore_notes: bool) {
        // eprintln!("{:?} {:?} {:?}", self.elements, self.branch_context, self.parser_state);

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
                // eprintln!(
                //     "Warning: elements between #BRANCHSTART and first #N, #E or #M is deprecated."
                // );
                // eprintln!("The commands will be accepted, while the notes will be ignored.");
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
                        // eprintln!("Foreign element {:?} applied", element);
                        // TODO duplicate
                        match element {
                            TjaElement::BpmChange(bpm) => self.parser_state.bpm = Bpm(*bpm),
                            TjaElement::Measure(a, b) => {
                                self.parser_state.measure = Measure(*a, *b)
                            }
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
                            kind: match self.parser_state.first_measure_in_branch {
                                true => BarLineKind::Branch,
                                false => BarLineKind::Normal,
                            },
                            visible: self.parser_state.bar_line,
                            branch: self.current_branch(),
                        });
                        self.parser_state.first_measure_in_branch = false;
                    }
                    note_index += 1;
                    self.parser_state.time += self.parser_state.measure.get_beat_count()
                        * self.parser_state.bpm.beat_duration()
                        / notes_count as f64;
                }
                TjaElement::BpmChange(bpm) if parse_tempo => self.parser_state.bpm = Bpm(*bpm),
                TjaElement::Gogo(gogo) => self.parser_state.gogo = *gogo,
                TjaElement::Measure(a, b) if parse_tempo => {
                    self.parser_state.measure = Measure(*a, *b)
                }
                TjaElement::Scroll(scroll) => self.parser_state.hs = *scroll,
                TjaElement::Delay(delay) if parse_tempo => self.parser_state.time += delay,
                TjaElement::BarLine(bar) => self.parser_state.bar_line = *bar,
                _ => {
                    // Element was ignored due to illegal syntax in the tja file
                    // eprintln!("Skipped: {:?}", element);
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
        Bpm(self.parser_state.bpm.0 * self.parser_state.hs)
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
        // TODO start time
        let judge_time = self
            .score
            .bar_lines
            .last()
            .map(|b| b.time)
            .unwrap_or_else(|| self.song.offset - self.song.bpm.beat_duration() * 4.0);
        self.terminate_measure(false);

        let condition = match Self::parse_branch_condition(branch_condition) {
            Ok(c) => c,
            Err(..) => {
                eprintln!("Invalid branch condition: {:?}", branch_condition);
                BranchCondition::Pass
            }
        };
        self.score.branches.push(Branch {
            judge_time,
            switch_time: self.parser_state.time,
            scroll_speed: self.scroll_speed(),
            condition,
            info: (),
        });
        // println!("{} {}\n", judge_time, self.parser_state.time);

        if !matches!(self.branch_context, BranchContext::Outside) {
            eprintln!("#BRANCHSTART was found before branch ends.");
            self.branch_end(false);
        }
        self.branch_context = BranchContext::Started;
        // println!("Start => {:?}", self.branch_context);

        self.parser_state.first_measure_in_branch = true;
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
        // println!("Switch({:?}) => {:?}", branch_type, self.branch_context);
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

    fn branch_end(&mut self, terminate_measure: bool) {
        if terminate_measure {
            self.terminate_measure(false);
        }

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
        // println!("Start => {:?}", self.branch_context);
    }

    fn section(&mut self) {
        self.push_branch_event(BranchEventKind::Section);
    }

    fn level_hold(&mut self) {
        let branch_type = match &self.branch_context {
            BranchContext::Outside | BranchContext::Started => {
                eprintln!("#LEVELHOLD before #N, #E or #M is ignored.");
                return;
            }
            BranchContext::First(context) => context.branch_type,
            BranchContext::Subsequent(context) | BranchContext::Duplicate(context) => {
                context.branch_type
            }
        };
        self.push_branch_event(BranchEventKind::LevelHold(branch_type));
    }

    fn push_branch_event(&mut self, kind: BranchEventKind) {
        self.score.branch_events.push(BranchEvent {
            time: self.parser_state.time,
            kind,
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

    let mut lines = source.lines();
    #[allow(clippy::never_loop)]
    loop {
        let player = load_tja_metadata(&mut song, lines.by_ref());
        let player = match player {
            None => break,
            Some(player) => player,
        };
        let mut song_context = ScoreParser::new(&song, player);
        let ended_with_end = song_context.parse_lines(lines.by_ref());
        song.score = Some(song_context.score);
        if !ended_with_end {
            eprintln!("Warning: The score did not ended with #END");
            break;
        }
        break;
    }

    Ok(song)
}

fn load_tja_metadata<'a, I>(song: &mut Song, lines: &mut I) -> Option<Player>
where
    I: Iterator<Item = &'a str>,
{
    for line in lines {
        #[allow(clippy::redundant_pattern_matching)]
        if let Some(remaining) = line.strip_prefix("#START") {
            let player = match remaining
                .chars()
                .skip_while(|c| *c != 'P' || *c != 'p')
                .nth(1)
            {
                Some('1') => Player::Double1P,
                Some('2') => Player::Double2P,
                _ => Player::Single,
            };
            return Some(player);
        } else if let Some(title) = line.strip_prefix("TITLE:") {
            // TODO warnings on override
            song.title = Some(title.to_string());
        } else if let Some(subtitle) = line.strip_prefix("SUBTITLE:") {
            if let Some(subtitle) = subtitle.strip_prefix("--") {
                song.subtitle = Some(Subtitle {
                    text: subtitle.to_string(),
                    style: SubtitleStyle::Suppress,
                });
            } else if let Some(subtitle) = subtitle.strip_prefix("++") {
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
        } else if let Some(_level) = line.strip_prefix("LEVEL:") {
            eprintln!("Warning: LEVEL not implemented");
        } else if let Some(bpm) = line.strip_prefix("BPM:") {
            // TODO error warnings and wider accepted format
            if let Some(bpm) = bpm.parse_first() {
                if bpm > 0.0 {
                    song.bpm = Bpm(bpm);
                }
            }
        } else if let Some(wave) = line.strip_prefix("WAVE:") {
            song.wave = Some(Path::new(wave).to_path_buf());
        } else if let Some(offset) = line.strip_prefix("OFFSET:") {
            if let Some(offset) = offset.parse_first() {
                song.offset = offset;
            }
        } else if let Some(balloon) = line.strip_prefix("BALLOON:") {
            song.balloons = balloon
                .split(',')
                .filter_map(ParseFirst::parse_first)
                .collect_vec();
        } else if let Some(song_volume) = line.strip_prefix("SONGVOL:") {
            if let Some(song_volume) = song_volume.parse_first() {
                song.song_volume = min(song_volume, 5000);
            }
        } else if let Some(se_volume) = line.strip_prefix("SEVOL:") {
            if let Some(se_volume) = se_volume.parse_first() {
                song.se_volume = min(se_volume, 5000);
            }
        } else if let Some(_) = line.strip_prefix("SCOREINIT:") {
            eprintln!("Warning: SCOREINIT not implemented")
        } else if let Some(_) = line.strip_prefix("SCOREDIFF:") {
            eprintln!("Warning: SCOREDIFF not implemented")
        } else if let Some(_) = line.strip_prefix("COURSE:") {
            eprintln!("Warning: COURSE not implemented")
        } else if let Some(_) = line.strip_prefix("STYLE:") {
            eprintln!("Warning: STYLE not implemented")
        } else if let Some(_) = line.strip_prefix("GAME:") {
            eprintln!("Warning: GAME not implemented")
        } else if let Some(_) = line.strip_prefix("LIFE:") {
            eprintln!("Warning: LIFE not implemented")
        } else if let Some(_) = line.strip_prefix("DEMOSTART:") {
            eprintln!("Warning: DEMOSTART not implemented")
        } else if let Some(_) = line.strip_prefix("SIDE:") {
            eprintln!("Warning: SIDE not implemented")
        } else if let Some(_) = line.strip_prefix("SCOREMODE:") {
            eprintln!("Warning: SCOREMODE not implemented")
        } else if let Some(_) = line.strip_prefix("TOTAL:") {
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
    None
}

trait ParseFirst<V> {
    fn parse_first(self) -> Option<V>;
}

impl ParseFirst<f64> for &str {
    fn parse_first(self) -> Option<f64> {
        static PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?ix)
                    ^\s*
                    (?P<value>
                        [+-]?
                        (
                            # inf|nan|
                            (
                                  [0-9]+\.[0-9]*
                                | [0-9]*\.[0-9]+
                                | [0-9]+
                            )
                            (e [+-]? [0-9]+)?
                        )
                    )
                ",
            )
            .unwrap()
        });
        PATTERN.captures(self)?["value"].parse().ok()
    }
}

static INTEGER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
            ^\s*
            (?P<value>
                [+-]? [0-9]+
            )
        ",
    )
    .unwrap()
});

macro_rules! parse_integer {
    ($($t: ty)*) => {
        $(
            impl ParseFirst<$t> for &str {
                fn parse_first(self: Self) -> Option<$t> {
                    INTEGER_PATTERN.captures(self)?["value"].parse().ok()
                }
            }
        )*
    }
}
parse_integer!(u64 u32 i64);

#[cfg(test)]
mod tests {
    use super::ParseFirst;

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_f64() {
        assert_eq!("3.14".parse_first(), Some(3.14));
        assert_eq!("-3.14".parse_first(), Some(-3.14));
        assert_eq!("2.5E10".parse_first(), Some(2.5e10));
        assert_eq!("2.5e10".parse_first(), Some(2.5e10));
        assert_eq!("2.5E-10".parse_first(), Some(2.5e-10));
        assert_eq!("5".parse_first(), Some(5.0));
        assert_eq!("5.".parse_first(), Some(5.0));
        assert_eq!(".5".parse_first(), Some(0.5));
        assert_eq!("0.5".parse_first(), Some(0.5));

        assert_eq!("inf".parse_first(), None as Option<f64>);
        assert_eq!("-inf".parse_first(), None as Option<f64>);
        assert_eq!("NaN".parse_first(), None as Option<f64>);

        assert_eq!("  3.14".parse_first(), Some(3.14));
        assert_eq!("      -3.14".parse_first(), Some(-3.14));
        assert_eq!("  \t\t\t2.5E10".parse_first(), Some(2.5e10));
        assert_eq!("  \t \t  2.5e10".parse_first(), Some(2.5e10));

        assert_eq!("  3.14abc".parse_first(), Some(3.14));
        assert_eq!("      -3.14e".parse_first(), Some(-3.14));
        assert_eq!("  \t\t\t2.5E10//".parse_first(), Some(2.5e10));
        assert_eq!("  \t \t  2.5e10e".parse_first(), Some(2.5e10));
        assert_eq!("  5.2.3.1".parse_first(), Some(5.2));
        assert_eq!("  120 //180".parse_first(), Some(120.0));
    }

    #[test]
    fn test_parse_u64() {
        assert_eq!("0".parse_first(), Some(0u64));
        assert_eq!("1234".parse_first(), Some(1234u64));
        assert_eq!("2147483648".parse_first(), Some(2147483648u64));

        assert_eq!("-0".parse_first(), None as Option<u64>);
        assert_eq!("-1234".parse_first(), None as Option<u64>);
        assert_eq!("-2147483648".parse_first(), None as Option<u64>);

        assert_eq!("a".parse_first(), None as Option<u64>);

        assert_eq!("   123".parse_first(), Some(123u64));
        assert_eq!("  \t123e2".parse_first(), Some(123u64));
        assert_eq!("  \t123//456".parse_first(), Some(123u64));
    }

    #[test]
    fn test_parse_i64() {
        assert_eq!("0".parse_first(), Some(0i64));
        assert_eq!("1234".parse_first(), Some(1234i64));
        assert_eq!("2147483648".parse_first(), Some(2147483648i64));

        assert_eq!("-0".parse_first(), Some(0i64));
        assert_eq!("-1234".parse_first(), Some(-1234i64));
        assert_eq!("-2147483648".parse_first(), Some(-2147483648i64));

        assert_eq!("a".parse_first(), None as Option<i64>);

        assert_eq!("   123".parse_first(), Some(123i64));
        assert_eq!("  \t123e2".parse_first(), Some(123i64));
        assert_eq!("  \t123//456".parse_first(), Some(123i64));
    }
}
