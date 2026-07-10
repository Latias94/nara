//! Unit-safe, non-zero scalar limits shared by engine domains.

use std::{num::NonZeroUsize, time::Duration};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

macro_rules! non_zero_usize_limit {
    ($name:ident, $unit:literal) => {
        #[doc = concat!("A non-zero limit measured in ", $unit, ".")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        #[repr(transparent)]
        pub struct $name(NonZeroUsize);

        impl $name {
            pub const ONE: Self = Self(NonZeroUsize::MIN);

            #[must_use]
            pub const fn new(value: usize) -> Option<Self> {
                match NonZeroUsize::new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            #[must_use]
            pub const fn get(self) -> usize {
                self.0.get()
            }
        }

        impl From<$name> for usize {
            fn from(value: $name) -> Self {
                value.get()
            }
        }
    };
}

non_zero_usize_limit!(ItemLimit, "items");
non_zero_usize_limit!(ByteLimit, "bytes");
non_zero_usize_limit!(DepthLimit, "nesting levels");

/// A non-zero elapsed-time limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct TimeLimit(Duration);

impl TimeLimit {
    pub const MIN: Self = Self(Duration::from_nanos(1));

    #[must_use]
    pub const fn new(value: Duration) -> Option<Self> {
        if value.is_zero() {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> Duration {
        self.0
    }
}

impl From<TimeLimit> for Duration {
    fn from(value: TimeLimit) -> Self {
        value.get()
    }
}

#[cfg(feature = "serde")]
impl Serialize for TimeLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for TimeLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Duration::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("time limit must be non-zero"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_limits_reject_zero() {
        assert_eq!(ItemLimit::new(0), None);
        assert_eq!(ByteLimit::new(0), None);
        assert_eq!(DepthLimit::new(0), None);
        assert_eq!(TimeLimit::new(Duration::ZERO), None);
    }

    #[test]
    fn scalar_limits_preserve_their_units() {
        assert_eq!(ItemLimit::new(3).map(ItemLimit::get), Some(3));
        assert_eq!(ByteLimit::new(4).map(ByteLimit::get), Some(4));
        assert_eq!(DepthLimit::new(5).map(DepthLimit::get), Some(5));
        assert_eq!(
            TimeLimit::new(Duration::from_millis(6)).map(TimeLimit::get),
            Some(Duration::from_millis(6))
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn time_limit_deserialization_preserves_the_non_zero_invariant() {
        let zero = serde_json::to_string(&Duration::ZERO).unwrap();
        assert!(serde_json::from_str::<TimeLimit>(&zero).is_err());

        let expected = TimeLimit::new(Duration::from_millis(7)).unwrap();
        let encoded = serde_json::to_string(&expected).unwrap();
        assert_eq!(
            serde_json::from_str::<TimeLimit>(&encoded).unwrap(),
            expected
        );
    }
}
