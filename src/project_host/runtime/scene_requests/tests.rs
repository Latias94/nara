use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use nara_core::ItemLimit;
use nara_ecs::bevy_ecs::system::SystemState;
use nara_fs::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode,
};
use nara_identity::SceneEntityId;
use nara_scene::SceneEntityRecord;
use nara_tasks::TaskPoolConfig;

#[cfg(target_os = "linux")]
use std::fs::File;

use super::{
    preparation::{TestGate, TestPhase},
    *,
};
use crate::project_host::{
    ProjectContentBudgetKind, ProjectContentBudgetSnapshot, ingest_project_manifest,
    resolve_product_recipe,
};
use crate::startup_scene::{CurrentScene, StartupSceneActivationPlugin, materialize_startup_scene};

#[test]
fn direct_world_without_project_owner_needs_no_tasks_or_current_scene() {
    let mut world = World::new();
    poll_project_scene_requests(&mut world);
    let mut access = SystemState::<ProjectScenes>::new(&mut world);
    let mut scenes = access.get_mut(&mut world).unwrap();
    assert_eq!(
        scenes.request(ProjectPath::new("second.scene.json").unwrap()),
        Err(ProjectSceneRequestError::Unavailable)
    );
    let result = ProjectScenes::scope(&mut world, |scenes| scenes.request(selected()));
    assert_eq!(result, Err(ProjectSceneRequestError::Unavailable));
    assert!(CurrentScene::get(&world).is_none());
    assert!(!world.contains_resource::<RequestOwner>());
    assert!(!world.contains_resource::<TaskPools>());
}

#[test]
fn scoped_requests_keep_cancelling_slot_until_physical_return_and_explicit_acknowledgement() {
    Fixture::new().with_world(|world| {
        assert_eq!(
            ProjectScenes::scope(world, |scenes| scenes.request(selected())),
            Err(ProjectSceneRequestError::TaskUnavailable)
        );
        world.insert_resource(pools());
        let (gate, control) = TestGate::new(TestPhase::AfterPreparation);
        world.resource_mut::<RequestOwner>().gate = Some(gate);
        let request = ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
        assert!(control.wait_until_started());
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Preparing)
            );
            assert!(scenes.prepared(request).is_none());
            assert!(!scenes.acknowledge(request));
            assert!(scenes.cancel(request));
        });
        let mut access = SystemState::<ProjectScenes>::new(world);
        {
            let scenes = access.get_mut(world).unwrap();
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Cancelling)
            );
        }
        for _ in 0..3 {
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Cancelling)
                );
                assert!(!scenes.cancel(request));
                assert!(!scenes.acknowledge(request));
                assert_eq!(
                    scenes.request(selected()),
                    Err(ProjectSceneRequestError::Busy)
                );
            });
        }
        control.release();
        wait_for_request(world, request, ProjectSceneRequestState::Cancelled);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Cancelled)
            );
            assert_eq!(
                scenes.request(selected()),
                Err(ProjectSceneRequestError::Busy)
            );
        });
        assert!(access.get_mut(world).unwrap().acknowledge(request));
        assert_eq!(
            ProjectScenes::scope(world, |scenes| scenes.state(request)),
            None
        );
        assert!(world.contains_resource::<RequestOwner>());
        assert!(CurrentScene::get(world).is_some());
    });
}

#[test]
fn scoped_prepared_inspection_preserves_source_receipt_pair_and_scheduled_stale_predicates() {
    Fixture::new().with_world(|world| {
        world.insert_resource(pools());
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        let source_view = CurrentScene::get(world).unwrap().source_view();
        let request = ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        let roots = ProjectScenes::scope(world, |scenes| {
            let prepared = scenes.prepared(request).unwrap();
            assert_eq!(prepared.selected_scene_path(), &selected());
            assert_eq!(prepared.expected_instance(), &expected);
            prepared.scene().roots.clone()
        });
        assert_eq!(roots, [SceneEntityId::new("second").unwrap()]);
        let current = CurrentScene::get(world).unwrap();
        assert_eq!(current.receipt(), &expected);
        assert_eq!(
            current.source().roots,
            [SceneEntityId::new("startup").unwrap()]
        );
        assert_eq!(
            source_view.with_document(|source| source as *const SceneDocument),
            Some(current.source() as *const SceneDocument)
        );

        let mut access = SystemState::<ProjectScenes>::new(world);
        {
            let scenes = access.get_mut(world).unwrap();
            assert_eq!(scenes.state(request), Some(ProjectSceneRequestState::Ready));
            let prepared = scenes.prepared(request).unwrap();
            assert_eq!(prepared.expected_instance(), &expected);
            assert_eq!(prepared.scene().roots, roots);
        }
        world
            .resource::<RequestOwner>()
            .closing
            .store(true, Ordering::Release);
        let stale = Some(ProjectSceneRequestState::Rejected(
            ProjectSceneRequestError::Stale,
        ));
        assert_eq!(access.get_mut(world).unwrap().state(request), stale);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(scenes.state(request), stale);
            assert!(scenes.prepared(request).is_none());
            assert!(scenes.acknowledge(request));
            assert_eq!(scenes.state(request), None);
        });
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
    });
}

#[test]
fn worker_prepares_selected_scene_and_refunds_rejected_content() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let pools = pools();
    let current = context
        .loader
        .load(&context.candidate, &context.plan)
        .unwrap();
    let baseline = context.loader.budget_snapshot();
    let mut preparation =
        Preparation::start(Arc::clone(&context), selected(), &pools, None).unwrap();
    let target = finish(&mut preparation).unwrap();
    assert_eq!(target.selected_scene_path(), &selected());
    assert_eq!(
        target.expanded_scene().roots,
        [SceneEntityId::new("second").unwrap()]
    );
    assert!(target.shares_component_bindings(context.plan.schema_validation().bindings()));
    assert_eq!(
        current.expanded_scene().roots,
        [SceneEntityId::new("startup").unwrap()]
    );
    assert_eq!(
        context.loader.budget_snapshot().active_generations(),
        baseline.active_generations() + 1
    );
    drop(target);
    assert_active_budget(&context, baseline);

    fs::write(
        fixture.root.join("scenes/second.scene.json"),
        b"{ malformed",
    )
    .unwrap();
    let expected = context
        .loader
        .load_scene(&context.candidate, &context.plan, &selected())
        .unwrap_err();
    let mut rejected = Preparation::start(Arc::clone(&context), selected(), &pools, None).unwrap();
    let failure = finish(&mut rejected).unwrap_err();
    assert_eq!(
        failure.error,
        ProjectSceneRequestError::Content(ProjectContentErrorKind::SceneFormat)
    );
    assert_eq!(&failure.diagnostics, expected.diagnostics());
    assert!(
        failure
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "project.content.scene-format-invalid")
    );
    assert_eq!(
        current.expanded_scene().roots,
        [SceneEntityId::new("startup").unwrap()]
    );
    assert_active_budget(&context, baseline);
    pools.shutdown_blocking().unwrap();
}

#[test]
fn cancellation_keeps_worker_and_prepared_generation_until_physical_return() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let pools = pools();
    let _current = context
        .loader
        .load(&context.candidate, &context.plan)
        .unwrap();
    let baseline = context.loader.budget_snapshot();
    for phase in [TestPhase::BeforeRead, TestPhase::AfterPreparation] {
        let (gate, control) = TestGate::new(phase);
        let mut preparation =
            Preparation::start(Arc::clone(&context), selected(), &pools, Some(gate)).unwrap();
        assert!(control.wait_until_started());
        preparation.cancel();
        for _ in 0..3 {
            assert!(
                preparation.poll().is_none(),
                "logical cancellation cannot release a held worker"
            );
        }
        if phase == TestPhase::AfterPreparation {
            assert_eq!(
                context.loader.budget_snapshot().active_generations(),
                baseline.active_generations() + 1
            );
        }
        control.release();
        assert!(
            matches!(finish(&mut preparation), Err(failure) if failure.error == ProjectSceneRequestError::TaskUnavailable)
        );
        assert_active_budget(&context, baseline);
    }
    pools.shutdown_blocking().unwrap();
}

#[test]
fn failed_worker_discards_its_prepared_snapshot_and_closed_pool_never_reads_inline() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let pools = pools();
    let _current = context
        .loader
        .load(&context.candidate, &context.plan)
        .unwrap();
    let baseline = context.loader.budget_snapshot();
    let (gate, control) = TestGate::panicking(TestPhase::AfterPreparation);
    let mut preparation =
        Preparation::start(Arc::clone(&context), selected(), &pools, Some(gate)).unwrap();
    assert!(control.wait_until_started());
    assert!(preparation.poll().is_none());
    control.release();
    assert!(
        matches!(finish(&mut preparation), Err(failure) if failure.error == ProjectSceneRequestError::TaskUnavailable)
    );
    assert_active_budget(&context, baseline);
    pools.shutdown_blocking().unwrap();
    assert!(matches!(
        Preparation::start(Arc::clone(&context), selected(), &pools, None),
        Err(ProjectSceneRequestError::TaskUnavailable)
    ));
    assert_active_budget(&context, baseline);
}

#[test]
fn inline_pool_rejects_scene_io_without_queuing_or_loading() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let pools = TaskPools::inline_for_tests(
        TaskPoolConfig::threaded(ItemLimit::ONE, ItemLimit::ONE, ItemLimit::ONE).unwrap(),
    )
    .unwrap();
    let baseline = context.loader.budget_snapshot();
    assert!(matches!(
        Preparation::start(Arc::clone(&context), selected(), &pools, None),
        Err(ProjectSceneRequestError::TaskUnavailable)
    ));
    assert_eq!(
        pools
            .stats()
            .for_kind(nara_tasks::TaskPoolKind::Io)
            .admitted,
        0
    );
    assert_eq!(pools.run_pending_for_tests().executed, 0);
    assert_active_budget(&context, baseline);
    pools.shutdown_blocking().unwrap();
}

fn selected() -> ProjectPath {
    ProjectPath::new("second.scene.json").unwrap()
}

fn pools() -> TaskPools {
    TaskPools::try_new(
        TaskPoolConfig::threaded(ItemLimit::ONE, ItemLimit::ONE, ItemLimit::ONE).unwrap(),
    )
    .unwrap()
}

fn finish(
    preparation: &mut Preparation,
) -> Result<ProjectContentSnapshot, preparation::PreparationFailure> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = preparation.poll() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "Scene preparation did not physically return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_request(
    world: &mut World,
    request: ProjectSceneRequestId,
    expected: ProjectSceneRequestState,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        poll_project_scene_requests(world);
        if ProjectScenes::scope(world, |scenes| scenes.state(request)) == Some(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Scene request did not reach {expected:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_active_budget(context: &Context, expected: ProjectContentBudgetSnapshot) {
    let actual = context.loader.budget_snapshot();
    assert_eq!(actual.active_generations(), expected.active_generations());
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(actual.active(kind), expected.active(kind), "{kind:?}");
    }
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn with_world(&self, inspect: impl FnOnce(&mut World) + Send + Sync + 'static) {
        let context = self.context();
        let snapshot = context
            .loader
            .load(&context.candidate, &context.plan)
            .unwrap();
        let inspect = Arc::new(Mutex::new(Some(inspect)));
        let inspection = Arc::clone(&inspect);
        let native = Arc::new(OnceLock::new());
        assert!(
            native
                .set(context.plan.schema_validation().snapshot().clone())
                .is_ok()
        );
        let mut app = App::new();
        app.add_plugins(nara_reflect::preloaded_component_registry_plugin(native))
            .unwrap()
            .add_plugins((
                nara_scene::SceneComponentsPlugin,
                nara_hierarchy::HierarchyPlugin,
                nara_transform::TransformPlugin,
                nara_render::RenderComponentsPlugin,
                nara_sprite::SpritePlugin,
                nara_tilemap::TilemapPlugin,
                nara_scene::SceneComponentBindingsPlugin::new(
                    context
                        .plan
                        .schema_validation()
                        .bindings()
                        .data_owners()
                        .iter()
                        .cloned(),
                ),
                StartupSceneActivationPlugin,
            ))
            .unwrap()
            .add_systems(nara_app::CoreStage::Update, move |world: &mut World| {
                if let Some(inspect) = inspection.lock().unwrap().take() {
                    inspect(world);
                    if let Some(pools) = world.get_resource::<TaskPools>() {
                        pools.shutdown_blocking().unwrap();
                    }
                }
            })
            .unwrap();
        let mut candidate = nara_app::RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                nara_app::RuntimeObligationLedger::new(),
                nara_app::RuntimeClosePolicy::default(),
            )
            .unwrap();
        candidate
            .with_admission_scope(move |scope| {
                scope.apply_command(move |world: &mut World| {
                    materialize_startup_scene(world, snapshot.scene_source()).unwrap();
                    let closing = Arc::clone(&world.resource::<Closing>().0);
                    world.insert_resource(RequestOwner {
                        context,
                        closing,
                        slot: None,
                        gate: None,
                    });
                });
            })
            .map(|outcome| assert!(outcome.post_operation_fault.is_none()))
            .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        runtime.drive(Duration::ZERO).unwrap();
        assert!(inspect.lock().unwrap().is_none());
        let mut retirement = runtime.begin_retirement();
        assert_eq!(
            retirement.drive_retirement(),
            nara_app::RuntimeCandidateRetirementState::Retired
        );
    }

    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_scene_request_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for child in ["assets", "scenes", "prefabs"] {
            fs::create_dir(root.join(child)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1

[project]
source_universe = '93f754e6-cbaf-5562-92f0-68ba1e50f6c8'
native_document_identity = 'project_location'
name = "Scene Request"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[content]
scenes = ["second.scene.json"]
[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        for (path, entity) in [
            ("startup.scene.json", "startup"),
            ("second.scene.json", "second"),
        ] {
            let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new(entity).unwrap(),
            )]);
            fs::write(
                root.join("scenes").join(path),
                scene.to_json_string().unwrap(),
            )
            .unwrap();
        }
        Self { root }
    }

    fn context(&self) -> Arc<Context> {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0x1 | 0x2 | 0x4)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let directory = File::open(&self.root).unwrap();
        let root = DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap();
        let candidate = ingest_project_manifest(
            &root
                .open_file(&RelativePath::new("nara.toml").unwrap())
                .unwrap(),
            None,
        )
        .unwrap();
        let plan = resolve_product_recipe(&candidate, crate::ProductRecipe::new()).unwrap();
        Arc::new(Context {
            candidate,
            plan,
            loader: Arc::new(ProjectContentLoader::new(root).unwrap()),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
