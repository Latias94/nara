//! Host-independent hostile publication traces for the private production history owner.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::graph::BufferMeaning;

// Deliberately not Clone: only leases, never native resource contents, may be copied.
struct Value(u64);
struct OtherMeaning;

fn descriptor(size: u64) -> ResourceDescriptor {
    ResourceDescriptor::Buffer {
        native: wgpu::BufferDescriptor {
            label: Some("history-state-test"),
            size,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        },
        meaning: BufferMeaning::of::<Value>(),
    }
}

fn keys() -> [HistoryKey; 4] {
    let first = nara_render::RenderViewId::try_acquire().unwrap();
    let second = nara_render::RenderViewId::try_acquire().unwrap();
    [
        HistoryKey {
            view: first,
            feature: 10,
            slot: 0,
        },
        HistoryKey {
            view: first,
            feature: 11,
            slot: 0,
        },
        HistoryKey {
            view: second,
            feature: 10,
            slot: 0,
        },
        HistoryKey {
            view: second,
            feature: 11,
            slot: 0,
        },
    ]
}

fn declarations(keys: &[HistoryKey]) -> Vec<(HistoryKey, ResourceDescriptor)> {
    keys.iter().map(|key| (*key, descriptor(16))).collect()
}

fn publish(history: &mut History<Value>, capture: u64, keys: &[HistoryKey], values: &[u64]) {
    assert_eq!(keys.len(), values.len());
    let mut basis = history.prepare(capture, &declarations(keys)).unwrap();
    let outputs: Vec<_> = keys
        .iter()
        .zip(values)
        .map(|(key, value)| (*key, Arc::new(Value(*value))))
        .collect();
    drop(history.commit(&mut basis, &outputs).unwrap());
}

type RowSnapshot = (HistoryKey, ResourceDescriptor, Option<(usize, u64)>, bool);

fn rows_snapshot(rows: &[HistoryRow<Value>]) -> Vec<RowSnapshot> {
    rows.iter()
        .map(|row| {
            (
                row.key,
                row.descriptor.clone(),
                row.current
                    .as_ref()
                    .map(|value| (Arc::as_ptr(value) as usize, value.0)),
                row.was_reset,
            )
        })
        .collect()
}

fn snapshot(history: &History<Value>) -> (u64, Option<u64>, Vec<RowSnapshot>) {
    (
        history.revision,
        history.last_capture,
        rows_snapshot(&history.rows),
    )
}

fn current(history: &History<Value>, key: HistoryKey) -> Option<&Arc<Value>> {
    history
        .rows
        .iter()
        .find(|row| row.key == key)
        .and_then(|row| row.current.as_ref())
}

#[test]
fn preparing_new_membership_pins_facts_without_publishing_it() {
    let [a, b, c, _] = keys();
    let mut history = History::new(3);
    publish(&mut history, 4, &[a, b], &[40, 41]);
    let before = snapshot(&history);

    let basis = history.prepare(5, &declarations(&[c, a])).unwrap();
    assert_eq!(snapshot(&history), before);
    assert_eq!(basis.previous(a).unwrap().unwrap().0, 40);
    assert!(Arc::ptr_eq(
        basis.previous(a).unwrap().unwrap(),
        current(&history, a).unwrap()
    ));
    assert!(basis.previous(c).unwrap().is_none());
    assert!(matches!(
        basis.previous(b),
        Err(HistoryError::OutputMismatch)
    ));
    assert_eq!(basis.next_rows.len(), 3);
    assert!(
        basis
            .next_rows
            .iter()
            .any(|row| row.key == b && row.current.is_some())
    );
    assert_eq!(history.validate(&basis), Ok(()));
    drop(basis);
    assert_eq!(snapshot(&history), before);
}

#[test]
fn siblings_keep_original_predecessors_and_only_one_can_publish() {
    let [a, b, _, _] = keys();
    let mut history = History::new(2);
    publish(&mut history, 10, &[a, b], &[1, 2]);
    let mut older = history.prepare(11, &declarations(&[a, b])).unwrap();
    let mut newer = history.prepare(12, &declarations(&[b, a])).unwrap();
    let pinned = rows_snapshot(&older.next_rows);

    // Several failed native attempts do not edit or refresh the candidate's basis.
    for _ in 0..3 {
        assert_eq!(history.validate(&older), Ok(()));
        assert_eq!(rows_snapshot(&older.next_rows), pinned);
    }
    drop(
        history
            .commit(
                &mut newer,
                &[(a, Arc::new(Value(10))), (b, Arc::new(Value(20)))],
            )
            .unwrap(),
    );
    assert_eq!(older.previous(a).unwrap().unwrap().0, 1);
    assert_eq!(older.previous(b).unwrap().unwrap().0, 2);
    assert_eq!(history.validate(&older), Err(HistoryError::StaleBasis));
    let published = snapshot(&history);
    assert!(matches!(
        history.commit(
            &mut older,
            &[(b, Arc::new(Value(3))), (a, Arc::new(Value(4)))]
        ),
        Err(HistoryError::StaleBasis)
    ));
    assert_eq!(snapshot(&history), published);
    assert_eq!(rows_snapshot(&older.next_rows), pinned);
    assert_eq!(history.validate(&newer), Err(HistoryError::StaleBasis));
}

#[test]
fn empty_reset_aba_invalidates_a_basis_even_when_predecessors_stay_empty() {
    let [a, _, _, _] = keys();
    let mut history = History::<Value>::new(1);
    let missing = history.prepare(0, &declarations(&[a])).unwrap();
    drop(history.reset(&declarations(&[a])).unwrap());
    assert_eq!(history.validate(&missing), Err(HistoryError::StaleBasis));

    let empty = history.prepare(0, &declarations(&[a])).unwrap();
    assert!(empty.previous(a).unwrap().is_none());
    drop(history.reset(&declarations(&[a])).unwrap());
    assert_eq!(history.validate(&empty), Err(HistoryError::StaleBasis));
    assert!(empty.previous(a).unwrap().is_none());
    assert_eq!(history.last_capture, None);

    let before_retirement = history.prepare(0, &declarations(&[a])).unwrap();
    drop(history.retire(&[a]).unwrap());
    drop(history.reset(&declarations(&[a])).unwrap());
    assert_eq!(
        history.validate(&before_retirement),
        Err(HistoryError::StaleBasis)
    );
    assert_eq!(history.last_capture, None);
}

#[test]
fn four_chains_follow_keys_across_view_declaration_and_output_reorder() {
    let [a, b, c, d] = keys();
    let mut history = History::new(4);
    publish(&mut history, 1, &[a, b, c, d], &[10, 20, 30, 40]);
    let mut basis = history.prepare(2, &declarations(&[d, b, a, c])).unwrap();
    assert_eq!(basis.previous(a).unwrap().unwrap().0, 10);
    assert_eq!(basis.previous(b).unwrap().unwrap().0, 20);
    assert_eq!(basis.previous(c).unwrap().unwrap().0, 30);
    assert_eq!(basis.previous(d).unwrap().unwrap().0, 40);

    let outputs = [
        (c, Arc::new(Value(33))),
        (a, Arc::new(Value(11))),
        (d, Arc::new(Value(44))),
        (b, Arc::new(Value(22))),
    ];
    drop(history.commit(&mut basis, &outputs).unwrap());
    for (key, expected) in [(a, 11), (b, 22), (c, 33), (d, 44)] {
        assert_eq!(current(&history, key).unwrap().0, expected);
    }
    assert_eq!(history.last_capture, Some(2));
}

#[test]
fn replacement_stages_new_empty_member_and_retains_unrequested_old_member() {
    let [a, b, c, d] = keys();
    let replacement = HistoryKey { feature: 99, ..a };
    let mut history = History::new(5);
    publish(&mut history, 8, &[a, b, c, d], &[1, 2, 3, 4]);
    let retained_a = Arc::clone(current(&history, a).unwrap());
    let before = snapshot(&history);
    let mut basis = history
        .prepare(9, &declarations(&[d, replacement, c, b]))
        .unwrap();
    assert!(basis.previous(replacement).unwrap().is_none());
    for (key, value) in [(b, 2), (c, 3), (d, 4)] {
        assert_eq!(basis.previous(key).unwrap().unwrap().0, value);
    }
    assert_eq!(snapshot(&history), before);
    assert!(matches!(
        history.commit(
            &mut basis,
            &[
                (b, Arc::new(Value(20))),
                (c, Arc::new(Value(30))),
                (d, Arc::new(Value(40))),
            ]
        ),
        Err(HistoryError::OutputMismatch)
    ));
    assert_eq!(snapshot(&history), before);
    assert!(basis.previous(replacement).unwrap().is_none());
    let outputs = [
        (b, Arc::new(Value(20))),
        (replacement, Arc::new(Value(90))),
        (d, Arc::new(Value(40))),
        (c, Arc::new(Value(30))),
    ];
    drop(history.commit(&mut basis, &outputs).unwrap());
    assert_eq!(history.rows.len(), 5);
    assert!(Arc::ptr_eq(current(&history, a).unwrap(), &retained_a));
    for (key, value) in [(replacement, 90), (b, 20), (c, 30), (d, 40)] {
        assert_eq!(current(&history, key).unwrap().0, value);
    }

    // Reclamation is explicit and authorized by original declaration leases in the parent.
    drop(history.retire(&[a]).unwrap());
    assert_eq!(history.rows.len(), 4);
    assert_eq!(history.last_capture, Some(9));
    assert_eq!(current(&history, replacement).unwrap().0, 90);
}

#[test]
fn partial_declarations_preserve_other_slots_and_their_exact_leases() {
    let [a, b, c, d] = keys();
    let mut history = History::new(4);
    publish(&mut history, 1, &[a, b, c, d], &[1, 2, 3, 4]);
    let retained: Vec<_> = [b, c, d]
        .into_iter()
        .map(|key| (key, Arc::clone(current(&history, key).unwrap())))
        .collect();
    publish(&mut history, 2, &[a], &[10]);
    assert_eq!(history.rows.len(), 4);
    for (key, lease) in retained {
        assert!(Arc::ptr_eq(current(&history, key).unwrap(), &lease));
    }
}

#[test]
fn reset_and_complete_reclamation_never_reopen_successful_captures() {
    let all = keys();
    let mut history = History::new(4);
    publish(&mut history, 42, &all, &[1, 2, 3, 4]);
    drop(history.reset(&declarations(&all)).unwrap());
    assert!(history.rows.iter().all(|row| row.current.is_none()));
    for capture in [0, 41, 42] {
        assert!(matches!(
            history.prepare(capture, &declarations(&all)),
            Err(HistoryError::RepeatedCapture)
        ));
    }
    drop(history.retire(&all).unwrap());
    assert!(history.rows.is_empty());
    assert_eq!(history.last_capture, Some(42));
    assert!(matches!(
        history.prepare(42, &[]),
        Err(HistoryError::RepeatedCapture)
    ));
    assert!(matches!(
        history.prepare(42, &declarations(&all)),
        Err(HistoryError::RepeatedCapture)
    ));
    publish(&mut history, 43, &all, &[10, 20, 30, 40]);
    assert_eq!(history.last_capture, Some(43));
}

#[test]
fn active_capacity_checks_full_retained_union_before_native_work() {
    let all = keys();
    let mut exact = History::<Value>::new(4);
    let basis = exact.prepare(1, &declarations(&all)).unwrap();
    assert_eq!(basis.next_rows.len(), 4);
    assert!(basis.next_rows.capacity() >= 4);
    assert!(exact.rows.is_empty());
    drop(exact.reset(&declarations(&all)).unwrap());
    assert_eq!(exact.rows.len(), 4);

    let mut short = History::new(3);
    assert!(matches!(
        short.prepare(1, &declarations(&all)),
        Err(HistoryError::Capacity)
    ));
    publish(&mut short, 1, &all[..3], &[1, 2, 3]);
    let before = snapshot(&short);
    assert!(matches!(
        short.prepare(2, &declarations(&all[3..])),
        Err(HistoryError::Capacity)
    ));
    assert!(matches!(
        short.reset(&declarations(&[all[0], all[3]])),
        Err(HistoryError::Capacity)
    ));
    assert_eq!(snapshot(&short), before);
    drop(short.retire(&[all[0]]).unwrap());
    assert!(short.prepare(2, &declarations(&all[3..])).is_ok());
}

#[test]
fn production_limit_accepts_512_slots_and_refuses_one_more_without_retirement() {
    let [first, _, _, _] = keys();
    let all: Vec<_> = (0..512).map(|slot| HistoryKey { slot, ..first }).collect();
    let values: Vec<_> = (0..512).collect();
    let mut history = History::new(512);
    publish(&mut history, 1, &all, &values);
    assert_eq!(history.rows.len(), 512);
    assert!(
        history.modeled_metadata_bytes().unwrap()
            <= History::<Value>::required_metadata_bytes(512).unwrap()
    );
    let before = snapshot(&history);
    let extra = HistoryKey { slot: 512, ..first };
    assert!(matches!(
        history.prepare(2, &declarations(&[extra])),
        Err(HistoryError::Capacity)
    ));
    assert!(matches!(
        history.reset(&declarations(&[extra])),
        Err(HistoryError::Capacity)
    ));
    assert_eq!(snapshot(&history), before);

    let short = History::<Value>::new(511);
    assert!(matches!(
        short.prepare(1, &declarations(&all)),
        Err(HistoryError::Capacity)
    ));
    let mut too_many = all;
    too_many.push(extra);
    assert!(matches!(
        history.prepare(2, &declarations(&too_many)),
        Err(HistoryError::Capacity)
    ));
    assert_eq!(snapshot(&history), before);
}

#[test]
fn metadata_accounts_for_full_staged_rows_even_with_one_declared_slot() {
    let [a, b, c, d] = keys();
    let mut history = History::new(4);
    let empty_bytes = history.modeled_metadata_bytes().unwrap();
    assert_eq!(
        History::<Value>::required_metadata_bytes(0).unwrap(),
        empty_bytes
    );
    let single = history.prepare(1, &declarations(&[a])).unwrap();
    let single_bytes = single.metadata_bytes().unwrap();
    publish(&mut history, 1, &[a, b, c, d], &[1, 2, 3, 4]);
    let live_bytes = history.modeled_metadata_bytes().unwrap();
    let basis = history.prepare(2, &declarations(&[a])).unwrap();
    assert!(
        basis.metadata_bytes().unwrap() >= single_bytes + 3 * mem::size_of::<HistoryRow<Value>>()
    );
    assert!(live_bytes >= empty_bytes + 4 * mem::size_of::<HistoryRow<Value>>());
    assert_eq!(history.modeled_metadata_bytes().unwrap(), live_bytes);
    assert_eq!(basis.declared.len(), 1);
    assert_eq!(basis.next_rows.len(), 4);
}

#[test]
fn zero_slot_group_still_advances_watermark_once_and_fences_empty_siblings() {
    let [a, _, _, _] = keys();
    let mut history = History::<Value>::new(0);
    let mut first = history.prepare(0, &[]).unwrap();
    let sibling = history.prepare(0, &[]).unwrap();
    assert!(matches!(
        history.prepare(0, &declarations(&[a])),
        Err(HistoryError::Capacity)
    ));
    drop(history.commit(&mut first, &[]).unwrap());
    assert_eq!(history.last_capture, Some(0));
    assert_eq!(history.revision, 1);
    assert_eq!(history.validate(&sibling), Err(HistoryError::StaleBasis));
    drop(history.reset(&[]).unwrap());
    drop(history.retire(&[]).unwrap());
    assert_eq!(history.last_capture, Some(0));
    assert!(matches!(
        history.prepare(0, &[]),
        Err(HistoryError::RepeatedCapture)
    ));
}

#[test]
fn rejected_output_sets_leave_every_published_and_staged_fact_unchanged() {
    let [a, b, c, d] = keys();
    let unknown = HistoryKey { feature: 900, ..a };
    let mut history = History::new(4);
    publish(&mut history, 1, &[a, b, c, d], &[1, 2, 3, 4]);
    let mut basis = history.prepare(2, &declarations(&[a, b, c, d])).unwrap();
    let before = snapshot(&history);
    let staged = rows_snapshot(&basis.next_rows);
    let invalid = [
        vec![],
        vec![(a, Arc::new(Value(10)))],
        vec![
            (a, Arc::new(Value(10))),
            (b, Arc::new(Value(20))),
            (c, Arc::new(Value(30))),
            (c, Arc::new(Value(31))),
        ],
        vec![
            (a, Arc::new(Value(10))),
            (b, Arc::new(Value(20))),
            (c, Arc::new(Value(30))),
            (unknown, Arc::new(Value(90))),
        ],
        vec![
            (a, Arc::new(Value(10))),
            (b, Arc::new(Value(20))),
            (c, Arc::new(Value(30))),
            (d, Arc::new(Value(40))),
            (unknown, Arc::new(Value(90))),
        ],
    ];
    for outputs in invalid {
        assert!(matches!(
            history.commit(&mut basis, &outputs),
            Err(HistoryError::OutputMismatch)
        ));
        assert_eq!(snapshot(&history), before);
        assert_eq!(rows_snapshot(&basis.next_rows), staged);
        assert_eq!(history.validate(&basis), Ok(()));
    }
    drop(
        history
            .commit(
                &mut basis,
                &[
                    (d, Arc::new(Value(40))),
                    (b, Arc::new(Value(20))),
                    (a, Arc::new(Value(10))),
                    (c, Arc::new(Value(30))),
                ],
            )
            .unwrap(),
    );
    assert_eq!(history.last_capture, Some(2));
}

#[test]
fn matching_revision_does_not_authorize_a_foreign_basis() {
    let [a, _, _, _] = keys();
    let first = History::<Value>::new(1);
    let mut second = History::<Value>::new(1);
    let mut foreign = first.prepare(1, &declarations(&[a])).unwrap();
    assert_eq!(first.revision, second.revision);
    assert_eq!(second.validate(&foreign), Err(HistoryError::ForeignBasis));
    assert!(matches!(
        second.commit(&mut foreign, &[(a, Arc::new(Value(1)))]),
        Err(HistoryError::ForeignBasis)
    ));
    assert!(second.rows.is_empty());
    assert_eq!(second.last_capture, None);
    assert_eq!(first.validate(&foreign), Ok(()));
}

#[test]
fn duplicate_declarations_and_reclamation_keys_refuse_without_mutation() {
    let [a, b, _, _] = keys();
    let mut history = History::new(2);
    publish(&mut history, 1, &[a], &[1]);
    let before = snapshot(&history);
    let duplicate = [(b, descriptor(16)), (b, descriptor(32))];
    assert!(matches!(
        history.prepare(2, &duplicate),
        Err(HistoryError::DuplicateSlot)
    ));
    assert!(matches!(
        history.reset(&duplicate),
        Err(HistoryError::DuplicateSlot)
    ));
    assert!(matches!(
        history.retire(&[a, a]),
        Err(HistoryError::DuplicateSlot)
    ));
    assert_eq!(snapshot(&history), before);
}

#[test]
fn only_explicit_reset_selects_a_new_descriptor_even_for_empty_slots() {
    let [a, b, _, _] = keys();
    let mut history = History::new(2);
    publish(&mut history, 1, &[a, b], &[1, 2]);
    let before = snapshot(&history);
    let different_size = [(a, descriptor(32))];
    assert!(matches!(
        history.prepare(2, &different_size),
        Err(HistoryError::DescriptorMismatch)
    ));
    let mut semantic = descriptor(16);
    let ResourceDescriptor::Buffer { meaning, .. } = &mut semantic else {
        unreachable!()
    };
    *meaning = BufferMeaning::of::<OtherMeaning>();
    assert!(matches!(
        history.prepare(2, &[(a, semantic)]),
        Err(HistoryError::DescriptorMismatch)
    ));
    assert_eq!(snapshot(&history), before);

    let old_b = Arc::clone(current(&history, b).unwrap());
    let old_basis = history.prepare(2, &declarations(&[a, b])).unwrap();
    let retired = history.reset(&different_size).unwrap();
    assert!(
        retired
            ._rows
            .iter()
            .any(|row| row.key == a && row.current.as_ref().is_some_and(|value| value.0 == 1))
    );
    assert_eq!(history.last_capture, Some(1));
    assert!(current(&history, a).is_none());
    assert!(Arc::ptr_eq(current(&history, b).unwrap(), &old_b));
    assert_eq!(history.validate(&old_basis), Err(HistoryError::StaleBasis));
    assert_eq!(old_basis.previous(a).unwrap().unwrap().0, 1);
    assert!(
        history
            .prepare(2, &different_size)
            .unwrap()
            .previous(a)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        history.prepare(2, &declarations(&[a])),
        Err(HistoryError::DescriptorMismatch)
    ));
}

#[test]
fn reset_cause_is_pinned_per_slot_and_never_inherited_by_an_unrelated_new_slot() {
    let [a, b, c, unknown] = keys();
    let mut history = History::new(3);
    assert!(history.previous(a).is_none());
    assert!(history.descriptor(a).is_none());
    publish(&mut history, 4, &[c], &[40]);
    drop(history.reset(&declarations(&[a])).unwrap());
    assert!(history.previous(a).is_none());
    assert_eq!(history.descriptor(a), Some(&descriptor(16)));
    assert_eq!(history.previous(c).unwrap().0, 40);

    let basis = history.prepare(5, &declarations(&[a, b, c])).unwrap();
    assert!(basis.previous(a).unwrap().is_none());
    assert_eq!(basis.was_reset(a), Ok(true));
    assert!(basis.previous(b).unwrap().is_none());
    assert_eq!(basis.was_reset(b), Ok(false));
    assert_eq!(basis.was_reset(c), Ok(false));
    assert!(history.descriptor(b).is_none());
    assert!(history.previous(unknown).is_none());
    assert!(history.descriptor(unknown).is_none());
    assert!(matches!(
        basis.previous(unknown),
        Err(HistoryError::OutputMismatch)
    ));
    assert_eq!(basis.was_reset(unknown), Err(HistoryError::OutputMismatch));

    drop(history.reset(&declarations(&[b])).unwrap());
    assert_eq!(history.validate(&basis), Err(HistoryError::StaleBasis));
    assert_eq!(basis.was_reset(b), Ok(false));
    let mut fresh = history.prepare(5, &declarations(&[a, b, c])).unwrap();
    assert_eq!(fresh.was_reset(a), Ok(true));
    assert_eq!(fresh.was_reset(b), Ok(true));
    assert_eq!(fresh.was_reset(c), Ok(false));
    drop(
        history
            .commit(
                &mut fresh,
                &[
                    (c, Arc::new(Value(41))),
                    (a, Arc::new(Value(50))),
                    (b, Arc::new(Value(60))),
                ],
            )
            .unwrap(),
    );
    assert!(matches!(fresh.previous(a), Err(HistoryError::StaleBasis)));
    assert_eq!(fresh.was_reset(a), Err(HistoryError::StaleBasis));
    let populated = history.prepare(6, &declarations(&[a, b, c])).unwrap();
    assert_eq!(populated.previous(a).unwrap().unwrap().0, 50);
    assert_eq!(populated.was_reset(a), Ok(true));
    assert_eq!(populated.was_reset(b), Ok(true));
    assert_eq!(populated.was_reset(c), Ok(false));

    drop(history.retire(&[a]).unwrap());
    let new = history.prepare(6, &declarations(&[a])).unwrap();
    assert!(new.previous(a).unwrap().is_none());
    assert_eq!(new.was_reset(a), Ok(false));
    assert_eq!(history.last_capture, Some(5));
}

#[test]
fn revision_exhaustion_refuses_prepare_reset_and_retire_without_wrapping() {
    let [a, _, _, _] = keys();
    let mut history = History::new(1);
    history.revision = u64::MAX - 1;
    publish(&mut history, u64::MAX, &[a], &[7]);
    assert_eq!(history.revision, u64::MAX);
    let before = snapshot(&history);
    assert!(matches!(
        history.prepare(u64::MAX, &declarations(&[a])),
        Err(HistoryError::RepeatedCapture)
    ));
    assert!(matches!(
        history.reset(&declarations(&[a])),
        Err(HistoryError::Overflow)
    ));
    assert!(matches!(history.retire(&[a]), Err(HistoryError::Overflow)));
    assert_eq!(snapshot(&history), before);

    let mut unpublished = History::<Value>::new(1);
    unpublished.revision = u64::MAX;
    assert!(matches!(
        unpublished.prepare(0, &declarations(&[a])),
        Err(HistoryError::Overflow)
    ));
    assert!(unpublished.rows.is_empty());
    assert_eq!(unpublished.last_capture, None);
    assert!(matches!(
        reserved::<HistoryRow<Value>>(usize::MAX),
        Err(HistoryError::Overflow)
    ));
    assert_eq!(
        History::<Value>::required_metadata_bytes(usize::MAX),
        Err(HistoryError::Overflow)
    );
}

#[test]
fn successive_replacements_and_retirement_reuse_capacity_without_reopening_captures() {
    let [a, b, _, _] = keys();
    let mut history = History::new(1);
    let basis = history.prepare(0, &declarations(&[a])).unwrap();
    let before = snapshot(&history);
    assert!(matches!(
        history.retire(&[b]),
        Err(HistoryError::OutputMismatch)
    ));
    assert_eq!(snapshot(&history), before);
    assert_eq!(history.validate(&basis), Ok(()));
    for capture in 0..32 {
        let key = HistoryKey {
            feature: capture,
            ..a
        };
        publish(&mut history, capture, &[key], &[capture]);
        drop(history.retire(&[key]).unwrap());
        assert!(history.rows.is_empty());
        assert_eq!(history.last_capture, Some(capture));
        assert!(matches!(
            history.prepare(capture, &declarations(&[key])),
            Err(HistoryError::RepeatedCapture)
        ));
    }
    assert_eq!(history.validate(&basis), Err(HistoryError::StaleBasis));
}

struct DropValue {
    value: u64,
    panic_on_drop: bool,
    drops: Arc<AtomicUsize>,
}

impl Drop for DropValue {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
        assert!(!self.panic_on_drop, "hostile retired history value");
    }
}

#[test]
fn panicking_old_value_is_destroyed_only_after_every_new_row_and_watermark_publish() {
    let [a, b, _, _] = keys();
    let drops = Arc::new(AtomicUsize::new(0));
    let mut history = History::new(2);
    let mut initial = history.prepare(1, &declarations(&[a, b])).unwrap();
    drop(
        history
            .commit(
                &mut initial,
                &[
                    (
                        a,
                        Arc::new(DropValue {
                            value: 1,
                            panic_on_drop: true,
                            drops: Arc::clone(&drops),
                        }),
                    ),
                    (
                        b,
                        Arc::new(DropValue {
                            value: 2,
                            panic_on_drop: false,
                            drops: Arc::clone(&drops),
                        }),
                    ),
                ],
            )
            .unwrap(),
    );
    let mut basis = history.prepare(2, &declarations(&[b, a])).unwrap();
    let retired = history
        .commit(
            &mut basis,
            &[
                (
                    b,
                    Arc::new(DropValue {
                        value: 20,
                        panic_on_drop: false,
                        drops: Arc::clone(&drops),
                    }),
                ),
                (
                    a,
                    Arc::new(DropValue {
                        value: 10,
                        panic_on_drop: false,
                        drops: Arc::clone(&drops),
                    }),
                ),
            ],
        )
        .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert!(catch_unwind(AssertUnwindSafe(|| drop(retired))).is_err());
    assert_eq!(history.revision, 2);
    assert_eq!(history.last_capture, Some(2));
    for (key, value) in [(a, 10), (b, 20)] {
        assert_eq!(
            history
                .rows
                .iter()
                .find(|row| row.key == key)
                .unwrap()
                .current
                .as_ref()
                .unwrap()
                .value,
            value
        );
    }
    assert_eq!(history.validate(&basis), Err(HistoryError::StaleBasis));
}

#[test]
fn reset_and_retire_return_hostile_values_for_cleanup_after_complete_mutation() {
    let [a, b, _, _] = keys();
    for retire in [false, true] {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut history = History::new(2);
        let mut initial = history.prepare(7, &declarations(&[a, b])).unwrap();
        drop(
            history
                .commit(
                    &mut initial,
                    &[
                        (
                            a,
                            Arc::new(DropValue {
                                value: 1,
                                panic_on_drop: true,
                                drops: Arc::clone(&drops),
                            }),
                        ),
                        (
                            b,
                            Arc::new(DropValue {
                                value: 2,
                                panic_on_drop: false,
                                drops: Arc::clone(&drops),
                            }),
                        ),
                    ],
                )
                .unwrap(),
        );
        let retired = if retire {
            history.retire(&[a]).unwrap()
        } else {
            history.reset(&[(a, descriptor(32))]).unwrap()
        };
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert!(catch_unwind(AssertUnwindSafe(|| drop(retired))).is_err());
        assert_eq!(history.revision, 2);
        assert_eq!(history.last_capture, Some(7));
        assert!(
            history
                .rows
                .iter()
                .find(|row| row.key == a)
                .is_none_or(|row| row.current.is_none())
        );
        assert_eq!(
            history
                .rows
                .iter()
                .find(|row| row.key == b)
                .unwrap()
                .current
                .as_ref()
                .unwrap()
                .value,
            2
        );
        assert_eq!(history.rows.len(), if retire { 1 } else { 2 });
    }
}
