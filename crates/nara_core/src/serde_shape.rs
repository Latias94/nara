//! Allocation-bounded structural preflight for serde-backed persistent files.

use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{self, Display, Formatter},
};

use serde::de::{DeserializeSeed, Deserializer, Error as DeError, MapAccess, SeqAccess, Visitor};

use crate::{ByteLimit, DepthLimit, ItemLimit};

/// Format-neutral limits for a structural serde preflight.
///
/// Format owners choose the values. This type intentionally has no engine-wide defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerdeShapeLimits {
    depth: DepthLimit,
    nodes: ItemLimit,
    container_items: ItemLimit,
    string_bytes: ByteLimit,
    total_string_bytes: ByteLimit,
}

impl SerdeShapeLimits {
    #[must_use]
    pub const fn new(
        depth: DepthLimit,
        nodes: ItemLimit,
        container_items: ItemLimit,
        string_bytes: ByteLimit,
        total_string_bytes: ByteLimit,
    ) -> Self {
        Self {
            depth,
            nodes,
            container_items,
            string_bytes,
            total_string_bytes,
        }
    }

    #[must_use]
    pub const fn depth(self) -> DepthLimit {
        self.depth
    }

    #[must_use]
    pub const fn nodes(self) -> ItemLimit {
        self.nodes
    }

    #[must_use]
    pub const fn container_items(self) -> ItemLimit {
        self.container_items
    }

    #[must_use]
    pub const fn string_bytes(self) -> ByteLimit {
        self.string_bytes
    }

    #[must_use]
    pub const fn total_string_bytes(self) -> ByteLimit {
        self.total_string_bytes
    }
}

/// A deterministic failure from [`preflight_serde_shape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerdeShapeError {
    DepthExceeded { maximum: usize },
    NodeLimitExceeded { maximum: usize },
    ContainerItemLimitExceeded { maximum: usize },
    StringLimitExceeded { maximum: usize },
    TotalStringLimitExceeded { maximum: usize },
    DuplicateMapKey,
}

impl Display for SerdeShapeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthExceeded { maximum } => {
                write!(
                    formatter,
                    "persistent data nesting exceeds the limit of {maximum}"
                )
            }
            Self::NodeLimitExceeded { maximum } => {
                write!(
                    formatter,
                    "persistent data node count exceeds the limit of {maximum}"
                )
            }
            Self::ContainerItemLimitExceeded { maximum } => write!(
                formatter,
                "persistent data container item count exceeds the limit of {maximum}"
            ),
            Self::StringLimitExceeded { maximum } => write!(
                formatter,
                "persistent data string exceeds the limit of {maximum} bytes"
            ),
            Self::TotalStringLimitExceeded { maximum } => write!(
                formatter,
                "persistent data total string bytes exceed the limit of {maximum}"
            ),
            Self::DuplicateMapKey => {
                formatter.write_str("persistent data contains a duplicate map key")
            }
        }
    }
}

impl Error for SerdeShapeError {}

/// A syntax or structural-limit failure from [`preflight_serde_shape`].
#[derive(Debug)]
pub enum SerdeShapePreflightError<E> {
    Shape(SerdeShapeError),
    Parse(E),
}

impl<E: Display> Display for SerdeShapePreflightError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(error) => Display::fmt(error, formatter),
            Self::Parse(error) => Display::fmt(error, formatter),
        }
    }
}

impl<E: Error + 'static> Error for SerdeShapePreflightError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) => Some(error),
            Self::Parse(error) => Some(error),
        }
    }
}

/// Walks a serde value without constructing its domain representation.
///
/// This enforces only format-neutral structure. Domain-specific limits, migrations, and semantic
/// validation remain the responsibility of the owning loader.
pub fn preflight_serde_shape<'de, D>(
    deserializer: D,
    limits: SerdeShapeLimits,
) -> Result<(), SerdeShapePreflightError<D::Error>>
where
    D: Deserializer<'de>,
{
    let mut tracker = ShapeTracker::new(limits);
    let result = ShapeSeed {
        tracker: &mut tracker,
        depth: 1,
    }
    .deserialize(deserializer);
    match result {
        Ok(()) => Ok(()),
        Err(error) => match tracker.failure {
            Some(error) => Err(SerdeShapePreflightError::Shape(error)),
            None => Err(SerdeShapePreflightError::Parse(error)),
        },
    }
}

struct ShapeTracker {
    limits: SerdeShapeLimits,
    nodes: usize,
    total_string_bytes: usize,
    failure: Option<SerdeShapeError>,
}

impl ShapeTracker {
    const fn new(limits: SerdeShapeLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            total_string_bytes: 0,
            failure: None,
        }
    }

    fn record_failure(&mut self, error: SerdeShapeError) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
    }

    fn observe_node(&mut self, depth: usize) -> Result<(), SerdeShapeError> {
        if depth > self.limits.depth().get() {
            return Err(SerdeShapeError::DepthExceeded {
                maximum: self.limits.depth().get(),
            });
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(SerdeShapeError::NodeLimitExceeded {
                maximum: self.limits.nodes().get(),
            })?;
        if self.nodes > self.limits.nodes().get() {
            return Err(SerdeShapeError::NodeLimitExceeded {
                maximum: self.limits.nodes().get(),
            });
        }
        Ok(())
    }

    fn observe_string_bytes(&mut self, len: usize) -> Result<(), SerdeShapeError> {
        if len > self.limits.string_bytes().get() {
            return Err(SerdeShapeError::StringLimitExceeded {
                maximum: self.limits.string_bytes().get(),
            });
        }
        self.total_string_bytes = self.total_string_bytes.checked_add(len).ok_or(
            SerdeShapeError::TotalStringLimitExceeded {
                maximum: self.limits.total_string_bytes().get(),
            },
        )?;
        if self.total_string_bytes > self.limits.total_string_bytes().get() {
            return Err(SerdeShapeError::TotalStringLimitExceeded {
                maximum: self.limits.total_string_bytes().get(),
            });
        }
        Ok(())
    }

    fn observe_string(&mut self, value: &str) -> Result<(), SerdeShapeError> {
        self.observe_string_bytes(value.len())
    }

    fn observe_container_item(&self, items: usize) -> Result<(), SerdeShapeError> {
        if items > self.limits.container_items().get() {
            return Err(SerdeShapeError::ContainerItemLimitExceeded {
                maximum: self.limits.container_items().get(),
            });
        }
        Ok(())
    }
}

struct ShapeSeed<'a> {
    tracker: &'a mut ShapeTracker,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for ShapeSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        if let Err(error) = self.tracker.observe_node(self.depth) {
            self.tracker.record_failure(error);
            return Err(D::Error::custom(error));
        }
        deserializer.deserialize_any(ShapeVisitor {
            tracker: self.tracker,
            depth: self.depth,
        })
    }
}

struct ShapeVisitor<'a> {
    tracker: &'a mut ShapeTracker,
    depth: usize,
}

impl<'de> Visitor<'de> for ShapeVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a serde-supported persistent value")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_char<E>(mut self, value: char) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.observe_string(&value.to_string())
    }

    fn visit_str<E>(mut self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.observe_string(value)
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        if let Err(error) = self.tracker.observe_string_bytes(value.len()) {
            self.tracker.record_failure(error);
            return Err(E::custom(error));
        }
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        ShapeSeed {
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
        }
        .deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = 0_usize;
        while let Some(()) = sequence.next_element_seed(ShapeSeed {
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
        })? {
            items = items.checked_add(1).ok_or_else(|| {
                let error = SerdeShapeError::ContainerItemLimitExceeded {
                    maximum: self.tracker.limits.container_items().get(),
                };
                self.tracker.record_failure(error);
                A::Error::custom(error)
            })?;
            if let Err(error) = self.tracker.observe_container_item(items) {
                self.tracker.record_failure(error);
                return Err(A::Error::custom(error));
            }
        }
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        let mut items = 0_usize;
        while let Some(key) = map.next_key_seed(MapKeySeed {
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
        })? {
            items = items.checked_add(1).ok_or_else(|| {
                let error = SerdeShapeError::ContainerItemLimitExceeded {
                    maximum: self.tracker.limits.container_items().get(),
                };
                self.tracker.record_failure(error);
                A::Error::custom(error)
            })?;
            if let Err(error) = self.tracker.observe_container_item(items) {
                self.tracker.record_failure(error);
                return Err(A::Error::custom(error));
            }
            if !keys.insert(key) {
                self.tracker
                    .record_failure(SerdeShapeError::DuplicateMapKey);
                return Err(A::Error::custom(SerdeShapeError::DuplicateMapKey));
            }
            map.next_value_seed(ShapeSeed {
                tracker: self.tracker,
                depth: self.depth.saturating_add(1),
            })?;
        }
        Ok(())
    }
}

impl ShapeVisitor<'_> {
    fn observe_string<E>(&mut self, value: &str) -> Result<(), E>
    where
        E: DeError,
    {
        if let Err(error) = self.tracker.observe_string(value) {
            self.tracker.record_failure(error);
            return Err(E::custom(error));
        }
        Ok(())
    }
}

struct MapKeySeed<'a> {
    tracker: &'a mut ShapeTracker,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for MapKeySeed<'_> {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        if let Err(error) = self.tracker.observe_node(self.depth) {
            self.tracker.record_failure(error);
            return Err(D::Error::custom(error));
        }
        let key = deserializer.deserialize_any(MapKeyVisitor)?;
        if let Err(error) = self.tracker.observe_string(&key) {
            self.tracker.record_failure(error);
            return Err(D::Error::custom(error));
        }
        Ok(key)
    }
}

struct MapKeyVisitor;

impl<'de> Visitor<'de> for MapKeyVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a persistent map or struct field identifier")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value.to_owned())
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value)
    }
}
