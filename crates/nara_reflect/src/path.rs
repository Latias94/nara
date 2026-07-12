//! Schema-aware component field paths and path mutation errors.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ComponentFieldPath {
    segments: Vec<ComponentFieldPathSegment>,
}

impl ComponentFieldPath {
    #[must_use]
    pub fn new(segments: impl IntoIterator<Item = ComponentFieldPathSegment>) -> Self {
        Self {
            segments: segments.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn from_fields(fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::new(fields.into_iter().map(ComponentFieldPathSegment::field))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    #[must_use]
    pub fn segments(&self) -> &[ComponentFieldPathSegment] {
        &self.segments
    }

    #[must_use]
    pub fn into_segments(self) -> Vec<ComponentFieldPathSegment> {
        self.segments
    }
}

impl FromIterator<ComponentFieldPathSegment> for ComponentFieldPath {
    fn from_iter<T: IntoIterator<Item = ComponentFieldPathSegment>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl Display for ComponentFieldPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            return formatter.write_str("<root>");
        }

        for (index, segment) in self.segments.iter().enumerate() {
            match segment {
                ComponentFieldPathSegment::Field(field) => {
                    if index > 0 {
                        formatter.write_str(".")?;
                    }
                    formatter.write_str(field)?;
                }
                ComponentFieldPathSegment::Index(list_index) => {
                    write!(formatter, "[{list_index}]")?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentFieldPathSegment {
    Field(String),
    Index(u32),
}

impl ComponentFieldPathSegment {
    #[must_use]
    pub fn field(field: impl Into<String>) -> Self {
        Self::Field(field.into())
    }

    #[must_use]
    pub const fn index(index: u32) -> Self {
        Self::Index(index)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentFieldPathSegment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("ComponentFieldPathSegment", 2)?;
        match self {
            Self::Field(field) => {
                state.serialize_field("kind", "field")?;
                state.serialize_field("value", field)?;
            }
            Self::Index(index) => {
                state.serialize_field("kind", "index")?;
                state.serialize_field("value", index)?;
            }
        }
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentFieldPathSegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            kind: String,
            value: SegmentValue,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum SegmentValue {
            Field(String),
            Index(u32),
        }

        let wire = Wire::deserialize(deserializer)?;
        match (wire.kind.as_str(), wire.value) {
            ("field", SegmentValue::Field(field)) => Ok(Self::Field(field)),
            ("index", SegmentValue::Index(index)) => Ok(Self::Index(index)),
            ("field", _) => Err(serde::de::Error::custom(
                "component field path segment 'field' value must be a string",
            )),
            ("index", _) => Err(serde::de::Error::custom(
                "component field path segment 'index' value must be an integer",
            )),
            (kind, _) => Err(serde::de::Error::custom(format!(
                "unknown component field path segment kind '{kind}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentFieldPathError {
    EmptyPath,
    ExpectedMap {
        path: ComponentFieldPath,
    },
    ExpectedList {
        path: ComponentFieldPath,
    },
    MissingField {
        path: ComponentFieldPath,
        field: String,
    },
    IndexOutOfBounds {
        path: ComponentFieldPath,
        index: u32,
        len: usize,
    },
}

impl Display for ComponentFieldPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => formatter.write_str("component field path is empty"),
            Self::ExpectedMap { path } => {
                write!(formatter, "expected map at component field path '{path}'")
            }
            Self::ExpectedList { path } => {
                write!(formatter, "expected list at component field path '{path}'")
            }
            Self::MissingField { path, field } => {
                write!(
                    formatter,
                    "missing component field '{field}' at path '{path}'"
                )
            }
            Self::IndexOutOfBounds { path, index, len } => {
                write!(
                    formatter,
                    "component field path '{path}' index {index} is out of bounds for list length {len}"
                )
            }
        }
    }
}

impl Error for ComponentFieldPathError {}

pub(crate) fn component_path_with(
    prefix: &[ComponentFieldPathSegment],
    segment: &ComponentFieldPathSegment,
) -> ComponentFieldPath {
    prefix
        .iter()
        .cloned()
        .chain(std::iter::once(segment.clone()))
        .collect()
}
