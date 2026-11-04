use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_RENDER_VIEW: AtomicU64 = AtomicU64::new(1);

/// Non-reused process-local identity for one View lifetime.
///
/// Camera entities, targets, extents and ordering do not establish identity. Manual producers
/// retain this value with their View owner across captures and acquire another root when that
/// lifetime ends. This runtime identity is not persistent camera authoring data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderViewId {
    root: u64,
    subview: Option<u32>,
}

impl RenderViewId {
    /// Allocates a new root incarnation, or returns `None` when process-local IDs are exhausted.
    #[must_use]
    pub fn try_acquire() -> Option<Self> {
        NEXT_RENDER_VIEW
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(|root| Self {
                root,
                subview: None,
            })
    }

    /// Identifies an explicit code-owned subview within this root's lifetime.
    ///
    /// The same root and index always produce the same identity, distinct from the root itself
    /// and every other root. Calling this on a subview selects another index in the same root;
    /// it does not create a nested namespace. The owner must retain each index's meaning for the
    /// entire root lifetime, or acquire a new root before reusing it for another View lifetime.
    #[must_use]
    pub const fn subview(self, index: u32) -> Self {
        Self {
            root: self.root,
            subview: Some(index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_view_roots_and_retained_subviews_do_not_alias() {
        let first = RenderViewId::try_acquire().unwrap();
        let second = RenderViewId::try_acquire().unwrap();
        let ids = [
            first,
            first.subview(0),
            first.subview(u32::MAX),
            second,
            second.subview(0),
            second.subview(u32::MAX),
        ];

        assert_eq!(first.subview(0).subview(u32::MAX), first.subview(u32::MAX));
        assert_eq!(
            ids.into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            ids.len()
        );
    }
}
