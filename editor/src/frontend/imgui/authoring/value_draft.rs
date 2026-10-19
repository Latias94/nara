use std::collections::BTreeMap;

use nara::{
    asset::AssetRef,
    reflect::{ComponentValue, ComponentValueKind, ComponentValueShape},
};

#[derive(Default)]
pub(super) struct RootDraft {
    pub node: Option<ValueDraft>,
    pub error: Option<String>,
}

pub(super) enum ValueDraft {
    Null,
    Bool(Option<bool>),
    Text {
        text: String,
        entered: bool,
    },
    Record {
        members: Vec<(String, ValueDraft)>,
        confirmed: bool,
    },
    Optional {
        value: Option<Box<ValueDraft>>,
        pending: Option<Box<ValueDraft>>,
        chosen: bool,
    },
    List {
        items: Vec<ValueDraft>,
        pending: Option<Box<ValueDraft>>,
        confirmed: bool,
    },
    Map {
        entries: BTreeMap<String, ValueDraft>,
        pending: Option<(String, bool, Box<ValueDraft>)>,
        confirmed: bool,
    },
}

impl ValueDraft {
    pub fn new(shape: &ComponentValueShape) -> Self {
        Self::from_value(shape, None)
    }

    pub fn from_value(shape: &ComponentValueShape, source: Option<&ComponentValue>) -> Self {
        match shape {
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Null,
                ..
            } => Self::Null,
            ComponentValueShape::Scalar {
                kind: ComponentValueKind::Bool,
                ..
            } => Self::Bool(source.and_then(ComponentValue::as_bool)),
            ComponentValueShape::Scalar { .. } | ComponentValueShape::AssetRef => Self::Text {
                text: source.map_or_else(String::new, |value| scalar_text(shape, value)),
                entered: source.is_some(),
            },
            ComponentValueShape::Record { members } => Self::Record {
                members: members
                    .iter()
                    .map(|member| {
                        (
                            member.name.clone(),
                            Self::from_value(
                                &member.shape,
                                source.and_then(|value| value.get(&member.name)),
                            ),
                        )
                    })
                    .collect(),
                confirmed: source.is_some() || !members.is_empty(),
            },
            ComponentValueShape::Optional { value: shape } => Self::Optional {
                value: source
                    .filter(|value| **value != ComponentValue::Null)
                    .map(|value| Box::new(Self::from_value(shape, Some(value)))),
                pending: None,
                chosen: source.is_some(),
            },
            ComponentValueShape::List { item } => Self::List {
                items: match source {
                    Some(ComponentValue::List(values)) => values
                        .iter()
                        .map(|value| Self::from_value(item, Some(value)))
                        .collect(),
                    _ => Vec::new(),
                },
                pending: None,
                confirmed: source.is_some(),
            },
            ComponentValueShape::Map { value: shape } => Self::Map {
                entries: match source {
                    Some(ComponentValue::Map(values)) => values
                        .iter()
                        .map(|(key, value)| (key.clone(), Self::from_value(shape, Some(value))))
                        .collect(),
                    _ => BTreeMap::new(),
                },
                pending: None,
                confirmed: source.is_some(),
            },
            ComponentValueShape::EntityRef | ComponentValueShape::Opaque { .. } => {
                unreachable!("unsupported shapes never open a draft")
            }
        }
    }

    pub fn value(&self, shape: &ComponentValueShape) -> Result<ComponentValue, String> {
        let value = match (self, shape) {
            (Self::Null, _) => ComponentValue::Null,
            (Self::Bool(value), _) => ComponentValue::Bool(value.ok_or("Choose true or false")?),
            (Self::Text { text, entered }, _) => {
                if !entered {
                    return Err("Enter a value explicitly".to_owned());
                }
                parse_scalar(shape, text)?
            }
            (
                Self::Record {
                    members: values,
                    confirmed,
                },
                ComponentValueShape::Record { members },
            ) => {
                if !confirmed {
                    return Err("Confirm the empty record explicitly".to_owned());
                }
                ComponentValue::Map(
                    values
                        .iter()
                        .zip(members)
                        .map(|((name, draft), member)| {
                            draft
                                .value(&member.shape)
                                .map(|value| (name.clone(), value))
                                .map_err(|error| format!("{name}: {error}"))
                        })
                        .collect::<Result<_, _>>()?,
                )
            }
            (
                Self::Optional {
                    value,
                    pending,
                    chosen,
                },
                ComponentValueShape::Optional { value: shape },
            ) => {
                if pending.is_some() {
                    return Err("Confirm or cancel the Some draft".to_owned());
                }
                if !chosen {
                    return Err("Choose Null or Some explicitly".to_owned());
                }
                value
                    .as_ref()
                    .map_or(Ok(ComponentValue::Null), |value| value.value(shape))?
            }
            (
                Self::List {
                    items,
                    pending,
                    confirmed,
                },
                ComponentValueShape::List { item },
            ) => {
                if pending.is_some() {
                    return Err("Confirm or cancel the new list item".to_owned());
                }
                if !confirmed {
                    return Err("Confirm the empty list or add an item".to_owned());
                }
                ComponentValue::List(
                    items
                        .iter()
                        .map(|draft| draft.value(item))
                        .collect::<Result<_, _>>()?,
                )
            }
            (
                Self::Map {
                    entries,
                    pending,
                    confirmed,
                },
                ComponentValueShape::Map { value: shape },
            ) => {
                if pending.is_some() {
                    return Err("Confirm or cancel the new map entry".to_owned());
                }
                if !confirmed {
                    return Err("Confirm the empty map or insert an entry".to_owned());
                }
                ComponentValue::Map(
                    entries
                        .iter()
                        .map(|(key, draft)| draft.value(shape).map(|value| (key.clone(), value)))
                        .collect::<Result<_, _>>()?,
                )
            }
            _ => return Err("The field shape changed; reopen the draft".to_owned()),
        };
        shape
            .validate_value(&value)
            .map_err(|error| error.to_string())?;
        Ok(value)
    }
}

pub(super) fn supported(shape: &ComponentValueShape) -> bool {
    match shape {
        ComponentValueShape::Scalar { .. } | ComponentValueShape::AssetRef => true,
        ComponentValueShape::Optional { value } => supported(value),
        ComponentValueShape::Record { members } => {
            members.iter().all(|member| supported(&member.shape))
        }
        ComponentValueShape::List { item } => scalar_item(item),
        ComponentValueShape::Map { value } => scalar_item(value),
        ComponentValueShape::EntityRef | ComponentValueShape::Opaque { .. } => false,
    }
}

fn scalar_item(shape: &ComponentValueShape) -> bool {
    match shape {
        ComponentValueShape::Scalar { .. } | ComponentValueShape::AssetRef => true,
        ComponentValueShape::Optional { value } => scalar_item(value),
        _ => false,
    }
}

fn scalar_text(shape: &ComponentValueShape, value: &ComponentValue) -> String {
    if matches!(shape, ComponentValueShape::AssetRef) {
        let kind = value.get("kind").and_then(ComponentValue::as_str);
        let value = value
            .get("value")
            .and_then(ComponentValue::as_str)
            .unwrap_or_default();
        return if kind == Some("stable_id") {
            format!("stable_id:{value}")
        } else {
            value.to_owned()
        };
    }
    match value {
        ComponentValue::I64(value) => value.to_string(),
        ComponentValue::U64(value) => value.to_string(),
        ComponentValue::F64(value) => value.get().to_string(),
        ComponentValue::String(value) => value.clone(),
        _ => String::new(),
    }
}

fn parse_scalar(shape: &ComponentValueShape, text: &str) -> Result<ComponentValue, String> {
    if matches!(shape, ComponentValueShape::AssetRef) {
        let reference = if let Some(id) = text.strip_prefix("stable_id:") {
            AssetRef::stable_id(id).map_err(|error| error.to_string())
        } else {
            AssetRef::path(text).map_err(|error| error.to_string())
        }?;
        let (kind, value) = match reference {
            AssetRef::Path(path) => ("path", path.to_string()),
            AssetRef::StableId(id) => ("stable_id", id.to_string()),
        };
        return Ok(ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("value", ComponentValue::String(value)),
        ]));
    }
    match shape.value_kind() {
        ComponentValueKind::I64 => text
            .parse()
            .map(ComponentValue::I64)
            .map_err(|_| "Expected a signed integer".to_owned()),
        ComponentValueKind::U64 => text
            .parse()
            .map(ComponentValue::U64)
            .map_err(|_| "Expected an unsigned integer".to_owned()),
        ComponentValueKind::F64 => {
            ComponentValue::f64(text.parse().map_err(|_| "Expected a finite number")?)
                .map_err(|error| error.to_string())
        }
        ComponentValueKind::String => Ok(ComponentValue::String(text.to_owned())),
        _ => Err("Unsupported scalar draft".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::reflect::ComponentNumericDomain;

    #[test]
    fn new_children_require_explicit_values_and_complete_record_members() {
        for kind in [
            ComponentValueKind::Bool,
            ComponentValueKind::I64,
            ComponentValueKind::String,
        ] {
            let shape = ComponentValueShape::scalar(kind);
            assert!(ValueDraft::new(&shape).value(&shape).is_err());
        }
        let shape = ComponentValueShape::record([
            (
                "x".into(),
                ComponentValueShape::scalar(ComponentValueKind::F64)
                    .with_numeric_domain(ComponentNumericDomain::F32),
            ),
            (
                "y".into(),
                ComponentValueShape::scalar(ComponentValueKind::F64)
                    .with_numeric_domain(ComponentNumericDomain::F32),
            ),
        ]);
        let mut draft = ValueDraft::new(&shape);
        let ValueDraft::Record { members, .. } = &mut draft else {
            panic!("record draft");
        };
        members[0].1 = ValueDraft::Text {
            text: "1".into(),
            entered: true,
        };
        assert!(draft.value(&shape).is_err());
        let ValueDraft::Record { members, .. } = &mut draft else {
            panic!("record draft");
        };
        members[1].1 = ValueDraft::Text {
            text: "1e100".into(),
            entered: true,
        };
        assert!(draft.value(&shape).is_err());
        let empty = ComponentValueShape::record([]);
        assert!(ValueDraft::new(&empty).value(&empty).is_err());
    }

    #[test]
    fn reference_and_opaque_descendants_cannot_gain_generic_write_controls() {
        for child in [
            ComponentValueShape::EntityRef,
            ComponentValueShape::opaque(ComponentValueKind::Map),
        ] {
            assert!(!supported(&ComponentValueShape::optional(
                ComponentValueShape::record([("child".into(), child)])
            )));
        }
    }
}
