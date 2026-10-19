//! Finite numeric intervals shared by persistent scalar validation.

use crate::{
    ComponentFloat, ComponentNumericDomain, ComponentShapeErrorKind, ComponentValue,
    ComponentValueKind,
};

/// One explicitly inclusive or exclusive numeric endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComponentNumericBound<T> {
    Included(T),
    Excluded(T),
}

// Struct variants keep the tag a string when an enclosing internally tagged shape buffers
// the representation. Adjacently tagged tuple variants lose their RON tag in that path.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "bound", rename_all = "snake_case", deny_unknown_fields)]
enum NumericBoundWire<T> {
    Included { value: T },
    Excluded { value: T },
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for ComponentNumericBound<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Included(value) => NumericBoundWire::Included { value },
            Self::Excluded(value) => NumericBoundWire::Excluded { value },
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ComponentNumericBound<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match NumericBoundWire::deserialize(deserializer)? {
            NumericBoundWire::Included { value } => Self::Included(value),
            NumericBoundWire::Excluded { value } => Self::Excluded(value),
        })
    }
}

impl<T: Copy> ComponentNumericBound<T> {
    const fn value(self) -> T {
        match self {
            Self::Included(value) | Self::Excluded(value) => value,
        }
    }

    const fn is_included(self) -> bool {
        matches!(self, Self::Included(_))
    }
}

/// A scalar constraint whose endpoints retain their signed, unsigned or finite float kind.
///
/// Missing endpoints are unbounded within the scalar's numeric domain. Catalog admission rejects
/// kind mismatches, endpoints outside that domain and intervals containing no representable value.
/// This describes validation, not a slider range or a request to clamp stored values.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ComponentNumericInterval {
    I64 {
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        min: Option<ComponentNumericBound<i64>>,
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        max: Option<ComponentNumericBound<i64>>,
    },
    U64 {
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        min: Option<ComponentNumericBound<u64>>,
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        max: Option<ComponentNumericBound<u64>>,
    },
    F64 {
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        min: Option<ComponentNumericBound<ComponentFloat>>,
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        max: Option<ComponentNumericBound<ComponentFloat>>,
    },
}

impl ComponentNumericInterval {
    #[must_use]
    pub const fn value_kind(self) -> ComponentValueKind {
        match self {
            Self::I64 { .. } => ComponentValueKind::I64,
            Self::U64 { .. } => ComponentValueKind::U64,
            Self::F64 { .. } => ComponentValueKind::F64,
        }
    }

    pub(crate) fn validate(
        self,
        kind: ComponentValueKind,
        domain: Option<ComponentNumericDomain>,
    ) -> Result<(), ComponentShapeErrorKind> {
        if self.value_kind() != kind {
            return Err(ComponentShapeErrorKind::InvalidNumericIntervalKind {
                expected: kind,
                actual: self.value_kind(),
            });
        }
        let nonempty = match self {
            Self::I64 { min, max } => {
                let (domain_min, domain_max) = match domain {
                    Some(ComponentNumericDomain::I8) => (i64::from(i8::MIN), i64::from(i8::MAX)),
                    Some(ComponentNumericDomain::I16) => (i64::from(i16::MIN), i64::from(i16::MAX)),
                    Some(ComponentNumericDomain::I32) => (i64::from(i32::MIN), i64::from(i32::MAX)),
                    _ => (i64::MIN, i64::MAX),
                };
                validate_endpoint_domain(min, max, domain_min, domain_max)?;
                let first = match min {
                    None => Some(domain_min),
                    Some(ComponentNumericBound::Included(value)) => Some(value),
                    Some(ComponentNumericBound::Excluded(value)) => value.checked_add(1),
                };
                let last = match max {
                    None => Some(domain_max),
                    Some(ComponentNumericBound::Included(value)) => Some(value),
                    Some(ComponentNumericBound::Excluded(value)) => value.checked_sub(1),
                };
                matches!((first, last), (Some(first), Some(last)) if first <= last
                    && first <= domain_max && last >= domain_min)
            }
            Self::U64 { min, max } => {
                let domain_max = match domain {
                    Some(ComponentNumericDomain::U8) => u64::from(u8::MAX),
                    Some(ComponentNumericDomain::U16) => u64::from(u16::MAX),
                    Some(ComponentNumericDomain::U32) => u64::from(u32::MAX),
                    _ => u64::MAX,
                };
                validate_endpoint_domain(min, max, 0, domain_max)?;
                let first = match min {
                    None => Some(0),
                    Some(ComponentNumericBound::Included(value)) => Some(value),
                    Some(ComponentNumericBound::Excluded(value)) => value.checked_add(1),
                };
                let last = match max {
                    None => Some(domain_max),
                    Some(ComponentNumericBound::Included(value)) => Some(value),
                    Some(ComponentNumericBound::Excluded(value)) => value.checked_sub(1),
                };
                matches!((first, last), (Some(first), Some(last)) if first <= last
                    && first <= domain_max)
            }
            Self::F64 { min, max } => {
                let min = min.map(float_bound);
                let max = max.map(float_bound);
                let is_f32 = domain == Some(ComponentNumericDomain::F32);
                let domain_max = if is_f32 {
                    f64::from(f32::MAX)
                } else {
                    f64::MAX
                };
                validate_endpoint_domain(min, max, -domain_max, domain_max)?;
                let first = min.map_or(-domain_max, |bound| float_endpoint(bound, is_f32, true));
                let last = max.map_or(domain_max, |bound| float_endpoint(bound, is_f32, false));
                first.is_finite() && last.is_finite() && first <= last
            }
        };
        if nonempty {
            Ok(())
        } else {
            Err(ComponentShapeErrorKind::EmptyNumericInterval)
        }
    }

    pub(crate) fn accepts_value(
        self,
        value: &ComponentValue,
        domain: Option<ComponentNumericDomain>,
    ) -> bool {
        match (self, value) {
            (Self::I64 { min, max }, ComponentValue::I64(value)) => contains(min, max, *value),
            (Self::U64 { min, max }, ComponentValue::U64(value)) => contains(min, max, *value),
            (Self::F64 { min, max }, ComponentValue::F64(value)) => {
                let value = if domain == Some(ComponentNumericDomain::F32) {
                    f64::from(value.get() as f32)
                } else {
                    value.get()
                };
                contains(min.map(float_bound), max.map(float_bound), value)
            }
            _ => false,
        }
    }

    pub(crate) fn feed_fingerprint(self, hasher: &mut blake3::Hasher) {
        match self {
            Self::I64 { min, max } => {
                feed(hasher, &[0]);
                feed_bound(hasher, min, |hasher, value| {
                    feed(hasher, &value.to_le_bytes())
                });
                feed_bound(hasher, max, |hasher, value| {
                    feed(hasher, &value.to_le_bytes())
                });
            }
            Self::U64 { min, max } => {
                feed(hasher, &[1]);
                feed_bound(hasher, min, |hasher, value| {
                    feed(hasher, &value.to_le_bytes())
                });
                feed_bound(hasher, max, |hasher, value| {
                    feed(hasher, &value.to_le_bytes())
                });
            }
            Self::F64 { min, max } => {
                feed(hasher, &[2]);
                feed_bound(hasher, min, feed_float);
                feed_bound(hasher, max, feed_float);
            }
        }
    }
}

fn validate_endpoint_domain<T: Copy + PartialOrd>(
    min: Option<ComponentNumericBound<T>>,
    max: Option<ComponentNumericBound<T>>,
    domain_min: T,
    domain_max: T,
) -> Result<(), ComponentShapeErrorKind> {
    for bound in [min, max].into_iter().flatten() {
        if bound.value() < domain_min || bound.value() > domain_max {
            return Err(ComponentShapeErrorKind::NumericIntervalBoundOutsideDomain);
        }
    }
    Ok(())
}

fn float_bound(bound: ComponentNumericBound<ComponentFloat>) -> ComponentNumericBound<f64> {
    match bound {
        ComponentNumericBound::Included(value) => ComponentNumericBound::Included(value.get()),
        ComponentNumericBound::Excluded(value) => ComponentNumericBound::Excluded(value.get()),
    }
}

fn float_endpoint(bound: ComponentNumericBound<f64>, is_f32: bool, lower: bool) -> f64 {
    let declared = bound.value();
    let candidate = if is_f32 {
        f64::from(declared as f32)
    } else {
        declared
    };
    let advance = if lower {
        candidate < declared || (candidate == declared && !bound.is_included())
    } else {
        candidate > declared || (candidate == declared && !bound.is_included())
    };
    if !advance {
        return candidate;
    }
    match (is_f32, lower) {
        (true, true) => f64::from((candidate as f32).next_up()),
        (true, false) => f64::from((candidate as f32).next_down()),
        (false, true) => candidate.next_up(),
        (false, false) => candidate.next_down(),
    }
}

fn contains<T: Copy + PartialOrd>(
    min: Option<ComponentNumericBound<T>>,
    max: Option<ComponentNumericBound<T>>,
    value: T,
) -> bool {
    min.is_none_or(|bound| match bound {
        ComponentNumericBound::Included(min) => value >= min,
        ComponentNumericBound::Excluded(min) => value > min,
    }) && max.is_none_or(|bound| match bound {
        ComponentNumericBound::Included(max) => value <= max,
        ComponentNumericBound::Excluded(max) => value < max,
    })
}

fn feed(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn feed_bound<T: Copy>(
    hasher: &mut blake3::Hasher,
    bound: Option<ComponentNumericBound<T>>,
    feed_value: impl FnOnce(&mut blake3::Hasher, T),
) {
    match bound {
        None => feed(hasher, &[0]),
        Some(bound) => {
            feed(hasher, &[if bound.is_included() { 1 } else { 2 }]);
            feed_value(hasher, bound.value());
        }
    }
}

fn feed_float(hasher: &mut blake3::Hasher, value: ComponentFloat) {
    let normalized = if value.get() == 0.0 { 0.0 } else { value.get() };
    feed(hasher, &normalized.to_bits().to_le_bytes());
}
