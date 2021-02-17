pub trait OptionCompare<T> {
    fn content_equals(self, other: Option<T>) -> bool;
}

impl<T: PartialEq> OptionCompare<T> for Option<T> {
    fn content_equals(self, other: Option<T>) -> bool {
        self.map_or(true, |a| other.map_or(true, |b| a == b))
    }
}

pub fn to_digits(n: u64) -> Vec<u32> {
    n.to_string()
        .chars()
        .map(|c| c.to_digit(10).unwrap())
        .collect()
}
