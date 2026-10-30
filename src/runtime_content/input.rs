//! Global processed Input retains its original typed acquisition independently of Scene residency.

use std::{
    fmt,
    mem::size_of,
    sync::{Arc, Weak},
};

use nara_asset::AssetProductRef;
use nara_core::ByteLimit;
use nara_input::{
    ActionMap, ActionMapAsset, ActionMapBindings, ActionMapDefinition, ActionMapDomain,
};

use super::{
    RuntimeContentAsset, RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentError,
    RuntimeContentLease,
};

/// One startup Input acquisition retained independently of current Scene content.
///
/// Clones and escaped bindings share the original typed asset, base and output reservation.
/// Acquiring the same Product again creates a separate owner and a separate charge. This value
/// grants no authority to install Input or share mutable context state between Worlds.
#[derive(Clone)]
pub struct RuntimeInputContent {
    inner: Arc<RuntimeInputOwner>,
}

pub(crate) struct RuntimeInputObservation(Weak<RuntimeInputOwner>);

impl RuntimeInputObservation {
    pub(crate) fn matches(&self, content: &RuntimeInputContent) -> bool {
        Weak::ptr_eq(&self.0, &Arc::downgrade(&content.inner))
    }
}

struct RuntimeInputOwner {
    asset: RuntimeContentAsset<ActionMapAsset>,
    _lease: RuntimeContentLease,
}

impl AsRef<ActionMapDefinition> for RuntimeInputOwner {
    fn as_ref(&self) -> &ActionMapDefinition {
        self.asset.product().definition()
    }
}

impl fmt::Debug for RuntimeInputContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeInputContent")
            .field("product", &self.product())
            .finish_non_exhaustive()
    }
}

impl RuntimeInputContent {
    pub(crate) fn observe(&self) -> RuntimeInputObservation {
        RuntimeInputObservation(Arc::downgrade(&self.inner))
    }
    /// The complete primary Product selected by the original Runtime configuration.
    #[must_use]
    pub fn product(&self) -> AssetProductRef {
        self.inner.asset.reference()
    }
    /// The original immutable content base and executable plan retained by this acquisition.
    #[must_use]
    pub fn base(&self) -> &RuntimeContentBase {
        self.inner.asset.base()
    }
    /// Borrows compiled data without detaching it from its acquisition owner.
    #[must_use]
    pub fn definition(&self) -> &ActionMapDefinition {
        self.inner.asset.product().definition()
    }
    /// Captures immutable compiled data while retaining this actual acquisition and its charge.
    #[must_use]
    pub fn bindings(&self) -> ActionMapBindings {
        ActionMapBindings::from_owner(Arc::clone(&self.inner))
    }
    /// Compares acquisition ownership, independently of equal Product or catalog bytes.
    #[must_use]
    pub fn shares_owner_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub(crate) fn prepare_state(&self) -> Result<ActionMap, RuntimeContentError> {
        let required = ActionMap::required_state_bytes();
        let lease = self.base().reserve_retained(required)?;
        ActionMap::new_with_retention_owner(
            self.bindings(),
            ByteLimit::new(required).expect("nonzero Input state cost"),
            Arc::new(lease),
        )
        .map_err(|_| RuntimeContentError::BudgetOverflow {
            kind: RuntimeContentBudgetKind::RetainedBytes,
        })
    }
}

impl RuntimeContentBase {
    /// Acquires the configured startup Input through this base's original selected family/codec.
    /// The returned owner and escaped bindings retain the actual typed acquisition and its charge.
    pub fn acquire_startup_input(
        &self,
    ) -> Result<Option<RuntimeInputContent>, RuntimeContentError> {
        let Some(product) = self.plan().settings().input().action_map else {
            return Ok(None);
        };
        let metadata =
            size_of::<RuntimeInputOwner>() + crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;
        let lease = self.reserve_retained(metadata)?;
        let asset = self
            .plan()
            .runtime_codecs()
            .acquire_selected::<ActionMapDomain>(self, product)?;
        Ok(Some(RuntimeInputContent {
            inner: Arc::new(RuntimeInputOwner {
                asset,
                _lease: lease,
            }),
        }))
    }
}

#[cfg(test)]
mod tests;
