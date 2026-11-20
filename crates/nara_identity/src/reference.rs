//! Pure authored reference grammar and shared logical cost accounting.
//!
//! Scene interprets routes and input endpoints in the actual lexical declaration occurrence.
//! These values neither select a key-global environment nor resolve or load a source.

use std::fmt::{self, Display, Formatter};

use thiserror::Error;

use crate::{EntityReference, SceneEntityId, SourceDocumentKey, SourceDocumentOrigin};

#[cfg(feature = "serde")]
mod wire;

const PERSISTENT_RUNTIME_UUID_BYTES: usize = 16;
const SOURCE_KEY_UUID_TEXT_BYTES: usize = 36;

/// An entity in one Prefab layer's inherited or instance-addition namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrefabEntityOrigin {
    SourceEntity(SceneEntityId),
    InstanceAddition(SceneEntityId),
}

impl PrefabEntityOrigin {
    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        match self {
            Self::SourceEntity(entity) | Self::InstanceAddition(entity) => entity,
        }
    }
}

/// Enters an instance only while its actual canonical base matches this claim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabInstanceHop {
    pub anchor: PrefabEntityOrigin,
    pub expected_base: SourceDocumentKey,
}

/// An authored target relative to its declaring overlay, never an expanded locator.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabEntityTarget {
    #[cfg_attr(feature = "serde", serde(default))]
    pub through: Vec<PrefabInstanceHop>,
    pub entity: PrefabEntityOrigin,
}

impl PrefabEntityTarget {
    #[must_use]
    pub const fn source(entity: SceneEntityId) -> Self {
        Self {
            through: Vec::new(),
            entity: PrefabEntityOrigin::SourceEntity(entity),
        }
    }

    #[must_use]
    pub const fn addition(entity: SceneEntityId) -> Self {
        Self {
            through: Vec::new(),
            entity: PrefabEntityOrigin::InstanceAddition(entity),
        }
    }

    /// Counts the terminal ID and each hop's anchor and canonical key up to `maximum`.
    /// Stops at the first hop that would exceed the allowance.
    #[must_use]
    pub fn logical_bytes_up_to(&self, maximum: usize) -> Option<usize> {
        route_logical_bytes(&self.through, self.entity.entity().as_str().len(), maximum)
    }
}

/// A declared reference input name, without occurrence or resolution authority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceInputId(String);

impl ReferenceInputId {
    pub const MAX_BYTES: usize = 128;

    /// Validates a nonempty ASCII name containing only letters, digits, `_`, `.`, or `-`.
    pub fn new(id: impl Into<String>) -> Result<Self, ReferenceInputIdError> {
        let id = id.into();
        if id.is_empty() {
            return Err(ReferenceInputIdError::Empty);
        }
        if id.len() > Self::MAX_BYTES {
            return Err(ReferenceInputIdError::TooLong {
                length: id.len(),
                maximum: Self::MAX_BYTES,
            });
        }
        if let Some(character) = id.chars().find(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '.' | '-')
        }) {
            return Err(ReferenceInputIdError::InvalidCharacter(character));
        }
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ReferenceInputId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ReferenceInputId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ReferenceInputId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

/// Validation failures retain only bounded diagnostics, never the rejected input string.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceInputIdError {
    #[error("reference input id cannot be empty")]
    Empty,
    #[error("reference input id exceeds its byte limit")]
    TooLong { length: usize, maximum: usize },
    #[error("reference input id contains an invalid character")]
    InvalidCharacter(char),
}

/// The endpoint of an authored reference after traversing its explicit occurrence route.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuthoredReferenceTarget {
    Entity(PrefabEntityOrigin),
    Input(ReferenceInputId),
}

/// A pure source reference interpreted by Scene in its lexical declaration occurrence.
///
/// An empty route starts in the payload's declaration environment. An input has no fallback
/// to a selected base, parent occurrence, or another occurrence with the same source key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct AuthoredEntityReference {
    #[cfg_attr(feature = "serde", serde(default))]
    pub through: Vec<PrefabInstanceHop>,
    pub target: AuthoredReferenceTarget,
}

impl AuthoredEntityReference {
    /// Constant-time logical node count, suitable for admission before scanning route bytes.
    #[must_use]
    pub fn cost_nodes(&self) -> usize {
        self.through.len().saturating_mul(2).saturating_add(2)
    }

    #[must_use]
    pub const fn source(entity: SceneEntityId) -> Self {
        Self {
            through: Vec::new(),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(entity)),
        }
    }

    #[must_use]
    pub const fn addition(entity: SceneEntityId) -> Self {
        Self {
            through: Vec::new(),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(entity)),
        }
    }

    #[must_use]
    pub const fn input(input: ReferenceInputId) -> Self {
        Self {
            through: Vec::new(),
            target: AuthoredReferenceTarget::Input(input),
        }
    }

    /// Returns the shared logical cost without allocating or serializing the reference.
    ///
    /// The root and endpoint own two nodes. Each hop owns two more: the hop and its complete
    /// source key. Bytes include the endpoint ID, every anchor ID, and every key's payload.
    /// Each key charges 36 bytes for its UUID, UTF-8 name/path bytes, and one byte for document
    /// kind. Origin tags, reference tags and serialized punctuation are excluded. Shared key
    /// names are charged for every hop. Both counters saturate on overflow.
    #[must_use]
    pub fn cost(&self) -> EntityReferenceCost {
        EntityReferenceCost {
            nodes: self.cost_nodes(),
            logical_bytes: self.logical_bytes_up_to(usize::MAX).unwrap_or(usize::MAX),
        }
    }

    fn logical_bytes_up_to(&self, maximum: usize) -> Option<usize> {
        let endpoint_bytes = match &self.target {
            AuthoredReferenceTarget::Entity(entity) => entity.entity().as_str().len(),
            AuthoredReferenceTarget::Input(input) => input.as_str().len(),
        };
        route_logical_bytes(&self.through, endpoint_bytes, maximum)
    }
}

impl From<PrefabEntityTarget> for AuthoredEntityReference {
    fn from(target: PrefabEntityTarget) -> Self {
        Self {
            through: target.through,
            target: AuthoredReferenceTarget::Entity(target.entity),
        }
    }
}

/// Shared logical reference cost, independent of allocator capacity and serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityReferenceCost {
    nodes: usize,
    logical_bytes: usize,
}

impl EntityReferenceCost {
    #[must_use]
    pub const fn nodes(&self) -> usize {
        self.nodes
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> usize {
        self.logical_bytes
    }
}

impl EntityReference {
    /// Returns the ID addressed by a `SceneLocal` or zero-hop authored `SourceEntity` reference.
    ///
    /// The ID is relative to the reference's lexical declaration namespace.
    #[must_use]
    pub fn direct_source_entity(&self) -> Option<&SceneEntityId> {
        match self {
            Self::SceneLocal { entity } => Some(entity),
            Self::Authored { reference } if reference.through.is_empty() => {
                match &reference.target {
                    AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(entity)) => {
                        Some(entity)
                    }
                    _ => None,
                }
            }
            Self::Authored { .. } | Self::Persistent { .. } => None,
        }
    }

    /// Returns the logical node count without visiting an authored reference's hops.
    /// Use [`Self::cost_up_to`] to check both node and byte ceilings.
    #[must_use]
    pub fn cost_nodes(&self) -> usize {
        match self {
            Self::Authored { reference } => reference.cost_nodes(),
            Self::SceneLocal { .. } | Self::Persistent { .. } => 1,
        }
    }

    /// Returns the canonical logical cost for reference-bearing value and compiler budgets.
    ///
    /// Native references retain their original cost: one node and the local ID's byte length,
    /// or one node and the persistent namespace's byte length plus 16 UUID bytes. Authored
    /// references use [`AuthoredEntityReference::cost`]. Accounting never allocates or formats
    /// an ID, and byte totals saturate on overflow.
    #[must_use]
    pub fn cost(&self) -> EntityReferenceCost {
        match self {
            Self::SceneLocal { entity } => EntityReferenceCost {
                nodes: 1,
                logical_bytes: entity.as_str().len(),
            },
            Self::Persistent { entity } => EntityReferenceCost {
                nodes: 1,
                logical_bytes: entity
                    .namespace
                    .as_str()
                    .len()
                    .saturating_add(PERSISTENT_RUNTIME_UUID_BYTES),
            },
            Self::Authored { reference } => reference.cost(),
        }
    }

    /// Returns the same saturating cost as [`Self::cost`] if both inclusive ceilings hold.
    ///
    /// Checks the constant-time node count first. For a finite byte ceiling, stops at the
    /// endpoint or first hop that puts the byte count over the ceiling.
    #[must_use]
    pub fn cost_up_to(
        &self,
        max_nodes: usize,
        max_logical_bytes: usize,
    ) -> Option<EntityReferenceCost> {
        let nodes = self.cost_nodes();
        if nodes > max_nodes {
            return None;
        }

        match self {
            Self::Authored { reference } if max_logical_bytes != usize::MAX => {
                let logical_bytes = reference.logical_bytes_up_to(max_logical_bytes)?;
                Some(EntityReferenceCost {
                    nodes,
                    logical_bytes,
                })
            }
            _ => {
                let cost = self.cost();
                (cost.logical_bytes() <= max_logical_bytes).then_some(cost)
            }
        }
    }
}

fn route_logical_bytes(
    through: &[PrefabInstanceHop],
    endpoint_bytes: usize,
    maximum: usize,
) -> Option<usize> {
    if endpoint_bytes > maximum {
        return None;
    }
    through.iter().try_fold(endpoint_bytes, |bytes, hop| {
        let bytes = bytes
            .checked_add(hop.anchor.entity().as_str().len())?
            .checked_add(source_key_logical_bytes(&hop.expected_base))?;
        (bytes <= maximum).then_some(bytes)
    })
}

fn source_key_logical_bytes(key: &SourceDocumentKey) -> usize {
    let text_bytes = match key.origin() {
        SourceDocumentOrigin::Named { name, .. } => name.len(),
        SourceDocumentOrigin::ProjectLocation { path, .. } => path.as_str().len(),
        SourceDocumentOrigin::StableAsset { .. } => 0,
    };
    text_bytes
        .saturating_add(SOURCE_KEY_UUID_TEXT_BYTES)
        .saturating_add(1)
}

#[cfg(test)]
mod tests;
