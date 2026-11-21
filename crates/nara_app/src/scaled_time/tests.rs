use super::*;

const NS: Duration = Duration::from_nanos(1);

#[test]
fn every_binary32_fractional_power_is_representable() {
    for power in 1..=149 {
        let rate = if power <= 126 {
            f32::from_bits((127 - power) << 23)
        } else {
            f32::from_bits(1 << (149 - power))
        };
        let bit = (149 - power) as usize;
        let mut words = [0; 3];
        words[bit / 64] = 1 << (bit % 64);
        let (whole, fraction) = ScaledTimeRemainder::ZERO.advance(NS, rate).unwrap();
        assert_eq!(whole, Duration::ZERO, "power {power}");
        assert_eq!(fraction.words(), words, "power {power}");
    }
}

#[test]
fn carrying_the_smallest_rate_crosses_every_word_at_the_nanosecond_boundary() {
    let largest = ScaledTimeRemainder::from_words([u64::MAX, u64::MAX, (1 << 21) - 1]).unwrap();
    assert!(ScaledTimeRemainder::from_words([0, 0, 1 << 21]).is_none());
    let smallest_rate = f32::from_bits(1);
    assert_eq!(
        largest.advance(NS, smallest_rate),
        Some((NS, ScaledTimeRemainder::ZERO))
    );
}

#[test]
fn decimal_rate_matches_its_binary32_value_over_equal_partitions() {
    let (whole, remainder) = ScaledTimeRemainder::ZERO.advance(NS * 10, 0.1).unwrap();
    assert_eq!(whole, NS);
    assert_eq!(remainder.words(), [0, 1 << 59, 0]);
    let mut partitioned_whole = Duration::ZERO;
    let mut partitioned_remainder = ScaledTimeRemainder::ZERO;
    for _ in 0..10 {
        let (whole, remainder) = partitioned_remainder.advance(NS, 0.1).unwrap();
        partitioned_whole += whole;
        partitioned_remainder = remainder;
    }
    assert_eq!(
        (partitioned_whole, partitioned_remainder),
        (whole, remainder)
    );
}

#[test]
fn large_intervals_preserve_whole_nanoseconds_and_refuse_overflow() {
    let zero = ScaledTimeRemainder::ZERO;
    let largest = Duration::MAX;
    let half = largest / 2;
    let half_nanos = ScaledTimeRemainder::from_words([0, 0, 1 << 20]).unwrap();
    assert_eq!(zero.advance(largest, 0.5), Some((half, half_nanos)));
    assert_eq!(
        half_nanos.advance(largest, 1.0),
        Some((largest, half_nanos))
    );
    assert_eq!(zero.advance(half, 2.0), Some((largest - NS, zero)));
    assert_eq!(zero.advance(half + NS, 2.0), None);

    let rate = f32::from_bits((127 + 93) << 23);
    let seconds = ((1_u128 << 93) / 1_000_000_000) as u64;
    let nanos = ((1_u128 << 93) % 1_000_000_000) as u32;
    assert_eq!(
        zero.advance(NS, rate),
        Some((Duration::new(seconds, nanos), zero))
    );
    assert_eq!(zero.advance(NS * 2, rate), None);
    assert_eq!(zero.advance(NS, f32::MAX), None);
    assert_eq!(
        half_nanos.advance(Duration::ZERO, f32::MAX),
        Some((Duration::ZERO, half_nanos))
    );
}

#[test]
fn a_fractional_carry_can_overflow_the_largest_virtual_delta() {
    let real = Duration::new(16_397_105_843_297_379_214, 222_222_222);
    let quarter = ScaledTimeRemainder::from_words([0, 0, 1 << 19]).unwrap();
    let three_quarters = ScaledTimeRemainder::from_words([0, 0, 3 << 19]).unwrap();
    assert_eq!(
        ScaledTimeRemainder::ZERO.advance(real, 1.125),
        Some((Duration::MAX, three_quarters))
    );
    assert_eq!(quarter.advance(real, 1.125), None);
}
