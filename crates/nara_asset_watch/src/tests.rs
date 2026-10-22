use super::*;
use crate::backend::admit_notify_event;
use nara_asset::{
    AssetPlugin, AssetRecord, AssetReloadRequest, AssetReloadRequestKind, AssetReloadRequests,
    AssetSourceChangeLimits, AssetSourceChanges, AssetSourceKind, ProjectAssetDatabase,
    StableAssetId, register_image_reload_consumer,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{
    DiagnosticsPlugin, PressureSourceId, RuntimeDiagnostics, RuntimePressureSnapshots,
};
use nara_ecs::{ResMut, Resource};
use notify::{Event, EventKind, event::ModifyKind, event::RenameMode};
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn temp_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "nara_asset_watch_test_{}_{}",
        std::process::id(),
        stamp
    ))
}

fn install_asset_prerequisites(app: &mut App) {
    app.add_plugins((
        nara_tasks::TaskPlugin::default(),
        AssetPlugin,
        DiagnosticsPlugin::default(),
    ))
    .unwrap();
}

fn install_watch_runtime(app: &mut App, queue: AssetWatchEventQueue) {
    let observer = queue.observer();
    app.insert_resource(observer).unwrap();
    app.insert_resource(AssetWatchRuntimeStatus::default())
        .unwrap();
    app.insert_resource(queue).unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        drain_asset_watch_events.in_set(AssetTaskUpdateSet::Poll),
    )
    .unwrap();
}

fn watch_pressure_value(world: &nara_ecs::World, metric: &str) -> Option<u64> {
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    world
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .and_then(|snapshot| {
            snapshot
                .measurements()
                .iter()
                .find(|measurement| measurement.metric().as_str() == metric)
                .map(|measurement| measurement.value())
        })
}

fn rescan_sender(world: &nara_ecs::World) -> AssetWatchEventSender {
    world
        .resource::<ActiveWatchRescan>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .queue
        .sender()
}

fn rescan_callback_lifetime(world: &nara_ecs::World) -> std::sync::Weak<()> {
    world
        .resource::<ActiveWatchRescan>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .watcher
        .callback_lifetime()
}

fn assert_callback_retired(lifetime: std::sync::Weak<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    // Some notify backends finish callback destruction on their own native worker.
    while lifetime.strong_count() != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        lifetime.strong_count(),
        0,
        "the exact native callback must be released"
    );
}

#[test]
fn source_modify_translates_to_modified_change() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let source = root.join("textures").join("player.png");
    fs::write(&source, b"png").unwrap();
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::modified(&source),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceChangeKind::Modified
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn meta_modify_maps_to_source_meta_change() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let meta = root.join("textures").join("player.png.meta");
    fs::write(&meta, b"meta").unwrap();
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::modified(&meta),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceChangeKind::MetaModified
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn notify_directory_unknown_and_rename_scope_remains_sticky_until_host_reconciliation() {
    use notify::event::{CreateKind, RemoveKind};
    for kind in [
        EventKind::Remove(RemoveKind::Folder),
        EventKind::Remove(RemoveKind::Any),
        EventKind::Create(CreateKind::Folder),
        EventKind::Modify(ModifyKind::Any),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        EventKind::Any,
        EventKind::Other,
    ] {
        let mut queue = AssetWatchEventQueue::new();
        let sender = queue.sender();
        admit_notify_event(
            &sender,
            Event::new(kind)
                .add_path(PathBuf::from("textures"))
                .add_path(PathBuf::from("renamed")),
        );
        assert!(queue.drain().rescan_required(), "{kind:?}");
        assert!(
            queue.drain().rescan_required(),
            "draining is not reconciliation: {kind:?}"
        );
        assert_eq!(
            sender.try_send(AssetWatchEvent::modified("textures/child.png")),
            Err(AssetWatchQueueSendError::RescanRequired)
        );
    }
}

#[test]
fn root_and_explicit_subtree_observations_require_complete_coverage() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let source_root = AssetSourceRoot::new(&root);
    for event in [
        AssetWatchEvent::modified(&root),
        AssetWatchEvent::subtree(root.join("already-removed")),
    ] {
        assert_eq!(
            AssetWatchTranslator.translate_event(&source_root, &event),
            Err(AssetWatchError::RescanRequired)
        );
    }
    remove_temp_root(&root);
}

#[test]
fn bounded_overflow_does_not_clear_when_accepted_prefix_is_drained() {
    let mut queue = AssetWatchEventQueue::with_limits(AssetWatchQueueLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(4096).unwrap(),
    ));
    let sender = queue.sender();
    sender
        .try_send(AssetWatchEvent::modified("textures/a.png"))
        .unwrap();
    assert!(
        sender
            .try_send(AssetWatchEvent::modified("textures/b.png"))
            .is_err()
    );
    assert!(queue.drain().rescan_required());
    assert!(queue.is_empty());
    assert!(queue.drain().rescan_required());
}

#[test]
fn owned_rescan_keeps_old_obligation_until_completed_acknowledgement() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let mut world = nara_ecs::World::new();
    world.insert_resource(AssetSourceRoot::new(&root));
    let queue = AssetWatchEventQueue::new();
    queue.sender().record_backend_failure();
    world.insert_resource(queue.observer());
    world.insert_resource(queue);
    world.insert_resource(AssetSourceChanges::default());
    world.insert_resource(AssetWatchRuntimeStatus::default());
    world.insert_resource(RuntimeDiagnostics::default());
    world.insert_resource(RuntimePressureSnapshots::default());
    let mut schedule = nara_ecs::schedule::Schedule::default();
    schedule.add_systems(drain_asset_watch_events);
    schedule.run(&mut world);
    let abandoned = AssetWatchRescan::begin(&mut world, AssetWatchQueueLimits::default()).unwrap();
    drop(abandoned);
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    let superseded = AssetWatchRescan::begin(&mut world, AssetWatchQueueLimits::default()).unwrap();
    let completed = AssetWatchRescan::begin(&mut world, AssetWatchQueueLimits::default()).unwrap();
    assert!(superseded.complete(&mut world).is_err());
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    let mut foreign = nara_ecs::World::new();
    foreign.insert_resource(AssetSourceRoot::new(&root));
    // A receipt cannot complete a different runtime, even if its source path is equal.
    let foreign_receipt =
        AssetWatchRescan::begin(&mut world, AssetWatchQueueLimits::default()).unwrap();
    assert!(foreign_receipt.complete(&mut foreign).is_err());
    assert!(completed.complete(&mut world).is_err());
    let overflowed = AssetWatchRescan::begin(
        &mut world,
        AssetWatchQueueLimits::new(ItemLimit::ONE, ByteLimit::new(4096).unwrap()),
    )
    .unwrap();
    let sender = rescan_sender(&world);
    sender
        .try_send(AssetWatchEvent::modified("textures/a.png"))
        .unwrap();
    assert!(
        sender
            .try_send(AssetWatchEvent::modified("textures/b.png"))
            .is_err()
    );
    assert!(overflowed.complete(&mut world).is_err());
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    let completed = AssetWatchRescan::begin(&mut world, AssetWatchQueueLimits::default()).unwrap();
    rescan_sender(&world)
        .try_send(AssetWatchEvent::modified("textures/during-scan.png"))
        .unwrap();
    completed.complete(&mut world).unwrap();
    assert!(
        !world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert!(world.resource::<AssetWatcher>().is_running());
    assert_eq!(
        world
            .resource_mut::<AssetWatchEventQueue>()
            .drain()
            .events(),
        [AssetWatchEvent::modified("textures/during-scan.png")]
    );
    world.remove_resource::<AssetWatcher>();
    remove_temp_root(&root);
}

#[test]
fn rescan_supersession_and_abandonment_retire_only_the_exact_native_watcher() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(&root)).unwrap();
    let world = app.world_mut().unwrap();
    world
        .resource::<AssetWatchEventQueue>()
        .sender()
        .record_backend_failure();
    let mut poll = nara_ecs::schedule::Schedule::default();
    poll.add_systems(drain_asset_watch_events);
    poll.run(world);

    let superseded = AssetWatchRescan::begin(world, AssetWatchQueueLimits::default()).unwrap();
    let old_callback = rescan_callback_lifetime(world);
    let old_sender = rescan_sender(world);
    assert_ne!(old_callback.strong_count(), 0);
    let abandoned = AssetWatchRescan::begin(world, AssetWatchQueueLimits::default()).unwrap();
    let replacement_callback = rescan_callback_lifetime(world);
    let replacement_sender = rescan_sender(world);
    assert_callback_retired(old_callback);
    assert_eq!(
        old_sender.try_send(AssetWatchEvent::modified("old.png")),
        Err(AssetWatchQueueSendError::Disconnected)
    );
    assert!(superseded.complete(world).is_err());
    assert_ne!(replacement_callback.strong_count(), 0);
    replacement_sender
        .try_send(AssetWatchEvent::modified("during-scan.png"))
        .unwrap();
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );

    drop(abandoned);
    assert_callback_retired(replacement_callback);
    assert_eq!(
        replacement_sender.try_send(AssetWatchEvent::modified("abandoned.png")),
        Err(AssetWatchQueueSendError::Disconnected)
    );
    assert!(
        world
            .resource::<ActiveWatchRescan>()
            .0
            .lock()
            .unwrap()
            .is_none()
    );
    poll.run(world);
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );

    let completed = AssetWatchRescan::begin(world, AssetWatchQueueLimits::default()).unwrap();
    let completed_callback = rescan_callback_lifetime(world);
    let completed_sender = rescan_sender(world);
    completed_sender
        .try_send(AssetWatchEvent::modified("retained.png"))
        .unwrap();
    completed.complete(world).unwrap();
    assert_ne!(
        completed_callback.strong_count(),
        0,
        "consuming the receipt must transfer, not retire, its watcher"
    );
    assert!(
        !world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert_eq!(
        world
            .resource_mut::<AssetWatchEventQueue>()
            .drain()
            .events(),
        [AssetWatchEvent::modified("retained.png")]
    );
    nara_app::RuntimeInstance::start(app)
        .unwrap()
        .finish()
        .unwrap();
    assert_callback_retired(completed_callback);
    remove_temp_root(&root);
}

#[test]
fn held_rescan_receipt_cannot_keep_native_watching_alive_after_runtime_stop() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(&root)).unwrap();
    app.world()
        .resource::<AssetWatchEventQueue>()
        .sender()
        .record_backend_failure();
    let host_receipt = Arc::new(Mutex::new(None));
    let receipt_sink = Arc::clone(&host_receipt);
    app.add_systems(CoreStage::Update, move |world: &mut nara_ecs::World| {
        let mut receipt = receipt_sink.lock().unwrap();
        if receipt.is_none() {
            *receipt =
                Some(AssetWatchRescan::begin(world, AssetWatchQueueLimits::default()).unwrap());
        }
    })
    .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let callback = rescan_callback_lifetime(runtime.world());
    let sender = rescan_sender(runtime.world());
    assert_ne!(callback.strong_count(), 0);
    assert!(host_receipt.lock().unwrap().is_some());
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );

    assert!(matches!(
        runtime.request_control(nara_app::RuntimeControl::Stop),
        nara_app::RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != nara_app::RuntimeState::Stopped && Instant::now() < deadline {
        runtime.drive(Duration::ZERO).unwrap();
        // Tasks shutdown is pollable; worker return is not guaranteed after two owner drives.
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(runtime.state(), nara_app::RuntimeState::Stopped);
    assert!(
        host_receipt.lock().unwrap().is_some(),
        "the Host still holds its receipt while the stopped World remains alive"
    );
    assert!(!runtime.world().contains_resource::<ActiveWatchRescan>());
    assert!(!runtime.world().contains_resource::<AssetWatcher>());
    assert_callback_retired(callback);
    assert_eq!(
        sender.try_send(AssetWatchEvent::modified("after-stop.png")),
        Err(AssetWatchQueueSendError::Disconnected)
    );
    drop(host_receipt.lock().unwrap().take());
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn meta_remove_maps_to_source_remove_change() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let meta = root.join("textures").join("player.png.meta");
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::removed(&meta),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceChangeKind::Removed
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn remove_translates_to_removed_change_without_file_existing() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let source = root.join("textures").join("player.png");
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::removed(&source),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceChangeKind::Removed
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn rename_translates_to_remove_and_modify() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let from = root.join("textures").join("old.png");
    let to = root.join("textures").join("new.png");
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::renamed(&from, &to),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![
            AssetSourceChange::new(
                AssetPath::new("textures/old.png").unwrap(),
                AssetSourceChangeKind::Removed
            ),
            AssetSourceChange::new(
                AssetPath::new("textures/new.png").unwrap(),
                AssetSourceChangeKind::Modified
            )
        ]
    );
    remove_temp_root(&root);
}

#[test]
fn rename_from_root_to_outside_keeps_in_root_remove() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let from = root.join("textures").join("old.png");
    let outside = root.with_file_name("outside.png");
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::renamed(&from, &outside),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/old.png").unwrap(),
            AssetSourceChangeKind::Removed
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn rename_from_outside_to_root_keeps_in_root_modify() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let outside = root.with_file_name("outside.png");
    let to = root.join("textures").join("new.png");
    let translator = AssetWatchTranslator;

    let changes = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::renamed(&outside, &to),
        )
        .unwrap();

    assert_eq!(
        changes,
        vec![AssetSourceChange::new(
            AssetPath::new("textures/new.png").unwrap(),
            AssetSourceChangeKind::Modified
        )]
    );
    remove_temp_root(&root);
}

#[test]
fn outside_root_path_is_rejected() {
    let root = temp_root();
    let outside = root.with_file_name("outside.png");
    fs::create_dir_all(&root).unwrap();
    let translator = AssetWatchTranslator;

    let error = translator
        .translate_event(
            &AssetSourceRoot::new(&root),
            &AssetWatchEvent::modified(&outside),
        )
        .unwrap_err();

    assert!(matches!(error, AssetWatchError::SourceOutsideRoot { .. }));
    remove_temp_root(&root);
}

#[test]
fn queue_overflow_rejects_the_whole_batch_and_requires_rescan() {
    let limits = AssetWatchQueueLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(4_096).expect("test byte limit is non-zero"),
    );
    let mut queue = AssetWatchEventQueue::with_limits(limits);
    let sender = queue.sender();
    sender
        .try_send(AssetWatchEvent::modified("textures/accepted.png"))
        .unwrap();

    let error = sender
        .try_send(AssetWatchEvent::modified("textures/rejected.png"))
        .unwrap_err();
    assert!(matches!(error, AssetWatchQueueSendError::Full { .. }));

    let suppressed = sender
        .try_send(AssetWatchEvent::modified("textures/suppressed.png"))
        .unwrap_err();
    assert_eq!(suppressed, AssetWatchQueueSendError::RescanRequired);

    let drained = queue.drain();
    assert!(drained.events().is_empty());
    assert_eq!(drained.captured_events(), 1);
    assert!(drained.rescan_required());
    let stats = sender.stats();
    assert_eq!(stats.overflow_rejections(), 1);
    assert_eq!(stats.suppressed_batches(), 1);
    assert_eq!(stats.discarded_events(), 3);

    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    install_watch_runtime(&mut app, queue);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let diagnostics = runtime.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.queue-overflow")
    );
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.rescan-required")
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "discarded-events"),
        Some(3)
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn callback_admission_does_not_contend_with_the_poll_receiver() {
    let mut queue = AssetWatchEventQueue::new();
    let sender = queue.sender();

    queue.with_receiver_held_for_tests(|| {
        sender
            .try_send(AssetWatchEvent::modified("textures/player.png"))
            .unwrap();
        assert_eq!(sender.stats().retained_events(), 1);
    });

    assert_eq!(queue.drain().events().len(), 1);
    assert!(queue.is_empty());
}

#[test]
fn concurrent_producer_rejection_is_non_blocking_and_requires_rescan() {
    let queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    sender
        .try_send(AssetWatchEvent::modified("textures/accepted.png"))
        .unwrap();
    let error = sender.with_admission_held_for_tests(|| {
        assert_eq!(sender.stats().retained_events(), 1);
        sender
            .try_send(AssetWatchEvent::modified("textures/player.png"))
            .unwrap_err()
    });
    assert_eq!(error, AssetWatchQueueSendError::Busy);
    assert_eq!(sender.stats().busy_rejections(), 1);
    assert_eq!(sender.stats().discarded_events(), 1);
    assert!(sender.stats().rescan_required());
}

#[derive(Debug, Default, Resource)]
struct WatchCapturedReloadRequests(Vec<AssetReloadRequest>);

fn install_reload_capture(app: &mut App) {
    let consumer = register_image_reload_consumer::<String>(app).unwrap();
    app.init_resource::<WatchCapturedReloadRequests>().unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut requests: ResMut<AssetReloadRequests>,
               mut captured: ResMut<WatchCapturedReloadRequests>| {
            captured.0.extend(
                requests
                    .drain_images(&consumer)
                    .expect("test consumer owns requests"),
            );
        })
        .in_set(AssetTaskUpdateSet::SpawnJobs),
    )
    .unwrap();
}

#[test]
fn poisoned_producer_admission_is_terminal_and_observable() {
    let queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        sender.with_admission_held_for_tests(|| panic!("poison producer admission"));
    }));
    assert!(panic.is_err());

    let error = sender
        .try_send(AssetWatchEvent::modified("textures/player.png"))
        .unwrap_err();

    assert_eq!(error, AssetWatchQueueSendError::Unavailable);
    assert_eq!(sender.stats().unavailable_failures(), 1);
    assert_eq!(sender.stats().discarded_events(), 1);
    assert!(sender.stats().rescan_required());
}

#[test]
fn sender_reports_disconnection_after_the_receiver_is_dropped() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let queue = AssetWatchEventQueue::new();
    let observer = queue.observer();
    let sender = queue.sender();
    drop(queue);

    let error = sender
        .try_send(AssetWatchEvent::modified("textures/player.png"))
        .unwrap_err();

    assert_eq!(error, AssetWatchQueueSendError::Disconnected);
    assert_eq!(sender.stats().disconnect_rejections(), 1);
    assert_eq!(sender.stats().discarded_events(), 1);

    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    app.insert_resource(observer).unwrap();
    app.insert_resource(AssetWatchRuntimeStatus::default())
        .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        drain_asset_watch_events.in_set(AssetTaskUpdateSet::Poll),
    )
    .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let diagnostics = runtime.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.queue-disconnected")
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "disconnect-rejections"),
        Some(1)
    );
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn malformed_rename_marks_callback_translation_loss() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let mut queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    let malformed = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
        .add_path(PathBuf::from("textures/only-one-path.png"));

    admit_notify_event(&sender, malformed);

    let drained = queue.drain();
    assert!(drained.events().is_empty());
    assert!(drained.rescan_required());
    assert_eq!(sender.stats().translation_failures(), 1);
    assert_eq!(sender.stats().discarded_events(), 1);

    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    install_watch_runtime(&mut app, queue);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let diagnostics = runtime.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.translation-failed")
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "translation-failures"),
        Some(1)
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn rescan_terminal_stops_the_live_watcher_backend() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let limits = AssetWatchQueueLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(4_096).expect("test byte limit is non-zero"),
    );
    let queue = AssetWatchEventQueue::with_limits(limits);
    let sender = queue.sender();
    let watcher = AssetWatcher::watch_recursive(&root, sender.clone()).unwrap();
    sender
        .try_send(AssetWatchEvent::modified("textures/accepted.png"))
        .unwrap();
    assert!(matches!(
        sender.try_send(AssetWatchEvent::modified("textures/rejected.png")),
        Err(AssetWatchQueueSendError::Full { .. })
    ));

    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    app.insert_resource(watcher).unwrap();
    install_watch_runtime(&mut app, queue);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert!(!runtime.world().resource::<AssetWatcher>().is_running());
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn queue_drains_into_source_changes() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let source = root.join("textures").join("player.png");
    fs::write(&source, b"png").unwrap();
    let mut queue = AssetWatchEventQueue::new();
    queue
        .sender()
        .try_send(AssetWatchEvent::modified(&source))
        .unwrap();
    let mut changes = AssetSourceChanges::new();

    let translator = AssetWatchTranslator;
    for event in queue.drain().into_events() {
        for change in translator
            .translate_event(&AssetSourceRoot::new(&root), &event)
            .unwrap()
        {
            changes.push(change).unwrap();
        }
    }

    assert_eq!(changes.len(), 1);
    assert!(queue.is_empty());
    remove_temp_root(&root);
}

#[test]
fn queue_drain_captures_only_the_prefix_present_at_entry() {
    let root = temp_root();
    let first = root.join("textures").join("first.png");
    let second = root.join("textures").join("second.png");
    let mut queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    sender
        .try_send(AssetWatchEvent::modified(first.clone()))
        .unwrap();

    let captured = queue.drain();
    sender
        .try_send(AssetWatchEvent::modified(second.clone()))
        .unwrap();

    assert_eq!(captured.events(), [AssetWatchEvent::modified(first)]);
    assert_eq!(queue.drain().events(), [AssetWatchEvent::modified(second)]);
}

#[test]
fn queued_watch_events_are_resolved_in_the_same_task_update() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let source = root.join("textures").join("player.png");
    fs::write(&source, b"png").unwrap();
    let record = AssetRecord::new(
        stable_id(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    );
    let queue = AssetWatchEventQueue::new();
    queue
        .sender()
        .try_send(AssetWatchEvent::modified(&source))
        .unwrap();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record)
        .unwrap();
    install_reload_capture(&mut app);
    install_watch_runtime(&mut app, queue);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let requests = runtime.world().resource::<WatchCapturedReloadRequests>();
    let request = requests.0.first().unwrap();
    assert_eq!(request.path().as_str(), "textures/player.png");
    assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn notify_backend_error_is_visible_in_runtime_diagnostics_and_pressure() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let queue = AssetWatchEventQueue::new();
    queue.sender().record_backend_failure();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    install_watch_runtime(&mut app, queue);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let diagnostic_codes = runtime
        .world()
        .resource::<RuntimeDiagnostics>()
        .iter()
        .map(|entry| entry.code().as_str())
        .collect::<Vec<_>>();
    assert!(diagnostic_codes.contains(&"asset-watch.backend-failed"));
    assert!(diagnostic_codes.contains(&"asset-watch.rescan-required"));
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    let pressure = runtime
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "backend-failures" && measurement.value() == 1
    }));
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn translation_failure_records_diagnostic_without_source_change() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let outside = root.with_file_name("outside.png");
    let queue = AssetWatchEventQueue::new();
    queue
        .sender()
        .try_send(AssetWatchEvent::modified(&outside))
        .unwrap();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    install_watch_runtime(&mut app, queue);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    assert!(runtime.world().resource::<AssetSourceChanges>().is_empty());
    let outside_path = outside.display().to_string();
    let diagnostics = runtime.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.translation-failed")
    );
    assert!(
        diagnostics
            .iter()
            .all(|entry| !entry.summary().as_str().contains(&outside_path))
    );
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    let pressure = runtime
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "discarded-events" && measurement.value() == 1
    }));
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn downstream_source_change_overflow_requires_a_rescan() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let queue = AssetWatchEventQueue::new();
    queue
        .sender()
        .try_send_batch(vec![
            AssetWatchEvent::modified(root.join("textures/a.png")),
            AssetWatchEvent::modified(root.join("textures/b.png")),
        ])
        .unwrap();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    app.insert_resource(AssetSourceChanges::with_limits(
        AssetSourceChangeLimits::new(ItemLimit::ONE, ByteLimit::new(4 * 1024).unwrap()),
    ))
    .unwrap();
    install_watch_runtime(&mut app, queue);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    assert!(runtime.world().resource::<AssetSourceChanges>().is_empty());
    assert!(
        runtime
            .world()
            .resource::<RuntimeDiagnostics>()
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.source-change-admission-failed")
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "source-admission-failures"),
        Some(1)
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "discarded-events"),
        Some(2)
    );
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn repeated_backend_failures_are_counted_without_unbounded_diagnostics() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let queue = AssetWatchEventQueue::new();
    let sender = queue.sender();
    sender.record_backend_failure();
    sender.record_backend_failure();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
    install_asset_prerequisites(&mut app);
    install_watch_runtime(&mut app, queue);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let diagnostics = runtime.world().resource::<RuntimeDiagnostics>();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|entry| entry.code().as_str() == "asset-watch.backend-failed")
            .count(),
        1
    );
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    let pressure = runtime
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "backend-failures" && measurement.value() == 2
    }));
    remove_temp_root(&root);
    runtime.finish().unwrap();
}

#[test]
fn plugin_rejects_root_that_differs_from_existing_asset_source_root() {
    let configured_root = temp_root();
    let watch_root = configured_root.with_file_name("nara_asset_watch_other_root");
    fs::create_dir_all(&configured_root).unwrap();
    let mut app = App::new();
    app.insert_resource(AssetSourceRoot::new(&configured_root))
        .unwrap();
    install_asset_prerequisites(&mut app);

    let Err(error) = app.add_plugin(AssetWatchPlugin::new(&watch_root)) else {
        panic!("watch plugin should reject a root that differs from AssetSourceRoot");
    };

    assert!(matches!(
        error,
        nara_app::AddPluginsError::Plugin(ref failure)
            if matches!(failure.error(), PluginError::SetupFailed { .. })
    ));
    remove_temp_root(&configured_root);
}

fn stable_id() -> StableAssetId {
    StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
}

fn remove_temp_root(path: &Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        panic!(
            "failed to remove temp test directory {}: {error}",
            path.display()
        );
    }
}
