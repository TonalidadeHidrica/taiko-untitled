use std::path::Path;

use anyhow::{anyhow, Context};

use taiko_untitled::tja::load_tja_from_file;

fn main() -> anyhow::Result<()> {
    Ok(())
}

fn load_score<P: AsRef<Path>>(p: P) -> anyhow::Result<()> {
    let song = load_tja_from_file(p).map_err(|e| anyhow!("{:?}", e))?;
    let score = song.score.context("This tja does not have a score");
    Ok(())
}
