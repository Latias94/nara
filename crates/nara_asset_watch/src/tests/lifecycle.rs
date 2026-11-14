use super::*;
use nara_app::{
    RuntimeCloseCause, RuntimeClosePolicy, RuntimeControl, RuntimeControlRequestResult,
    RuntimeInstance, RuntimeObligationLedger, RuntimeState,
};
use std::sync::mpsc;

fn configured_app(root: &Path) -> (App, AssetWatchRuntime, AssetSourceRoot) {
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(root, AssetPathScope::ROOT))
        .unwrap();
    let lifecycle = app.world().resource::<AssetWatchRuntime>().clone();
    let source = app.world().resource::<AssetSourceRoot>().clone();
    app.world_mut().unwrap().remove_resource::<AssetWatcher>();
    assert_retirements_joined(&lifecycle);
    (app, lifecycle, source)
}

#[test]
fn original_callback_blocks_join_after_facade_drop_until_existing_runtime_close_retry() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let file = root.join("observed.txt");
    fs::write(&file, b"before watch").unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut release = Some(release_rx);
    let watcher = lifecycle
        .watch_paths_with_callback_for_test(
            &source,
            &[AssetWatchPath::new(root.clone(), true)],
            queue.sender(),
            Box::new(move |_| {
                if let Some(release) = release.take() {
                    let _ = entered_tx.send(());
                    let _ = release.recv_timeout(Duration::from_secs(10));
                }
            }),
        )
        .unwrap();
    let callback = watcher.callback_lifetime();
    let mut runtime = RuntimeInstance::start_with_options(
        app,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    fs::write(&file, b"enter actual original native callback").unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(watcher);
    lifecycle.poll_retired().unwrap();
    assert_eq!(lifecycle.stats().active(), 0);
    assert_eq!(lifecycle.stats().retiring(), 1);
    assert_eq!(lifecycle.stats().joined(), baseline.joined());
    assert_ne!(callback.strong_count(), 0);

    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    assert!(
        runtime
            .close_evidence()
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    assert!(
        runtime
            .close_evidence()
            .pending_participants()
            .iter()
            .any(|pending| { pending.participant() == ASSET_WATCH_CLOSE_PARTICIPANT })
    );
    assert_eq!(lifecycle.stats().retiring(), 1);
    assert_eq!(lifecycle.stats().joined(), baseline.joined());
    assert!(matches!(
        lifecycle.watch_recursive(&source, queue.sender()),
        Err(AssetWatchError::RuntimeClosing)
    ));
    assert_eq!(lifecycle.stats().started(), baseline.started() + 1);

    release_tx.send(()).unwrap();
    assert_retirements_joined(&lifecycle);
    assert_eq!(callback.strong_count(), 0);
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        runtime.drive(Duration::ZERO).unwrap();
        if runtime.state() == RuntimeState::CloseIncomplete {
            assert!(matches!(
                runtime.request_control(RuntimeControl::RetryClose),
                RuntimeControlRequestResult::Accepted(_)
            ));
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(lifecycle.stats().started(), lifecycle.stats().joined());
    assert!(!runtime.close_evidence().has_terminal_failure());
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn retained_facade_and_factory_cannot_extend_backend_after_pause_resume_and_stop() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let queue = AssetWatchEventQueue::new();
    let mut retained = lifecycle.watch_recursive(&source, queue.sender()).unwrap();
    let observer = lifecycle.observer();
    let callback = retained.callback_lifetime();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    for (control, expected) in [
        (RuntimeControl::Pause, RuntimeState::Paused),
        (RuntimeControl::Resume, RuntimeState::Running),
    ] {
        assert!(matches!(
            runtime.request_control(control),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), expected);
        lifecycle.poll_retired().unwrap();
        assert!(retained.is_running());
        assert_eq!(lifecycle.stats().active(), 1);
    }
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        runtime.drive(Duration::ZERO).unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let stats = lifecycle.stats();
    assert_eq!(stats.active(), 0);
    assert_eq!(stats.retiring(), 0);
    assert_eq!(stats.started(), stats.joined());
    assert_eq!(callback.strong_count(), 0);
    assert!(!retained.is_running());
    assert!(retained.is_closed());
    assert_eq!(retained.try_close(), Ok(true));
    assert!(matches!(
        lifecycle.watch_paths(
            &source,
            &[AssetWatchPath::new(root.clone(), false)],
            queue.sender()
        ),
        Err(AssetWatchError::RuntimeClosing)
    ));
    assert_eq!(lifecycle.stats(), stats);
    assert_eq!(observer.stats(), stats);
    runtime.finish().unwrap();
    drop(retained);
    assert_eq!(lifecycle.stats(), stats);
    drop(lifecycle);
    assert_eq!(observer.stats(), stats);
    remove_temp_root(&root);
}

#[test]
fn partial_watch_setup_failure_keeps_the_constructed_original_worker_in_close_custody() {
    let root = temp_root();
    fs::create_dir_all(root.join("assets")).unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let result = lifecycle.watch_paths(
        &source,
        &[
            AssetWatchPath::new(root.join("assets"), true),
            AssetWatchPath::new(root.join("missing-scenes"), true),
        ],
        queue.sender(),
    );
    assert!(matches!(result, Err(AssetWatchError::Notify(_))));
    let failed = lifecycle.stats();
    assert_eq!(failed.started(), baseline.started() + 1);
    assert_eq!(failed.active(), 0);
    assert_eq!(failed.started(), failed.joined() + failed.retiring() as u64);
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    assert_eq!(lifecycle.stats().active(), 0);
    assert_eq!(lifecycle.stats().retiring(), 0);
    assert_eq!(lifecycle.stats().started(), lifecycle.stats().joined());
    remove_temp_root(&root);
}

#[test]
fn capacity_counts_blocked_retirements_and_factory_reaps_joined_workers_before_reuse() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let file = root.join("observed.txt");
    fs::write(&file, b"before watch").unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let mut active = Vec::new();
    for _ in 1..ASSET_WATCH_NATIVE_INSTANCE_LIMIT {
        active.push(lifecycle.watch_recursive(&source, queue.sender()).unwrap());
    }
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut release = Some(release_rx);
    let retiring = lifecycle
        .watch_paths_with_callback_for_test(
            &source,
            &[AssetWatchPath::new(root.clone(), true)],
            queue.sender(),
            Box::new(move |_| {
                if let Some(release) = release.take() {
                    let _ = entered_tx.send(());
                    let _ = release.recv_timeout(Duration::from_secs(10));
                }
            }),
        )
        .unwrap();
    let callback = retiring.callback_lifetime();
    fs::write(&file, b"hold one original worker").unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(retiring);
    assert_eq!(
        lifecycle.stats().active(),
        ASSET_WATCH_NATIVE_INSTANCE_LIMIT - 1
    );
    assert_eq!(lifecycle.stats().retiring(), 1);
    assert!(matches!(
        lifecycle.watch_recursive(&source, queue.sender()),
        Err(AssetWatchError::NativeCapacityExceeded { limit }) if limit == ASSET_WATCH_NATIVE_INSTANCE_LIMIT
    ));
    assert_eq!(
        lifecycle.stats().started(),
        baseline.started() + ASSET_WATCH_NATIVE_INSTANCE_LIMIT as u64
    );
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let replacement = loop {
        match lifecycle.watch_recursive(&source, queue.sender()) {
            Ok(watcher) => break watcher,
            Err(AssetWatchError::NativeCapacityExceeded { .. }) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("unexpected replacement failure: {error}"),
        }
    };
    assert_eq!(callback.strong_count(), 0);
    assert_eq!(lifecycle.stats().joined(), baseline.joined() + 1);
    assert_eq!(
        lifecycle.stats().active(),
        ASSET_WATCH_NATIVE_INSTANCE_LIMIT
    );
    drop(active);
    drop(replacement);
    assert_retirements_joined(&lifecycle);
    assert_eq!(lifecycle.stats().started(), lifecycle.stats().joined());
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn selected_directories_share_one_worker_and_leave_unselected_subtrees_unwatched() {
    let root = temp_root();
    for path in ["assets/nested", "scenes", "prefabs", "target/deep"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let files = [
        root.join("project.toml"),
        root.join("assets/nested/image.png"),
        root.join("scenes/level.scene"),
        root.join("prefabs/actor.prefab"),
    ];
    for file in &files {
        fs::write(file, b"initial content").unwrap();
    }
    let ignored = root.join("target/deep/compiler-output");
    fs::write(&ignored, b"initial compiler output").unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observations = Arc::clone(&seen);
    let mut watcher = lifecycle
        .watch_paths_with_callback_for_test(
            &source,
            &[
                AssetWatchPath::new(root.clone(), false),
                AssetWatchPath::new(root.join("assets"), true),
                AssetWatchPath::new(root.join("scenes"), true),
                AssetWatchPath::new(root.join("prefabs"), true),
            ],
            queue.sender(),
            Box::new(move |event| {
                if let Ok(event) = event {
                    observations
                        .lock()
                        .unwrap()
                        .extend(event.paths.iter().cloned());
                }
            }),
        )
        .unwrap();
    assert_eq!(lifecycle.stats().started(), baseline.started() + 1);
    assert_eq!(lifecycle.stats().active(), 1);
    lifecycle.poll_retired().unwrap();
    assert!(watcher.is_running());
    for file in &files {
        fs::write(file, b"modified through selected native directory").unwrap();
    }
    fs::write(&ignored, b"unselected compiler output").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let seen = seen.lock().unwrap();
        if files.iter().all(|file| {
            seen.iter()
                .any(|path| same_lexical_path(file, path).unwrap())
        }) {
            break;
        }
        drop(seen);
        assert!(
            Instant::now() < deadline,
            "selected native directories were not all observed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    watcher.try_close().unwrap();
    assert_retirements_joined(&lifecycle);
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|path| !same_lexical_path(&ignored, path).unwrap())
    );
    let scoped = AssetSourceRoot::new(
        &root,
        AssetPathScope::under(AssetSourcePath::new("packages/source").unwrap()),
    );
    assert_eq!(
        AssetWatchTranslator
            .translate_event(&scoped, &AssetWatchEvent::modified(&files[1]))
            .unwrap(),
        [AssetSourceChange::new(
            AssetSourcePath::new("packages/source/assets/nested/image.png").unwrap(),
            AssetSourceChangeKind::Modified,
        )]
    );
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn invalid_selected_paths_are_rejected_without_native_construction() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let outside = temp_root();
    fs::create_dir_all(&outside).unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let before = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    assert!(matches!(
        lifecycle.watch_paths(&source, &[], queue.sender()),
        Err(AssetWatchError::NoWatchPaths)
    ));
    assert!(matches!(
        lifecycle.watch_paths(
            &source,
            &[AssetWatchPath::new(outside.clone(), true)],
            queue.sender()
        ),
        Err(AssetWatchError::SourceOutsideRoot { .. })
    ));
    assert_eq!(lifecycle.stats(), before);
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    remove_temp_root(&root);
    remove_temp_root(&outside);
}

#[cfg(target_os = "linux")]
#[test]
fn selected_symlink_cannot_escape_the_canonical_source_root() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let outside = temp_root();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let before = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    for path in [root.join("escape"), root.join("escape/not-created-yet")] {
        assert!(matches!(
            lifecycle.watch_paths(&source, &[AssetWatchPath::new(path, true)], queue.sender()),
            Err(AssetWatchError::SourceOutsideRoot { .. })
        ));
    }
    assert_eq!(lifecycle.stats(), before);
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    remove_temp_root(&root);
    remove_temp_root(&outside);
}

#[cfg(target_os = "linux")]
#[test]
fn original_worker_panic_is_joined_but_never_reported_as_clean_runtime_close() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let file = root.join("observed.txt");
    fs::write(&file, b"initial content").unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let queue = AssetWatchEventQueue::new();
    let mut watcher = lifecycle
        .watch_paths_with_callback_for_test(
            &source,
            &[AssetWatchPath::new(root.clone(), true)],
            queue.sender(),
            Box::new(|_| panic!("intentional original inotify worker failure")),
        )
        .unwrap();
    let callback = watcher.callback_lifetime();
    fs::write(&file, b"panic on original worker").unwrap();
    assert_callback_retired(callback);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match watcher.try_close() {
            Err(AssetWatchError::NativeClose { joined: true, .. }) => break,
            Ok(false) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            result => panic!("original worker panic lost its failure evidence: {result:?}"),
        }
    }
    assert!(watcher.is_closed());
    assert_eq!(lifecycle.stats().retiring(), 0);
    assert_eq!(lifecycle.stats().started(), lifecycle.stats().joined());
    assert!(matches!(
        lifecycle.poll_retired(),
        Err(AssetWatchError::NativeClose { joined: true, .. })
    ));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("running runtime must accept Stop");
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        runtime.drive(Duration::ZERO).unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        runtime.control_status(ticket),
        Some(nara_app::RuntimeControlStatus::Failed(
            nara_app::RuntimeControlFailure::CloseFailed
        ))
    );
    assert!(runtime.close_evidence().has_terminal_failure());
    assert!(runtime.close_evidence().pending_participants().is_empty());
    let failure = runtime.finish().unwrap_err();
    assert_eq!(
        failure.retirement_state(),
        nara_app::RuntimeCandidateRetirementState::Retired
    );
    assert!(failure.close_evidence().causes().iter().any(|cause| matches!(
        cause,
        RuntimeCloseCause::ParticipantError { participant, code: "asset-watch.native-join-failed", disposition: nara_app::RuntimeCloseErrorDisposition::Terminal, .. }
            if *participant == ASSET_WATCH_CLOSE_PARTICIPANT
    )));
    drop(failure);
    remove_temp_root(&root);
}
