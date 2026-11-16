//! Component-local value storage shared only by independently admitted private owners.

use std::{
    fmt::{self, Debug, Formatter},
    mem::size_of,
    ops::Deref,
    sync::Arc,
};

use nara_reflect::ComponentValue;

/// Cloning shares storage; public source records use [`Self::deep_clone`] instead.
#[derive(Clone, PartialEq)]
pub(crate) struct ComponentPayload(Arc<ComponentValue>);

impl ComponentPayload {
    pub(crate) fn into_owned(self) -> ComponentValue {
        Arc::unwrap_or_clone(self.0)
    }

    pub(crate) fn make_mut(&mut self) -> &mut ComponentValue {
        Arc::make_mut(&mut self.0)
    }

    pub(crate) fn get_mut(&mut self) -> Option<&mut ComponentValue> {
        Arc::get_mut(&mut self.0)
    }

    pub(crate) fn deep_clone(&self) -> Self {
        Self::from(self.0.as_ref().clone())
    }

    /// Models the root value allocation and control/allocator overhead for each retained owner.
    /// The inline handle and descendant storage are accounted separately, without deduplication.
    pub(crate) const fn retained_bytes(&self) -> usize {
        size_of::<ComponentValue>() + 4 * size_of::<usize>()
    }
}

impl From<ComponentValue> for ComponentPayload {
    fn from(value: ComponentValue) -> Self {
        Self(Arc::new(value))
    }
}

impl Deref for ComponentPayload {
    type Target = ComponentValue;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl Debug for ComponentPayload {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.0.as_ref(), formatter)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(self.0.as_ref(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <ComponentValue as serde::Deserialize>::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests;
