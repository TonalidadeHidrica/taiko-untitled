use crate::structs::{
    typed::{NoteContent, QuotaRenda, RendaContent, RendaKind, SingleNote, UnlimitedRenda},
    *,
};
use crate::tja::Score;
use itertools::Itertools;
use std::collections::VecDeque;
use std::convert::Infallible;

pub struct OfGameState(Infallible);
impl typed::NoteInfo for OfGameState {
    type Note = ();
    type SingleNote = SingleNoteState;
    type RendaContent = RendaState;
    type UnlimitedRenda = ();
    type QuotaRenda = QuotaRendaState;
}

#[derive(Default, Debug, Clone)]
pub struct SingleNoteState {
    pub hit: bool,
}
#[derive(Default, Debug, Clone)]
pub struct RendaState {
    pub count: u64,
}
#[derive(Default, Debug, Clone)]
pub struct QuotaRendaState {
    pub finished: bool,
}

// TODO use define_types macro
pub type Note = typed::Note<OfGameState>;

pub struct GameState<'a> {
    #[allow(dead_code)]
    score: &'a Score,
    notes: Vec<Note>,

    auto: bool,

    // maybe this should not be here
    flying_notes: VecDeque<FlyingNote>,
    judge_strs: VecDeque<JudgeStr>,
}

impl From<&just::Note> for Note {
    fn from(note: &just::Note) -> Self {
        Self {
            scroll_speed: note.scroll_speed.clone(),
            time: note.time,
            content: match &note.content {
                NoteContent::Single(note) => NoteContent::Single(SingleNote {
                    kind: note.kind.clone(),
                    info: Default::default(),
                }),
                NoteContent::Renda(note) => NoteContent::Renda(RendaContent {
                    kind: match &note.kind {
                        RendaKind::Unlimited(note) => RendaKind::Unlimited(UnlimitedRenda {
                            size: note.size.clone(),
                            info: (),
                        }),
                        RendaKind::Quota(note) => RendaKind::Quota(QuotaRenda {
                            kind: note.kind.clone(),
                            quota: note.quota,
                            info: Default::default(),
                        }),
                    },
                    end_time: note.end_time,
                    info: Default::default(),
                }),
            },
            info: (),
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
        if let Some((note, diff)) = self
            .notes
            .iter_mut()
            .filter_map(|note| match note {
                Note {
                    time: note_time,
                    content:
                        typed::NoteContent::Single(
                            note
                            @
                            SingleNote {
                                info: SingleNoteState { hit: false },
                                ..
                            },
                        ),
                    ..
                } if note.kind.color == color => {
                    let diff = (time - *note_time).abs();
                    if diff <= 0.150 / 2.0 {
                        // let kind = kind.clone();
                        Some((note, diff))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .next()
        {
            note.info.hit = true;
            self.flying_notes.push_back(FlyingNote {
                time,
                kind: note.kind.clone(),
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
