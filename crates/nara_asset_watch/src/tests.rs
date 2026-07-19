use super::*;
use crate::backend::admit_notify_event;
use nara_asset::{
    AssetPlugin, AssetRecord, AssetReloadRequestKind, AssetReloadRequests, AssetSourceChanges,
    AssetSourceKind, ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{
    DiagnosticsPlugin, PressureSourceId, RuntimeDiagnostics, RuntimePressureSnapshots,
};
use notify::{Event, EventKind, event::ModifyKind, event::RenameMode};
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    time::{SystemTime, UNIX_EPOCH},
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

fn watch_pressure_value(app: &App, metric: &str) -> Option<u64> {
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    app.world()
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
    app.update().unwrap();

    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
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
    assert_eq!(watch_pressure_value(&app, "discarded-events"), Some(3));
    remove_temp_root(&root);
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
    app.update().unwrap();

    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.queue-disconnected")
    );
    assert_eq!(watch_pressure_value(&app, "disconnect-rejections"), Some(1));
    assert!(
        app.world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
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
    app.update().unwrap();

    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "asset-watch.translation-failed")
    );
    assert_eq!(watch_pressure_value(&app, "translation-failures"), Some(1));
    remove_temp_root(&root);
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
    app.update().unwrap();

    assert!(
        app.world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert!(!app.world().resource::<AssetWatcher>().is_running());
    remove_temp_root(&root);
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
            changes.push(change);
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
    install_watch_runtime(&mut app, queue);

    app.update().unwrap();

    let requests = app.world().resource::<AssetReloadRequests>();
    let request = requests.iter().next().unwrap();
    assert_eq!(request.path().as_str(), "textures/player.png");
    assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
    remove_temp_root(&root);
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

    app.update().unwrap();

    let diagnostic_codes = app
        .world()
        .resource::<RuntimeDiagnostics>()
        .iter()
        .map(|entry| entry.code().as_str())
        .collect::<Vec<_>>();
    assert!(diagnostic_codes.contains(&"asset-watch.backend-failed"));
    assert!(diagnostic_codes.contains(&"asset-watch.rescan-required"));
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    let pressure = app
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "backend-failures" && measurement.value() == 1
    }));
    assert!(
        app.world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
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

    app.update().unwrap();

    assert!(app.world().resource::<AssetSourceChanges>().is_empty());
    let outside_path = outside.display().to_string();
    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
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
    let pressure = app
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "discarded-events" && measurement.value() == 1
    }));
    assert!(
        app.world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    remove_temp_root(&root);
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

    app.update().unwrap();

    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|entry| entry.code().as_str() == "asset-watch.backend-failed")
            .count(),
        1
    );
    let source = PressureSourceId::new("nara.asset-watch.queue").unwrap();
    let pressure = app
        .world()
        .resource::<RuntimePressureSnapshots>()
        .get(&source)
        .unwrap();
    assert!(pressure.measurements().iter().any(|measurement| {
        measurement.metric().as_str() == "backend-failures" && measurement.value() == 2
    }));
    remove_temp_root(&root);
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
        nara_app::AddPluginsError::Plugin(PluginError::SetupFailed { .. })
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
