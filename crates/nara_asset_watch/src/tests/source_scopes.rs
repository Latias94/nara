use super::*;

fn scoped_root(root: &Path, prefix: &str) -> AssetSourceRoot {
    AssetSourceRoot::new(
        root,
        AssetPathScope::under(AssetSourcePath::new(prefix).unwrap()),
    )
}

#[test]
fn real_file_changes_keep_same_name_roots_independent_in_nested_scopes() {
    let root = temp_root();
    for (native, prefix) in [
        ("first", "content/first/images"),
        ("second", "content/second/images"),
    ] {
        let native_root = root.join(native);
        fs::create_dir_all(native_root.join("textures")).unwrap();
        let bound = scoped_root(&native_root, prefix);
        let source = native_root.join("textures/player.png");
        let sidecar = native_root.join("textures/player.png.meta");
        fs::write(&source, b"first source").unwrap();
        fs::write(&source, b"modified source").unwrap();
        fs::write(&sidecar, b"metadata").unwrap();
        let canonical = AssetSourcePath::new(format!("{prefix}/textures/player.png")).unwrap();
        let translator = AssetWatchTranslator;
        for (event, kind) in [
            (
                AssetWatchEvent::modified(&source),
                AssetSourceChangeKind::Modified,
            ),
            (
                AssetWatchEvent::modified(&sidecar),
                AssetSourceChangeKind::MetaModified,
            ),
        ] {
            assert_eq!(
                translator.translate_event(&bound, &event).unwrap(),
                [AssetSourceChange::new(canonical.clone(), kind)]
            );
        }
        fs::remove_file(&source).unwrap();
        assert_eq!(
            translator
                .translate_event(&bound, &AssetWatchEvent::removed(&source))
                .unwrap(),
            [AssetSourceChange::new(
                canonical.clone(),
                AssetSourceChangeKind::Removed
            )]
        );
        fs::remove_file(&sidecar).unwrap();
        assert_eq!(
            translator
                .translate_event(&bound, &AssetWatchEvent::removed(&sidecar))
                .unwrap(),
            [AssetSourceChange::new(
                canonical,
                AssetSourceChangeKind::Removed
            )]
        );
    }
    remove_temp_root(&root);
}

#[test]
fn real_file_and_sidecar_renames_map_both_ends_without_losing_atomic_save_order() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let bound = scoped_root(&root, "catalog/selected/images");
    let translator = AssetWatchTranslator;
    let old_path = AssetSourcePath::new("catalog/selected/images/textures/old.png").unwrap();
    let new_path = AssetSourcePath::new("catalog/selected/images/textures/new.png").unwrap();
    let mut changes = AssetSourceChanges::new();
    for suffix in [".png", ".png.meta"] {
        let from = root.join(format!("textures/old{suffix}"));
        let to = root.join(format!("textures/new{suffix}"));
        fs::write(&from, b"observed file").unwrap();
        fs::rename(&from, &to).unwrap();
        let translated = translator
            .translate_event(&bound, &AssetWatchEvent::renamed(&from, &to))
            .unwrap();
        assert_eq!(
            translated,
            [
                AssetSourceChange::new(old_path.clone(), AssetSourceChangeKind::Removed),
                AssetSourceChange::new(
                    new_path.clone(),
                    if suffix.ends_with(".meta") {
                        AssetSourceChangeKind::MetaModified
                    } else {
                        AssetSourceChangeKind::Modified
                    },
                ),
            ]
        );
        changes.try_extend(translated).unwrap();
    }
    let source = root.join("textures/new.png");
    let sidecar = root.join("textures/new.png.meta");
    fs::remove_file(&source).unwrap();
    let removed = translator
        .translate_event(&bound, &AssetWatchEvent::removed(&source))
        .unwrap();
    fs::write(&sidecar, b"changed metadata after removal").unwrap();
    let metadata = translator
        .translate_event(&bound, &AssetWatchEvent::modified(&sidecar))
        .unwrap();
    changes
        .try_extend(removed.into_iter().chain(metadata))
        .unwrap();
    let before_recreation = changes.drain_coalesced();
    assert_eq!(
        before_recreation
            .iter()
            .map(|change| (change.path(), change.kind(), change.metadata_dirty()))
            .collect::<Vec<_>>(),
        [
            (&new_path, AssetSourceChangeKind::Removed, true),
            (&old_path, AssetSourceChangeKind::Removed, false),
        ]
    );
    // Metadata edits do not resurrect a removed file; source recreation retains that dirtiness.
    changes.try_extend(before_recreation).unwrap();
    fs::write(&source, b"atomic replacement").unwrap();
    let modified = translator
        .translate_event(&bound, &AssetWatchEvent::modified(&source))
        .unwrap();
    changes.try_extend(modified).unwrap();
    let coalesced = changes.drain_coalesced();
    assert_eq!(
        coalesced
            .iter()
            .map(|change| (change.path(), change.kind(), change.metadata_dirty()))
            .collect::<Vec<_>>(),
        [
            (&new_path, AssetSourceChangeKind::MetaModified, true),
            (&old_path, AssetSourceChangeKind::Removed, false),
        ]
    );
    remove_temp_root(&root);
}

#[test]
fn lookalike_native_directory_does_not_enter_the_bound_logical_scope() {
    let root = temp_root();
    let admitted = root.join("images");
    let outside = root.join("images-extra");
    fs::create_dir_all(&admitted).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let source = outside.join("player.png");
    fs::write(&source, b"outside").unwrap();
    assert!(matches!(
        AssetWatchTranslator.translate_event(
            &scoped_root(&admitted, "content/images"),
            &AssetWatchEvent::modified(&source),
        ),
        Err(AssetWatchError::SourceOutsideRoot { .. })
    ));
    remove_temp_root(&root);
}

#[test]
fn plugin_rejects_a_different_scope_for_the_same_native_directory() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let bound = scoped_root(&root, "content/first/images");
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.insert_resource(bound.clone()).unwrap();
    let different_scope =
        AssetPathScope::under(AssetSourcePath::new("content/second/images").unwrap());
    assert!(matches!(
        app.add_plugin(AssetWatchPlugin::new(&root, different_scope)),
        Err(nara_app::AddPluginsError::Plugin(ref failure))
            if matches!(failure.error(), PluginError::SetupFailed { .. })
    ));
    assert_eq!(app.world().resource::<AssetSourceRoot>(), &bound);
    assert!(!app.world().contains_resource::<AssetWatcher>());
    assert!(!app.world().contains_resource::<AssetWatchEventQueue>());
    drop(app);
    remove_temp_root(&root);
}

#[test]
fn rescan_receipt_refuses_a_rebound_scope_and_retires_its_native_callback() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let bound = scoped_root(&root, "content/first/images");
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(&root, bound.scope().clone()))
        .unwrap();
    app.world()
        .resource::<AssetWatchEventQueue>()
        .sender()
        .record_backend_failure();
    let mut poll = nara_ecs::schedule::Schedule::default();
    poll.add_systems(drain_asset_watch_events);
    let world = app.world_mut().unwrap();
    poll.run(world);
    let receipt = AssetWatchRescan::begin(world, AssetWatchQueueLimits::default()).unwrap();
    let callback = rescan_callback_lifetime(world);
    let sender = rescan_sender(world);
    world.insert_resource(scoped_root(&root, "content/second/images"));
    assert_eq!(
        receipt.complete(world),
        Err(AssetWatchError::RescanRequired)
    );
    assert_callback_retired(callback);
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert_eq!(
        sender.try_send(AssetWatchEvent::modified("after-scope-change.png")),
        Err(AssetWatchQueueSendError::Disconnected)
    );
    drop(app);
    remove_temp_root(&root);
}

#[test]
fn running_watcher_refuses_to_reinterpret_queued_events_after_scope_rebinding() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("image.png");
    fs::write(&source, b"actual source").unwrap();
    let bound = scoped_root(&root, "content/first/images");
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(&root, bound.scope().clone()))
        .unwrap();
    let callback = app.world().resource::<AssetWatcher>().callback_lifetime();
    app.world()
        .resource::<AssetWatchEventQueue>()
        .sender()
        .try_send(AssetWatchEvent::modified(&source))
        .unwrap();
    let mut poll = nara_ecs::schedule::Schedule::default();
    poll.add_systems(drain_asset_watch_events);
    let world = app.world_mut().unwrap();
    world.insert_resource(scoped_root(&root, "content/second/images"));
    poll.run(world);
    assert!(world.resource::<AssetSourceChanges>().is_empty());
    assert!(
        world
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert!(!world.resource::<AssetWatcher>().is_running());
    assert_callback_retired(callback);
    poll.run(world);
    assert_eq!(watch_pressure_value(world, "translation-failures"), Some(1));
    drop(app);
    remove_temp_root(&root);
}

#[test]
fn canonical_prefix_bytes_remain_subject_to_downstream_change_admission() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("image.png");
    fs::write(&source, b"source").unwrap();
    let bound = scoped_root(&root, "content/selected/long-image-directory");
    let limits = AssetSourceChangeLimits::new(ItemLimit::ONE, ByteLimit::new(16).unwrap());
    let mut local = AssetSourceChanges::with_limits(limits);
    local
        .modified(AssetSourcePath::new("image.png").unwrap())
        .unwrap();
    let queue = AssetWatchEventQueue::new();
    queue
        .sender()
        .try_send(AssetWatchEvent::modified(&source))
        .unwrap();
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.insert_resource(bound).unwrap();
    app.insert_resource(AssetSourceChanges::with_limits(limits))
        .unwrap();
    install_watch_runtime(&mut app, queue);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<AssetSourceChanges>().is_empty());
    assert!(
        runtime
            .world()
            .resource::<AssetWatchRuntimeStatus>()
            .requires_rescan()
    );
    assert_eq!(
        watch_pressure_value(runtime.world(), "source-admission-failures"),
        Some(1)
    );
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn native_file_observation_maps_the_selected_scope_or_requires_complete_reconciliation() {
    let root = temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let source = root.join("textures/player.png");
    let sidecar = root.join("textures/player.png.meta");
    fs::write(&source, b"initial source").unwrap();
    fs::write(&sidecar, b"initial metadata").unwrap();
    let bound = scoped_root(&root, "packages/selected/image-sources");
    let mut app = App::new();
    install_asset_prerequisites(&mut app);
    app.add_plugin(AssetWatchPlugin::new(&root, bound.scope().clone()))
        .unwrap();
    assert_eq!(app.world().resource::<AssetSourceRoot>(), &bound);
    let callback = app.world().resource::<AssetWatcher>().callback_lifetime();
    let mut poll = nara_ecs::schedule::Schedule::default();
    poll.add_systems(drain_asset_watch_events);
    'observations: for (file, kind) in [
        (&source, AssetSourceChangeKind::Modified),
        (&sidecar, AssetSourceChangeKind::MetaModified),
    ] {
        fs::write(file, b"actual native filesystem modification").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let world = app.world_mut().unwrap();
            poll.run(world);
            if world
                .resource::<AssetWatchRuntimeStatus>()
                .requires_rescan()
            {
                assert!(!world.resource::<AssetWatcher>().is_running());
                assert!(world.resource::<AssetSourceChanges>().is_empty());
                break 'observations;
            }
            let changes = world.resource_mut::<AssetSourceChanges>().drain_coalesced();
            if changes.iter().any(|change| {
                change.path().as_str() == "packages/selected/image-sources/textures/player.png"
                    && (change.kind() == kind
                        || (kind == AssetSourceChangeKind::MetaModified && change.metadata_dirty()))
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native watcher did not observe a real file write"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    nara_app::RuntimeInstance::start(app)
        .unwrap()
        .finish()
        .unwrap();
    assert_callback_retired(callback);
    remove_temp_root(&root);
}
