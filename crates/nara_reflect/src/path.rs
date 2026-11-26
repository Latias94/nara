//! Schema-aware component field paths and path mutation errors.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    str::{Chars, FromStr},
};

use crate::ComponentTypeId;

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

    /// Parses a qualified path after matching the already known component ID.
    pub fn parse_for(
        component: &ComponentTypeId,
        qualified: &str,
    ) -> Result<Self, ComponentFieldPathParseError> {
        let prefix_len = component.as_str().len() + 1;
        let relative = qualified
            .strip_prefix(component.as_str())
            .and_then(|suffix| suffix.strip_prefix('.'))
            .ok_or_else(|| ComponentFieldPathParseError::ComponentPrefix(component.clone()))?;
        relative.parse().map_err(|error| match error {
            ComponentFieldPathParseError::Syntax { offset } => {
                ComponentFieldPathParseError::Syntax {
                    offset: prefix_len + offset,
                }
            }
            error => error,
        })
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
                ComponentFieldPathSegment::Field(field) if bare_field(field) => {
                    if index > 0 {
                        formatter.write_str(".")?;
                    }
                    formatter.write_str(field)?;
                }
                ComponentFieldPathSegment::Field(field) => write!(formatter, "[{field:?}]")?,
                ComponentFieldPathSegment::Index(list_index) => {
                    write!(formatter, "[{list_index}]")?;
                }
            }
        }

        Ok(())
    }
}

impl FromStr for ComponentFieldPath {
    type Err = ComponentFieldPathParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parser = FieldPathParser {
            text,
            chars: text.chars(),
        };
        let first = if parser.peek() == Some('[') {
            let segment = parser.bracket()?;
            if !matches!(segment, ComponentFieldPathSegment::Field(_)) {
                return Err(ComponentFieldPathParseError::Syntax { offset: 0 });
            }
            segment
        } else {
            ComponentFieldPathSegment::Field(parser.field()?)
        };
        let mut path = Self {
            segments: vec![first],
        };
        while let Some(next) = parser.peek() {
            let segment = match next {
                '.' => {
                    parser.chars.next();
                    ComponentFieldPathSegment::Field(parser.field()?)
                }
                '[' => parser.bracket()?,
                _ => return Err(parser.error()),
            };
            path.segments.push(segment);
        }
        Ok(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComponentFieldPathParseError {
    #[error("invalid component field path syntax at byte {offset}")]
    Syntax { offset: usize },
    #[error("expected component field path prefixed by '{0}.'")]
    ComponentPrefix(ComponentTypeId),
}

struct FieldPathParser<'a> {
    text: &'a str,
    chars: Chars<'a>,
}

impl FieldPathParser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.clone().next()
    }

    fn offset(&self) -> usize {
        self.text.len() - self.chars.as_str().len()
    }

    fn error(&self) -> ComponentFieldPathParseError {
        ComponentFieldPathParseError::Syntax {
            offset: self.offset(),
        }
    }

    fn consume(&mut self, expected: char) -> Result<(), ComponentFieldPathParseError> {
        if self.peek() == Some(expected) {
            self.chars.next();
            Ok(())
        } else {
            Err(self.error())
        }
    }

    fn token(
        &mut self,
        accepts: impl Fn(char) -> bool,
    ) -> Result<&str, ComponentFieldPathParseError> {
        let start = self.offset();
        while self.peek().is_some_and(&accepts) {
            self.chars.next();
        }
        let end = self.offset();
        if start == end {
            return Err(self.error());
        }
        Ok(&self.text[start..end])
    }

    fn field(&mut self) -> Result<String, ComponentFieldPathParseError> {
        self.token(bare_field_char).map(str::to_owned)
    }

    fn bracket(&mut self) -> Result<ComponentFieldPathSegment, ComponentFieldPathParseError> {
        self.consume('[')?;
        let segment = if self.peek() == Some('"') {
            self.chars.next();
            ComponentFieldPathSegment::Field(self.quoted_field()?)
        } else {
            let start = self.offset();
            let index = self
                .token(|character| character.is_ascii_digit())?
                .parse::<u32>()
                .map_err(|_| ComponentFieldPathParseError::Syntax { offset: start })?;
            ComponentFieldPathSegment::Index(index)
        };
        self.consume(']')?;
        Ok(segment)
    }

    fn quoted_field(&mut self) -> Result<String, ComponentFieldPathParseError> {
        let mut name = String::new();
        while let Some(character) = self.chars.next() {
            match character {
                '"' => return Ok(name),
                '\\' => {
                    let escaped = match self.chars.next() {
                        Some('\\') => '\\',
                        Some('"') => '"',
                        Some('\'') => '\'',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('0') => '\0',
                        Some('u') => self.unicode_escape()?,
                        _ => return Err(self.error()),
                    };
                    name.push(escaped);
                }
                character if !character.is_control() => name.push(character),
                _ => return Err(self.error()),
            }
        }
        Err(self.error())
    }

    fn unicode_escape(&mut self) -> Result<char, ComponentFieldPathParseError> {
        self.consume('{')?;
        let start = self.offset();
        let digits = self.token(|character| character.is_ascii_hexdigit())?;
        let character = (digits.len() <= 6)
            .then(|| u32::from_str_radix(digits, 16).ok())
            .flatten()
            .and_then(char::from_u32)
            .ok_or(ComponentFieldPathParseError::Syntax { offset: start })?;
        self.consume('}')?;
        Ok(character)
    }
}

fn bare_field(field: &str) -> bool {
    !field.is_empty() && field.chars().all(bare_field_char)
}

fn bare_field_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
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
    InvalidPackedElement {
        path: ComponentFieldPath,
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
            Self::InvalidPackedElement { path } => {
                write!(
                    formatter,
                    "invalid packed element at component field path '{path}'"
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
