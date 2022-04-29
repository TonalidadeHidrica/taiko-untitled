use std::collections::BTreeMap;

use ffmpeg4::frame;
use itertools::Itertools;
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

use crate::{
    game_graphics::game_rect,
    structs::{NoteColor, NoteSize, SingleNoteKind},
};

pub type NoteEndpoint = (bool, f64, f64, bool);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DetectedNote {
    pub kind: SingleNoteKind,
    pub left: NoteEndpoint,
    pub right: NoteEndpoint,
}
impl DetectedNote {
    pub fn note_x(self) -> f64 {
        (self.left.2 + self.right.1) / 2. - 195. / 2.
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetectedNotePositionsResult {
    pub list: Vec<NoteEndpoint>,
    pub notes: Vec<DetectedNote>,
}

#[derive(Serialize, Deserialize)]
pub struct NotePositionsResult {
    pub time_base: (i32, i32),
    pub results: BTreeMap<i64, DetectedNotePositionsResult>,
}

pub fn detect_note_positions(frame: &frame::Video) -> DetectedNotePositionsResult {
    let focus_y = 385;

    let s = frame.stride(0);
    let data = &frame.data(0)[focus_y as usize * s..];

    let mut list = vec![];
    let mut notes = vec![];
    let mut bef = 0;
    let mut start = None;
    for (i, &d) in data
        .iter()
        .enumerate()
        .take(1920)
        .skip(game_rect().x as usize)
    {
        let intersection = || i as f64 + (200.0 - bef as f64) / (d as f64 - bef as f64);
        if bef <= 200 && 200 < d {
            start = Some(intersection());
        } else if bef > 200 && 200 >= d {
            let start = start.take().expect("There should always be a start");
            let end = intersection();
            let bef = {
                let t = start as usize;
                let s = t.saturating_sub(7).max(game_rect().x as usize);
                data[s..t].iter().any(|&d| d <= 48)
            };
            let aft = {
                let s = end as usize;
                let t = s.saturating_add(7).min(1920);
                data[s..t].iter().any(|&d| d <= 48)
            };
            list.push(Some((bef, start, end, aft)));
        }
        bef = d;
    }
    for i in 1..=list.len().saturating_sub(1) {
        let (s, t) = list.split_at_mut(i);
        let (s_opt, t_opt) = (s.last_mut().unwrap(), &mut t[0]);
        let (s, t) = match (*s_opt, *t_opt) {
            (Some(s), Some(t)) => (s, t),
            _ => continue,
        };
        if s.0 && !s.3 && !t.0 && t.3 {
            // 77, 119
            let size = t.1 - s.2;
            let size = if (72.0..82.0).contains(&size) {
                Some(NoteSize::Small)
            } else if (115.0..125.0).contains(&size) {
                Some(NoteSize::Large)
            } else {
                None
            };
            let color = {
                let k = (focus_y as usize / 2) * frame.stride(2);
                let data = &frame.data(2)[k..];
                let (mut pos, mut neg) = (0, 0);
                for &d in ((s.2 as usize) / 2..=(t.1 as usize) / 2).filter_map(|i| data.get(i)) {
                    if d >= 128 {
                        pos += 1;
                    } else {
                        neg += 1;
                    }
                }
                if pos > neg {
                    NoteColor::Don
                } else {
                    NoteColor::Ka
                }
            };
            if let Some(size) = size {
                notes.push(DetectedNote {
                    left: s,
                    right: t,
                    kind: SingleNoteKind { size, color },
                });
                s_opt.take();
                t_opt.take();
            }
        }
    }
    let list = list.into_iter().flatten().collect_vec();
    DetectedNotePositionsResult { list, notes }
}

#[derive(Serialize, Deserialize)]
pub struct GroupNotesResult {
    pub groups: Vec<GroupedNote>,
}

#[derive(Serialize, Deserialize)]
pub struct GroupedNote {
    pub kind: SingleNoteKind,
    pub positions: Vec<(i64, NotNan<f64>)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SegmentList {
    pub kind: SegmentListKind,
    pub points: Vec<(i64, f64)>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum SegmentListKind {
    Add,
    Remove,
    Measure,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetermineFrameTimeResult {
    pub durations: Vec<((i64, i64), f64)>,
    pub segments: Vec<(i64, i64)>,
    pub notes: Vec<DeterminedNote>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeterminedNote {
    pub kind: SingleNoteKind,
    pub a: f64,
    pub b: f64,
}
