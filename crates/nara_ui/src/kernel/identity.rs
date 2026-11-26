use std::{any::TypeId, fmt, sync::Arc};

use thiserror::Error;

pub const MAX_UI_KEY_BYTES: usize = 256;

/// Bounded textual identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiKeyAtom(String);

impl UiKeyAtom {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        let value = value.into();
        if value.is_empty() {
            return Err(UiKeyError::EmptyAtom);
        }
        if value.len() > MAX_UI_KEY_BYTES {
            return Err(UiKeyError::AtomTooLong {
                actual: value.len(),
                maximum: MAX_UI_KEY_BYTES,
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
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum UiIdentity {
    #[cfg(feature = "runtime")]
    Entity(u64),
    Named {
        domain: UiKeyAtom,
        local: UiKeyAtom,
    },
}

impl UiIdentity {
    fn named(domain: impl Into<String>, local: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self::Named {
            domain: UiKeyAtom::new(domain)?,
            local: UiKeyAtom::new(local)?,
        })
    }

    fn encoded_len(&self) -> usize {
        match self {
            #[cfg(feature = "runtime")]
            Self::Entity(_) => 1 + size_of::<u64>(),
            Self::Named { domain, local } => 5 + domain.as_str().len() + local.as_str().len(),
        }
    }
}

/// Identity of one layout root on its surface.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiRootKey(Arc<UiIdentity>);

impl UiRootKey {
    pub fn runtime(
        domain: impl Into<String>,
        local: impl Into<String>,
    ) -> Result<Self, UiKeyError> {
        let identity = UiIdentity::named(domain, local)?;
        validate_key_len(3 + identity.encoded_len())?;
        Ok(Self(Arc::new(identity)))
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn entity(bits: u64) -> Self {
        Self(Arc::new(UiIdentity::Entity(bits)))
    }

    pub(crate) fn encoded_len(&self) -> usize {
        3 + self.0.encoded_len()
    }
}

/// Identity of one node relative to its layout root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiLocalKey(Arc<UiIdentity>);

impl UiLocalKey {
    pub fn runtime(
        domain: impl Into<String>,
        local: impl Into<String>,
    ) -> Result<Self, UiKeyError> {
        let identity = UiIdentity::named(domain, local)?;
        validate_key_len(identity.encoded_len())?;
        Ok(Self(Arc::new(identity)))
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn entity(bits: u64) -> Self {
        Self(Arc::new(UiIdentity::Entity(bits)))
    }

    fn encoded_len(&self) -> usize {
        self.0.encoded_len()
    }
}

/// Node identity used by immutable frame data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiKey {
    root: UiRootKey,
    local: Option<UiLocalKey>,
}

impl UiKey {
    pub fn root(root: UiRootKey) -> Result<Self, UiKeyError> {
        Ok(Self { root, local: None })
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

    /// Size used by the frame compiler to bound keys.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        self.root.encoded_len() + self.local.as_ref().map_or(0, UiLocalKey::encoded_len)
    }
}

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

/// Image identity retained in paint data without an asset or renderer handle.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiImageId(UiImageIdentity);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum UiImageIdentity {
    Named(UiKeyAtom),
    External { provider: TypeId, token: u64 },
}

impl UiImageId {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self(UiImageIdentity::Named(UiKeyAtom::new(value)?)))
    }

    /// Selects a process-local resource in the provider's identity domain.
    #[must_use]
    pub fn external<T: 'static>(token: u64) -> Self {
        Self(UiImageIdentity::External {
            provider: TypeId::of::<T>(),
            token,
        })
    }

    #[must_use]
    pub fn as_name(&self) -> Option<&str> {
        match &self.0 {
            UiImageIdentity::Named(name) => Some(name.as_str()),
            UiImageIdentity::External { .. } => None,
        }
    }
}

impl fmt::Display for UiImageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            UiImageIdentity::Named(name) => formatter.write_str(name.as_str()),
            UiImageIdentity::External { provider, token } => {
                write!(formatter, "external:{provider:?}/{token}")
            }
        }
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
    #[error("UI key has {actual} bytes; maximum is {maximum}")]
    KeyTooLong { actual: usize, maximum: usize },
}
