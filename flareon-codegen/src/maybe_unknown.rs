/// Wraps a type whose value may or may not be possible to be determined using
/// the information available.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MaybeUnknown<T> {
    /// Indicates that this instance is determined to be a certain value
    /// (possibly [`None`] if wrapping an [`Option`]).
    Determined(T),
    /// Indicates that the value is unknown.
    Unknown,
}

impl<T> MaybeUnknown<T> {
    pub fn unwrap(self) -> T {
        self.expect("called `MaybeUnknown::unwrap()` on an `Unknown` value")
    }

    pub fn expect(self, msg: &str) -> T {
        match self {
            MaybeUnknown::Determined(value) => value,
            MaybeUnknown::Unknown => {
                panic!("{}", msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maybe_unknown_determined() {
        let value = MaybeUnknown::Determined(42);
        assert_eq!(value.unwrap(), 42);
    }

    #[test]
    #[should_panic(expected = "called `MaybeUnknown::unwrap()` on an `Unknown` value")]
    fn maybe_unknown_unknown_unwrap() {
        let value: MaybeUnknown<i32> = MaybeUnknown::Unknown;
        assert_eq!(value.unwrap(), 42);
    }

    #[test]
    fn maybe_unknown_expect() {
        let value = MaybeUnknown::Determined(42);
        assert_eq!(value.expect("value should be determined"), 42);
    }

    #[test]
    #[should_panic(expected = "value should be determined")]
    fn maybe_unknown_unknown_expect() {
        let value: MaybeUnknown<i32> = MaybeUnknown::Unknown;
        value.expect("value should be determined");
    }
}
