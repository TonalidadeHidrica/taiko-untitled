use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::{bail, Context};
use clap::{Args, Parser, Subcommand};
use enum_map::EnumMap;
use ffmpeg4::{format, frame, media};

use fs_err::File;
use itertools::Itertools;
use ordered_float::NotNan;
use taiko_untitled::{
    analyze::{detect_note_positions, GroupNotesResult, GroupedNote, NotePositionsResult},
    ffmpeg_utils::{next_frame, FilteredPacketIter},
};

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(Subcommand)]
enum Sub {
    VideoToNotePositions(VideoToNotePositions),
    GroupNotes(GroupNotes),
}

#[derive(Args)]
struct VideoToNotePositions {
    video_path: PathBuf,
    output_path: PathBuf,
}

#[derive(Args)]
struct GroupNotes {
    json_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    match &opts.sub {
        Sub::VideoToNotePositions(args) => video_to_note_positions(args),
        Sub::GroupNotes(args) => group_notes(args),
    }
}

fn video_to_note_positions(args: &VideoToNotePositions) -> anyhow::Result<()> {
    let mut input_context = format::input(&args.video_path)?;
    let stream = input_context
        .streams()
        .best(media::Type::Video)
        .context("No video stream found")?;
    let stream_index = stream.index();
    let time_base = stream.time_base();
    let mut decoder = stream.codec().decoder().video()?;
    decoder.set_parameters(stream.parameters())?;
    let mut packet_iterator = FilteredPacketIter(input_context.packets(), stream_index);
    let mut frame = frame::Video::empty();

    let mut result = NotePositionsResult {
        time_base: (time_base.0, time_base.1),
        results: BTreeMap::new(),
    };
    while next_frame(&mut packet_iterator, &mut decoder, &mut frame)? {
        let pts = frame.pts().unwrap();
        result.results.insert(pts, detect_note_positions(&frame));
    }

    serde_json::to_writer(BufWriter::new(File::create(&args.output_path)?), &result)?;

    Ok(())
}

fn group_notes(args: &GroupNotes) -> anyhow::Result<()> {
    let result: NotePositionsResult =
        serde_json::from_reader(BufReader::new(File::open(&args.json_path)?))?;
    // note min = 450.02
    let mut map = EnumMap::<_, Vec<_>>::default();
    for (&pts, frame) in &result.results {
        let mut tmp = EnumMap::<_, BTreeSet<_>>::default();
        for note in &frame.notes {
            tmp[note.kind].insert(NotNan::new(note.note_x()).context("note_x is NaN")?);
        }
        tmp.into_iter()
            .for_each(|(kind, notes)| map[kind].push((pts, notes)));
    }

    let mut result = GroupNotesResult { groups: vec![] };
    for (kind, frames) in map.iter_mut() {
        for i in 0..frames.len() {
            let pts_this = frames[i].0;
            while let Some(&this) = frames[i].1.iter().next() {
                frames[i].1.remove(&this);
                let mut frames = frames[i + 1..].iter_mut();
                let (pts_next, next) = frames
                    .by_ref()
                    .find_map(|(pts, set)| {
                        let &found = set.range(..this).last()?;
                        set.remove(&found);
                        Some((*pts, found))
                    })
                    .with_context(|| {
                        format!(
                            "Cannot find the next element for pts={}, this={}",
                            pts_this, this
                        )
                    })?;
                let mut positions = vec![(pts_this, this), (pts_next, next)];
                for (pts, set) in frames {
                    let (pts_1, note_x_1) = positions[positions.len() - 1];
                    let (pts_2, note_x_2) = positions[positions.len() - 2];
                    let note_x = NotNan::new(map_float(
                        *pts as f64,
                        pts_1 as f64,
                        pts_2 as f64,
                        *note_x_1,
                        *note_x_2,
                    ))
                    .context("NaN")?;
                    if *note_x < 440.0 {
                        break;
                    }
                    let eps = NotNan::new(16.0).unwrap();
                    match &set.range(note_x - eps..note_x + eps).take(2).collect_vec()[..] {
                        [] => {}
                        &[&found] => {
                            set.remove(&found);
                            positions.push((*pts, found));
                        }
                        &[&a, &b] => {
                            bail!("Multiple candidates found at pts={pts}: {a}, {b}");
                        }
                        _ => unreachable!(),
                    }
                }
                result.groups.push(GroupedNote { kind, positions });
            }
        }
    }

    serde_json::to_writer(BufWriter::new(File::create(&args.output_path)?), &result)?;

    Ok(())
}

fn map_float(x: f64, sx: f64, tx: f64, sy: f64, ty: f64) -> f64 {
    sy + (x - sx) / (tx - sx) * (ty - sy)
}
