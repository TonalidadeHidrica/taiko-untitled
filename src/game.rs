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

impl<'a> GameState<'a> {
    pub fn new(score: &'a Score) -> Self {
        GameState {
            score,
            notes: score.notes.iter().map(Into::into).collect_vec(),

            auto: false,
            flying_notes: Default::default(),
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
        if let Some((remains, kind)) = self
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
                    if (time - *note_time).abs() <= 0.150 / 2.0 {
                        // let kind = kind.clone();
                        Some((&mut note.remains, &*kind))
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
        }
    }

    pub fn flying_notes<F>(&mut self, filter_out: F) -> impl Iterator<Item = &FlyingNote>
    where
        F: FnMut(&&FlyingNote) -> bool,
    {
        let count = self.flying_notes.iter().take_while(filter_out).count();
        self.flying_notes.drain(..count);
        self.flying_notes.iter()
    }
}
