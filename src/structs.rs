use enum_map::Enum;

pub mod typed {
    use super::*;
    use std::fmt::Debug;

    pub trait AdditionalInfo {
        type Note: Debug + Clone;
        type SingleNote: Debug + Clone;
        type RendaContent: Debug + Clone;
        type UnlimitedRenda: Debug + Clone;
        type QuotaRenda: Debug + Clone;
        type Branch: Debug + Clone;
    }

    impl AdditionalInfo for () {
        type Note = ();
        type SingleNote = ();
        type RendaContent = ();
        type UnlimitedRenda = ();
        type QuotaRenda = ();
        type Branch = ();
    }

    #[derive(Default, Debug)]
    pub struct Score<T: AdditionalInfo> {
        pub notes: Vec<Note<T>>,
        pub bar_lines: Vec<BarLine>,
        pub branches: Vec<Branch<T>>,
        pub branch_events: Vec<BranchEvent>,
    }

    #[derive(Clone, Debug)]
    pub struct Note<T: AdditionalInfo> {
        pub scroll_speed: Bpm,
        pub time: f64,
        pub content: NoteContent<T>,
        pub branch: Option<BranchType>,
        pub info: T::Note,
    }

    // impl <T: AdditionalInfo> Note<T> {
    //     fn cloned_with<U, F>(self, f: F) -> Note<U> where U: AdditionalInfo, F: FnOnce() -> U::Note {
    //         Note {
    //             scroll_speed: self.scroll_speed,
    //             time: self.time,
    //         }
    //     }
    // }

    #[derive(Clone, Debug)]
    pub enum NoteContent<T: AdditionalInfo> {
        Single(SingleNote<T>),
        Renda(RendaContent<T>),
    }

    #[derive(Clone, Copy, Debug)]
    pub struct SingleNote<T: AdditionalInfo> {
        pub kind: SingleNoteKind,
        pub info: T::SingleNote,
    }

    impl<T: AdditionalInfo> SingleNote<T> {
        pub fn clone_with<U, F>(&self, info: F) -> SingleNote<U>
        where
            U: AdditionalInfo,
            F: FnOnce() -> U::SingleNote,
        {
            SingleNote {
                kind: self.kind,
                info: info(),
            }
        }

        pub fn clone_with_default<U>(&self) -> SingleNote<U>
        where
            U: AdditionalInfo,
            U::SingleNote: Default,
        {
            self.clone_with(Default::default)
        }
    }

    #[derive(Clone, Debug)]
    pub struct RendaContent<T: AdditionalInfo> {
        pub kind: RendaKind<T>,
        pub end_time: f64,
        pub info: T::RendaContent,
    }

    #[derive(Clone, Debug)]
    pub enum RendaKind<T: AdditionalInfo> {
        Unlimited(UnlimitedRenda<T>),
        Quota(QuotaRenda<T>),
    }

    #[derive(Clone, Copy, Debug)]
    pub struct UnlimitedRenda<T: AdditionalInfo> {
        pub size: NoteSize,
        pub info: T::UnlimitedRenda,
    }

    impl<T: AdditionalInfo> UnlimitedRenda<T> {
        pub fn clone_with<U, F>(&self, info: F) -> UnlimitedRenda<U>
        where
            U: AdditionalInfo,
            F: FnOnce() -> U::UnlimitedRenda,
        {
            UnlimitedRenda {
                size: self.size,
                info: info(),
            }
        }

        pub fn clone_with_default<U>(&self) -> UnlimitedRenda<U>
        where
            U: AdditionalInfo,
            U::UnlimitedRenda: Default,
        {
            self.clone_with(Default::default)
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct QuotaRenda<T: AdditionalInfo> {
        pub kind: QuotaRendaKind,
        pub quota: u64,
        pub info: T::QuotaRenda,
    }

    impl<T: AdditionalInfo> QuotaRenda<T> {
        pub fn clone_with<U, F>(&self, info: F) -> QuotaRenda<U>
        where
            U: AdditionalInfo,
            F: FnOnce() -> U::QuotaRenda,
        {
            QuotaRenda {
                kind: self.kind,
                quota: self.quota,
                info: info(),
            }
        }

        pub fn clone_with_default<U>(&self) -> QuotaRenda<U>
        where
            U: AdditionalInfo,
            U::QuotaRenda: Default,
        {
            self.clone_with(Default::default)
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Branch<T: AdditionalInfo> {
        pub judge_time: f64,
        pub switch_time: f64,
        pub scroll_speed: Bpm,
        pub condition: BranchCondition,
        pub info: T::Branch,
    }

    impl<T: AdditionalInfo> Branch<T> {
        pub fn with_info<U: AdditionalInfo>(&self, info: U::Branch) -> Branch<U> {
            Branch {
                judge_time: self.judge_time,
                switch_time: self.switch_time,
                scroll_speed: self.scroll_speed,
                condition: self.condition,
                info,
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LevelUra(#[allow(dead_code)] Level, #[allow(dead_code)] bool);

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Easy,
    Normal,
    Hard,
    Oni,
}

#[derive(Clone, Copy, Debug)]
pub struct SingleNoteKind {
    pub color: NoteColor,
    pub size: NoteSize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteColor {
    Don,
    Ka,
}

#[derive(Clone, Copy, Debug)]
pub enum NoteSize {
    Small,
    Large,
}

#[derive(Clone, Copy, Debug)]
pub enum QuotaRendaKind {
    Balloon,
    Potato,
}

#[derive(Clone, Copy, Debug)]
pub enum BranchCondition {
    Pass,
    Renda(i64, i64),
    Precision(f64, f64),
    Score(i64, i64),
}

#[derive(Clone, Copy, Debug)]
pub struct Measure(pub f64, pub f64);

#[derive(Clone, Copy, Debug)]
pub struct BarLine {
    pub time: f64,
    pub scroll_speed: Bpm,
    pub kind: BarLineKind,
    pub visible: bool,
    pub branch: Option<BranchType>,
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum BarLineKind {
    Normal,
    Branch,
}

impl Default for Measure {
    fn default() -> Self {
        Measure(4.0, 4.0)
    }
}

impl Measure {
    pub fn get_beat_count(&self) -> f64 {
        self.0 / self.1 * 4.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bpm(pub f64);

impl Bpm {
    pub fn beat_duration(&self) -> f64 {
        60.0 / self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enum)]
pub enum BranchType {
    Normal,
    Expert,
    Master,
}

impl Default for BranchType {
    fn default() -> Self {
        Self::Normal
    }
}

// TODO use (or create?) some derive macro
impl BranchType {
    pub fn next(self) -> Option<Self> {
        use BranchType::*;
        match self {
            Normal => Some(Expert),
            Expert => Some(Master),
            Master => None,
        }
    }

    pub fn prev(self) -> Option<Self> {
        use BranchType::*;
        match self {
            Normal => None,
            Expert => Some(Normal),
            Master => Some(Expert),
        }
    }

    pub fn saturating_next(self) -> Self {
        self.next().unwrap_or(Self::Master)
    }

    pub fn saturating_prev(self) -> Self {
        self.prev().unwrap_or(Self::Normal)
    }

    pub fn matches(self, other: Option<Self>) -> bool {
        other.map_or(true, |x| x == self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BranchEvent {
    pub time: f64,
    pub kind: BranchEventKind,
}

#[derive(Clone, Copy, Debug)]
pub enum BranchEventKind {
    LevelHold(BranchType),
    Section,
}

macro_rules! define_types {
    ($ty: ty) => {
        pub type Score = $crate::structs::typed::Score<$ty>;
        pub type Note = $crate::structs::typed::Note<$ty>;
        pub type NoteContent = $crate::structs::typed::NoteContent<$ty>;
        pub type SingleNote = $crate::structs::typed::SingleNote<$ty>;
        pub type RendaContent = $crate::structs::typed::RendaContent<$ty>;
        pub type RendaKind = $crate::structs::typed::RendaKind<$ty>;
        pub type UnlimitedRenda = $crate::structs::typed::UnlimitedRenda<$ty>;
        pub type QuotaRenda = $crate::structs::typed::QuotaRenda<$ty>;
        pub type Branch = $crate::structs::typed::Branch<$ty>;
    };
}

pub mod just {
    define_types!(());
}
