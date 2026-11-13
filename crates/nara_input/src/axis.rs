//! Validated normalized scalar axes and continuous scalar dead zones.

use std::{error::Error, fmt};

/// The reason a scalar axis value or dead-zone threshold was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisValueError {
    /// The value is NaN or infinite.
    NonFinite,
    /// The axis value is outside the inclusive range `[-1, 1]`.
    OutOfRange,
    /// The dead-zone threshold is outside the half-open range `[0, 1)`.
    InvalidDeadZone,
}

impl fmt::Display for AxisValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "axis values and dead-zone thresholds must be finite",
            Self::OutOfRange => "axis value must be in the inclusive range [-1, 1]",
            Self::InvalidDeadZone => "axis dead-zone threshold must be in the range [0, 1)",
        })
    }
}

impl Error for AxisValueError {}

/// A finite normalized signed axis value in `[-1, 1]`.
///
/// Construction rejects non-finite values and canonicalizes negative zero to positive zero.
/// These invariants make equality reflexive, including after deserialization.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct AxisValue(f32);

impl Eq for AxisValue {}

impl Default for AxisValue {
    fn default() -> Self {
        Self::ZERO
    }
}

impl AxisValue {
    /// The neutral axis value, represented as positive zero.
    pub const ZERO: Self = Self(0.0);

    /// Validates an already normalized axis value without clamping it.
    ///
    /// # Errors
    ///
    /// Returns [`AxisValueError::NonFinite`] for NaN or infinity and
    /// [`AxisValueError::OutOfRange`] for finite values outside `[-1, 1]`.
    pub fn new(value: f32) -> Result<Self, AxisValueError> {
        if !value.is_finite() {
            return Err(AxisValueError::NonFinite);
        }
        if !(-1.0..=1.0).contains(&value) {
            return Err(AxisValueError::OutOfRange);
        }
        Ok(if value == 0.0 {
            Self::ZERO
        } else {
            Self(value)
        })
    }

    /// Returns the validated normalized value.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AxisValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A finite scalar dead-zone threshold in `[0, 1)`.
///
/// The default is [`Self::NONE`], so dead zones are applied only when explicitly configured.
/// Negative zero is canonicalized to positive zero, and non-finite values are rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct AxisDeadZone(f32);

impl Eq for AxisDeadZone {}

impl Default for AxisDeadZone {
    fn default() -> Self {
        Self::NONE
    }
}

impl AxisDeadZone {
    /// No dead zone; applying it preserves every validated axis value.
    pub const NONE: Self = Self(0.0);

    /// Validates a scalar dead-zone threshold without clamping it.
    ///
    /// # Errors
    ///
    /// Returns [`AxisValueError::NonFinite`] for NaN or infinity and
    /// [`AxisValueError::InvalidDeadZone`] for finite thresholds outside `[0, 1)`.
    pub fn new(threshold: f32) -> Result<Self, AxisValueError> {
        if !threshold.is_finite() {
            return Err(AxisValueError::NonFinite);
        }
        if !(0.0..1.0).contains(&threshold) {
            return Err(AxisValueError::InvalidDeadZone);
        }
        Ok(if threshold == 0.0 {
            Self::NONE
        } else {
            Self(threshold)
        })
    }

    /// Returns the validated threshold.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Applies a continuous, sign-preserving scalar dead-zone remap.
    ///
    /// Magnitudes at or below the threshold become positive zero. Outside the dead zone,
    /// the magnitude is `(abs(value) - threshold) / (1 - threshold)`, bounded to `[0, 1]`
    /// against floating-point rounding. Both normalized endpoints remain unchanged.
    #[must_use]
    pub fn apply(self, value: AxisValue) -> AxisValue {
        let magnitude = value.0.abs();
        if magnitude <= self.0 {
            return AxisValue::ZERO;
        }
        let remapped = ((magnitude - self.0) / (1.0 - self.0)).clamp(0.0, 1.0);
        if remapped == 0.0 {
            AxisValue::ZERO
        } else {
            AxisValue(remapped.copysign(value.0))
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AxisDeadZone {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let threshold = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(threshold).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{AxisDeadZone, AxisValue, AxisValueError};

    #[test]
    fn constructors_reject_non_finite_values() {
        for value in [f32::NAN, -f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            assert_eq!(AxisValue::new(value), Err(AxisValueError::NonFinite));
            assert_eq!(AxisDeadZone::new(value), Err(AxisValueError::NonFinite));
        }
    }

    #[test]
    fn axis_values_admit_exact_boundaries_and_reject_outside_values() {
        for value in [
            -1.0,
            -0.5,
            -f32::from_bits(1),
            0.0,
            f32::from_bits(1),
            0.5,
            1.0,
        ] {
            assert_eq!(AxisValue::new(value).unwrap().get(), value);
        }
        let above_one = f32::from_bits(1.0_f32.to_bits() + 1);
        for value in [-f32::MAX, -1.5, -above_one, above_one, 1.5, f32::MAX] {
            assert_eq!(AxisValue::new(value), Err(AxisValueError::OutOfRange));
        }
    }

    #[test]
    fn dead_zones_admit_exact_boundaries_and_reject_outside_thresholds() {
        let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
        for threshold in [0.0, f32::from_bits(1), 0.5, below_one] {
            assert_eq!(AxisDeadZone::new(threshold).unwrap().get(), threshold);
        }
        for threshold in [-f32::MAX, -1.0, -f32::from_bits(1), 1.0, 1.5, f32::MAX] {
            assert_eq!(
                AxisDeadZone::new(threshold),
                Err(AxisValueError::InvalidDeadZone)
            );
        }
    }

    #[test]
    fn defaults_and_both_zero_signs_have_canonical_positive_zero() {
        assert_eq!(AxisValue::default(), AxisValue::ZERO);
        assert_eq!(AxisDeadZone::default(), AxisDeadZone::NONE);
        assert_eq!(AxisValue::default().get().to_bits(), 0.0_f32.to_bits());
        assert_eq!(AxisDeadZone::default().get().to_bits(), 0.0_f32.to_bits());
        for zero in [-0.0, 0.0] {
            let value = AxisValue::new(zero).unwrap();
            let dead_zone = AxisDeadZone::new(zero).unwrap();
            assert_eq!(value, AxisValue::ZERO);
            assert_eq!(dead_zone, AxisDeadZone::NONE);
            assert_eq!(value.get().to_bits(), 0.0_f32.to_bits());
            assert_eq!(dead_zone.get().to_bits(), 0.0_f32.to_bits());
        }
    }

    #[test]
    fn no_dead_zone_preserves_values_including_subnormals_and_endpoints() {
        let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
        for value in [
            -1.0,
            -below_one,
            -0.5,
            -f32::MIN_POSITIVE,
            -f32::from_bits(1),
            0.0,
            f32::from_bits(1),
            f32::MIN_POSITIVE,
            0.5,
            below_one,
            1.0,
        ] {
            let value = AxisValue::new(value).unwrap();
            assert_eq!(
                AxisDeadZone::NONE.apply(value).get().to_bits(),
                value.get().to_bits()
            );
        }
    }

    #[test]
    fn remap_matches_fixed_scalar_cases_and_canonicalizes_neutral_outputs() {
        let dead_zone = AxisDeadZone::new(0.25).unwrap();
        for (input, expected) in [
            (-1.0, -1.0),
            (-0.8125, -0.75),
            (-0.625, -0.5),
            (-0.4375, -0.25),
            (-0.25, 0.0),
            (-0.125, 0.0),
            (0.0, 0.0),
            (0.125, 0.0),
            (0.25, 0.0),
            (0.4375, 0.25),
            (0.625, 0.5),
            (0.8125, 0.75),
            (1.0, 1.0),
        ] {
            let output = dead_zone.apply(AxisValue::new(input).unwrap());
            assert_eq!(output.get(), expected, "input {input}");
            if expected == 0.0 {
                assert_eq!(output.get().to_bits(), 0.0_f32.to_bits());
            }
        }
    }

    #[test]
    fn remap_is_continuous_at_the_threshold_and_endpoints() {
        let dead_zone = AxisDeadZone::new(0.5).unwrap();
        let below = f32::from_bits(0.5_f32.to_bits() - 1);
        let above = f32::from_bits(0.5_f32.to_bits() + 1);
        let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
        for sign in [-1.0, 1.0] {
            for magnitude in [below, 0.5] {
                assert_eq!(
                    dead_zone.apply(AxisValue::new(sign * magnitude).unwrap()),
                    AxisValue::ZERO,
                );
            }
            assert_eq!(
                dead_zone.apply(AxisValue::new(sign * above).unwrap()).get(),
                sign * f32::EPSILON,
            );
            assert_eq!(
                dead_zone
                    .apply(AxisValue::new(sign * below_one).unwrap())
                    .get(),
                sign * (1.0 - f32::EPSILON),
            );
            assert_eq!(dead_zone.apply(AxisValue::new(sign).unwrap()).get(), sign);
        }
    }

    #[test]
    fn thresholds_nearest_one_preserve_endpoints_and_resolve_the_last_step() {
        let last_step = f32::from_bits(1.0_f32.to_bits() - 1);
        let previous_step = f32::from_bits(1.0_f32.to_bits() - 2);
        let last_dead_zone = AxisDeadZone::new(last_step).unwrap();
        let previous_dead_zone = AxisDeadZone::new(previous_step).unwrap();
        for sign in [-1.0, 1.0] {
            assert_eq!(
                last_dead_zone.apply(AxisValue::new(sign * last_step).unwrap()),
                AxisValue::ZERO,
            );
            assert_eq!(
                last_dead_zone.apply(AxisValue::new(sign).unwrap()).get(),
                sign
            );
            assert_eq!(
                previous_dead_zone
                    .apply(AxisValue::new(sign * last_step).unwrap())
                    .get(),
                sign * 0.5,
            );
            assert_eq!(
                previous_dead_zone
                    .apply(AxisValue::new(sign).unwrap())
                    .get(),
                sign
            );
        }
    }

    #[test]
    fn remap_is_bounded_monotonic_and_sign_preserving() {
        let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
        for threshold in [0.0, 0.125, 0.25, 0.5, 0.75, 0.875, below_one] {
            let dead_zone = AxisDeadZone::new(threshold).unwrap();
            let mut previous = -1.0;
            for input in [
                -1.0,
                -below_one,
                -0.875,
                -0.75,
                -0.625,
                -0.5,
                -0.25,
                -f32::from_bits(1),
                0.0,
                f32::from_bits(1),
                0.25,
                0.5,
                0.625,
                0.75,
                0.875,
                below_one,
                1.0,
            ] {
                let output = dead_zone.apply(AxisValue::new(input).unwrap()).get();
                assert!(output.is_finite() && (-1.0..=1.0).contains(&output));
                assert!(output >= previous, "threshold {threshold}, input {input}");
                if output == 0.0 {
                    assert_eq!(output.to_bits(), 0.0_f32.to_bits());
                } else {
                    assert_eq!(output.is_sign_negative(), input.is_sign_negative());
                }
                previous = output;
            }
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use serde::{
        Deserialize,
        de::value::{Error as DeserializeError, F32Deserializer},
    };

    use super::{AxisDeadZone, AxisValue, AxisValueError};

    #[test]
    fn scalar_json_round_trips_preserve_values_and_canonicalize_zero() {
        let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
        for raw in [-1.0, -0.875, -0.0, 0.0, 0.875, below_one, 1.0] {
            let value = AxisValue::new(raw).unwrap();
            let json = serde_json::to_string(&value).unwrap();
            assert!(
                serde_json::from_str::<serde_json::Value>(&json)
                    .unwrap()
                    .is_number()
            );
            assert_eq!(serde_json::from_str::<AxisValue>(&json).unwrap(), value);
        }
        for raw in [-0.0, 0.0, 0.125, 0.5, below_one] {
            let dead_zone = AxisDeadZone::new(raw).unwrap();
            let json = serde_json::to_string(&dead_zone).unwrap();
            assert!(
                serde_json::from_str::<serde_json::Value>(&json)
                    .unwrap()
                    .is_number()
            );
            assert_eq!(
                serde_json::from_str::<AxisDeadZone>(&json).unwrap(),
                dead_zone
            );
        }
        assert_eq!(
            serde_json::from_str::<AxisValue>("-0.0")
                .unwrap()
                .get()
                .to_bits(),
            0
        );
        assert_eq!(
            serde_json::from_str::<AxisDeadZone>("-0.0")
                .unwrap()
                .get()
                .to_bits(),
            0
        );
    }

    #[test]
    fn json_cannot_bypass_range_validation_or_supply_a_wrapper_object() {
        for json in [
            "-1.0000001",
            "1.0000001",
            "-2",
            "2",
            "-3.4028237e38",
            "3.4028237e38",
        ] {
            assert!(serde_json::from_str::<AxisValue>(json).is_err(), "{json}");
        }
        for json in ["-0.0000001", "-1", "1", "1.0000001", "2", "3.4028237e38"] {
            assert!(
                serde_json::from_str::<AxisDeadZone>(json).is_err(),
                "{json}"
            );
        }
        for json in [
            "NaN",
            "Infinity",
            "-Infinity",
            "null",
            "true",
            "\"0.25\"",
            "[0.25]",
            "{\"value\":0.25}",
        ] {
            assert!(serde_json::from_str::<AxisValue>(json).is_err(), "{json}");
            assert!(
                serde_json::from_str::<AxisDeadZone>(json).is_err(),
                "{json}"
            );
        }
    }

    #[test]
    fn non_finite_scalar_deserializers_use_constructor_validation() {
        for raw in [f32::NAN, -f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            let value = F32Deserializer::<DeserializeError>::new(raw);
            let dead_zone = F32Deserializer::<DeserializeError>::new(raw);
            assert_eq!(
                AxisValue::deserialize(value).unwrap_err().to_string(),
                AxisValueError::NonFinite.to_string(),
            );
            assert_eq!(
                AxisDeadZone::deserialize(dead_zone)
                    .unwrap_err()
                    .to_string(),
                AxisValueError::NonFinite.to_string(),
            );
        }
    }
}
