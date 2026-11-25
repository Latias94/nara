use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

#[test]
fn pending_notifications_share_one_event_until_acknowledged() {
    fn require_send_clone<T: Send + Clone>() {}
    require_send_clone::<WinitMaintenanceWake>();
    let state = MaintenanceWakeState::default();
    let sends = AtomicUsize::new(0);
    let notify = || {
        state.notify(|| {
            sends.fetch_add(1, Ordering::SeqCst);
            true
        })
    };
    assert!(state.is_live());
    assert_eq!(state.acknowledgements(), 0);
    assert!(!state.acknowledge());
    for _ in 0..4096 {
        assert!(notify());
    }
    assert!(state.is_pending());
    assert_eq!(sends.load(Ordering::SeqCst), 1);
    assert!(state.acknowledge());
    assert!(!state.is_pending());
    assert_eq!(state.acknowledgements(), 1);
    assert!(!state.acknowledge());
    assert!(notify());
    assert_eq!(sends.load(Ordering::SeqCst), 2);
    assert!(state.acknowledge());
}

#[test]
fn refused_delivery_and_retirement_permanently_reject_notifications() {
    for pending in [false, true] {
        let state = MaintenanceWakeState::default();
        if pending {
            assert!(state.notify(|| true));
            state.revoke();
        } else {
            assert!(!state.notify(|| false));
        }
        assert!(!state.is_live());
        assert!(!state.acknowledge());
        assert!(!state.notify(|| panic!("revoked state attempted native delivery")));
        state.revoke();
        assert!(!state.notify(|| true));
    }
}

#[test]
fn concurrent_notifications_bound_the_queue_and_rearm_after_dispatch() {
    let state = Arc::new(MaintenanceWakeState::default());
    let (send, receive) = mpsc::sync_channel(1);
    let sends = Arc::new(AtomicUsize::new(0));
    for round in 1..=3 {
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let state = state.clone();
                let send = send.clone();
                let sends = sends.clone();
                scope.spawn(move || {
                    for _ in 0..2048 {
                        assert!(state.notify(|| {
                            sends.fetch_add(1, Ordering::SeqCst);
                            send.try_send(()).is_ok()
                        }));
                    }
                });
            }
        });
        assert_eq!(sends.load(Ordering::SeqCst), round);
        assert_eq!(receive.try_recv(), Ok(()));
        assert!(receive.try_recv().is_err());
        assert!(state.acknowledge());
        assert!(state.is_live());
    }
}

#[test]
fn notifications_across_acknowledgement_leave_a_new_event_pending() {
    let state = Arc::new(MaintenanceWakeState::default());
    let (events, received) = mpsc::sync_channel(1);
    let (resume, ready) = mpsc::channel();
    let (coalesced, observed) = mpsc::channel();
    assert!(state.notify(|| events.try_send(()).is_ok()));
    let worker_state = state.clone();
    let worker = std::thread::spawn(move || {
        let before = worker_state.notify(|| events.try_send(()).is_ok());
        let _ = coalesced.send(before);
        if ready.recv().is_err() {
            return false;
        }
        worker_state.notify(|| events.try_send(()).is_ok())
    });
    let before = observed.recv().unwrap();
    received.recv().unwrap();
    let acknowledged = state.acknowledge();
    let resumed = resume.send(()).is_ok();
    let after = worker.join().unwrap();
    assert!(before && acknowledged && resumed && after);
    assert_eq!(received.try_recv(), Ok(()));
    assert!(state.acknowledge());
    assert!(received.try_recv().is_err());
}

#[test]
fn a_coalesced_sender_cannot_accept_before_an_inflight_delivery_refuses() {
    let state = Arc::new(MaintenanceWakeState::default());
    let (entered, receiving) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let first_state = state.clone();
    let first = std::thread::spawn(move || {
        first_state.notify(|| {
            let _ = entered.send(());
            let _ = released.recv();
            false
        })
    });
    receiving.recv().unwrap();
    let second_state = state.clone();
    let (second_started, started) = mpsc::channel();
    let (second_done, done) = mpsc::channel();
    let second = std::thread::spawn(move || {
        let _ = second_started.send(());
        let accepted = second_state.notify(|| true);
        let _ = second_done.send(accepted);
        accepted
    });
    started.recv().unwrap();
    let still_waiting = matches!(done.try_recv(), Err(mpsc::TryRecvError::Empty));
    let _ = release.send(());
    let first_result = first.join().unwrap();
    let second_result = second.join().unwrap();
    assert!(!first_result);
    assert!(!second_result);
    assert!(still_waiting);
    assert!(!state.is_live());
}
