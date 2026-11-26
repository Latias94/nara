use nara_reflect::{
    ComponentFloat, ComponentNumericBound, ComponentNumericDomain, ComponentNumericInterval,
    ComponentValue, ComponentValueShape,
};
use open_gpui_base::NumericRange;

pub(super) fn range(shape: &ComponentValueShape, value: f64) -> Option<NumericRange> {
    let interval = shape.numeric_interval();
    let (min, max, step) = match shape.numeric_domain() {
        Some(ComponentNumericDomain::I8) => {
            signed(interval, i64::from(i8::MIN), i64::from(i8::MAX))?
        }
        Some(ComponentNumericDomain::I16) => {
            signed(interval, i64::from(i16::MIN), i64::from(i16::MAX))?
        }
        Some(ComponentNumericDomain::I32) => {
            signed(interval, i64::from(i32::MIN), i64::from(i32::MAX))?
        }
        Some(ComponentNumericDomain::U8) => unsigned(interval, u64::from(u8::MAX))?,
        Some(ComponentNumericDomain::U16) => unsigned(interval, u64::from(u16::MAX))?,
        Some(ComponentNumericDomain::U32) => unsigned(interval, u64::from(u32::MAX))?,
        _ if shape.value_kind() == nara_reflect::ComponentValueKind::F64 => {
            let limit = if shape.numeric_domain() == Some(ComponentNumericDomain::F32) {
                f64::from(f32::MAX)
            } else {
                f64::MAX / 4.
            };
            let (min, max) = match interval {
                Some(ComponentNumericInterval::F64 { min, max }) => {
                    (included(min, -limit)?, included(max, limit)?)
                }
                None => (-limit, limit),
                _ => return None,
            };
            let step = 2f64.powf(((max - min) / 100.).log2().floor()).min(0.125);
            (min, max, step)
        }
        _ => return None,
    };
    let range = NumericRange::new(min, max)?.with_step(step)?;
    if !(min..=max).contains(&value) {
        return None;
    }
    if range.normalize(value) == value {
        Some(range)
    } else {
        range.with_step_origin(value)
    }
}

#[cfg(test)]
mod tests {
    use nara_scene::authoring::builtin_component_schemas;

    use super::*;

    #[test]
    fn positive_sprite_size_keeps_the_authored_value_and_a_small_step() {
        let schemas = builtin_component_schemas().unwrap();
        let sprite = schemas
            .iter()
            .find(|schema| schema.id().as_str() == "nara.SpriteSource")
            .unwrap();
        let size = sprite
            .fields()
            .iter()
            .find(|field| field.path().to_string() == "size")
            .unwrap();
        let ComponentValueShape::Record { members } = size.shape() else {
            panic!("Sprite size is a vector");
        };
        let value = f64::from(0.65_f32);
        let range = range(&members[0].shape, value).unwrap();
        assert_eq!(range.normalize(value), value);
        assert_eq!(range.step(), Some(0.125));
        assert_eq!(range.normalize(value + 0.14), value + 0.125);
    }
}

fn signed(
    interval: Option<ComponentNumericInterval>,
    lower: i64,
    upper: i64,
) -> Option<(f64, f64, f64)> {
    let (min, max) = match interval {
        Some(ComponentNumericInterval::I64 { min, max }) => (
            min.map_or(Some(lower), |bound| match bound {
                ComponentNumericBound::Included(value) => Some(value),
                ComponentNumericBound::Excluded(value) => value.checked_add(1),
            })?,
            max.map_or(Some(upper), |bound| match bound {
                ComponentNumericBound::Included(value) => Some(value),
                ComponentNumericBound::Excluded(value) => value.checked_sub(1),
            })?,
        ),
        None => (lower, upper),
        _ => return None,
    };
    Some((min as f64, max as f64, 1.))
}

fn unsigned(interval: Option<ComponentNumericInterval>, upper: u64) -> Option<(f64, f64, f64)> {
    let (min, max) = match interval {
        Some(ComponentNumericInterval::U64 { min, max }) => (
            min.map_or(Some(0), |bound| match bound {
                ComponentNumericBound::Included(value) => Some(value),
                ComponentNumericBound::Excluded(value) => value.checked_add(1),
            })?,
            max.map_or(Some(upper), |bound| match bound {
                ComponentNumericBound::Included(value) => Some(value),
                ComponentNumericBound::Excluded(value) => value.checked_sub(1),
            })?,
        ),
        None => (0, upper),
        _ => return None,
    };
    Some((min as f64, max as f64, 1.))
}

fn included(bound: Option<ComponentNumericBound<ComponentFloat>>, fallback: f64) -> Option<f64> {
    match bound {
        None => Some(fallback),
        Some(ComponentNumericBound::Included(bound)) => Some(bound.get()),
        Some(ComponentNumericBound::Excluded(_)) => None,
    }
}

pub(super) fn displayed(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::F64(value) => Some(value.get()),
        ComponentValue::I64(value) => i32::try_from(*value).ok().map(f64::from),
        ComponentValue::U64(value) => u32::try_from(*value).ok().map(f64::from),
        _ => None,
    }
}

pub(super) fn draft_text(shape: &ComponentValueShape, value: f64) -> String {
    match shape.value_kind() {
        nara_reflect::ComponentValueKind::I64 | nara_reflect::ComponentValueKind::U64 => {
            value.round().to_string()
        }
        _ => value.to_string(),
    }
}
