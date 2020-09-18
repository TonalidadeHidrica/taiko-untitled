use crate::tja::{Note as RawNote, NoteColor, NoteContent as NC, Score};
use itertools::Itertools;

pub struct GameState<'a> {
    #[allow(dead_code)]
    score: &'a Score,
    notes: Vec<Note>,

    auto: bool,
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

impl<'a> GameState<'a> {
    pub fn new(score: &'a Score) -> Self {
        GameState {
            score,
            notes: score.notes.iter().map(Into::into).collect_vec(),

            auto: false,
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
        if let Some(note) = self
            .notes
            .iter_mut()
            .filter(|note| match note {
                Note {
                    remains: true,
                    note:
                        RawNote {
                            content:
                                NC::Normal {
                                    color: note_color,
                                    time: note_time,
                                    ..
                                },
                            ..
                        },
                } if *note_color == color => (time - *note_time).abs() <= 0.150 / 2.0,
                _ => false,
            })
            .next()
        {
            note.remains = false;
        }
    }
}
