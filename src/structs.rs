use enum_map::Enum;

pub mod typed {
    use super::*;
    use std::fmt::Debug;

    pub trait NoteInfo {
        type Note: Debug + Clone;
        type SingleNote: Debug + Clone;
        type RendaContent: Debug + Clone;
        type UnlimitedRenda: Debug + Clone;
        type QuotaRenda: Debug + Clone;
        type Branch: Debug + Clone;
    }

    impl NoteInfo for () {
        type Note = ();
        type SingleNote = ();
        type RendaContent = ();
        type UnlimitedRenda = ();
        type QuotaRenda = ();
        type Branch = ();
    }

    #[derive(Clone, Debug)]
    pub struct Note<T: NoteInfo> {
        pub scroll_speed: Bpm,
        pub time: f64,
        pub content: NoteContent<T>,
        pub branch: Option<BranchType>,
        pub info: T::Note,
    }

    #[derive(Clone, Debug)]
    pub enum NoteContent<T: NoteInfo> {
        Single(SingleNote<T>),
        Renda(RendaContent<T>),
    }

    #[derive(Clone, Copy, Debug)]
    pub struct SingleNote<T: NoteInfo> {
        pub kind: SingleNoteKind,
        pub info: T::SingleNote,
    }

    #[derive(Clone, Debug)]
    pub struct RendaContent<T: NoteInfo> {
        pub kind: RendaKind<T>,
        pub end_time: f64,
        pub info: T::RendaContent,
    }

    #[derive(Clone, Debug)]
    pub enum RendaKind<T: NoteInfo> {
        Unlimited(UnlimitedRenda<T>),
        Quota(QuotaRenda<T>),
    }

    #[derive(Clone, Copy, Debug)]
    pub struct UnlimitedRenda<T: NoteInfo> {
        pub size: NoteSize,
        pub info: T::UnlimitedRenda,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct QuotaRenda<T: NoteInfo> {
        pub kind: QuotaRendaKind,
        pub quota: u64,
        pub info: T::QuotaRenda,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Branch<T: NoteInfo> {
        pub time: f64,
        pub scroll_speed: Bpm,
        pub condition: BranchCondition,
        pub info: T::Branch,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LevelUra(Level, bool);

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
    Renda(u64, u64),
    Precision(f64, f64),
    Score(u64, u64),
}

#[derive(Debug)]
pub struct Measure(pub f64, pub f64);

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
    pub fn get_beat_duration(&self) -> f64 {
        60.0 / self.0
    }
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum BranchType {
    Normal,
    Expert,
    Master,
}

macro_rules! define_types {
    ($ty: ty) => {
        pub type Note = super::typed::Note<$ty>;
        pub type NoteContent = super::typed::NoteContent<$ty>;
        pub type SingleNote = super::typed::SingleNote<$ty>;
        pub type RendaContent = super::typed::RendaContent<$ty>;
        pub type RendaKind = super::typed::RendaKind<$ty>;
        pub type UnlimitedRenda = super::typed::UnlimitedRenda<$ty>;
        pub type QuotaRenda = super::typed::QuotaRenda<$ty>;
        pub type Branch = super::typed::Branch<$ty>;
    };
}

pub mod just {
    define_types!(());
}
