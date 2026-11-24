use std::{path::PathBuf, thread, time::Instant};

use notify::event::{AccessKind, CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode};

use super::*;

fn request_path() -> PathBuf {
    Path::new("private-control").join(REQUEST_FILE)
}

#[test]
fn construction_acquires_nothing_and_empty_close_is_idempotent() {
    let mut watch = RequestWatch::new();
    assert_eq!(watch.snapshot(), Snapshot::default());
    assert!(watch.watcher.is_none());
    assert!(!watch.pending());
    assert_eq!(watch.close(), Ok(()));
    assert_eq!(watch.close(), Ok(()));
    assert!(!watch.snapshot().watcher_started);
    assert!(watch.snapshot().watcher_closed);
    assert!(watch.start(Path::new("unused")).is_err());
}

#[test]
fn closing_without_a_hint_still_waits_for_the_reconciliation_timeout() {
    let mut watch = RequestWatch::new();
    assert_eq!(watch.close(), Ok(()));
    watch.acknowledge().unwrap();
    let timeout = Duration::from_millis(20);
    let started = Instant::now();
    watch.wait(timeout).unwrap();
    assert!(started.elapsed() >= timeout);
    assert!(!watch.pending());
}

#[test]
fn original_callback_coalesces_create_modify_and_atomic_rename_hints() {
    let watch = RequestWatch::new();
    let request = request_path();
    let events = [
        Event::new(EventKind::Create(CreateKind::File)).add_path(request.clone()),
        Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any))).add_path(request.clone()),
        Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(request.clone()),
        Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(request.with_file_name("host-request-1.tmp"))
            .add_path(request.clone()),
    ];
    for event in events {
        watch.shared.on_event(&request, Ok(event));
    }
    let snapshot = watch.snapshot();
    assert_eq!(snapshot.callbacks, 4);
    assert_eq!(snapshot.hints, 4);
    assert_eq!(snapshot.coalesced_hints, 3);
    assert_eq!(snapshot.native_wake_delivery_failures, 0);
    assert!(!snapshot.watcher_started);
    assert!(!snapshot.counters_saturated);
    assert!(watch.pending());
    watch.acknowledge().unwrap();
    assert!(!watch.pending());
    assert_eq!(watch.snapshot(), snapshot);
    watch.shared.on_event(
        &request,
        Ok(Event::new(EventKind::Create(CreateKind::Any)).add_path(request.clone())),
    );
    assert!(watch.pending());
    assert_eq!(watch.snapshot().hints, 5);
    assert_eq!(watch.snapshot().coalesced_hints, 3);
}

#[test]
fn definite_response_temporary_access_and_consumed_request_events_are_not_request_hints() {
    let watch = RequestWatch::new();
    let request = request_path();
    let events = [
        Event::new(EventKind::Create(CreateKind::File))
            .add_path(request.with_file_name("response-1.json")),
        Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
            .add_path(request.with_file_name("host-request-1.tmp")),
        Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(request.with_file_name(".continuity-1.tmp"))
            .add_path(request.with_file_name("response-1.json")),
        Event::new(EventKind::Any).add_path(request.with_file_name("response-2.json")),
        Event::new(EventKind::Access(AccessKind::Any)).add_path(request.clone()),
        Event::new(EventKind::Remove(RemoveKind::File)).add_path(request.clone()),
        Event::new(EventKind::Remove(RemoveKind::Any)).add_path(request.clone()),
        Event::new(EventKind::Create(CreateKind::File))
            .add_path(request.parent().unwrap().join("nested").join(REQUEST_FILE)),
    ];
    for event in events {
        watch.shared.on_event(&request, Ok(event));
        assert!(!watch.pending());
    }
    assert_eq!(watch.snapshot().callbacks, 8);
    assert_eq!(watch.snapshot().hints, 0);
}

#[test]
fn uncertain_root_and_rescan_events_retain_reconciliation_hints() {
    let watch = RequestWatch::new();
    let request = request_path();
    let events = [
        Event::new(EventKind::Any),
        Event::new(EventKind::Other),
        Event::new(EventKind::Modify(ModifyKind::Any)),
        Event::new(EventKind::Remove(RemoveKind::File)),
        Event::new(EventKind::Remove(RemoveKind::Folder))
            .add_path(request.parent().unwrap().to_owned()),
        Event::new(EventKind::Access(AccessKind::Any))
            .add_path(request.with_file_name("response-1.json"))
            .set_flag(Flag::Rescan),
        Event::new(EventKind::Remove(RemoveKind::File))
            .add_path(request.clone())
            .set_flag(Flag::Rescan),
    ];
    for event in events {
        watch.acknowledge().unwrap();
        watch.shared.on_event(&request, Ok(event));
        assert!(watch.pending());
    }
    assert_eq!(watch.snapshot().callbacks, 7);
    assert_eq!(watch.snapshot().hints, 7);
    assert_eq!(watch.snapshot().coalesced_hints, 0);
}

#[test]
fn callback_failure_is_bounded_sticky_and_survives_acknowledgment_and_close() {
    let mut watch = RequestWatch::new();
    let request = request_path();
    watch.shared.on_event(
        &request,
        Err(
            notify::Error::generic(&"unbounded backend detail ".repeat(1024))
                .add_path(request.clone()),
        ),
    );
    let failure = watch.acknowledge().unwrap_err();
    assert!(failure.len() < 128);
    assert!(!watch.shared.lock().dirty);
    assert!(watch.pending());
    assert_eq!(watch.wait(Duration::from_secs(1)), Err(failure.clone()));
    watch.shared.on_event(
        &request,
        Err(notify::Error::generic("a different backend failure")),
    );
    assert_eq!(watch.acknowledge(), Err(failure.clone()));
    assert_eq!(watch.close(), Err(failure.clone()));
    assert!(watch.snapshot().watcher_closed);
    assert_eq!(watch.close(), Err(failure));
    assert_eq!(watch.snapshot().callbacks, 2);
    assert_eq!(watch.snapshot().hints, 2);
}

#[test]
fn counters_saturate_without_losing_the_overflow_fact() {
    let watch = RequestWatch::new();
    let request = request_path();
    {
        let mut state = watch.shared.lock();
        state.dirty = true;
        state.snapshot.callbacks = u64::MAX;
        state.snapshot.hints = u64::MAX;
        state.snapshot.coalesced_hints = u64::MAX;
    }
    watch
        .shared
        .on_event(&request, Ok(Event::new(EventKind::Any)));
    watch.acknowledge().unwrap();
    let snapshot = watch.snapshot();
    assert_eq!(snapshot.callbacks, u64::MAX);
    assert_eq!(snapshot.hints, u64::MAX);
    assert_eq!(snapshot.coalesced_hints, u64::MAX);
    assert!(snapshot.counters_saturated);
}

#[test]
fn wait_does_not_lose_a_hint_published_between_check_and_wait() {
    let watch = RequestWatch::new();
    let request = request_path();
    assert!(!watch.pending());
    watch
        .shared
        .on_event(&request, Ok(Event::new(EventKind::Any)));
    let started = Instant::now();
    watch.wait(Duration::from_secs(2)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(watch.pending());
}

#[test]
fn wait_releases_the_predicate_lock_for_the_original_callback() {
    let watch = RequestWatch::new();
    let shared = Arc::clone(&watch.shared);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let publish = Arc::clone(&barrier);
    let callback = thread::spawn(move || {
        publish.wait();
        shared.on_event(&request_path(), Ok(Event::new(EventKind::Any)));
    });
    barrier.wait();
    let started = Instant::now();
    watch.wait(Duration::from_secs(2)).unwrap();
    callback.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(watch.pending());
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod native {
    use std::{
        fs,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct Directory {
        path: PathBuf,
        root: PathBuf,
    }

    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = fs::canonicalize(std::env::temp_dir()).unwrap();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "nara-request-watch-{}-{stamp}-{sequence}",
                std::process::id(),
            ));
            fs::create_dir(&path).unwrap();
            Self { path, root }
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let Ok(resolved) = fs::canonicalize(&self.path) else {
                return;
            };
            if resolved.parent() != Some(self.root.as_path())
                || resolved.file_name() != self.path.file_name()
            {
                if !thread::panicking() {
                    panic!("owned test directory resolved outside its original temporary root");
                }
                return;
            }
            let result = fs::remove_dir_all(resolved);
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }

    #[test]
    fn real_atomic_rename_notifies_without_reading_or_consuming_the_packet() {
        let directory = Directory::new();
        let mut watch = RequestWatch::new();
        watch.start(&directory.path).unwrap();
        watch.acknowledge().unwrap();
        let temporary = directory.path.join("host-request-1.tmp");
        fs::write(&temporary, b"packet bytes remain with the fixed inbox").unwrap();
        let request = directory.path.join(REQUEST_FILE);
        fs::rename(temporary, &request).unwrap();
        watch.wait(Duration::from_secs(5)).unwrap();
        assert!(watch.pending());
        let snapshot = watch.snapshot();
        assert!(snapshot.callbacks > 0);
        assert!(snapshot.hints > 0);
        assert!(snapshot.watcher_started);
        assert!(!snapshot.watcher_closed);
        assert_eq!(
            fs::read(request).unwrap(),
            b"packet bytes remain with the fixed inbox",
        );
        assert_eq!(watch.close(), Ok(()));
        assert!(watch.watcher.is_none());
        let closed = watch.snapshot();
        assert!(closed.watcher_closed);
        assert_eq!(watch.close(), Ok(()));
        assert_eq!(watch.snapshot(), closed);
    }

    #[test]
    fn already_published_packet_retains_an_initial_hint_after_watch_registration() {
        let directory = Directory::new();
        let request = directory.path.join(REQUEST_FILE);
        fs::write(&request, b"published before native subscription").unwrap();
        let mut watch = RequestWatch::new();
        watch.start(&directory.path).unwrap();
        assert!(watch.pending());
        watch.acknowledge().unwrap();
        assert_eq!(
            fs::read(request).unwrap(),
            b"published before native subscription",
        );
        assert_eq!(watch.close(), Ok(()));
        assert!(watch.snapshot().watcher_started);
        assert!(watch.snapshot().watcher_closed);
    }

    #[test]
    fn partial_registration_failure_releases_the_watcher_and_preserves_the_error() {
        let directory = Directory::new();
        let mut watch = RequestWatch::new();
        let failure = watch
            .start(&directory.path.join("missing-directory"))
            .unwrap_err();
        assert!(watch.watcher.is_none());
        assert!(watch.snapshot().watcher_started);
        assert!(watch.snapshot().watcher_closed);
        assert_eq!(watch.acknowledge(), Err(failure.clone()));
        assert_eq!(watch.close(), Err(failure));
    }

    #[test]
    fn close_discards_an_in_flight_native_request_hint() {
        let directory = Directory::new();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (handled_tx, handled_rx) = mpsc::channel();
        let mut resume = Some(resume_rx);
        let mut watch = RequestWatch::new();
        let shared = Arc::clone(&watch.shared);
        let request = directory.path.join(REQUEST_FILE);
        let request_path = request.clone();
        let original = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if event
                .as_ref()
                .is_ok_and(|event| request_hint(&request_path, event))
                && let Some(resume) = resume.take()
            {
                let _ = entered_tx.send(());
                let _ = resume.recv_timeout(Duration::from_secs(10));
                shared.on_event(&request_path, event);
                let _ = handled_tx.send(());
            }
        })
        .unwrap();
        watch.shared.lock().snapshot.watcher_started = true;
        watch
            .watcher
            .insert(original)
            .watch(&directory.path, RecursiveMode::NonRecursive)
            .unwrap();
        fs::write(request, b"request in flight at close").unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(watch.close(), Ok(()));
        assert!(watch.watcher.is_none());
        let closed = watch.snapshot();
        assert!(closed.watcher_closed);
        resume_tx.send(()).unwrap();
        handled_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(watch.snapshot(), closed);
        assert!(!watch.pending());
        assert_eq!(watch.acknowledge(), Ok(()));
    }
}
