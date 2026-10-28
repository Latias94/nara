use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use nara_tasks::TaskPoolConfig;

fn pools() -> TaskPools {
    let worker = nara_core::ItemLimit::new(1).unwrap();
    TaskPools::try_new(TaskPoolConfig::threaded(worker, worker, worker).unwrap()).unwrap()
}

use super::*;

#[test]
fn cancelled_finite_work_retains_its_actual_result_until_physical_return() {
    let pools = pools();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut pending = Pending::start(&pools, move || {
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(15)).unwrap();
        73_u64
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    pending.cancel();
    let started = Instant::now();
    assert!(pending.poll().is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        pending.task.as_ref().unwrap().return_observer().state(),
        TaskReturnState::Pending
    );
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        if let Some(result) = pending.poll() {
            break result;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert_eq!(result, Ok(73));
    assert_eq!(
        pending.task.as_ref().unwrap().return_observer().state(),
        TaskReturnState::Returned
    );
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn panicked_started_work_requires_reconciliation_instead_of_inventing_an_unpublished_result() {
    let pools = pools();
    let mut pending = Pending::<()>::start(&pools, || panic!("injected finite publication unwind"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        if let Some(result) = pending.poll() {
            break result;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert_eq!(
        result,
        Err(EditorSchemaOwnerAdoptionError::RequiresReconcile)
    );
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(report.has_cleanup_failure());
}
