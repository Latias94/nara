use std::time::Duration;

const RATE_FRACTION_BITS: u32 = f32::MANTISSA_DIGITS - 1;
const RATE_EXPONENT_BIAS: u32 = 127;
const FRACTION_BITS: u32 = RATE_EXPONENT_BIAS - 1 + RATE_FRACTION_BITS;
const TOP_WORD_BITS: u32 = FRACTION_BITS - 128;
const TOP_WORD_MASK: u64 = (1 << TOP_WORD_BITS) - 1;

/// The pending fraction of a virtual nanosecond in the App's elapsed clock.
///
/// Every finite nonnegative binary32 rate is an integer multiple of `2^-149`. The fraction
/// uses that unit so scaling whole real nanoseconds and carrying them between frames is exact.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScaledTimeRemainder([u64; 3]);

impl ScaledTimeRemainder {
    pub const ZERO: Self = Self([0; 3]);

    /// Reads three unsigned words, least significant first. A value of one nanosecond or
    /// greater is outside the normalized range.
    #[must_use]
    pub const fn from_words(words: [u64; 3]) -> Option<Self> {
        if words[2] <= TOP_WORD_MASK {
            Some(Self(words))
        } else {
            None
        }
    }

    /// Returns the normalized fraction's words, least significant first.
    #[must_use]
    pub const fn words(self) -> [u64; 3] {
        self.0
    }

    /// Applies a rate already admitted by `RuntimeTimeSettings`.
    pub(crate) fn advance(self, delta: Duration, rate: f32) -> Option<(Duration, Self)> {
        if delta.is_zero() || rate == 0.0 {
            return Some((Duration::ZERO, self));
        }
        if rate == 1.0 {
            return Some((delta, self));
        }

        let bits = rate.to_bits();
        let exponent = bits >> RATE_FRACTION_BITS;
        let implicit_bit = 1 << RATE_FRACTION_BITS;
        let mut significand = bits & (implicit_bit - 1);
        if exponent != 0 {
            significand |= implicit_bit;
        }
        let product = delta.as_nanos() * u128::from(significand);
        let shift = exponent.max(1) as i32 - (RATE_EXPONENT_BIAS + RATE_FRACTION_BITS) as i32;
        let (whole, remainder) = if shift >= 0 {
            let shift = shift as u32;
            if product > Duration::MAX.as_nanos() >> shift {
                return None;
            }
            (product << shift, self)
        } else {
            let denominator_bits = (-shift) as u32;
            let whole = product.checked_shr(denominator_bits).unwrap_or(0);
            let alignment = (FRACTION_BITS - denominator_bits) as i32;
            let added = Self([
                word_at(product, -alignment),
                word_at(product, 64 - alignment),
                word_at(product, 128 - alignment) & TOP_WORD_MASK,
            ]);
            let (carry, remainder) = self.add(added);
            (whole + u128::from(carry), remainder)
        };

        let seconds = u64::try_from(whole / 1_000_000_000).ok()?;
        let nanos = (whole % 1_000_000_000) as u32;
        Some((Duration::new(seconds, nanos), remainder))
    }

    fn add(self, added: Self) -> (u64, Self) {
        let (low, carry) = self.0[0].overflowing_add(added.0[0]);
        let middle = u128::from(self.0[1]) + u128::from(added.0[1]) + u128::from(carry);
        let high = self.0[2] + added.0[2] + (middle >> 64) as u64;
        (
            high >> TOP_WORD_BITS,
            Self([low, middle as u64, high & TOP_WORD_MASK]),
        )
    }
}

fn word_at(value: u128, start: i32) -> u64 {
    if start < 0 {
        value.checked_shl((-start) as u32).unwrap_or(0) as u64
    } else {
        value.checked_shr(start as u32).unwrap_or(0) as u64
    }
}

#[cfg(test)]
mod tests;
