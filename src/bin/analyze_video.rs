use std::{collections::BTreeMap, io::BufWriter, path::PathBuf};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use ffmpeg4::{format, frame, media};

use fs_err::File;
use serde::{Deserialize, Serialize};
use taiko_untitled::{
    analyze::{detect_note_positions, DetectedNotePositionsResult},
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
}

#[derive(Args)]
struct VideoToNotePositions {
    video_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    match &opts.sub {
        Sub::VideoToNotePositions(args) => video_to_note_positions(args),
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

#[derive(Serialize, Deserialize)]
struct NotePositionsResult {
    time_base: (i32, i32),
    results: BTreeMap<i64, DetectedNotePositionsResult>,
}
