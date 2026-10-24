use std::sync::Arc;

use thiserror::Error;

pub const MAX_UI_KEY_BYTES: usize = 256;
const MAX_UI_KEY_ATOM_BYTES: usize = MAX_UI_KEY_BYTES;

/// Bounded textual identifier. Binary source coordinates use [`UiOpaqueId`] instead.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiKeyAtom(String);

/// Exact bounded identity bytes supplied by an owning adapter, without text interpretation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiOpaqueId(Arc<[u8]>);

impl UiOpaqueId {
    pub fn new(value: impl AsRef<[u8]>) -> Result<Self, UiKeyError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(UiKeyError::EmptyAtom);
        }
        if value.len() > MAX_UI_KEY_ATOM_BYTES {
            return Err(UiKeyError::AtomTooLong {
                actual: value.len(),
                maximum: MAX_UI_KEY_ATOM_BYTES,
            });
        }
        Ok(Self(Arc::from(value)))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn encoded_len(&self) -> usize {
        self.0.len()
    }
}

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

/// Complete adapter-owned persistent address, independent of its eventual UI root.
///
/// Context and placement are required exact values. The adapter owns their canonical structure;
/// the kernel neither interprets Scene paths nor inherits missing context from a root key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiPersistentIdentity {
    context: UiOpaqueId,
    placement: UiOpaqueId,
}

impl UiPersistentIdentity {
    #[must_use]
    pub const fn new(context: UiOpaqueId, placement: UiOpaqueId) -> Self {
        Self { context, placement }
    }

    #[must_use]
    pub const fn context(&self) -> &UiOpaqueId {
        &self.context
    }

    #[must_use]
    pub const fn placement(&self) -> &UiOpaqueId {
        &self.placement
    }

    fn encoded_len(&self) -> usize {
        4 + self.context.encoded_len() + self.placement.encoded_len()
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
        4 + self.domain.encoded_len() + self.local.encoded_len()
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

    fn canonical_bytes(&self) -> [u8; MAX_UI_KEY_BYTES] {
        debug_assert!(self.encoded_len() <= MAX_UI_KEY_BYTES);
        let (tag, first, second) = match self {
            Self::Persistent(identity) => (
                0,
                identity.context.as_bytes(),
                identity.placement.as_bytes(),
            ),
            Self::Runtime(identity) => (
                1,
                identity.domain.as_str().as_bytes(),
                identity.local.as_str().as_bytes(),
            ),
        };
        let mut output = [0; MAX_UI_KEY_BYTES];
        output[0] = tag;
        output[1..3].copy_from_slice(&(first.len() as u16).to_le_bytes());
        let end_first = 3 + first.len();
        output[3..end_first].copy_from_slice(first);
        let end_second = end_first + second.len();
        output[end_first..end_second].copy_from_slice(second);
        // The outer address length frames the final field. Its length belongs after its bytes:
        // placing it first would erase shared path prefixes whenever leaf lengths differ.
        output[end_second..end_second + 2].copy_from_slice(&(second.len() as u16).to_le_bytes());
        output
    }

    fn common_prefix_len(&self, other: &Self) -> usize {
        let left = self.canonical_bytes();
        let right = other.canonical_bytes();
        left[..self.encoded_len()]
            .iter()
            .zip(&right[..other.encoded_len()])
            .take_while(|(left, right)| left == right)
            .count()
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
        validate_key_len(3 + identity.encoded_len())?;
        Ok(Self(Arc::new(identity)))
    }

    pub(crate) fn encoded_len(&self) -> usize {
        3 + self.0.encoded_len()
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
        let key = Self { root, local: None };
        validate_key_len(key.encoded_len())?;
        Ok(key)
    }

    pub fn scoped(root: UiRootKey, local: UiLocalKey) -> Result<Self, UiKeyError> {
        let key = Self {
            root,
            local: Some(local),
        };
        validate_key_len(key.encoded_len())?;
        Ok(key)
    }

    #[must_use]
    pub const fn root_key(&self) -> &UiRootKey {
        &self.root
    }

    #[must_use]
    pub const fn local_key(&self) -> Option<&UiLocalKey> {
        self.local.as_ref()
    }

    /// Exact canonical cost used by admission and owner-local reduced key limits.
    ///
    /// Shared content is charged once in the canonical scoped representation, regardless of
    /// pointer sharing. This is not an allocator or stable wire-format size promise.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        match &self.local {
            None => self.root.encoded_len(),
            Some(local) => {
                7 + self.root.0.encoded_len() + local.encoded_len()
                    - self.root.0.common_prefix_len(&local.0)
            }
        }
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
pub struct UiImageId(UiKeyAtom);

impl UiImageId {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self(UiKeyAtom::new(value)?))
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
