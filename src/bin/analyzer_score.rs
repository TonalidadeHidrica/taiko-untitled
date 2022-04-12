use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chardetng::EncodingDetector;
use clap::Parser;
use itertools::Itertools;

use num::{BigInt, BigRational, Zero};
use taiko_untitled::{structs::SingleNoteKind, tja::ParseFirst};

#[derive(Parser)]
struct Opts {
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let mut measure_length = (4u64, 4u64);
    let mut beat = BigRational::zero();
    let mut notes = vec![];

    let score = load_score(&opts.file)?;
    for (i, elements) in (1..).zip(score.iter()) {
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
        {
            let d = u64::try_from(step_per_note.denom()).context("Too large denominator")?;
            // divisor of 48 or 64
            if 48 % d > 0 && 64 % d > 0 {
                bail!(
                    "Measure {}: {}/{} => {} {:?}",
                    i, measure_length.0, measure_length.1, step_per_note, elements
                );
            }
        }

        for element in elements {
            match element {
                TjaElement::NoteChar(_, c) => {
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
                        notes.push(NoteScore {
                            beat: beat.clone(),
                            kind: SingleNoteKind { color, size },
                        });
                    }
                    beat += &step_per_note;
                }
                TjaElement::BpmChange(_) => {},
                TjaElement::Measure(_, _) => {},
                TjaElement::Scroll(_) => {},
            }
        }
        if note_count == 0 {
            beat += &step_measure;
        }
    }
    println!("{:?}", notes);
    Ok(())
}

#[derive(Clone, Debug)]
struct NoteScore {
    kind: SingleNoteKind,
    beat: BigRational,
}

#[derive(Clone, Copy, Debug)]
enum TjaElement {
    NoteChar(usize, char),
    BpmChange(f64),
    Measure(f64, f64),
    Scroll(f64),
}

#[allow(clippy::if_same_then_else)]
fn load_score<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<Vec<Vec<TjaElement>>> {
    let buf = fs_err::read(&path)?;
    let mut detector = EncodingDetector::new();
    detector.feed(&buf, true);
    let encoding = detector.guess(None, true);
    let (source, actual_encoding, replacement) = encoding.decode(&buf);
    if encoding != actual_encoding || replacement {
        bail!("Failed to decode {:?}", path);
    }
    let mut lines = (1..).zip(source.lines());
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

    Ok(measures)
}
