use crate::structs::{
    typed::{NoteContent, QuotaRenda, RendaContent, RendaKind, SingleNote, UnlimitedRenda},
    *,
};
use crate::tja::Score;
use itertools::Itertools;
use num::clamp;
use std::collections::VecDeque;
use std::convert::Infallible;

#[derive(Debug)]
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
    pub judge: Option<JudgeOrPassed>,
}
impl SingleNoteState {
    pub fn visible(&self) -> bool {
        !matches!(self.judge, Some(JudgeOrPassed::Judge(..)))
    }
}

#[derive(Default, Debug, Clone)]
pub struct RendaState {
    pub count: u64,
}
#[derive(Default, Debug, Clone)]
pub struct QuotaRendaState {
    // TODO we don't actually need this field
    pub finished: bool,
}

impl SingleNote<OfGameState> {
    fn corresponds(&self, color: &Option<NoteColor>) -> bool {
        color.as_ref().map_or(false, |c| &self.kind.color == c)
    }
}

// TODO use define_types macro
pub type Note = typed::Note<OfGameState>;

pub struct GameManager<'a> {
    #[allow(dead_code)]
    score: &'a Score,
    notes: Vec<Note>,

    auto: bool,

    judge_pointer: usize,
    judge_bad_pointer: usize,

    pub game_state: GameState,
    pub animation_state: AnimationState,
}

#[derive(Default, Debug)]
pub struct GameState {
    pub good_count: u32,
    pub ok_count: u32,
    pub bad_count: u32,

    pub combo: u32,
    // f64 has enough precision.  See the test below
    pub gauge: f64,
}

impl GameState {
    pub fn judge_count_mut(&mut self, judge: Judge) -> &mut u32 {
        match judge {
            Judge::Good => &mut self.good_count,
            Judge::Ok => &mut self.ok_count,
            Judge::Bad => &mut self.bad_count,
        }
    }

    fn update_with_judge(&mut self, judge: Judge) {
        *self.judge_count_mut(judge) += 1;
        match judge {
            Judge::Bad => self.combo = 0,
            _ => self.combo += 1,
        }
        self.gauge += match judge {
            Judge::Good => 20,
            Judge::Ok => 10,
            Judge::Bad => -40,
        } as f64;
        self.gauge = clamp(self.gauge, 0.0, 10000.0);
    }
}

#[derive(Default)]
pub struct AnimationState {
    flying_notes: VecDeque<FlyingNote>,
    judge_strs: VecDeque<JudgeStr>,
    pub last_combo_update: f64,
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

#[derive(Clone, Copy, Debug)]
pub enum JudgeOrPassed {
    Judge(Judge),
    Passed,
}

impl From<Judge> for JudgeOrPassed {
    fn from(judge: Judge) -> Self {
        Self::Judge(judge)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Judge {
    Good,
    Ok,
    Bad,
}

// https://discord.com/channels/194465239708729352/194465566042488833/657745859060039681
const GOOD_WINDOW: f64 = 25.0250015258789 / 1000.0;
const OK_WINDOW: f64 = 75.0750045776367 / 1000.0;
const BAD_WINDOW: f64 = 108.441665649414 / 1000.0;

impl<'a> GameManager<'a> {
    pub fn new(score: &'a Score) -> Self {
        GameManager {
            score,
            notes: score.notes.iter().map(Into::into).collect_vec(),

            auto: false,

            judge_pointer: 0,
            judge_bad_pointer: 0,

            game_state: Default::default(),
            animation_state: Default::default(),
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

    pub fn hit(&mut self, color: Option<NoteColor>, time: f64) {
        let game_state = &mut self.game_state;
        let animation_state = &mut self.animation_state;
        let _ = check_on_timeline(&mut self.notes, &mut self.judge_pointer, |note| match note
            .content
        {
            NoteContent::Single(ref mut single_note) => match note.time - time {
                t if t.abs() <= OK_WINDOW => {
                    if single_note.info.judge.is_none() && single_note.corresponds(&color) {
                        let judge = if t.abs() <= GOOD_WINDOW {
                            Judge::Good
                        } else {
                            Judge::Ok
                        };
                        single_note.info.judge = Some(judge.into());

                        game_state.update_with_judge(judge);
                        animation_state.flying_notes.push_back(FlyingNote {
                            time,
                            kind: single_note.kind.clone(),
                        });
                        animation_state
                            .judge_strs
                            .push_back(JudgeStr { time, judge });
                        animation_state.last_combo_update = time;

                        JudgeOnTimeline::BreakWith(())
                    } else {
                        JudgeOnTimeline::Continue
                    }
                }
                t if t < 0.0 => {
                    if single_note.info.judge.is_none() {
                        single_note.info.judge = Some(JudgeOrPassed::Passed);
                        game_state.update_with_judge(Judge::Bad);
                    }
                    JudgeOnTimeline::Past
                }
                t if t > 0.0 => JudgeOnTimeline::Break,
                _ => unreachable!(),
            },
            NoteContent::Renda(ref mut renda) => match () {
                _ if note.time <= time && time < renda.end_time => {
                    match (&mut renda.kind, &color) {
                        (RendaKind::Unlimited(renda_u), Some(color)) => {
                            renda.info.count += 1;
                            animation_state.flying_notes.push_back(FlyingNote {
                                time,
                                kind: SingleNoteKind {
                                    color: color.clone(),
                                    size: renda_u.size.clone(),
                                },
                            });
                        }
                        (RendaKind::Quota(ref mut renda_q), Some(NoteColor::Don)) => {
                            if !renda_q.info.finished {
                                renda.info.count += 1;
                                if renda.info.count >= renda_q.quota {
                                    renda_q.info.finished = true;
                                }
                                animation_state.flying_notes.push_back(FlyingNote {
                                    time,
                                    kind: SingleNoteKind {
                                        color: NoteColor::Don,
                                        size: NoteSize::Small,
                                    },
                                });
                            }
                        }
                        _ => {}
                    };
                    JudgeOnTimeline::BreakWith(())
                }
                _ if renda.end_time <= time => JudgeOnTimeline::Past,
                _ if time < note.time => JudgeOnTimeline::Break,
                _ => unreachable!(),
            },
        })
        .is_some()
            || check_on_timeline(&mut self.notes, &mut self.judge_bad_pointer, |note| {
                if let NoteContent::Single(ref mut single_note) = note.content {
                    match note.time - time {
                        t if t.abs() <= BAD_WINDOW => {
                            if matches!(single_note.info.judge, None | Some(JudgeOrPassed::Passed))
                                && single_note.corresponds(&color)
                            {
                                let was_none = single_note.info.judge.is_none();
                                let judge = Judge::Bad;
                                single_note.info.judge = Some(judge.into());
                                if was_none {
                                    game_state.update_with_judge(judge);
                                }
                                animation_state
                                    .judge_strs
                                    .push_back(JudgeStr { time, judge });
                                JudgeOnTimeline::BreakWith(())
                            } else {
                                JudgeOnTimeline::Continue
                            }
                        }
                        t if t < 0.0 => JudgeOnTimeline::Past,
                        t if t > 0.0 => JudgeOnTimeline::Break,
                        _ => unreachable!(),
                    }
                } else {
                    JudgeOnTimeline::Past
                }
            })
            .is_some();
    }

    pub fn flying_notes<F>(&mut self, filter_out: F) -> impl DoubleEndedIterator<Item = &FlyingNote>
    where
        F: FnMut(&&FlyingNote) -> bool,
    {
        filter_out_and_iter(&mut self.animation_state.flying_notes, filter_out)
    }

    pub fn judge_strs<F>(&mut self, filter_out: F) -> impl DoubleEndedIterator<Item = &JudgeStr>
    where
        F: FnMut(&&JudgeStr) -> bool,
    {
        filter_out_and_iter(&mut self.animation_state.judge_strs, filter_out)
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

// TODO naming and structure
pub enum JudgeOnTimeline<T> {
    Past,
    Continue,
    BreakWith(T),
    Break,
}

fn check_on_timeline<T, U, F>(vec: &mut Vec<T>, pointer: &mut usize, mut f: F) -> Option<U>
where
    F: FnMut(&mut T) -> JudgeOnTimeline<U>,
{
    for (i, e) in vec[*pointer..].iter_mut().enumerate() {
        match f(e) {
            JudgeOnTimeline::Past => *pointer = i + 1,
            JudgeOnTimeline::Break => break,
            JudgeOnTimeline::BreakWith(u) => return Some(u),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// In the original system, gauge count is calculated as integer with maximumm value of 10000.
    /// We use f64 to store the gauge value, which is precise enough to store exact values.
    #[test]
    fn f64_has_enough_precision() {
        let mut f = 0.0;
        for i in 0..=10000 {
            assert_eq!(f, i as f32);
            f += 1.0;
        }
    }
}
