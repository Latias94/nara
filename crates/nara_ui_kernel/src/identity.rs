use std::sync::Arc;

use thiserror::Error;

pub const MAX_UI_KEY_BYTES: usize = 256;
const MAX_UI_KEY_ATOM_BYTES: usize = MAX_UI_KEY_BYTES;
const MAX_PROJECTION_STEPS: usize = 32;

/// An exact, opaque identity atom supplied by an owning adapter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiKeyAtom(String);

/// Semantic alias used when an atom represents an identity owned outside the kernel.
pub type UiOpaqueId = UiKeyAtom;

impl UiKeyAtom {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        let value = value.into();
        if value.is_empty() {
            return Err(UiKeyError::EmptyAtom);
        }
        if value.len() > MAX_UI_KEY_ATOM_BYTES {
            return Err(UiKeyError::AtomTooLong {
                actual: value.len(),
                maximum: MAX_UI_KEY_ATOM_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(UiKeyError::ControlCharacter);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn encoded_len(&self) -> usize {
        self.0.len()
    }
}

/// One exact source-document/source-entity step in a projection path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiProjectionStep {
    source_document: UiOpaqueId,
    source_entity: UiOpaqueId,
}

impl UiProjectionStep {
    #[must_use]
    pub const fn new(source_document: UiOpaqueId, source_entity: UiOpaqueId) -> Self {
        Self {
            source_document,
            source_entity,
        }
    }

    #[must_use]
    pub const fn source_document(&self) -> &UiOpaqueId {
        &self.source_document
    }

    #[must_use]
    pub const fn source_entity(&self) -> &UiOpaqueId {
        &self.source_entity
    }

    fn encoded_len(&self) -> usize {
        2 + self.source_document.encoded_len() + self.source_entity.encoded_len()
    }
}

/// A structured projection location kept separate from the projected source entity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiProjectionPath {
    root_anchor: UiOpaqueId,
    source_chain: Vec<UiProjectionStep>,
}

impl UiProjectionPath {
    #[must_use]
    pub const fn new(root_anchor: UiOpaqueId) -> Self {
        Self {
            root_anchor,
            source_chain: Vec::new(),
        }
    }

    pub fn with_step(mut self, step: UiProjectionStep) -> Result<Self, UiKeyError> {
        if self.source_chain.len() == MAX_PROJECTION_STEPS {
            return Err(UiKeyError::ProjectionDepthExceeded {
                maximum: MAX_PROJECTION_STEPS,
            });
        }
        self.source_chain.push(step);
        Ok(self)
    }

    #[must_use]
    pub const fn root_anchor(&self) -> &UiOpaqueId {
        &self.root_anchor
    }

    #[must_use]
    pub fn source_chain(&self) -> &[UiProjectionStep] {
        &self.source_chain
    }

    fn encoded_len(&self) -> usize {
        2 + self.root_anchor.encoded_len()
            + self
                .source_chain
                .iter()
                .map(UiProjectionStep::encoded_len)
                .sum::<usize>()
    }
}

/// Persistent source identity, optionally located through a structured projection path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiPersistentIdentity {
    source_entity: UiOpaqueId,
    projection: Option<UiProjectionPath>,
}

impl UiPersistentIdentity {
    #[must_use]
    pub const fn local(source_entity: UiOpaqueId) -> Self {
        Self {
            source_entity,
            projection: None,
        }
    }

    #[must_use]
    pub const fn projected(source_entity: UiOpaqueId, projection: UiProjectionPath) -> Self {
        Self {
            source_entity,
            projection: Some(projection),
        }
    }

    #[must_use]
    pub const fn source_entity(&self) -> &UiOpaqueId {
        &self.source_entity
    }

    #[must_use]
    pub const fn projection(&self) -> Option<&UiProjectionPath> {
        self.projection.as_ref()
    }

    fn encoded_len(&self) -> usize {
        2 + self.source_entity.encoded_len()
            + self
                .projection
                .as_ref()
                .map_or(0, UiProjectionPath::encoded_len)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct UiRuntimeIdentity {
    domain: UiKeyAtom,
    local: UiKeyAtom,
}

impl UiRuntimeIdentity {
    const fn new(domain: UiKeyAtom, local: UiKeyAtom) -> Self {
        Self { domain, local }
    }

    fn encoded_len(&self) -> usize {
        2 + self.domain.encoded_len() + self.local.encoded_len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum UiIdentity {
    Persistent(UiPersistentIdentity),
    Runtime(UiRuntimeIdentity),
}

impl UiIdentity {
    fn encoded_len(&self) -> usize {
        1 + match self {
            Self::Persistent(identity) => identity.encoded_len(),
            Self::Runtime(identity) => identity.encoded_len(),
        }
    }
}

/// Stable identity of one authored root on a surface.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiRootKey(Arc<UiIdentity>);

impl UiRootKey {
    pub fn persistent(identity: UiPersistentIdentity) -> Result<Self, UiKeyError> {
        Self::admit(UiIdentity::Persistent(identity))
    }

    pub fn runtime(
        domain: impl Into<String>,
        local: impl Into<String>,
    ) -> Result<Self, UiKeyError> {
        Self::admit(UiIdentity::Runtime(UiRuntimeIdentity::new(
            UiKeyAtom::new(domain)?,
            UiKeyAtom::new(local)?,
        )))
    }

    fn admit(identity: UiIdentity) -> Result<Self, UiKeyError> {
        validate_key_len(identity.encoded_len())?;
        Ok(Self(Arc::new(identity)))
    }

    pub(crate) fn encoded_len(&self) -> usize {
        self.0.encoded_len()
    }
}

/// Stable identity of one keyed node inside an authored root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiLocalKey(Arc<UiIdentity>);

impl UiLocalKey {
    pub fn persistent(identity: UiPersistentIdentity) -> Result<Self, UiKeyError> {
        Self::admit(UiIdentity::Persistent(identity))
    }

    pub fn runtime(
        domain: impl Into<String>,
        local: impl Into<String>,
    ) -> Result<Self, UiKeyError> {
        Self::admit(UiIdentity::Runtime(UiRuntimeIdentity::new(
            UiKeyAtom::new(domain)?,
            UiKeyAtom::new(local)?,
        )))
    }

    fn admit(identity: UiIdentity) -> Result<Self, UiKeyError> {
        validate_key_len(identity.encoded_len())?;
        Ok(Self(Arc::new(identity)))
    }

    fn encoded_len(&self) -> usize {
        self.0.encoded_len()
    }
}

/// Exact root-scoped logical node identity used by frame facts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiKey {
    root: UiRootKey,
    local: Option<UiLocalKey>,
}

impl UiKey {
    pub fn root(root: UiRootKey) -> Result<Self, UiKeyError> {
        validate_key_len(1 + root.encoded_len())?;
        Ok(Self { root, local: None })
    }

    pub fn scoped(root: UiRootKey, local: UiLocalKey) -> Result<Self, UiKeyError> {
        validate_key_len(2 + root.encoded_len() + local.encoded_len())?;
        Ok(Self {
            root,
            local: Some(local),
        })
    }

    #[must_use]
    pub const fn root_key(&self) -> &UiRootKey {
        &self.root
    }

    #[must_use]
    pub const fn local_key(&self) -> Option<&UiLocalKey> {
        self.local.as_ref()
    }

    pub(crate) fn encoded_len(&self) -> usize {
        1 + self.root.encoded_len() + self.local.as_ref().map_or(0, UiLocalKey::encoded_len)
    }
}

/// Bounded typed action identifier. It names intent but carries no callback or domain authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiActionId(UiKeyAtom);

impl UiActionId {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self(UiKeyAtom::new(value)?))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Opaque image identity retained by the kernel without importing an asset or renderer handle.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiImageId(UiOpaqueId);

impl UiImageId {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self(UiOpaqueId::new(value)?))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

fn validate_key_len(actual: usize) -> Result<(), UiKeyError> {
    if actual > MAX_UI_KEY_BYTES {
        Err(UiKeyError::KeyTooLong {
            actual,
            maximum: MAX_UI_KEY_BYTES,
        })
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiKeyError {
    #[error("UI key atom is empty")]
    EmptyAtom,
    #[error("UI key atom has {actual} bytes; maximum is {maximum}")]
    AtomTooLong { actual: usize, maximum: usize },
    #[error("UI key atom contains a control character")]
    ControlCharacter,
    #[error("UI projection path exceeds {maximum} steps")]
    ProjectionDepthExceeded { maximum: usize },
    #[error("UI key has {actual} encoded bytes; maximum is {maximum}")]
    KeyTooLong { actual: usize, maximum: usize },
}
