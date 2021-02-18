use std::path::PathBuf;

use crate::tja::Song;

pub enum GameMode {
    Play {
        music_position: Option<f64>,
    },
    Pause {
        path: PathBuf,
        song: Song,
        music_position: f64,
    },
    Exit,
}
