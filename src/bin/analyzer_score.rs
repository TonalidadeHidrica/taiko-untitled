use std::{
    collections::BTreeMap,
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chardetng::EncodingDetector;
use clap::Parser;
use itertools::Itertools;

use num::{range_step_inclusive, BigInt, BigRational, Integer, One, ToPrimitive, Zero};
use ordered_float::NotNan;
use taiko_untitled::{
    structs::{Bpm, SingleNoteKind},
    tja::ParseFirst,
};

#[derive(Parser)]
struct Opts {
    paths: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let mut notes_map = BTreeMap::<_, Vec<_>>::new();
    for path in &opts.paths {
        let (bpm, notes) = load_score(&path)?;
        let bpm_ratio = BigRational::from_float(bpm.0).context("Convert BPM to ratio")?;
        let ratio = &bpm_ratio / BigRational::from_integer(BigInt::from(125));
        // println!("bpm = {:?} = {:?} => {:?}", bpm, bpm_ratio, ratio);
        for note in notes.into_iter().map(|note| NoteScore {
            beat: note.beat / &ratio * BigRational::from_integer(BigInt::from(4)),
            ..note
        }) {
            notes_map.entry(note.beat.clone()).or_default().push(note);
        }
    }
    let notes_map = notes_map;

    let last_beat = notes_map.range(..).last().unwrap().0.ceil();
    let mut current_scroll = 1.0;
    let mut line_first = true;
    let four = BigRational::from_integer(BigInt::from(4));
    let beat_step = BigRational::one() / BigRational::from_integer(BigInt::from(1));
    println!("#MEASURE {}", &beat_step / &four);
    for i in range_step_inclusive(BigRational::zero(), last_beat, beat_step.clone()) {
        let notes = notes_map
            .range(i.clone()..i.clone() + &beat_step)
            .map(|v| (v.0 - i.clone(), v.1))
            .collect_vec();
        let lcm = notes
            .iter()
            .map(|v| v.0.denom())
            .fold(BigInt::one(), |x, y| x.lcm(y));
        let mut slots = vec![None; lcm.clone().to_usize().unwrap()];
        // if slots.len() >= 640 {  // Does it really work?
        //     bail!("Too long measure: {}, {:?}", slots.len(), notes.iter().map(|x| x.0.to_string()).collect_vec());
        // }
        for (beat, notes) in notes {
            let index = (beat * lcm.clone()).to_usize().unwrap();
            let note = notes.iter().min_by_key(|n| n.scroll).unwrap();
            slots[index] = Some((note.scroll, note.kind));
        }
        if !line_first {
            println!();
            line_first = true;
        }
        if (i % BigRational::from_integer(BigInt::from(8))).is_zero() {
            println!("#BARLINEON");
        } else {
            println!("#BARLINEOFF");
        }
        for slot in slots {
            let c = match slot {
                None => '0',
                Some((scroll, kind)) => {
                    if current_scroll != *scroll {
                        current_scroll = *scroll;
                        if !line_first {
                            println!();
                            // line_first = true;
                        }
                        println!("#SCROLL {}", *scroll / 125.);
                    }
                    use taiko_untitled::structs::NoteColor::*;
                    use taiko_untitled::structs::NoteSize::*;
                    match (kind.color, kind.size) {
                        (Don, Small) => '1',
                        (Ka, Small) => '2',
                        (Don, Large) => '3',
                        (Ka, Large) => '4',
                    }
                }
            };
            print!("{}", c);
            line_first = false;
        }
        print!(",");
        line_first = false;
    }
    if !line_first {
        println!();
    }
    println!("#END");

    Ok(())
}

#[derive(Clone, Debug)]
#[allow(unused)]
struct NoteScore {
    kind: SingleNoteKind,
    beat: BigRational,
    scroll: NotNan<f64>,
    line: usize,
}

#[derive(Clone, Copy, Debug)]
enum TjaElement {
    NoteChar(usize, char),
    BpmChange(f64),
    Measure(f64, f64),
    Scroll(f64),
}

fn load_score<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<(Bpm, Vec<NoteScore>)> {
    let mut measure_length = (4u64, 4u64);
    let mut beat = BigRational::zero();
    let mut notes = vec![];
    let mut hs = 1.0;
    let path = path.as_ref();

    let (mut bpm, score) = parse_score(&path)?;
    for (_measure_index, elements) in (1..).zip(score.iter()) {
        let measure_elems = elements
            .iter()
            .enumerate()
            .filter_map(|(i, x)| match x {
                TjaElement::Measure(x, y) => Some((i, (x, y))),
                _ => None,
            })
            .take(2)
            .collect_vec();
        if measure_elems.len() >= 2 {
            bail!("Mulitple #MEASURE in the same measure");
        }
        if let Some(&(measure_i, (x, y))) = measure_elems.get(0) {
            if elements.iter().enumerate().any(|(i, x)| match x {
                TjaElement::NoteChar(..) => i < measure_i,
                _ => false,
            }) {
                bail!("#MEASURE after a note in a measure");
            }
            if x.fract() > 1e-5 || y.fract() > 1e-5 {
                bail!("fractional measure");
            }
            let x = x.trunc() as u64;
            let y = y.trunc() as u64;
            measure_length = (x, y);
        }

        let note_count = elements
            .iter()
            .filter(|x| matches!(x, TjaElement::NoteChar(..)))
            .count();
        let step_measure = BigRational::new(measure_length.0.into(), measure_length.1.into());
        let step_per_note = &step_measure / &BigInt::from(note_count.max(1));

        for &element in elements {
            match element {
                TjaElement::NoteChar(i, c) => {
                    use taiko_untitled::structs::NoteColor::*;
                    use taiko_untitled::structs::NoteSize::*;
                    let kind = match c {
                        '0' => None,
                        '1' => Some((Don, Small)),
                        '2' => Some((Ka, Small)),
                        '3' => Some((Don, Large)),
                        '4' => Some((Ka, Large)),
                        _ => bail!("Unknown note char"),
                    };
                    if let Some((color, size)) = kind {
                        {
                            let d = u64::try_from(beat.denom()).context("Too large denominator")?;
                            let ends_with =
                                |s: &str| path.file_name().unwrap().to_str().unwrap().ends_with(s);
                            let exception = ends_with("BPM187.5.tja")
                                && (i == 18 || i == 53 || i == 95)
                                && (note_count == 19)
                                || ends_with("BPM218.75.tja") && (i == 18) && (note_count == 44);
                            // divisor of 48 or 64 => well, we need 192 or 128... and 144 ?!
                            if !(192 % d == 0 || 128 % d == 0 || 144 % d == 0 || exception) {
                                bail!(
                                    "File {:?} Line {}: {}/{} => {} {:?}",
                                    path,
                                    i,
                                    measure_length.0,
                                    measure_length.1,
                                    step_per_note,
                                    elements
                                );
                            }
                        }
                        notes.push(NoteScore {
                            beat: beat.clone(),
                            kind: SingleNoteKind { color, size },
                            scroll: NotNan::new(bpm.0 * hs)?,
                            line: i,
                        });
                    }
                    beat += &step_per_note;
                }
                TjaElement::BpmChange(b) => bpm = Bpm(b),
                TjaElement::Measure(_, _) => {}
                TjaElement::Scroll(s) => hs = s,
            }
        }
        if note_count == 0 {
            beat += &step_measure;
        }
    }

    Ok((bpm, notes))
}

#[allow(clippy::if_same_then_else)]
fn parse_score<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<(Bpm, Vec<Vec<TjaElement>>)> {
    let buf = fs_err::read(&path)?;
    let mut detector = EncodingDetector::new();
    detector.feed(&buf, true);
    let encoding = detector.guess(None, true);
    let (source, actual_encoding, replacement) = encoding.decode(&buf);
    if encoding != actual_encoding || replacement {
        bail!("Failed to decode {:?}", path);
    }
    let mut lines = (1..).zip(source.lines());

    let bpm = lines
        .find_map(|(_, line)| {
            let bpm = line.strip_prefix("BPM:")?;
            let bpm = bpm.parse_first()?;
            (bpm > 0.0).then(|| Bpm(bpm))
        })
        .context("BPM not found")?;

    lines.by_ref().find(|x| x.1.starts_with("#START"));

    let mut elements_buffer = vec![];
    let mut measures = vec![];

    for (i, line) in lines {
        // TODO check if this parser is compatible
        let line = line
            .split("//")
            .next()
            .expect("Unexpected: split() must have one element");
        if line.starts_with("#END") {
            break;
        } else if let Some(bpm) = line.strip_prefix("#BPMCHANGE") {
            if let Some(bpm) = bpm.parse_first() {
                elements_buffer.push(TjaElement::BpmChange(bpm));
            } else {
                eprintln!("Parse error: {}", line);
            }
        } else if line.starts_with("#GOGOSTART") {
        } else if line.starts_with("#GOGOEND") {
        } else if let Some(measure) = line.strip_prefix("#MEASURE") {
            if let [x, y] = &measure.split('/').collect_vec()[..] {
                if let (Some(x), Some(y)) = (x.parse_first(), y.parse_first()) {
                    elements_buffer.push(TjaElement::Measure(x, y));
                }
            }
        } else if let Some(scroll) = line.strip_prefix("#SCROLL") {
            if let Some(scroll) = scroll.parse_first() {
                elements_buffer.push(TjaElement::Scroll(scroll));
            } else {
                println!("Ignored: {}", line);
            }
        } else if let Some(_delay) = line.strip_prefix("#DELAY") {
            bail!("Delay cannot be used.");
        } else if let Some(_branch_condition) = line.strip_prefix("#BRANCHSTART") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#BRANCHEND") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#N") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#E") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#M") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#SECTION") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#LEVELHOLD") {
            bail!("Branches cannot be used.");
        } else if line.starts_with("#BARLINEON") {
        } else if line.starts_with("#BARLINEOFF") {
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
            elements_buffer.extend(line.chars().filter_map(|c| match c {
                '0'..='9' => Some(TjaElement::NoteChar(i, c)),
                _ => None,
            }));
            if split.next().is_some() {
                measures.push(elements_buffer);
                elements_buffer = Vec::new();
            }
        }
    }

    Ok((bpm, measures))
}
