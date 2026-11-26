use super::*;
use nara_app::{RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeState};
use std::sync::mpsc;

fn configured_app(root: &Path) -> (App, AssetWatchRuntime, AssetSourceRoot) {
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(root, AssetPathScope::ROOT))
        .unwrap();
    let lifecycle = app.world().resource::<AssetWatchRuntime>().clone();
    let source = app.world().resource::<AssetSourceRoot>().clone();
    app.world_mut().unwrap().remove_resource::<AssetWatcher>();
    assert_eq!(lifecycle.stats().active(), 0);
    (app, lifecycle, source)
}

#[test]
fn runtime_close_revokes_publication_while_a_native_callback_is_in_flight() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let file = root.join("observed.txt");
    fs::write(&file, b"before watch").unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (callback_lifetime, callback_retired) = mpsc::channel::<()>();
    let mut release = Some(release_rx);
    let watcher = lifecycle
        .watch_paths_with_callback_for_test(
            &source,
            &[AssetWatchPath::new(root.clone(), true)],
            sender.clone(),
            Box::new(move |result| {
                let _callback_lifetime = &callback_lifetime;
                if result.as_ref().is_ok_and(|event| {
                    matches!(
                        event.kind,
                        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_))
                    )
                }) && let Some(release) = release.take()
                {
                    let _ = entered_tx.send(());
                    let _ = release.recv_timeout(Duration::from_secs(10));
                }
            }),
        )
        .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    fs::write(&file, b"hold the callback before publication").unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let before_close = sender.stats();
    assert_eq!(before_close.accepted_events(), 0);
    assert!(!before_close.rescan_required());
    runtime.finish().unwrap();
    assert!(!watcher.is_running());
    assert_eq!(lifecycle.stats().active(), 0);
    assert_eq!(callback_retired.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert!(matches!(
        lifecycle.watch_recursive(&source, sender.clone()),
        Err(AssetWatchError::RuntimeClosing)
    ));
    assert_eq!(lifecycle.stats().started(), baseline.started() + 1);

    release_tx.send(()).unwrap();
    assert_eq!(
        callback_retired.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    assert_eq!(sender.stats(), before_close);
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
    assert!(!retained.is_running());
    retained.close();
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
fn partial_watch_setup_failure_releases_the_handle_and_leaves_the_factory_usable() {
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
    let watcher = lifecycle.watch_recursive(&source, queue.sender()).unwrap();
    assert!(watcher.is_running());
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    assert_eq!(lifecycle.stats().active(), 0);
    assert!(!watcher.is_running());
    remove_temp_root(&root);
}

#[test]
fn capacity_counts_owned_handles_and_allows_immediate_reuse() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let (app, lifecycle, source) = configured_app(&root);
    let baseline = lifecycle.stats();
    let queue = AssetWatchEventQueue::new();
    let mut active = Vec::new();
    for _ in 0..ASSET_WATCH_NATIVE_INSTANCE_LIMIT {
        active.push(lifecycle.watch_recursive(&source, queue.sender()).unwrap());
    }
    assert_eq!(
        lifecycle.stats().active(),
        ASSET_WATCH_NATIVE_INSTANCE_LIMIT
    );
    assert!(matches!(
        lifecycle.watch_recursive(&source, queue.sender()),
        Err(AssetWatchError::NativeCapacityExceeded { limit }) if limit == ASSET_WATCH_NATIVE_INSTANCE_LIMIT
    ));
    assert_eq!(
        lifecycle.stats().started(),
        baseline.started() + ASSET_WATCH_NATIVE_INSTANCE_LIMIT as u64
    );
    drop(active.pop().unwrap());
    let replacement = lifecycle.watch_recursive(&source, queue.sender()).unwrap();
    assert!(replacement.is_running());
    assert_eq!(
        lifecycle.stats().active(),
        ASSET_WATCH_NATIVE_INSTANCE_LIMIT
    );
    drop(active);
    drop(replacement);
    assert_eq!(lifecycle.stats().active(), 0);
    RuntimeInstance::start(app).unwrap().finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn selected_directories_deliver_events_through_one_watcher() {
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
    let mut queue = AssetWatchEventQueue::new();
    let (events, native_events) = mpsc::channel();
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
                let _ = events.send(format!("{event:?}"));
            }),
        )
        .unwrap();
    assert_eq!(lifecycle.stats().started(), baseline.started() + 1);
    assert_eq!(lifecycle.stats().active(), 1);
    assert!(watcher.is_running());
    fs::write(&ignored, b"unselected compiler output").unwrap();
    for file in &files {
        fs::write(file, b"modified through selected native directory").unwrap();
    }
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let drained = queue.drain();
        assert!(
            !drained.rescan_required(),
            "native watch requested a rescan: {:?}; events: {:#?}",
            queue.stats(),
            native_events.try_iter().collect::<Vec<_>>()
        );
        seen.extend(drained.into_events());
        if files.iter().all(|file| {
            seen.iter()
                .any(|event| same_lexical_path(file, event.path()).unwrap())
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "selected native directories were not all observed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    watcher.close();
    assert_eq!(lifecycle.stats().active(), 0);
    let drained = queue.drain();
    assert!(!drained.rescan_required());
    seen.extend(drained.into_events());
    assert!(
        seen.iter()
            .all(|event| !same_lexical_path(&ignored, event.path()).unwrap())
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
