pub mod typed {
    use super::*;
    use std::fmt::Debug;

    pub trait NoteInfo {
        type Note: Debug + Clone;
        type SingleNote: Debug + Clone;
        type RendaContent: Debug + Clone;
        type UnlimitedRenda: Debug + Clone;
        type QuotaRenda: Debug + Clone;
    }

    impl NoteInfo for () {
        type Note = ();
        type SingleNote = ();
        type RendaContent = ();
        type UnlimitedRenda = ();
        type QuotaRenda = ();
    }

    #[derive(Clone, Debug)]
    pub struct Note<T: NoteInfo> {
        pub scroll_speed: Bpm,
        pub time: f64,
        pub content: NoteContent<T>,
        pub info: T::Note,
    }

    #[derive(Clone, Debug)]
    pub enum NoteContent<T: NoteInfo> {
        Single(SingleNote<T>),
        Renda(RendaContent<T>),
    }

    #[derive(Clone, Debug)]
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

    #[derive(Clone, Debug)]
    pub struct UnlimitedRenda<T: NoteInfo> {
        pub size: NoteSize,
        pub info: T::UnlimitedRenda,
    }

    #[derive(Clone, Debug)]
    pub struct QuotaRenda<T: NoteInfo> {
        pub kind: QuotaRendaKind,
        pub quota: u64,
        pub info: T::QuotaRenda,
    }
}

#[derive(Clone, Debug)]
pub struct SingleNoteKind {
    pub color: NoteColor,
    pub size: NoteSize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NoteColor {
    Don,
    Ka,
}

#[derive(Clone, Debug)]
pub enum NoteSize {
    Small,
    Large,
}

#[derive(Clone, Debug)]
pub enum QuotaRendaKind {
    Balloon,
    Potato,
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

#[derive(Clone, Debug)]
pub struct Bpm(pub f64);

impl Bpm {
    pub fn get_beat_duration(&self) -> f64 {
        60.0 / self.0
    }
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
    };
}

pub mod just {
    define_types!(());
}
