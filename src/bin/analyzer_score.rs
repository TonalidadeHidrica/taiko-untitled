use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::bail;
use chardetng::EncodingDetector;
use clap::Parser;
use itertools::Itertools;

use taiko_untitled::tja::ParseFirst;

#[derive(Parser)]
struct Opts {
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    println!("{:?}", load_score(&opts.file));
    Ok(())
}

#[derive(Clone, Debug)]
enum TjaElement {
    NoteChar(char),
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
    let mut lines = source.lines();
    lines.by_ref().find(|x| x.starts_with("#START"));

    let mut elements_buffer = vec![];
    let mut measures = vec![];

    for line in lines {
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
                '0'..='9' => Some(TjaElement::NoteChar(c)),
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
