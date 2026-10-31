//! Private publication state for one original CPU lineage in one Host/producer realization.
//!
//! Preparation owns a complete staged successor without changing live membership. The Host
//! validates the same basis before native work and at final publication; failed native attempts
//! retain it unchanged. All fallible metadata allocation precedes native work. This module does
//! not allocate native resources, register lifetimes, or authorize submission.

use std::{mem, sync::Arc};

use crate::graph::ResourceDescriptor;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HistoryKey {
    pub(crate) view: nara_render::RenderViewId,
    pub(crate) feature: u64,
    pub(crate) slot: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum HistoryError {
    #[error("history revision or metadata size overflow")]
    Overflow,
    #[error("history active slot capacity exceeded")]
    Capacity,
    #[error("history metadata allocation failed")]
    Allocation,
    #[error("history slot declared more than once")]
    DuplicateSlot,
    #[error("history descriptor changed without an explicit reset")]
    DescriptorMismatch,
    #[error("history basis was invalidated by another publication or owner operation")]
    StaleBasis,
    #[error("history capture has already been superseded or published")]
    RepeatedCapture,
    #[error("history keys or outputs do not match the complete declared slot set")]
    OutputMismatch,
    #[error("history basis belongs to another lineage realization")]
    ForeignBasis,
}

struct HistoryRow<T> {
    key: HistoryKey,
    descriptor: ResourceDescriptor,
    current: Option<Arc<T>>,
    was_reset: bool,
}

pub(crate) struct History<T> {
    owner: Arc<()>,
    max_active_slots: usize,
    revision: u64,
    last_capture: Option<u64>,
    rows: Vec<HistoryRow<T>>,
}

/// Move-only pinned facts and a preallocated full successor, including unrequested retained rows.
/// A successful commit consumes its successor. Failures do not change this basis.
pub(crate) struct HistoryBasis<T> {
    owner: Arc<()>,
    revision: u64,
    last_capture: Option<u64>,
    capture: u64,
    next_revision: u64,
    declared: Vec<HistoryKey>,
    next_rows: Vec<HistoryRow<T>>,
}

/// Keeps the previous complete table alive until the caller reaches a safe cleanup boundary.
/// Retained rows also occur in the new table; retiring this table only releases its own leases.
#[must_use = "keep old rows alive until the enclosing Host publication is coherent"]
pub(crate) struct RetiredRows<T> {
    _rows: Vec<HistoryRow<T>>,
}

impl<T> History<T> {
    pub(crate) fn new(max_active_slots: usize) -> Self {
        Self {
            owner: Arc::new(()),
            max_active_slots,
            revision: 0,
            last_capture: None,
            rows: Vec::new(),
        }
    }

    /// Read-only initialization diagnostics. Build inputs must instead use their pinned basis.
    #[cfg(test)]
    pub(crate) fn previous(&self, key: HistoryKey) -> Option<&Arc<T>> {
        self.row(key).and_then(|row| row.current.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn descriptor(&self, key: HistoryKey) -> Option<&ResourceDescriptor> {
        self.row(key).map(|row| &row.descriptor)
    }

    /// Rust-value storage owned by this live table, including its inline value and identity Arc.
    /// Excludes allocator overhead, static descriptor text, shared T allocations and candidates.
    #[cfg(test)]
    pub(crate) fn modeled_metadata_bytes(&self) -> Result<usize, HistoryError> {
        Self::required_metadata_bytes(self.rows.capacity())
    }

    /// Conservative full-capacity realization charge, reserved by the parent before construction.
    /// Includes the inline History, identity Arc counters and complete live row capacity. The
    /// parent's declaration lease table and separately owned candidate bases are additional.
    pub(crate) fn required_metadata_bytes(max_active_slots: usize) -> Result<usize, HistoryError> {
        let rows = collection_bytes::<HistoryRow<T>>(max_active_slots)?;
        mem::size_of::<Self>()
            .checked_add(2 * mem::size_of::<usize>())
            .and_then(|bytes| bytes.checked_add(rows))
            .ok_or(HistoryError::Overflow)
    }

    /// Admits the union of retained and declared slots before native work. An absent declaration
    /// never retires an existing slot, and even an explicitly empty slot retains its descriptor.
    pub(crate) fn prepare(
        &self,
        capture: u64,
        declarations: &[(HistoryKey, ResourceDescriptor)],
    ) -> Result<HistoryBasis<T>, HistoryError> {
        if self.last_capture.is_some_and(|last| capture <= last) {
            return Err(HistoryError::RepeatedCapture);
        }
        let next_revision = self.next_revision()?;
        let next_rows = self.stage_rows(declarations, false)?;
        let mut declared = reserved(declarations.len())?;
        declared.extend(declarations.iter().map(|(key, _)| *key));
        declared.sort_unstable();
        Ok(HistoryBasis {
            owner: Arc::clone(&self.owner),
            revision: self.revision,
            last_capture: self.last_capture,
            capture,
            next_revision,
            declared,
            next_rows,
        })
    }

    /// Global revision fencing is intentional: this initial scope has one group per capture.
    pub(crate) fn validate(&self, basis: &HistoryBasis<T>) -> Result<(), HistoryError> {
        if !Arc::ptr_eq(&self.owner, &basis.owner) {
            return Err(HistoryError::ForeignBasis);
        }
        if self.revision != basis.revision || self.last_capture != basis.last_capture {
            return Err(HistoryError::StaleBasis);
        }
        if self.last_capture.is_some_and(|last| basis.capture <= last) {
            return Err(HistoryError::RepeatedCapture);
        }
        Ok(())
    }

    /// Publishes exactly one output for each declared key, regardless of declaration/output order.
    /// All refusal happens before changing either the owner or basis. Once validated, publication
    /// needs no allocation, fallible operation, or destruction of a predecessor's final lease.
    pub(crate) fn commit(
        &mut self,
        basis: &mut HistoryBasis<T>,
        outputs: &[(HistoryKey, Arc<T>)],
    ) -> Result<RetiredRows<T>, HistoryError> {
        self.validate(basis)?;
        if outputs.len() != basis.declared.len() {
            return Err(HistoryError::OutputMismatch);
        }
        for (index, (key, _)) in outputs.iter().enumerate() {
            if basis.declared.binary_search(key).is_err()
                || outputs[..index].iter().any(|(previous, _)| previous == key)
            {
                return Err(HistoryError::OutputMismatch);
            }
        }

        for row in &mut basis.next_rows {
            if let Some((_, output)) = outputs.iter().find(|(key, _)| *key == row.key) {
                // The live table still pins every replaced predecessor. Releasing this staged
                // reference cannot invoke T::drop, even when an old value has a hostile destructor.
                row.current = Some(Arc::clone(output));
            }
        }
        let rows = mem::take(&mut basis.next_rows);
        Ok(self.publish(rows, basis.next_revision, Some(basis.capture)))
    }

    /// Explicitly empties the selected slots and may replace their descriptors. The successful
    /// capture watermark survives even a reset of every slot or an empty reset declaration.
    pub(crate) fn reset(
        &mut self,
        declarations: &[(HistoryKey, ResourceDescriptor)],
    ) -> Result<RetiredRows<T>, HistoryError> {
        let revision = self.next_revision()?;
        let mut rows = self.stage_rows(declarations, true)?;
        for row in &mut rows {
            if let Some((_, descriptor)) = declarations.iter().find(|(key, _)| *key == row.key) {
                row.descriptor = descriptor.clone();
                // The unchanged live table keeps this value alive until publication has finished.
                row.current = None;
                row.was_reset = true;
            }
        }
        Ok(self.publish(rows, revision, self.last_capture))
    }

    /// The parent supplies known unique keys only after their original declaration leases permit
    /// reclamation. Retirement fences old candidates without clearing successful deduplication.
    pub(crate) fn retire(&mut self, keys: &[HistoryKey]) -> Result<RetiredRows<T>, HistoryError> {
        let revision = self.next_revision()?;
        for (index, key) in keys.iter().enumerate() {
            if keys[..index].contains(key) {
                return Err(HistoryError::DuplicateSlot);
            }
            if self.row(*key).is_none() {
                return Err(HistoryError::OutputMismatch);
            }
        }
        let count = self
            .rows
            .iter()
            .filter(|row| !keys.contains(&row.key))
            .count();
        let mut rows = reserved(count)?;
        for row in &self.rows {
            if !keys.contains(&row.key) {
                rows.push(HistoryRow {
                    key: row.key,
                    descriptor: row.descriptor.clone(),
                    current: row.current.as_ref().map(Arc::clone),
                    was_reset: row.was_reset,
                });
            }
        }
        Ok(self.publish(rows, revision, self.last_capture))
    }

    fn next_revision(&self) -> Result<u64, HistoryError> {
        self.revision.checked_add(1).ok_or(HistoryError::Overflow)
    }

    fn row(&self, key: HistoryKey) -> Option<&HistoryRow<T>> {
        self.rows
            .binary_search_by_key(&key, |row| row.key)
            .ok()
            .and_then(|index| self.rows.get(index))
    }

    fn stage_rows(
        &self,
        declarations: &[(HistoryKey, ResourceDescriptor)],
        allow_descriptor_change: bool,
    ) -> Result<Vec<HistoryRow<T>>, HistoryError> {
        if declarations.len() > self.max_active_slots {
            return Err(HistoryError::Capacity);
        }
        let mut count = self.rows.len();
        for (index, (key, descriptor)) in declarations.iter().enumerate() {
            if declarations[..index]
                .iter()
                .any(|(previous, _)| previous == key)
            {
                return Err(HistoryError::DuplicateSlot);
            }
            if let Some(row) = self.row(*key) {
                if !allow_descriptor_change && row.descriptor != *descriptor {
                    return Err(HistoryError::DescriptorMismatch);
                }
            } else {
                count = count.checked_add(1).ok_or(HistoryError::Overflow)?;
            }
        }
        if count > self.max_active_slots {
            return Err(HistoryError::Capacity);
        }

        let mut rows = reserved(count)?;
        for row in &self.rows {
            rows.push(HistoryRow {
                key: row.key,
                descriptor: row.descriptor.clone(),
                current: row.current.as_ref().map(Arc::clone),
                was_reset: row.was_reset,
            });
        }
        for (key, descriptor) in declarations {
            if self.row(*key).is_none() {
                rows.push(HistoryRow {
                    key: *key,
                    descriptor: descriptor.clone(),
                    current: None,
                    was_reset: false,
                });
            }
        }
        rows.sort_unstable_by_key(|row| row.key);
        Ok(rows)
    }

    fn publish(
        &mut self,
        rows: Vec<HistoryRow<T>>,
        revision: u64,
        last_capture: Option<u64>,
    ) -> RetiredRows<T> {
        let retired = mem::replace(&mut self.rows, rows);
        self.revision = revision;
        self.last_capture = last_capture;
        RetiredRows { _rows: retired }
    }
}

impl<T> HistoryBasis<T> {
    /// Returns the exact pinned predecessor, with None only for a declared empty slot. An
    /// undeclared key refuses; a consumed basis is stale. This never consults the owner's latest
    /// state and does not require T: Clone.
    pub(crate) fn previous(&self, key: HistoryKey) -> Result<Option<&Arc<T>>, HistoryError> {
        Ok(self.declared_row(key)?.current.as_ref())
    }

    /// Whether this exact slot was explicitly reset. A populated predecessor takes precedence
    /// over this diagnostic; an unrelated new slot never inherits another slot's reset reason.
    pub(crate) fn was_reset(&self, key: HistoryKey) -> Result<bool, HistoryError> {
        Ok(self.declared_row(key)?.was_reset)
    }

    fn declared_row(&self, key: HistoryKey) -> Result<&HistoryRow<T>, HistoryError> {
        if self.declared.binary_search(&key).is_err() {
            return Err(HistoryError::OutputMismatch);
        }
        self.next_rows
            .binary_search_by_key(&key, |row| row.key)
            .ok()
            .and_then(|index| self.next_rows.get(index))
            .ok_or(HistoryError::StaleBasis)
    }

    /// Additional Rust-value storage owned by this basis's preallocated vectors. The containing
    /// PreparedHistory already counts the inline basis. Shared owner/T allocations, allocator
    /// overhead and static descriptor text are excluded; the full successor includes retained
    /// unrequested slots, not just this capture's declarations.
    pub(crate) fn metadata_bytes(&self) -> Result<usize, HistoryError> {
        collection_bytes::<HistoryRow<T>>(self.next_rows.capacity())?
            .checked_add(collection_bytes::<HistoryKey>(self.declared.capacity())?)
            .ok_or(HistoryError::Overflow)
    }
}

fn collection_bytes<T>(count: usize) -> Result<usize, HistoryError> {
    let bytes = count
        .checked_mul(mem::size_of::<T>())
        .ok_or(HistoryError::Overflow)?;
    if bytes > isize::MAX as usize {
        return Err(HistoryError::Overflow);
    }
    Ok(bytes)
}

fn reserved<T>(count: usize) -> Result<Vec<T>, HistoryError> {
    collection_bytes::<T>(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| HistoryError::Allocation)?;
    Ok(values)
}

#[cfg(test)]
mod tests;
