use super::*;

#[path = "activity_scopes/comparison.rs"]
mod comparison;

use nara_app::{
    RuntimeClosePolicy, RuntimeControl, RuntimeControlFailure, RuntimeControlRequestResult,
    RuntimeControlStatus, RuntimeDriveError, RuntimeInstance, RuntimeObligationLedger,
    RuntimeState,
};
use nara_tasks::{
    TaskFacilities, TaskPlugin, TaskReturnState, TaskScopeCloseReport, TaskShutdownReport,
};

fn shared_image_app(
    facilities: &Arc<TaskFacilities>,
    root: &Path,
    record: &AssetRecord,
    mut server: AssetServer,
    initial_pixels: &[u8; 4],
) -> (App, Handle<ImageAsset>) {
    let mut app = App::new();
    // Register the managed scope before installing any task facade or consumer.
    app.add_plugin(TaskPlugin::shared(Arc::clone(facilities)))
        .unwrap();
    app.init_resource::<ImagePipelinePort>().unwrap();
    app.__register_driver_port::<ImagePipelinePort>(ImagePipelinePort::apply_driver_input_in_world)
        .unwrap();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_asset::AssetPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        ImagePreparePlugin,
    ))
    .unwrap();
    app.add_plugin(ImagePlugin::default().with_source_directory(image_source_directory(root)))
        .unwrap();

    let handle = server.reserve_record::<ImageAsset>(record).unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    app.world()
        .resource::<ImageImporter>()
        .import_image(
            bytes_request(record, &rgba_png(1, 1, initial_pixels)),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap()
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    events.drain();
    let world = app.world_mut().unwrap();
    world.insert_resource(server);
    world.insert_resource(images);
    world.insert_resource(states);
    world.insert_resource(events);
    world
        .resource_mut::<nara_asset::ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    (app, handle)
}

fn drive_until(
    runtime: &mut RuntimeInstance,
    ready: impl Fn(&RuntimeInstance) -> bool,
) -> Result<bool, RuntimeDriveError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(runtime) && Instant::now() < deadline {
        runtime.drive(Duration::ZERO)?;
        thread::sleep(Duration::from_millis(1));
    }
    Ok(ready(runtime))
}

fn assert_reloaded_image(
    runtime: &RuntimeInstance,
    handle: Handle<ImageAsset>,
    pixels: &[u8; 4],
    initial_version: AssetVersion,
    applied: u32,
) {
    let world = runtime.world();
    let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
    assert_eq!(image.pixels(), pixels);
    let states = world.resource::<AssetStates>();
    assert_eq!(
        states.version(handle.id()).unwrap().raw(),
        initial_version.raw() + u64::from(applied)
    );
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loaded
    );
    let stats = world.resource::<ImageReloadStats>();
    assert_eq!(stats.spawned, applied);
    assert_eq!(stats.applied, applied);
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.rejected, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.cancelled, 0);
    assert_eq!(stats.stale, 0);
    assert_eq!(stats.removed, 0);
}

#[test]
fn shared_image_scopes_keep_same_asset_key_and_result_application_independent() {
    let root = unique_temp_root();
    let first_root = root.join("first");
    let sibling_root = root.join("sibling");
    let record = image_record("textures/player.png");
    let first_pixels = [0, 255, 0, 255];
    let sibling_pixels = [0, 0, 255, 255];
    for (directory, pixels) in [(&first_root, first_pixels), (&sibling_root, sibling_pixels)] {
        fs::create_dir_all(directory.join("textures")).unwrap();
        fs::write(
            directory.join(record.path().as_str()),
            rgba_png(1, 1, &pixels),
        )
        .unwrap();
    }
    let facilities = Arc::new(TaskFacilities::inline_for_tests(bounded_task_config(1, 4)).unwrap());
    // Separate reservations would allocate different process-local IDs and miss key isolation.
    let mut server = AssetServer::new();
    server.reserve_record::<ImageAsset>(&record).unwrap();
    let (first_app, first_handle) = shared_image_app(
        &facilities,
        &first_root,
        &record,
        server.clone(),
        &[255, 0, 0, 255],
    );
    let (sibling_app, sibling_handle) = shared_image_app(
        &facilities,
        &sibling_root,
        &record,
        server,
        &[255, 255, 0, 255],
    );
    assert_eq!(first_handle.id(), sibling_handle.id());
    let mut first = RuntimeInstance::start(first_app).unwrap();
    let mut sibling = RuntimeInstance::start(sibling_app).unwrap();
    let first_version = first
        .world()
        .resource::<AssetStates>()
        .version(first_handle.id())
        .unwrap();
    let sibling_version = sibling
        .world()
        .resource::<AssetStates>()
        .version(sibling_handle.id())
        .unwrap();

    // Both actual Image jobs stay queued with the same domain and AssetId-derived coalesce key.
    for runtime in [&mut first, &mut sibling] {
        edit_image_pipeline(
            runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: AssetSourceChangeKind::Modified,
            },
        );
        runtime.drive(Duration::ZERO).unwrap();
        let stats = runtime.world().resource::<ImageReloadStats>();
        assert_eq!((stats.spawned, stats.pending, stats.applied), (1, 1, 0));
    }
    let queued = facilities.stats().for_kind(TaskPoolKind::Io);
    assert_eq!(queued.queued, 2);
    assert_eq!(queued.coalesced, 0);
    assert_eq!(queued.cancelled, 0);
    assert_eq!(facilities.run_pending_for_tests().executed, 2);

    // Poll the later submission first: its scope cutoff must accept only its own result.
    sibling.drive(Duration::ZERO).unwrap();
    assert_reloaded_image(
        &sibling,
        sibling_handle,
        &sibling_pixels,
        sibling_version,
        1,
    );
    assert_eq!(first.world().resource::<ImageReloadStats>().applied, 0);
    assert_eq!(
        first
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(first_handle)
            .unwrap()
            .pixels(),
        &[255, 0, 0, 255]
    );
    assert_eq!(
        first
            .world()
            .resource::<AssetStates>()
            .version(first_handle.id()),
        Some(first_version)
    );
    first.drive(Duration::ZERO).unwrap();
    assert_reloaded_image(&first, first_handle, &first_pixels, first_version, 1);
    assert_ne!(
        first
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(first_handle)
            .unwrap()
            .source()
            .source_hash(),
        sibling
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(sibling_handle)
            .unwrap()
            .source()
            .source_hash()
    );

    for runtime in [&mut first, &mut sibling] {
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        assert!(drive_until(runtime, |runtime| runtime.state() == RuntimeState::Stopped).unwrap());
        let report = runtime.world().resource::<TaskScopeCloseReport>();
        assert!(report.is_complete());
        assert!(!report.cleanup_failed);
        assert!(!runtime.world().contains_resource::<TaskPools>());
        assert!(!runtime.world().contains_resource::<TaskShutdownReport>());
    }
    first.finish().unwrap();
    sibling.finish().unwrap();
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
    remove_temp_root(&root);
}

#[test]
fn incomplete_image_runtime_stop_preserves_sibling_reload_and_shared_workers() {
    let root = unique_temp_root();
    let first_root = root.join("first");
    let sibling_root = root.join("sibling");
    let record = image_record("textures/player.png");
    let first_pixels = [0, 255, 0, 255];
    let sibling_pixels = [0, 0, 255, 255];
    for (directory, pixels) in [(&first_root, first_pixels), (&sibling_root, sibling_pixels)] {
        fs::create_dir_all(directory.join("textures")).unwrap();
        fs::write(
            directory.join(record.path().as_str()),
            rgba_png(1, 1, &pixels),
        )
        .unwrap();
    }
    // One IO worker stays runnable. This does not promise fairness under total saturation.
    let facilities = Arc::new(TaskFacilities::try_new(bounded_task_config(2, 4)).unwrap());
    let live_workers = || {
        let stats = facilities.stats();
        TaskPoolKind::ALL
            .into_iter()
            .map(|kind| stats.for_kind(kind).live_workers)
            .sum::<usize>()
    };
    assert_eq!(live_workers(), 4);
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        2
    );
    let mut server = AssetServer::new();
    server.reserve_record::<ImageAsset>(&record).unwrap();
    let (first_app, first_handle) = shared_image_app(
        &facilities,
        &first_root,
        &record,
        server.clone(),
        &[255, 0, 0, 255],
    );
    let (sibling_app, sibling_handle) = shared_image_app(
        &facilities,
        &sibling_root,
        &record,
        server,
        &[255, 255, 0, 255],
    );
    assert_eq!(first_handle.id(), sibling_handle.id());
    let mut first = RuntimeInstance::start_with_options(
        first_app,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::new(Duration::from_millis(30)),
    )
    .unwrap();
    let mut sibling = RuntimeInstance::start(sibling_app).unwrap();
    let first_version = first
        .world()
        .resource::<AssetStates>()
        .version(first_handle.id())
        .unwrap();
    let sibling_version = sibling
        .world()
        .resource::<AssetStates>()
        .version(sibling_handle.id())
        .unwrap();
    edit_image_pipeline(
        &mut first,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    assert!(
        drive_until(&mut first, |runtime| runtime
            .world()
            .resource::<ImageReloadStats>()
            .applied
            == 1)
        .unwrap()
    );
    assert_reloaded_image(&first, first_handle, &first_pixels, first_version, 1);

    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut held = first
        .world()
        .resource::<TaskPools>()
        .spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(0, IMAGE_RELOAD_TASK_DOMAIN),
            move |_cancellation| {
                let _ = entered.send(());
                let _ = released.recv_timeout(Duration::from_secs(30));
            },
        )
        .into_handle()
        .unwrap();
    // Dropping the sender on an assertion/drive failure also unblocks the worker.
    let entered = started.recv_timeout(Duration::from_secs(5));
    let stop = first.request_control(RuntimeControl::Stop);
    let incomplete = drive_until(&mut first, |runtime| {
        runtime.state() == RuntimeState::CloseIncomplete
    });

    edit_image_pipeline(
        &mut sibling,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    let reloaded = drive_until(&mut sibling, |runtime| {
        runtime.world().resource::<ImageReloadStats>().applied == 1
    });
    let scope_report = first
        .world()
        .get_resource::<TaskScopeCloseReport>()
        .copied();
    let return_while_reloading = held.return_observer().state();
    let terminal_while_held = held.is_finished();
    let workers_while_held = live_workers();
    let physical_report_absent = !first.world().contains_resource::<TaskShutdownReport>();
    // Release uncooperative work before checking observations that can panic.
    let released = release.send(());

    entered.unwrap();
    released.unwrap();
    assert!(incomplete.unwrap());
    assert!(
        reloaded.unwrap(),
        "sibling Image reload must finish before the held task is released"
    );
    let RuntimeControlRequestResult::Accepted(stop_ticket) = stop else {
        panic!("Stop was not accepted: {stop:?}");
    };
    assert_eq!(
        first.control_status(stop_ticket),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::CloseIncomplete
        ))
    );
    let scope_report = scope_report.unwrap();
    assert_eq!(scope_report.pending, 1);
    assert!(!scope_report.cleanup_failed);
    assert_eq!(return_while_reloading, TaskReturnState::Pending);
    assert!(
        terminal_while_held,
        "cancellation publication is not physical return"
    );
    assert_eq!(workers_while_held, 4);
    assert!(physical_report_absent);
    assert_eq!(sibling.state(), RuntimeState::Running);
    assert_reloaded_image(
        &sibling,
        sibling_handle,
        &sibling_pixels,
        sibling_version,
        1,
    );

    assert!(matches!(
        first.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert!(
        drive_until(&mut first, |runtime| runtime.state()
            == RuntimeState::Stopped)
        .unwrap()
    );
    let report = first.world().resource::<TaskScopeCloseReport>();
    assert!(report.is_complete());
    assert!(!report.cleanup_failed);
    assert_eq!(held.return_observer().state(), TaskReturnState::Returned);
    assert!(matches!(
        held.try_take(),
        Some(nara_tasks::TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::ScopeClosed,
            before_start: false,
        }))
    ));
    assert!(!first.world().contains_resource::<TaskPools>());
    assert!(!first.world().contains_resource::<TaskShutdownReport>());
    assert_reloaded_image(&first, first_handle, &first_pixels, first_version, 1);
    first.finish().unwrap();
    assert_eq!(
        live_workers(),
        4,
        "retiring one Runtime must not join shared workers"
    );

    let next_pixels = [120, 40, 220, 255];
    fs::write(
        sibling_root.join(record.path().as_str()),
        rgba_png(1, 1, &next_pixels),
    )
    .unwrap();
    edit_image_pipeline(
        &mut sibling,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    assert!(
        drive_until(&mut sibling, |runtime| runtime
            .world()
            .resource::<ImageReloadStats>()
            .applied
            == 2)
        .unwrap()
    );
    assert_reloaded_image(&sibling, sibling_handle, &next_pixels, sibling_version, 2);
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).coalesced, 0);
    assert!(matches!(
        sibling.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert!(
        drive_until(&mut sibling, |runtime| runtime.state()
            == RuntimeState::Stopped)
        .unwrap()
    );
    let report = sibling.world().resource::<TaskScopeCloseReport>();
    assert!(report.is_complete());
    assert!(!report.cleanup_failed);
    assert!(!sibling.world().contains_resource::<TaskShutdownReport>());
    sibling.finish().unwrap();
    assert_eq!(
        live_workers(),
        4,
        "scope completion is not physical facility shutdown"
    );

    let physical: TaskShutdownReport = facilities.shutdown_blocking().unwrap();
    assert!(physical.is_complete());
    assert!(!physical.has_cleanup_failure());
    assert_eq!(
        TaskPoolKind::ALL
            .into_iter()
            .map(|kind| physical.for_kind(kind).joined_workers)
            .sum::<usize>(),
        4
    );
    assert_eq!(live_workers(), 0);
    remove_temp_root(&root);
}
