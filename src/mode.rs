use std::path::PathBuf;

use crate::tja::Song;

pub enum GameMode {
    Play,
    Pause { path: PathBuf, song: Song },
    Exit,
}
