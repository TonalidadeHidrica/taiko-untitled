pub trait OptionCompare<T> {
    fn content_equals(self, other: Option<T>) -> bool;
}

impl<T: PartialEq> OptionCompare<T> for Option<T> {
    fn content_equals(self, other: Option<T>) -> bool {
        self.map_or(true, |a| other.map_or(true, |b| a == b))
    }
}
