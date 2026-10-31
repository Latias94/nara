//! CPU-only history-owner tests. Include from composition.rs, not from a native GPU test binary.

use super::history::*;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn key(view: u64, instance: u64) -> SlotKey {
    SlotKey {
        producer: 1,
        view,
        instance,
        incarnation: 1,
        slot: 0,
    }
}

fn registered<T>(keys: &[SlotKey]) -> History<T> {
    let mut history = History::new();
    for &key in keys {
        history.register(key).unwrap();
    }
    history.register_group(1, keys).unwrap();
    history
}

#[test]
fn registered_groups_are_complete_immutable_nonoverlapping_and_single_producer() {
    let a = key(1, 1);
    let b = key(1, 2);
    let other = SlotKey {
        producer: 2,
        ..key(2, 1)
    };
    let mut history = registered::<u32>(&[a, b]);
    history.register(other).unwrap();
    assert_eq!(
        history.register_group(1, &[b, a]),
        Err(HistoryError::DuplicateGroup)
    );
    assert_eq!(
        history.register_group(2, &[a]),
        Err(HistoryError::GroupOverlap)
    );
    assert_eq!(
        history.register_group(2, &[other, b]),
        Err(HistoryError::MixedProducer)
    );
    // Rejected registration reserves neither the ID nor the previously checked free slot.
    history.register_group(2, &[other]).unwrap();

    let ticket = history.prepare(1, 1).unwrap();
    let observer = history.prepare(1, 1).unwrap();
    assert_eq!(
        history.publish(ticket, &[Arc::new(10)]).err(),
        Some(HistoryError::OutputCount)
    );
    assert_eq!(history.validate_ticket(&observer), Ok(()));
    assert!(history.current(a).unwrap().is_none());
    assert!(history.current(b).unwrap().is_none());

    let outputs = [Arc::new(10), Arc::new(20)];
    drop(history.publish(observer, &outputs).unwrap());
    let next = history.prepare(2, 1).unwrap();
    assert!(Arc::ptr_eq(next.previous(0).unwrap().unwrap(), &outputs[0]));
    assert!(Arc::ptr_eq(next.previous(1).unwrap().unwrap(), &outputs[1]));
    // Another producer has an independent capture clock.
    assert!(history.prepare(1, 2).is_ok());
}

#[test]
fn invalid_previous_position_is_not_empty_history() {
    let a = key(1, 1);
    let history = registered::<u32>(&[a]);
    let ticket = history.prepare(1, 1).unwrap();
    assert!(ticket.previous(0).unwrap().is_none());
    assert_eq!(
        ticket.previous(1).err(),
        Some(HistoryError::InvalidPosition)
    );
    assert_eq!(
        ticket.previous(usize::MAX).err(),
        Some(HistoryError::InvalidPosition)
    );
}

#[test]
fn empty_reset_rejects_old_basis_but_allows_explicit_unpublished_capture_retry() {
    let a = key(1, 1);
    let mut history = registered::<u32>(&[a]);
    let old = history.prepare(1, 1).unwrap();
    let retired = history.reset(&[a]).unwrap();
    assert!(retired.is_empty());
    drop(retired);
    assert_eq!(history.validate_ticket(&old), Err(HistoryError::StaleBasis));
    assert_eq!(
        history.publish(old, &[Arc::new(1)]).err(),
        Some(HistoryError::StaleBasis)
    );
    assert!(history.current(a).unwrap().is_none());

    let retry = history.prepare(1, 1).unwrap();
    let output = Arc::new(2);
    drop(history.publish(retry, &[Arc::clone(&output)]).unwrap());
    assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &output));
    drop(history.reset(&[a]).unwrap());
    assert_eq!(
        history.prepare(1, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
    assert!(history.prepare(2, 1).is_ok());
}

#[test]
fn two_prepared_tickets_for_the_same_capture_commit_only_once() {
    let a = key(1, 1);
    let mut history = registered::<u32>(&[a]);
    let first = history.prepare(42, 1).unwrap();
    let duplicate = history.prepare(42, 1).unwrap();
    let output = Arc::new(42);
    drop(history.publish(first, &[Arc::clone(&output)]).unwrap());
    let observer = history.prepare(43, 1).unwrap();

    assert_eq!(
        history.publish(duplicate, &[Arc::new(99)]).err(),
        Some(HistoryError::StaleBasis)
    );
    // A valid observer also proves the failed publication changed no revision/reset stamp.
    assert_eq!(history.validate_ticket(&observer), Ok(()));
    assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &output));
    assert_eq!(
        history.prepare(42, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
    drop(history.reset(&[a]).unwrap());
    assert_eq!(
        history.prepare(42, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
}

#[test]
fn either_publication_order_invalidates_sibling_tickets_without_repinning() {
    let a = key(1, 1);
    for (winner_capture, late_capture) in [(41, 42), (42, 41)] {
        let mut history = registered::<u64>(&[a]);
        let initial = Arc::new(40);
        let first = history.prepare(40, 1).unwrap();
        drop(history.publish(first, &[Arc::clone(&initial)]).unwrap());
        let winner = history.prepare(winner_capture, 1).unwrap();
        let late = history.prepare(late_capture, 1).unwrap();
        let output = Arc::new(winner_capture);
        drop(history.publish(winner, &[Arc::clone(&output)]).unwrap());

        assert!(Arc::ptr_eq(late.previous(0).unwrap().unwrap(), &initial));
        assert_eq!(
            history.validate_ticket(&late),
            Err(HistoryError::StaleBasis)
        );
        assert_eq!(
            history.publish(late, &[Arc::new(late_capture)]).err(),
            Some(HistoryError::StaleBasis)
        );
        assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &output));
        if late_capture > winner_capture {
            let rebuilt = history.prepare(late_capture, 1).unwrap();
            assert!(Arc::ptr_eq(rebuilt.previous(0).unwrap().unwrap(), &output));
            drop(history.publish(rebuilt, &[Arc::new(late_capture)]).unwrap());
        } else {
            assert_eq!(
                history.prepare(late_capture, 1).err(),
                Some(HistoryError::RepeatedCapture)
            );
        }
    }
}

#[test]
fn one_slot_reset_rejects_the_whole_old_group_and_retry_pins_remaining_originals() {
    let a = key(1, 1);
    let b = key(2, 1);
    let mut history = registered::<u32>(&[a, b]);
    let outputs = [Arc::new(10), Arc::new(20)];
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &outputs).unwrap());
    let stale = history.prepare(2, 1).unwrap();
    let retired = history.reset(&[b]).unwrap();
    assert_eq!(retired.len(), 1);
    assert!(Arc::ptr_eq(
        stale.previous(1).unwrap().unwrap(),
        &outputs[1]
    ));
    drop(retired);

    let retry = history.prepare(2, 1).unwrap();
    assert!(Arc::ptr_eq(
        retry.previous(0).unwrap().unwrap(),
        &outputs[0]
    ));
    assert!(retry.previous(1).unwrap().is_none());
    assert_eq!(
        history.publish(stale, &[Arc::new(11), Arc::new(21)]).err(),
        Some(HistoryError::StaleBasis)
    );
    assert_eq!(history.validate_ticket(&retry), Ok(()));
    assert!(Arc::ptr_eq(
        history.current(a).unwrap().unwrap(),
        &outputs[0]
    ));
    assert!(history.current(b).unwrap().is_none());
    // Reset did not permit a second partial publication of already committed capture 1.
    assert_eq!(
        history.prepare(1, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
    let next = [Arc::new(11), Arc::new(21)];
    drop(history.publish(retry, &next).unwrap());
    assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &next[0]));
    assert!(Arc::ptr_eq(history.current(b).unwrap().unwrap(), &next[1]));
}

#[test]
fn discarded_attempts_leave_original_ticket_inputs_available_for_retry() {
    let a = key(1, 1);
    let mut history = registered::<u32>(&[a]);
    let initial = Arc::new(10);
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &[Arc::clone(&initial)]).unwrap());

    let retry = history.prepare(2, 1).unwrap();
    // A native caller keeps this ticket when a retryable ready/submit attempt fails.
    // This CPU test covers history custody only; it does not simulate GPU submission success.
    assert_eq!(history.validate_ticket(&retry), Ok(()));
    drop(history.prepare(2, 1).unwrap());
    assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &initial));
    assert!(Arc::ptr_eq(retry.previous(0).unwrap().unwrap(), &initial));
    assert_eq!(history.validate_ticket(&retry), Ok(()));
    let next = Arc::new(20);
    drop(history.publish(retry, &[Arc::clone(&next)]).unwrap());
    assert!(Arc::ptr_eq(history.current(a).unwrap().unwrap(), &next));
}

#[test]
fn matching_group_ids_in_foreign_owners_do_not_authorize_tickets() {
    let a = key(1, 1);
    let first = registered::<u32>(&[a]);
    let mut other = registered::<u32>(&[a]);
    let foreign = first.prepare(1, 1).unwrap();
    let observer = other.prepare(1, 1).unwrap();
    assert_eq!(
        other.validate_ticket(&foreign),
        Err(HistoryError::ForeignOwner)
    );
    assert_eq!(
        other.publish(foreign, &[Arc::new(1)]).err(),
        Some(HistoryError::ForeignOwner)
    );
    assert_eq!(other.validate_ticket(&observer), Ok(()));
    assert!(first.current(a).unwrap().is_none());
    assert!(other.current(a).unwrap().is_none());
}

#[test]
fn retirement_requires_new_slot_and_group_incarnations_and_preserves_survivors() {
    let keys = [key(1, 1), key(1, 2), key(2, 1), key(2, 2)];
    let mut history = registered::<u32>(&keys);
    let outputs = [Arc::new(11), Arc::new(12), Arc::new(21), Arc::new(22)];
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &outputs).unwrap());
    let old = history.prepare(2, 1).unwrap();
    drop(history.retire(keys[1]).unwrap());
    assert_eq!(history.prepare(2, 1).err(), Some(HistoryError::Retired));
    assert_eq!(history.register(keys[1]), Err(HistoryError::DuplicateSlot));

    let replacement = SlotKey {
        incarnation: 2,
        ..keys[1]
    };
    history.register(replacement).unwrap();
    let order = [keys[3], keys[0], keys[2], replacement];
    assert_eq!(
        history.register_group(1, &order),
        Err(HistoryError::DuplicateGroup)
    );
    history.register_group(2, &order).unwrap();
    let current = history.prepare(2, 2).unwrap();
    assert!(Arc::ptr_eq(
        current.previous(0).unwrap().unwrap(),
        &outputs[3]
    ));
    assert!(Arc::ptr_eq(
        current.previous(1).unwrap().unwrap(),
        &outputs[0]
    ));
    assert!(Arc::ptr_eq(
        current.previous(2).unwrap().unwrap(),
        &outputs[2]
    ));
    assert!(current.previous(3).unwrap().is_none());
    assert_eq!(
        history.publish(old, &outputs).err(),
        Some(HistoryError::Retired)
    );
    assert_eq!(history.validate_ticket(&current), Ok(()));
    assert!(history.current(replacement).unwrap().is_none());
    let next = [Arc::new(32), Arc::new(31), Arc::new(30), Arc::new(29)];
    drop(history.publish(current, &next).unwrap());
    for (slot, output) in order.into_iter().zip(&next) {
        assert!(Arc::ptr_eq(history.current(slot).unwrap().unwrap(), output));
    }
}

#[test]
fn explicit_group_retirement_fences_tickets_without_resetting_surviving_capture_history() {
    let a = key(1, 1);
    let b = key(2, 1);
    let mut history = registered::<u32>(&[a, b]);
    let outputs = [Arc::new(10), Arc::new(20)];
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &outputs).unwrap());
    let old = history.prepare(2, 1).unwrap();
    history.retire_group(1).unwrap();
    assert_eq!(history.retire_group(1), Err(HistoryError::Retired));
    assert_eq!(
        history.register_group(1, &[b, a]),
        Err(HistoryError::DuplicateGroup)
    );
    history.register_group(2, &[b, a]).unwrap();
    assert_eq!(
        history.prepare(1, 2).err(),
        Some(HistoryError::RepeatedCapture)
    );
    let next = history.prepare(2, 2).unwrap();
    assert!(Arc::ptr_eq(next.previous(0).unwrap().unwrap(), &outputs[1]));
    assert!(Arc::ptr_eq(next.previous(1).unwrap().unwrap(), &outputs[0]));
    assert_eq!(
        history.publish(old, &outputs).err(),
        Some(HistoryError::Retired)
    );
    assert_eq!(history.validate_ticket(&next), Ok(()));
}

struct DropProbe {
    index: usize,
    panic_on_drop: bool,
    drops: Arc<[AtomicUsize; 4]>,
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.drops[self.index].fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.panic_on_drop,
            "injected retired-value destructor failure"
        );
    }
}

fn probe(index: usize, panic_on_drop: bool, drops: &Arc<[AtomicUsize; 4]>) -> Arc<DropProbe> {
    Arc::new(DropProbe {
        index,
        panic_on_drop,
        drops: Arc::clone(drops),
    })
}

#[test]
fn panicking_retired_cleanup_occurs_only_after_the_complete_publication() {
    let keys = [key(1, 1), key(2, 1)];
    let mut history = registered(&keys);
    let drops = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
    let original = [probe(0, true, &drops), probe(1, false, &drops)];
    let weak = original.each_ref().map(Arc::downgrade);
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &original).unwrap());
    drop(original);
    let next = [probe(2, false, &drops), probe(3, false, &drops)];
    let ticket = history.prepare(2, 1).unwrap();

    let retired = history.publish(ticket, &next).unwrap();
    assert_eq!(retired.len(), 2);
    assert_eq!(drops[0].load(Ordering::SeqCst), 0);
    assert_eq!(drops[1].load(Ordering::SeqCst), 0);
    assert!(weak.iter().all(|value| value.upgrade().is_some()));
    for (key, output) in keys.into_iter().zip(&next) {
        assert!(Arc::ptr_eq(history.current(key).unwrap().unwrap(), output));
    }
    assert_eq!(
        history.prepare(2, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
    let observer = history.prepare(3, 1).unwrap();

    assert!(catch_unwind(AssertUnwindSafe(|| drop(retired))).is_err());
    assert_eq!(drops[0].load(Ordering::SeqCst), 1);
    assert_eq!(drops[1].load(Ordering::SeqCst), 1);
    assert!(weak.iter().all(|value| value.upgrade().is_none()));
    assert_eq!(history.validate_ticket(&observer), Ok(()));
    for (key, output) in keys.into_iter().zip(&next) {
        assert!(Arc::ptr_eq(history.current(key).unwrap().unwrap(), output));
    }
}

#[test]
fn panicking_reset_cleanup_occurs_only_after_every_slot_is_reset() {
    let keys = [key(1, 1), key(2, 1)];
    let mut history = registered(&keys);
    let drops = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
    let original = [probe(0, true, &drops), probe(1, false, &drops)];
    let weak = original.each_ref().map(Arc::downgrade);
    let first = history.prepare(1, 1).unwrap();
    drop(history.publish(first, &original).unwrap());
    drop(original);

    let retired = history.reset(&keys).unwrap();
    assert_eq!(retired.len(), 2);
    assert!(
        keys.iter()
            .all(|key| history.current(*key).unwrap().is_none())
    );
    assert_eq!(drops[0].load(Ordering::SeqCst), 0);
    assert_eq!(drops[1].load(Ordering::SeqCst), 0);
    assert_eq!(
        history.prepare(1, 1).err(),
        Some(HistoryError::RepeatedCapture)
    );
    let observer = history.prepare(2, 1).unwrap();

    assert!(catch_unwind(AssertUnwindSafe(|| drop(retired))).is_err());
    assert_eq!(drops[0].load(Ordering::SeqCst), 1);
    assert_eq!(drops[1].load(Ordering::SeqCst), 1);
    assert!(weak.iter().all(|value| value.upgrade().is_none()));
    assert!(
        keys.iter()
            .all(|key| history.current(*key).unwrap().is_none())
    );
    assert_eq!(history.validate_ticket(&observer), Ok(()));
}
