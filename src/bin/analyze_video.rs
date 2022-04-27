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
use itertools::{zip, Itertools};
use ordered_float::NotNan;
use taiko_untitled::{
    analyze::{
        detect_note_positions, GroupNotesResult, GroupedNote, NotePositionsResult, SegmentList,
        SegmentListKind,
    },
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
    FixGroup(FixGroup),
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

#[derive(Args)]
struct FixGroup {
    positions_path: PathBuf,
    groups_path: PathBuf,
    fix_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    match &opts.sub {
        Sub::VideoToNotePositions(args) => video_to_note_positions(args),
        Sub::GroupNotes(args) => group_notes(args),
        Sub::FixGroup(args) => fix_group(args),
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

    let stop_frames = {
        let mut stop_frames = BTreeSet::new();
        for ((_, bef), (&pts, aft)) in result.results.iter().tuple_windows() {
            if bef.notes.len() != aft.notes.len() || bef.notes.is_empty() {
                continue;
            }
            let all_same = zip(&bef.notes, &aft.notes).all(|(bef, aft)| {
                bef.kind == aft.kind && (bef.note_x() - aft.note_x()).abs() < 1.0
            });
            if all_same {
                stop_frames.insert(pts);
            }
        }
        stop_frames
    };

    let mut result = GroupNotesResult { groups: vec![] };
    for (kind, frames) in map.iter_mut() {
        for i in 0..frames.len() {
            let pts_this = frames[i].0;
            while let Some(&this) = frames[i].1.iter().next() {
                frames[i].1.remove(&this);
                let mut frames = frames[i + 1..].iter_mut();
                let (set, pts_next, next) = frames
                    .by_ref()
                    .find_map(|(pts, set)| {
                        let &next = set.range(..this).next()?;
                        Some((set, *pts, next))
                    })
                    .with_context(|| {
                        format!(
                            "Cannot find the next element for pts={}, this={}",
                            pts_this, this
                        )
                    })?;
                if stop_frames.range(pts_this..=pts_next).next().is_some() {
                    continue;
                }
                set.remove(&next);
                let mut positions = vec![(pts_this, this), (pts_next, next)];
                let (mut pts_1, mut note_x_1) = (pts_this, this);
                let (mut pts_2, mut note_x_2) = (pts_next, next);
                println!("# {:?}", positions);
                for (pts, set) in frames {
                    let stop_frame = stop_frames.contains(pts);
                    let (note_x, eps) = if stop_frame {
                        (note_x_1, NotNan::new(1.0).unwrap())
                    } else {
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
                        (note_x, eps)
                    };
                    match &set.range(note_x - eps..note_x + eps).take(2).collect_vec()[..] {
                        [] => {}
                        &[&found] => {
                            println!("  => {:?}", (*pts, found));
                            set.remove(&found);
                            positions.push((*pts, found));
                            if !stop_frame {
                                (pts_2, note_x_2) = (pts_1, note_x_1);
                                (pts_1, note_x_1) = (*pts, found);
                            } else {
                                let pts_diff = *pts - pts_1;
                                pts_1 += pts_diff;
                                pts_2 += pts_diff;
                            }
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

fn fix_group(args: &FixGroup) -> anyhow::Result<()> {
    let groups: GroupNotesResult =
        serde_json::from_reader(BufReader::new(File::open(&args.groups_path)?))?;
    let fix: Vec<SegmentList> =
        serde_json::from_reader(BufReader::new(File::open(&args.fix_path)?))?;

    // type Vertex = (i64, NotNan<f64>);
    let mut edges = BTreeMap::<_, Vec<_>>::new();
    for group in groups.groups {
        for (s, t) in group.positions.into_iter().tuple_windows() {
            edges.entry(s).or_default().push(t);
            edges.entry(t).or_default().push(s);
        }
    }
    for segment in fix {
        let points = segment
            .points
            .into_iter()
            .map(|(x, y)| (x, NotNan::new(y).unwrap()))
            .collect_vec();
        match segment.kind {
            SegmentListKind::Add => {
                for (s, t) in points.into_iter().tuple_windows() {
                    edges.entry(s).or_default().push(t);
                    edges.entry(t).or_default().push(s);
                }
            }
            SegmentListKind::Remove => {
                for (s, t) in points.into_iter().tuple_windows() {
                    let index = edges[&s].iter().position(|x| x == &t).unwrap();
                    edges.get_mut(&s).unwrap().swap_remove(index);
                    let index = edges[&t].iter().position(|x| x == &s).unwrap();
                    edges.get_mut(&t).unwrap().swap_remove(index);
                }
            }
            _ => {}
        }
    }

    let mut paths = vec![];
    while let Some((&(mut s), es)) = edges.iter().next() {
        if es.len() > 1 {
            bail!("{:?} => {:?}", s, es);
        }
        let es = edges.remove(&s).unwrap();
        let mut path = vec![s];
        if let Some(&(mut t)) = es.get(0) {
            loop {
                path.push(t);
                let es = edges.remove(&t).unwrap();
                let u = match &es[..] {
                    [_] => break,
                    &[u, ss] if s == ss => u,
                    &[ss, u] if s == ss => u,
                    es => bail!("{:?} => {:?}", t, es),
                };
                (s, t) = (t, u);
            }
        }
        paths.push(path);
    }

    Ok(())
}
