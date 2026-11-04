use std::{
    any::{Any, TypeId},
    cell::Cell,
    collections::HashMap,
    fmt,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_ui_kernel::{MAX_UI_KEY_BYTES, UiKey, UiKeyAtom, UiKeyError, UiOpaqueId, UiRootKey};
use thiserror::Error;

use crate::UiTextFieldError;

static NEXT_STORE: AtomicU64 = AtomicU64::new(1);

/// An exact root and sequence of parent/leaf atoms, independent of labels and row indices.
///
/// The complete root costs [`UiKey::encoded_len`] bytes; each path atom adds its UTF-8 bytes
/// and a two-byte length. The total must fit [`MAX_UI_KEY_BYTES`]. Atoms are compared in full,
/// so a delimiter inside one atom cannot alias a sequence of separate atoms.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UiStateKey {
    root: UiRootKey,
    path: Vec<UiKeyAtom>,
    encoded_len: usize,
}

impl UiStateKey {
    pub fn child(&self, leaf: impl Into<String>) -> Result<Self, UiStateError> {
        let leaf = UiKeyAtom::new(leaf)?;
        let encoded_len = self.encoded_len + 2 + leaf.as_str().len();
        if encoded_len > MAX_UI_KEY_BYTES {
            return Err(UiStateError::PathTooLong {
                actual: encoded_len,
                maximum: MAX_UI_KEY_BYTES,
            });
        }
        let mut path = self.path.clone();
        path.push(leaf);
        Ok(Self {
            root: self.root.clone(),
            path,
            encoded_len,
        })
    }

    #[must_use]
    pub const fn root_key(&self) -> &UiRootKey {
        &self.root
    }
}

/// Exact owner-supplied binding and schema for one process-local state value.
///
/// The Rust value's `TypeId` is checked separately. This is not a reload or persistence codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiStateBinding {
    binding: UiOpaqueId,
    schema: u64,
}

impl UiStateBinding {
    #[must_use]
    pub const fn new(binding: UiOpaqueId, schema: u64) -> Self {
        Self { binding, schema }
    }
}

/// A typed reference to one store and state incarnation, without owning its value.
///
/// Handles may outlive eviction or their store. Access then returns [`UiStateError::StaleHandle`],
/// including when another store uses the same root, path, binding and Rust type.
pub struct UiStateHandle<T> {
    key: UiStateKey,
    store: u64,
    incarnation: u64,
    marker: PhantomData<fn() -> T>,
}

impl<T> UiStateHandle<T> {
    #[must_use]
    pub const fn key(&self) -> &UiStateKey {
        &self.key
    }

    fn stale(&self) -> UiStateError {
        UiStateError::StaleHandle {
            key: self.key.clone(),
        }
    }
}

impl<T> Clone for UiStateHandle<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            store: self.store,
            incarnation: self.incarnation,
            marker: PhantomData,
        }
    }
}

impl<T> fmt::Debug for UiStateHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UiStateHandle")
            .field("key", &self.key)
            .field("store", &self.store)
            .field("incarnation", &self.incarnation)
            .finish()
    }
}

impl<T> PartialEq for UiStateHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.store == other.store && self.incarnation == other.incarnation
    }
}

impl<T> Eq for UiStateHandle<T> {}

/// Membership changes from one successfully finished pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiStateChanges {
    /// Previously committed incarnations selected without replacement.
    pub reused: usize,
    /// Freshly initialized incarnations, including explicit replacements.
    pub initialized: usize,
    /// Previously committed paths given fresh incarnations; also counted in `initialized`.
    pub replaced: usize,
    /// Old incarnations dropped by omission or replacement; includes `replaced`.
    pub retired: usize,
    /// Committed state count after the pass.
    pub live: usize,
}

struct StateEntry {
    binding: UiStateBinding,
    incarnation: u64,
    value: Box<dyn Any>,
}

enum Selection {
    Reused(u64),
    Staged(StateEntry),
}

impl Selection {
    fn incarnation(&self) -> u64 {
        match self {
            Self::Reused(incarnation) => *incarnation,
            Self::Staged(entry) => entry.incarnation,
        }
    }
}

/// Panel-instance-local ownership of bounded, process-local behavior state.
///
/// A pass selects at most `max_states` values. Committed plus staged payloads occupy at most
/// `2 * max_states` slots during composition; custom payload bytes are not bounded by this slot
/// limit. Values need not be `Send` or `Sync`. No document, framework, GPU or executor is owned.
///
/// Composition reads selected state; updates use [`Self::get_mut`] between passes. A finished
/// pass drops omitted/replaced values, including their local drafts and preedit, without
/// publishing an edit. Dropping a pass drops only its staged values. There is no hidden-state
/// retention or pinning policy.
///
/// Retirement discards owned local payloads. Native focus, IME sessions and external tasks
/// remain the responsibility of their Host or original owners; this store does not cancel them.
pub struct UiStateStore {
    root: UiStateKey,
    identity: u64,
    next_incarnation: u64,
    max_states: usize,
    states: HashMap<UiStateKey, StateEntry>,
    last_changes: UiStateChanges,
}

impl UiStateStore {
    pub fn new(root: UiRootKey, max_states: usize) -> Result<Self, UiStateError> {
        if max_states == 0 {
            return Err(UiStateError::InvalidLimit);
        }
        let encoded_len = UiKey::root(root.clone())?.encoded_len();
        let identity = NEXT_STORE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| UiStateError::IdentityExhausted)?;
        Ok(Self {
            root: UiStateKey {
                root,
                path: Vec::new(),
                encoded_len,
            },
            identity,
            next_incarnation: 1,
            max_states,
            states: HashMap::new(),
            last_changes: UiStateChanges::default(),
        })
    }

    #[must_use]
    pub fn root(&self) -> UiStateKey {
        self.root.clone()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn begin(&mut self) -> UiStatePass<'_> {
        UiStatePass {
            store: self,
            selected: HashMap::new(),
            aborted: Cell::new(false),
        }
    }

    pub fn get<T: 'static>(&self, handle: &UiStateHandle<T>) -> Result<&T, UiStateError> {
        if handle.store != self.identity {
            return Err(handle.stale());
        }
        let entry = self
            .states
            .get(handle.key())
            .filter(|entry| entry.incarnation == handle.incarnation)
            .ok_or_else(|| handle.stale())?;
        entry.value.downcast_ref().ok_or_else(|| handle.stale())
    }

    pub fn get_mut<T: 'static>(
        &mut self,
        handle: &UiStateHandle<T>,
    ) -> Result<&mut T, UiStateError> {
        if handle.store != self.identity {
            return Err(handle.stale());
        }
        let entry = self
            .states
            .get_mut(handle.key())
            .filter(|entry| entry.incarnation == handle.incarnation)
            .ok_or_else(|| handle.stale())?;
        entry.value.downcast_mut().ok_or_else(|| handle.stale())
    }

    /// Returns the last successful pass's changes; failed/dropped passes leave them intact.
    #[must_use]
    pub const fn last_changes(&self) -> UiStateChanges {
        self.last_changes
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut dyn Any> {
        self.states.values_mut().map(|entry| entry.value.as_mut())
    }

    fn allocate_incarnation(&mut self) -> Result<u64, UiStateError> {
        let incarnation = self.next_incarnation;
        self.next_incarnation = incarnation
            .checked_add(1)
            .ok_or(UiStateError::IdentityExhausted)?;
        Ok(incarnation)
    }
}

/// A transactional membership selection with read-only access to selected values.
///
/// Only [`Self::finish`] commits. Any selection/initialization/read error poisons the pass,
/// even if swallowed by its caller. Dropping an unfinished pass preserves prior membership
/// and values, and drops staged values. Allocated incarnations are never reused after abort.
#[must_use = "finish the state pass to commit its membership; dropping it rolls back"]
pub struct UiStatePass<'a> {
    store: &'a mut UiStateStore,
    selected: HashMap<UiStateKey, Selection>,
    aborted: Cell<bool>,
}

impl UiStatePass<'_> {
    #[must_use]
    pub fn root(&self) -> UiStateKey {
        self.store.root()
    }

    /// Selects compatible state, initializing lazily only when its path is absent.
    ///
    /// Existing state must match both the exact binding/schema and Rust type. A mismatch
    /// requires an explicit [`Self::replace`] in a new pass, rather than implicit destruction.
    pub fn try_state<T: 'static, E: From<UiStateError>>(
        &mut self,
        key: UiStateKey,
        binding: UiStateBinding,
        init: impl FnOnce() -> Result<T, E>,
    ) -> Result<UiStateHandle<T>, E> {
        self.select(key, binding, init, false)
    }

    /// Selects a fresh incarnation, even when the old type and binding are compatible.
    ///
    /// The old value and its handles remain valid if this pass fails or is dropped. Successful
    /// replacement counts as one `initialized`, one `replaced` and one `retired` state.
    pub fn replace<T: 'static, E: From<UiStateError>>(
        &mut self,
        key: UiStateKey,
        binding: UiStateBinding,
        init: impl FnOnce() -> Result<T, E>,
    ) -> Result<UiStateHandle<T>, E> {
        self.select(key, binding, init, true)
    }

    /// Reads a staged or reused incarnation selected by this pass.
    ///
    /// An unselected, replaced or foreign handle fails and poisons the pass.
    pub fn read<T: 'static>(&self, handle: &UiStateHandle<T>) -> Result<&T, UiStateError> {
        if self.aborted.get() {
            return Err(UiStateError::Aborted);
        }
        let result = (|| {
            if handle.store != self.store.identity {
                return Err(handle.stale());
            }
            let selection = self
                .selected
                .get(handle.key())
                .filter(|selection| selection.incarnation() == handle.incarnation)
                .ok_or_else(|| handle.stale())?;
            let entry = match selection {
                Selection::Reused(_) => self
                    .store
                    .states
                    .get(handle.key())
                    .ok_or_else(|| handle.stale())?,
                Selection::Staged(entry) => entry,
            };
            entry.value.downcast_ref().ok_or_else(|| handle.stale())
        })();
        if result.is_err() {
            self.aborted.set(true);
        }
        result
    }

    pub fn finish(self) -> Result<UiStateChanges, UiStateError> {
        if self.aborted.get() {
            return Err(UiStateError::Aborted);
        }
        let reused = self
            .selected
            .values()
            .filter(|selection| matches!(selection, Selection::Reused(_)))
            .count();
        let replaced = self
            .selected
            .iter()
            .filter(|(key, selection)| {
                matches!(selection, Selection::Staged(_)) && self.store.states.contains_key(*key)
            })
            .count();
        let changes = UiStateChanges {
            reused,
            initialized: self.selected.len() - reused,
            replaced,
            retired: self.store.states.len() - reused,
            live: self.selected.len(),
        };
        self.store
            .states
            .retain(|key, _| matches!(self.selected.get(key), Some(Selection::Reused(_))));
        for (key, selection) in self.selected {
            if let Selection::Staged(entry) = selection {
                self.store.states.insert(key, entry);
            }
        }
        self.store.last_changes = changes;
        Ok(changes)
    }

    fn select<T: 'static, E: From<UiStateError>>(
        &mut self,
        key: UiStateKey,
        binding: UiStateBinding,
        init: impl FnOnce() -> Result<T, E>,
        replace: bool,
    ) -> Result<UiStateHandle<T>, E> {
        if self.aborted.get() {
            return Err(UiStateError::Aborted.into());
        }
        // Leave the pass poisoned across every early return, including an initializer unwind.
        self.aborted.set(true);
        if key.root != self.store.root.root {
            return Err(UiStateError::ForeignRoot {
                key,
                expected: self.store.root.root.clone(),
            }
            .into());
        }
        if self.selected.contains_key(&key) {
            return Err(UiStateError::DuplicateKey { key }.into());
        }
        if self.selected.len() >= self.store.max_states {
            return Err(UiStateError::Capacity {
                maximum: self.store.max_states,
            }
            .into());
        }
        let selection = if !replace && let Some(entry) = self.store.states.get(&key) {
            if entry.binding != binding || entry.value.as_ref().type_id() != TypeId::of::<T>() {
                return Err(UiStateError::IncompatibleState { key }.into());
            }
            Selection::Reused(entry.incarnation)
        } else {
            let incarnation = self.store.allocate_incarnation().map_err(E::from)?;
            Selection::Staged(StateEntry {
                binding,
                incarnation,
                value: Box::new(init()?),
            })
        };
        let handle = UiStateHandle {
            key: key.clone(),
            store: self.store.identity,
            incarnation: selection.incarnation(),
            marker: PhantomData,
        };
        self.selected.insert(key, selection);
        self.aborted.set(false);
        Ok(handle)
    }
}

#[derive(Debug, Error)]
pub enum UiStateError {
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    TextField(#[from] UiTextFieldError),
    #[error("UI state slot limit must be greater than zero")]
    InvalidLimit,
    #[error("UI state pass exceeds its maximum of {maximum} selected states")]
    Capacity { maximum: usize },
    #[error("UI state path has {actual} encoded bytes; maximum is {maximum}")]
    PathTooLong { actual: usize, maximum: usize },
    #[error("UI state key {key:?} belongs to another root; expected {expected:?}")]
    ForeignRoot {
        key: UiStateKey,
        expected: UiRootKey,
    },
    #[error("UI state key {key:?} was selected more than once in one pass")]
    DuplicateKey { key: UiStateKey },
    #[error("UI state at {key:?} has an incompatible Rust type, binding or schema")]
    IncompatibleState { key: UiStateKey },
    #[error("UI state handle at {key:?} belongs to a retired or unselected incarnation or store")]
    StaleHandle { key: UiStateKey },
    #[error("UI state store or incarnation identity is exhausted")]
    IdentityExhausted,
    #[error("UI state pass was aborted by an earlier error")]
    Aborted,
}
