use crate::tja::{
    Note as RawNote, NoteColor, NoteContent as NC, Score, SingleNoteContent, SingleNoteKind,
};
use itertools::Itertools;
use std::collections::VecDeque;

pub struct GameState<'a> {
    #[allow(dead_code)]
    score: &'a Score,
    notes: Vec<Note>,

    auto: bool,

    // maybe this should not be here
    flying_notes: VecDeque<FlyingNote>,
    judge_strs: VecDeque<JudgeStr>,
}

pub struct Note {
    pub note: RawNote,
    pub remains: bool,
}

impl From<&RawNote> for Note {
    fn from(note: &RawNote) -> Self {
        Note {
            note: note.clone(),
            remains: true,
        }
    }
}

pub struct FlyingNote {
    pub time: f64,
    pub kind: SingleNoteKind,
}

pub struct JudgeStr {
    pub time: f64,
    pub judge: Judge,
}

pub enum Judge {
    Good,
    Ok,
    Bad,
}

impl<'a> GameState<'a> {
    pub fn new(score: &'a Score) -> Self {
        GameState {
            score,
            notes: score.notes.iter().map(Into::into).collect_vec(),

            auto: false,
            flying_notes: Default::default(),
            judge_strs: Default::default(),
        }
    }

    fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
        dbg!(auto);
    }

    pub fn switch_auto(&mut self) -> bool {
        self.set_auto(!self.auto);
        self.auto
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn hit(&mut self, color: NoteColor, time: f64) {
        if let Some((remains, kind, diff)) = self
            .notes
            .iter_mut()
            .filter_map(|note| match note {
                Note {
                    remains: true,
                    note:
                        RawNote {
                            content:
                                NC::Normal(SingleNoteContent {
                                    kind,
                                    time: note_time,
                                    ..
                                }),
                            ..
                        },
                } if kind.color == color => {
                    let diff = (time - *note_time).abs();
                    if diff <= 0.150 / 2.0 {
                        // let kind = kind.clone();
                        Some((&mut note.remains, &*kind, diff))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .next()
        {
            *remains = false;
            self.flying_notes.push_back(FlyingNote {
                time,
                kind: kind.clone(),
            });
            self.judge_strs.push_back(JudgeStr {
                time,
                judge: if diff <= 0.050 / 2.0 {
                    Judge::Good
                } else {
                    Judge::Ok
                },
            });
        }
    }

    pub fn flying_notes<F>(&mut self, filter_out: F) -> impl DoubleEndedIterator<Item = &FlyingNote>
    where
        F: FnMut(&&FlyingNote) -> bool,
    {
        filter_out_and_iter(&mut self.flying_notes, filter_out)
    }

    pub fn judge_strs<F>(&mut self, filter_out: F) -> impl DoubleEndedIterator<Item = &JudgeStr>
    where
        F: FnMut(&&JudgeStr) -> bool,
    {
        filter_out_and_iter(&mut self.judge_strs, filter_out)
    }
}

fn filter_out_and_iter<T, F>(
    vec: &mut VecDeque<T>,
    filter_out: F,
) -> impl DoubleEndedIterator<Item = &T>
where
    F: FnMut(&&T) -> bool,
{
    let count = vec.iter().take_while(filter_out).count();
    vec.drain(..count);
    vec.iter()
}
