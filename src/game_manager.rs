use crate::game_graphics::BranchAnimationState;
use crate::structs::*;
use boolinator::Boolinator;
use enum_map::{enum_map, Enum, EnumMap};
use itertools::Itertools;
use num::clamp;
use std::collections::VecDeque;
use std::convert::Infallible;

#[derive(Debug)]
pub struct OfGameState(Infallible);

impl typed::AdditionalInfo for OfGameState {
    type Note = ();
    type SingleNote = SingleNoteInfo;
    type RendaContent = RendaState;
    type UnlimitedRenda = ();
    type QuotaRenda = QuotaRendaState;
    type Branch = BranchState;
}

#[derive(Default, Debug, Clone)]
pub struct SingleNoteInfo {
    pub judge: Option<JudgeOrPassed>,
    gauge_delta: EnumMap<Judge, f64>,
}
impl SingleNoteInfo {
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

#[derive(Default, Debug, Clone)]
pub struct BranchState {
    pub determined_branch: Option<BranchType>,
}

impl SingleNote {
    fn corresponds(&self, color: &Option<NoteColor>) -> bool {
        color.as_ref().map_or(false, |c| &self.kind.color == c)
    }
}

// // TODO use define_types macro
// pub type Note = typed::Note;
// pub type Branch = typed::Branch;
define_types!(OfGameState);

pub struct GameManager {
    pub score: Score,

    auto: bool,

    judge_pointer: usize,
    judge_bad_pointer: usize,
    judge_branch_pointer: usize,
    judge_branch_bad_pointer: usize,

    next_branch_pointer: usize,
    game_state_section: GameState,
    branch_event_pointer: usize,
    branch_event_branch_pointer: usize,

    pub game_state: GameState,
    pub animation_state: AnimationState,
}

#[derive(Clone, Copy, Default, Debug, derive_more::Sub)]
pub struct GameState {
    // The following integers are signed integers to enable subtractions
    pub score: i64,

    pub good_count: i64,
    pub ok_count: i64,
    pub bad_count: i64,
    pub renda_count: i64,

    pub combo: i64,
    // f64 has enough precision.  See the test below
    pub gauge: f64,
}

impl GameState {
    pub fn judge_count_mut(&mut self, judge: Judge) -> &mut i64 {
        match judge {
            Judge::Good => &mut self.good_count,
            Judge::Ok => &mut self.ok_count,
            Judge::Bad => &mut self.bad_count,
        }
    }

    fn update_with_judge<J: Into<JudgeOrPassed>>(&mut self, note: &mut SingleNote, judge: J) {
        let judge = judge.into();
        let was_none = note.info.judge.is_none();
        note.info.judge = Some(judge);

        if was_none {
            let judge = judge.into();
            *self.judge_count_mut(judge) += 1;
            match judge {
                Judge::Bad => self.combo = 0,
                _ => self.combo += 1,
            }
            self.gauge = clamp(self.gauge + note.info.gauge_delta[judge], 0.0, 10000.0);
        }
    }
}

// TODO move entire animation state
#[derive(Default)]
pub struct AnimationState {
    flying_notes: VecDeque<FlyingNote>,
    judge_strs: VecDeque<JudgeStr>,
    pub last_combo_update: f64,
    pub branch_state: BranchAnimationState,
}

impl Note {
    fn new(note: &just::Note, gauge_delta: &EnumMap<Judge, f64>) -> Self {
        Self {
            scroll_speed: note.scroll_speed,
            time: note.time,
            content: match &note.content {
                just::NoteContent::Single(note) => NoteContent::Single(SingleNote {
                    kind: note.kind,
                    info: SingleNoteInfo {
                        judge: None,
                        gauge_delta: *gauge_delta,
                    },
                }),
                just::NoteContent::Renda(note) => NoteContent::Renda(RendaContent {
                    kind: match &note.kind {
                        just::RendaKind::Unlimited(note) => RendaKind::Unlimited(UnlimitedRenda {
                            size: note.size,
                            info: (),
                        }),
                        just::RendaKind::Quota(note) => RendaKind::Quota(QuotaRenda {
                            kind: note.kind,
                            quota: note.quota,
                            info: Default::default(),
                        }),
                    },
                    end_time: note.end_time,
                    info: Default::default(),
                }),
            },
            branch: note.branch,
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

impl From<JudgeOrPassed> for Judge {
    fn from(judge: JudgeOrPassed) -> Self {
        match judge {
            JudgeOrPassed::Judge(judge) => judge,
            _ => Judge::Bad,
        }
    }
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum Judge {
    Good,
    Ok,
    Bad,
}

// https://discord.com/channels/194465239708729352/194465566042488833/657745859060039681
const GOOD_WINDOW: f64 = 25.0250015258789 / 1000.0;
const OK_WINDOW: f64 = 75.0750045776367 / 1000.0;
const BAD_WINDOW: f64 = 108.441665649414 / 1000.0;

fn get_gauge_good_delta(score: &just::Score) -> f64 {
    let mut counts = EnumMap::<_, usize>::new();
    for note in &score.notes {
        if let just::NoteContent::Single(..) = note.content {
            match note.branch {
                Some(branch) => counts[branch] += 1,
                None => counts.values_mut().for_each(|v| *v += 1),
            }
        }
    }
    let combo_count = counts.values().max().unwrap();
    // TODO change values depending on difficulties
    match *combo_count {
        n if n >= 1 => (13113.0 / n as f64).round(),
        _ => 0.0,
    }
}

impl GameManager {
    pub fn new(score: &just::Score) -> Self {
        let good_delta = get_gauge_good_delta(score);
        let gauge_delta = enum_map![
            Judge::Good => good_delta,
            Judge::Ok => (good_delta / 2.0).trunc(),
            Judge::Bad => -good_delta * 2.0,
        ];
        Self {
            score: Score {
                notes: score
                    .notes
                    .iter()
                    .map(|note| Note::new(note, &gauge_delta))
                    .collect_vec(),
                bar_lines: score.bar_lines.clone(),
                branches: score
                    .branches
                    .iter()
                    .map(|b| b.with_info(BranchState::default()))
                    .collect_vec(),
                branch_events: score.branch_events.clone(),
            },

            auto: false,

            judge_pointer: 0,
            judge_bad_pointer: 0,
            judge_branch_pointer: 0,
            judge_branch_bad_pointer: 0,

            next_branch_pointer: 0,
            game_state_section: Default::default(),
            branch_event_pointer: 0,
            branch_event_branch_pointer: 0,

            game_state: Default::default(),
            animation_state: Default::default(),
        }
    }

    pub fn auto(&self) -> bool {
        self.auto
    }

    fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
        dbg!(auto);
    }

    pub fn switch_auto(&mut self) -> bool {
        self.set_auto(!self.auto);
        self.auto
    }

    pub fn hit(&mut self, color: Option<NoteColor>, time: f64) {
        // Process branch events (i.e. #LEVELHOLD and #SECTION)
        while let Some(event) = self.score.branch_events.get(self.branch_event_pointer) {
            if time < event.time {
                break;
            }
            match event.kind {
                BranchEventKind::Section => {
                    self.game_state_section = self.game_state;
                }
                BranchEventKind::LevelHold(branch) => {
                    if branch
                        == branch_at(
                            &self.score.branches,
                            &mut self.branch_event_branch_pointer,
                            event.time,
                        )
                    {
                        println!("Level Holded");
                        self.score.branches[self.next_branch_pointer..]
                            .iter_mut()
                            .for_each(|v| v.info.determined_branch = Some(branch));
                        self.next_branch_pointer = self.score.branches.len();
                    }
                }
            }
            self.branch_event_pointer += 1;
        }

        // Determine upcoming branch if needed
        if let Some(branch) = self.score.branches.get_mut(self.next_branch_pointer) {
            if branch.judge_time <= time {
                let diff = self.game_state - self.game_state_section;
                let new_branch = match branch.condition {
                    BranchCondition::Pass => None,
                    BranchCondition::Precision(e, m) => {
                        let score = 2 * diff.good_count + diff.ok_count;
                        let total = 2 * (diff.good_count + diff.ok_count + diff.bad_count);
                        let precision = if total == 0 {
                            0.0
                        } else {
                            score as f64 / total as f64 * 100.0
                        };
                        branch_by_candidate(precision, e, m).into()
                    }
                    BranchCondition::Renda(e, m) => {
                        branch_by_candidate(diff.renda_count, e, m).into()
                    }
                    BranchCondition::Score(e, m) => branch_by_candidate(diff.score, e, m).into(),
                };
                branch.info.determined_branch = new_branch;
                self.next_branch_pointer += 1;

                if let Some(new_branch) = new_branch {
                    self.animation_state.branch_state.set(new_branch, time);
                }
            }
        }

        let Self {
            game_state,
            animation_state,
            score: Score {
                notes, branches, ..
            },
            judge_pointer,
            judge_bad_pointer,
            judge_branch_pointer,
            judge_branch_bad_pointer,
            ..
        } = self;

        let check_note = |note: &mut Note, branch_matches: bool| match note.content {
            NoteContent::Single(ref mut single_note) => match note.time - time {
                t if t.abs() <= OK_WINDOW => {
                    if single_note.info.judge.is_none()
                        && single_note.corresponds(&color)
                        && branch_matches
                    {
                        let judge = if t.abs() <= GOOD_WINDOW {
                            Judge::Good
                        } else {
                            Judge::Ok
                        };

                        game_state.update_with_judge(single_note, judge);
                        animation_state.flying_notes.push_back(FlyingNote {
                            time,
                            kind: single_note.kind,
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
                    if single_note.info.judge.is_none() && branch_matches {
                        game_state.update_with_judge(single_note, JudgeOrPassed::Passed);
                    }
                    JudgeOnTimeline::Past
                }
                t if t > 0.0 => JudgeOnTimeline::Break,
                _ => unreachable!(),
            },
            NoteContent::Renda(ref mut renda) => match () {
                _ if note.time <= time && time < renda.end_time => {
                    if branch_matches {
                        match (&mut renda.kind, &color) {
                            (RendaKind::Unlimited(renda_u), &Some(color)) => {
                                game_state.renda_count += 1;
                                renda.info.count += 1;
                                animation_state.flying_notes.push_back(FlyingNote {
                                    time,
                                    kind: SingleNoteKind {
                                        color,
                                        size: renda_u.size,
                                    },
                                });
                            }
                            (RendaKind::Quota(ref mut renda_q), Some(NoteColor::Don)) => {
                                if !renda_q.info.finished {
                                    game_state.renda_count += 1;
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
                    } else {
                        JudgeOnTimeline::Continue
                    }
                }
                _ if renda.end_time <= time => JudgeOnTimeline::Past,
                _ if time < note.time => match branch_matches {
                    true => JudgeOnTimeline::Break,
                    false => JudgeOnTimeline::Continue,
                },
                _ => unreachable!(),
            },
        };
        let first_hit_check = check_note_wrapper(
            notes,
            branches,
            judge_pointer,
            judge_branch_pointer,
            check_note,
        )
        .is_some();

        let check_note_bad = |note: &mut Note, branch_matches: bool| {
            if let NoteContent::Single(ref mut single_note) = note.content {
                match note.time - time {
                    t if t.abs() <= BAD_WINDOW => {
                        if matches!(single_note.info.judge, None | Some(JudgeOrPassed::Passed))
                            && single_note.corresponds(&color)
                            && branch_matches
                        {
                            let judge = Judge::Bad;
                            game_state.update_with_judge(single_note, judge);
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
        };
        if !first_hit_check {
            check_note_wrapper(
                notes,
                branches,
                judge_bad_pointer,
                judge_branch_bad_pointer,
                check_note_bad,
            );
        }
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

fn branch_at(branches: &[Branch], branch_pointer: &mut usize, time: f64) -> BranchType {
    while branches.get(*branch_pointer).map_or(false, |branch| {
        branch.switch_time <= time && branch.info.determined_branch.is_some()
    }) {
        *branch_pointer += 1;
    }
    (*branch_pointer > 0)
        .and_option_from(|| branches[*branch_pointer - 1].info.determined_branch)
        .unwrap_or(BranchType::Normal)
}

pub fn check_note_wrapper<F, T>(
    notes: &mut [Note],
    branches: &[Branch],
    judge_pointer: &mut usize,
    judge_branch_pointer: &mut usize,
    mut check_note: F,
) -> Option<T>
where
    F: FnMut(&mut Note, bool) -> JudgeOnTimeline<T>,
{
    let mut branch_pointer = *judge_branch_pointer;
    check_on_timeline(notes, judge_pointer, |note: &mut Note| {
        let branch = branch_at(branches, &mut branch_pointer, note.time);
        let branch_matches = note.branch.map_or(true, |b| b == branch);

        let ret = check_note(note, branch_matches);

        if let JudgeOnTimeline::Past = ret {
            *judge_branch_pointer = branch_pointer;
        }
        ret
    })
}

fn branch_by_candidate<T>(v: T, e: T, m: T) -> BranchType
where
    T: PartialOrd + std::fmt::Debug,
{
    match v {
        v if v >= m => BranchType::Master,
        v if v >= e => BranchType::Expert,
        _ => BranchType::Normal,
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
#[derive(Debug)]
pub enum JudgeOnTimeline<T> {
    Past,
    Continue,
    BreakWith(T),
    Break,
}

fn check_on_timeline<T, U, F>(vec: &mut [T], pointer: &mut usize, mut f: F) -> Option<U>
where
    F: FnMut(&mut T) -> JudgeOnTimeline<U>,
{
    let origin = *pointer;
    for (mut i, e) in vec[origin..].iter_mut().enumerate() {
        i += origin;
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
