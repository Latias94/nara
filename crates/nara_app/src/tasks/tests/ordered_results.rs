use crate::tasks::{
    OrderedTaskResults, TaskCompletionCutoffError, TaskDomainKey, TaskFacilities, TaskHandle,
    TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools, TaskShutdownPolicy, TaskSpawnRequest,
    TaskTerminal,
};
use nara_core::ItemLimit;

#[derive(Debug)]
struct Value(u32);

#[test]
fn a_ready_later_result_waits_for_the_earlier_result_at_the_next_cutoff() {
    use std::{sync::mpsc, time::Duration};

    let pools = TaskPools::try_new_lazy(config()).unwrap();
    let (release, gate) = mpsc::channel();
    let first = pools
        .spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(1, TaskDomainKey::new(7)),
            move |_| {
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Value(11)
            },
        )
        .into_handle()
        .unwrap();
    let first_returned = first.return_observer();
    let second = spawn(&pools, 2, TaskPoolKind::Compute, 22);
    let second_returned = second.return_observer();
    let mut ordered = OrderedTaskResults::default();
    ordered.push(first).unwrap();
    ordered.push(second).unwrap();
    super::wait_until(|| second_returned.state() == super::TaskReturnState::Returned);
    let original = pools.capture_completion_cutoff();
    release.send(()).unwrap();
    super::wait_until(|| first_returned.state() == super::TaskReturnState::Returned);
    assert!(
        ordered
            .capture_ready_prefix(&original)
            .unwrap()
            .into_terminals()
            .unwrap()
            .is_empty()
    );
    let next = pools.capture_completion_cutoff();
    let ready = ordered
        .capture_ready_prefix(&next)
        .unwrap()
        .into_terminals()
        .unwrap();
    assert!(
        matches!(&ready[..], [first, second] if matches!(first.terminal, TaskTerminal::Completed(Value(11))) && matches!(second.terminal, TaskTerminal::Completed(Value(22))))
    );
}

fn config() -> TaskPoolConfig {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::new(8).unwrap());
    TaskPoolConfig::new(kind, kind, kind, TaskShutdownPolicy::default()).unwrap()
}

fn spawn(pools: &TaskPools, tick: u64, kind: TaskPoolKind, value: u32) -> TaskHandle<Value> {
    pools
        .spawn(
            kind,
            TaskSpawnRequest::new(tick, TaskDomainKey::new(7)),
            move |_| Value(value),
        )
        .into_handle()
        .unwrap()
}

#[test]
fn a_spent_handle_cannot_discard_a_previous_available_result() {
    let pools = TaskPools::inline_for_tests(config()).unwrap();
    let available = spawn(&pools, 10, TaskPoolKind::Io, 11);
    let mut spent = spawn(&pools, 20, TaskPoolKind::Io, 22);
    let spent_descriptor = spent.descriptor();
    assert_eq!(pools.run_pending_for_tests().executed, 2);
    assert!(matches!(
        spent.try_take(),
        Some(TaskTerminal::Completed(Value(22)))
    ));
    assert!(spent.is_finished());

    let mut ordered = OrderedTaskResults::default();
    ordered.push(available).unwrap();
    let mut returned = ordered.push(spent).expect_err("consumed result");
    assert_eq!(returned.descriptor(), spent_descriptor);
    assert!(returned.try_take().is_none());
    assert_eq!(ordered.len(), 1);
    let cutoff = pools.capture_completion_cutoff();
    let ready = ordered
        .capture_ready_prefix(&cutoff)
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(ready.len(), 1);
    assert!(matches!(
        ready[0].terminal,
        TaskTerminal::Completed(Value(11))
    ));
    assert!(
        ordered
            .capture_ready_prefix(&cutoff)
            .unwrap()
            .into_terminals()
            .unwrap()
            .is_empty()
    );
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn sibling_scopes_reject_mixed_results_and_return_the_original_pending_handle() {
    let facilities = TaskFacilities::inline_for_tests(config()).unwrap();
    let mut first = facilities.scope();
    let mut second = facilities.scope();
    let a = first.spawner();
    let another_a = first.spawner();
    let b = second.spawner();
    let own = spawn(&a, 10, TaskPoolKind::Io, 11);
    let foreign = spawn(&b, 20, TaskPoolKind::Io, 22);
    let original = foreign.descriptor();
    let later = spawn(&another_a, 30, TaskPoolKind::Compute, 33);
    let mut ordered = OrderedTaskResults::default();
    ordered.push(own).unwrap();
    let mut returned = ordered.push(foreign).expect_err("sibling completion owner");
    assert_eq!(returned.descriptor(), original);
    assert!(!returned.is_finished());
    assert!(returned.try_take().is_none());
    assert_eq!(ordered.len(), 1);
    ordered.push(later).unwrap();
    let mut sibling = OrderedTaskResults::default();
    sibling.push(returned).unwrap();
    assert_eq!(facilities.run_pending_for_tests().executed, 3);

    let error = sibling
        .capture_ready_prefix(&a.capture_completion_cutoff())
        .unwrap_err();
    assert_eq!(
        error,
        TaskCompletionCutoffError::PoolMismatch { task: original }
    );
    assert_eq!(sibling.len(), 1);
    let ready = ordered
        .capture_ready_prefix(&another_a.capture_completion_cutoff())
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(ready.len(), 2);
    assert!(matches!(
        ready[0].terminal,
        TaskTerminal::Completed(Value(11))
    ));
    assert!(matches!(
        ready[1].terminal,
        TaskTerminal::Completed(Value(33))
    ));
    let ready = sibling
        .capture_ready_prefix(&b.capture_completion_cutoff())
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(ready.len(), 1);
    assert!(matches!(
        ready[0].terminal,
        TaskTerminal::Completed(Value(22))
    ));
    assert!(first.poll_close().is_complete());
    assert!(second.poll_close().is_complete());
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn an_ordered_stream_rebinds_only_after_its_last_pending_handle_leaves() {
    let facilities = TaskFacilities::inline_for_tests(config()).unwrap();
    let mut first = facilities.scope();
    let mut second = facilities.scope();
    let a = first.spawner();
    let b = second.spawner();
    let mut ordered = OrderedTaskResults::default();
    ordered.push(spawn(&a, 10, TaskPoolKind::Io, 1)).unwrap();
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    let cutoff = a.capture_completion_cutoff();
    ordered.push(spawn(&a, 20, TaskPoolKind::Io, 2)).unwrap();
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    let captured = ordered.capture_ready_prefix(&cutoff).unwrap();
    assert_eq!(ordered.len(), 1);
    let foreign = spawn(&b, 30, TaskPoolKind::Io, 3);
    let original = foreign.descriptor();
    let foreign = ordered
        .push(foreign)
        .expect_err("the stream is still bound");
    assert_eq!(foreign.descriptor(), original);
    assert_eq!(ordered.len(), 1);
    let later = ordered
        .capture_ready_prefix(&a.capture_completion_cutoff())
        .unwrap();
    assert!(ordered.is_empty());

    ordered.push(foreign).unwrap();
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    assert!(
        ordered
            .capture_ready_prefix(&a.capture_completion_cutoff())
            .is_err()
    );
    let rebound = ordered
        .capture_ready_prefix(&b.capture_completion_cutoff())
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(rebound.len(), 1);
    assert!(matches!(
        rebound[0].terminal,
        TaskTerminal::Completed(Value(3))
    ));
    let ready = captured.into_terminals().unwrap();
    assert_eq!(ready.len(), 1);
    assert!(matches!(
        ready[0].terminal,
        TaskTerminal::Completed(Value(1))
    ));
    let ready = later.into_terminals().unwrap();
    assert_eq!(ready.len(), 1);
    assert!(matches!(
        ready[0].terminal,
        TaskTerminal::Completed(Value(2))
    ));
    assert!(first.poll_close().is_complete());
    assert!(second.poll_close().is_complete());
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn distinct_facilities_reject_both_equal_and_different_order_keys_without_consuming_results() {
    for tick in [10, 20] {
        let a = TaskPools::inline_for_tests(config()).unwrap();
        let b = TaskPools::inline_for_tests(config()).unwrap();
        let own = spawn(&a, 10, TaskPoolKind::Io, 1);
        let foreign = spawn(&b, tick, TaskPoolKind::Io, 2);
        assert_eq!(own.order_key() == foreign.order_key(), tick == 10);
        let descriptor = foreign.descriptor();
        let mut ordered = OrderedTaskResults::default();
        ordered.push(own).unwrap();
        let mut returned = ordered.push(foreign).expect_err("foreign completion owner");
        assert_eq!(returned.descriptor(), descriptor);
        assert_eq!(a.run_pending_for_tests().executed, 1);
        assert_eq!(b.run_pending_for_tests().executed, 1);
        assert!(matches!(
            returned.try_take(),
            Some(TaskTerminal::Completed(Value(2)))
        ));
        let ready = ordered
            .capture_ready_prefix(&a.capture_completion_cutoff())
            .unwrap()
            .into_terminals()
            .unwrap();
        assert_eq!(ready.len(), 1);
        assert!(matches!(
            ready[0].terminal,
            TaskTerminal::Completed(Value(1))
        ));
        assert!(a.shutdown_blocking().unwrap().is_complete());
        assert!(b.shutdown_blocking().unwrap().is_complete());
    }
}
