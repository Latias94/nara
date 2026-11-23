use std::{
    fs,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara_engine::{
    ProductRecipe,
    advanced_prelude::CurrentScene,
    app::{
        App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    core::ItemLimit,
    ecs::{ResMut, Resource},
    fs::{DirectoryCapability, RelativePath},
    project::RuntimeConfiguration,
    project_host::{
        HeadlessSession, HeadlessSessionClose, ingest_project_manifest,
        open_explicit_product_project_root, resolve_product_recipe,
    },
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentLimits, CookedContentRequest,
    },
    runtime_content::{RuntimeContentBase, RuntimeContentError, RuntimeContentTarget},
    scene::{SceneDocument, SceneEntityId, SceneEntityRecord},
    tasks::{
        TaskCancellation, TaskCancellationReason, TaskCancellationToken, TaskCoalesceKey,
        TaskDomainKey, TaskFacilities, TaskHandle, TaskKindConfig, TaskOverloadPolicy,
        TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest, TaskStats, TaskTerminal,
    },
};

const WAIT: Duration = Duration::from_secs(5);
const GATE_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(1);
const REQUEST: TaskSpawnRequest = TaskSpawnRequest::new(1, TaskDomainKey::new(7)).with_overload(
    TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(11)),
);

#[derive(Default, Resource)]
struct FixedTicks(u64);

#[derive(Default)]
struct CountFixedTicks;

impl Plugin for CountFixedTicks {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.project-task-activity"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<FixedTicks>()?
            .add_systems(CoreStage::FixedUpdate, |mut ticks: ResMut<FixedTicks>| {
                ticks.0 += 1
            })?;
        Ok(())
    }
}

fn private_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_plugin::<CountFixedTicks>()
        .unwrap()
}

struct Project {
    root: DirectoryCapability,
    directory: tempfile::TempDir,
    configuration: RuntimeConfiguration,
    startup: AssetProductRef,
    facilities: Arc<TaskFacilities>,
    recipe: ProductRecipe,
}

impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        for folder in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(path.join(folder)).unwrap();
        }
        fs::write(
            path.join("nara.toml"),
            r#"schema_version = 1

[project]
name = "Shared Tasks consumer"
source_universe = "186c8d14-57a8-4f5a-8d3a-000000000099"
native_document_identity = "stable_metadata"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "minimal"

[capabilities]
requested = ["runtime-2d"]

[tasks.io]
workers = 2
pending_capacity = 4
[tasks.compute]
workers = 1
pending_capacity = 4
[tasks.async_compute]
workers = 1
pending_capacity = 4
"#,
        )
        .unwrap();
        let source = StableAssetId::parse_str("f1825d9c-94d8-46ad-8512-000000000001").unwrap();
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(counter_entity())]);
        let meta = AssetMeta::new(
            AssetMetaIdentity::new(
                source,
                AssetPath::new("startup.scene.json").unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            path.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            path.join("scenes/startup.scene.json.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();

        let root = open_explicit_product_project_root(path).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let configuration = candidate.runtime_configuration();
        let facilities =
            Arc::new(TaskFacilities::try_new_lazy(configuration.tasks().pool_config).unwrap());
        let recipe = private_recipe()
            .with_shared_task_facilities(Arc::clone(&facilities))
            .unwrap();
        let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        fs::write(
            path.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        Self {
            root,
            directory,
            configuration: configuration.to_configuration(),
            startup: AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap(),
            facilities,
            recipe,
        }
    }

    fn source_session(&self) -> HeadlessSession {
        HeadlessSession::new(self.root.share(), self.recipe.clone())
    }

    fn pack_session(&self, source: &HeadlessSession) -> HeadlessSession {
        let limits = CookedContentLimits::default();
        let mut builder = CookedContentBuilder::new(
            CookedContentRequest {
                target: RuntimeContentTarget::new("test-host", "project-task-activity").unwrap(),
                configuration: self.configuration.clone(),
                startup_scene: self.startup,
                scenes: vec![self.startup],
                addressable_roots: vec![],
            },
            self.recipe.clone(),
            limits,
        )
        .unwrap();
        source
            .observe(|world| {
                CurrentScene::get(world)
                    .unwrap()
                    .source_view()
                    .with_compiled_scene(|scene, resolution| {
                        builder.add_scene(self.startup, scene, resolution.unwrap())
                    })
                    .unwrap()
            })
            .unwrap()
            .unwrap();
        let cooked = builder.finish().unwrap();
        let pack_name = "content.pack";
        let path = RelativePath::new(pack_name).unwrap();
        let mut file = fs::File::create_new(self.directory.path().join(pack_name)).unwrap();
        cooked.base().write_pack(&mut file).unwrap();
        drop(file);
        let open_pack = |recipe| {
            RuntimeContentBase::open_pack(
                self.root.open_file(&path).unwrap(),
                cooked.expectation(),
                recipe,
                limits.runtime,
            )
        };
        assert!(matches!(
            open_pack(private_recipe()),
            Err(RuntimeContentError::RecipeMismatch)
        ));

        let config = *self.facilities.config();
        let different_config = config
            .with_kind(
                TaskPoolKind::Io,
                TaskKindConfig::new(
                    config.kind(TaskPoolKind::Io).workers(),
                    ItemLimit::new(5).unwrap(),
                ),
            )
            .unwrap();
        let different_facilities =
            Arc::new(TaskFacilities::try_new_lazy(different_config).unwrap());
        let different_recipe = private_recipe()
            .with_shared_task_facilities(different_facilities)
            .unwrap();
        assert!(matches!(
            open_pack(different_recipe),
            Err(RuntimeContentError::RecipeMismatch)
        ));

        let equivalent_facilities = Arc::new(TaskFacilities::try_new_lazy(config).unwrap());
        let equivalent_recipe = private_recipe()
            .with_shared_task_facilities(Arc::clone(&equivalent_facilities))
            .unwrap();
        let mut equivalent =
            HeadlessSession::from_runtime_content(open_pack(equivalent_recipe).unwrap());
        equivalent.start().unwrap();
        advance_tick(&mut equivalent, 1);
        close_cleanly(&mut equivalent);
        assert!(
            equivalent_facilities
                .shutdown_blocking()
                .unwrap()
                .is_complete()
        );

        HeadlessSession::from_runtime_content(open_pack(self.recipe.clone()).unwrap())
    }
}

fn counter_entity() -> SceneEntityId {
    SceneEntityId::new("counter").unwrap()
}

fn observed_ticks(session: &HeadlessSession) -> (u64, u64) {
    session
        .observe(|world| {
            (
                world.resource::<FixedTime>().tick(),
                world.resource::<FixedTicks>().0,
            )
        })
        .unwrap()
}

fn advance_tick(session: &mut HeadlessSession, expected: u64) {
    let timestep = session
        .observe(|world| world.resource::<FixedTime>().timestep())
        .unwrap();
    assert!(session.frame(timestep).unwrap().is_none());
    assert_eq!(observed_ticks(session), (expected, expected));
}

fn spawn_io<T: Send + 'static>(
    session: &HeadlessSession,
    task: impl FnOnce(TaskCancellationToken) -> T + Send + 'static,
) -> TaskHandle<T> {
    session
        .observe(|world| {
            world
                .resource::<TaskPools>()
                .spawn(TaskPoolKind::Io, REQUEST, task)
                .into_handle()
        })
        .unwrap()
        .unwrap()
}

fn hold_io<T: Send + 'static>(
    session: &HeadlessSession,
    result: T,
) -> (TaskHandle<T>, mpsc::Sender<()>) {
    let (started, start) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let task = spawn_io(session, move |_| {
        let _ = started.send(());
        assert_ne!(
            gate.recv_timeout(GATE_TIMEOUT),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
        result
    });
    start
        .recv_timeout(WAIT)
        .expect("held I/O task did not start");
    (task, release)
}

fn wait_until(description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready() {
        assert!(Instant::now() < deadline, "{description}");
        thread::sleep(POLL_INTERVAL);
    }
}

fn assert_io_completed(mut task: TaskHandle<u32>, expected: u32) {
    let returned = task.return_observer();
    wait_until("sibling I/O task did not return", || {
        returned.state() != TaskReturnState::Pending
    });
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert_eq!(task.try_take(), Some(TaskTerminal::Completed(expected)));
}

fn close_cleanly(session: &mut HeadlessSession) {
    wait_until("Headless session did not retire", || {
        match session.close() {
            HeadlessSessionClose::Pending => false,
            HeadlessSessionClose::Complete => true,
            HeadlessSessionClose::Failed => panic!("{:?}", session.diagnostics()),
        }
    });
    assert!(session.is_closed());
    assert!(
        !session.diagnostics().has_errors(),
        "{:?}",
        session.diagnostics()
    );
}

struct Activities<'a> {
    first: HeadlessSession,
    second: HeadlessSession,
    facilities: &'a TaskFacilities,
}

impl Drop for Activities<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            let deadline = Instant::now() + WAIT;
            for session in [&mut self.first, &mut self.second] {
                while Instant::now() < deadline && session.close() == HeadlessSessionClose::Pending
                {
                    thread::sleep(POLL_INTERVAL);
                }
            }
            let _ = self.facilities.shutdown_blocking();
        }
    }
}

fn assert_independent_activity(
    first: HeadlessSession,
    second: HeadlessSession,
    facilities: &TaskFacilities,
) {
    let mut sessions = Activities {
        first,
        second,
        facilities,
    };
    sessions.second.start().unwrap();
    let sibling_generation = sessions.second.runtime_generation().unwrap();
    assert_ne!(
        sessions.first.runtime_generation().unwrap(),
        sibling_generation
    );
    for session in [&sessions.first, &sessions.second] {
        session
            .observe(|world| {
                let scene = CurrentScene::get(world).unwrap();
                assert_eq!(scene.source().roots(), &[counter_entity()]);
            })
            .unwrap();
    }
    advance_tick(&mut sessions.first, 1);
    assert_eq!(observed_ticks(&sessions.second), (0, 0));
    assert_eq!(facilities.stats(), TaskStats::default());

    let (late_result, disposal) = mpsc::channel::<()>();
    let (mut first_task, release_first) = hold_io(&sessions.first, late_result);
    let first_return = first_task.return_observer();
    assert_eq!(first_return.state(), TaskReturnState::Pending);
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        2
    );

    assert_eq!(sessions.first.close(), HeadlessSessionClose::Pending);
    wait_until("the original scope did not cancel its running task", || {
        assert_eq!(sessions.first.close(), HeadlessSessionClose::Pending);
        first_task.is_finished()
    });
    assert!(matches!(
        first_task.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::ScopeClosed,
            before_start: false,
        }))
    ));
    assert_eq!(disposal.try_recv(), Err(mpsc::TryRecvError::Empty));

    advance_tick(&mut sessions.second, 1);
    let sibling_task = spawn_io(&sessions.second, |_| 29);
    assert_ne!(first_task.id(), sibling_task.id());
    assert_io_completed(sibling_task, 29);
    let (sibling_running, release_sibling) = hold_io(&sessions.second, 31);
    assert_eq!(first_return.state(), TaskReturnState::Pending);
    assert_eq!(sessions.first.close(), HeadlessSessionClose::Pending);

    release_first.send(()).unwrap();
    close_cleanly(&mut sessions.first);
    assert_eq!(first_return.state(), TaskReturnState::Returned);
    assert!(first_task.try_take().is_none());
    assert_eq!(disposal.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    assert_eq!(
        sibling_running.return_observer().state(),
        TaskReturnState::Pending
    );
    assert!(!sibling_running.is_finished());
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        2
    );

    assert_eq!(
        sessions.second.runtime_generation(),
        Some(sibling_generation)
    );
    advance_tick(&mut sessions.second, 2);
    release_sibling.send(()).unwrap();
    assert_io_completed(sibling_running, 31);
    assert_io_completed(spawn_io(&sessions.second, |_| 37), 37);
    close_cleanly(&mut sessions.second);

    let stats = facilities.stats();
    let io = stats.for_kind(TaskPoolKind::Io);
    assert_eq!(
        (io.admitted, io.started, io.completed, io.cancelled),
        (4, 4, 3, 1)
    );
    assert_eq!(io.shutdowns, 0);
    assert_eq!(io.live_workers, 2);
    for kind in [TaskPoolKind::Compute, TaskPoolKind::AsyncCompute] {
        assert_eq!(stats.for_kind(kind), Default::default());
    }
    let joined = facilities.shutdown_blocking().unwrap();
    assert!(joined.is_complete(), "{joined:?}");
    assert!(!joined.has_cleanup_failure(), "{joined:?}");
    assert!(!joined.timed_out(), "{joined:?}");
    for (kind, expected) in TaskPoolKind::ALL.into_iter().zip([2, 0, 0]) {
        assert_eq!(joined.for_kind(kind).joined_workers, expected, "{kind:?}");
        assert_eq!(joined.for_kind(kind).panicked_workers, 0, "{kind:?}");
        assert_eq!(
            facilities.stats().for_kind(kind).live_workers,
            0,
            "{kind:?}"
        );
    }
}

#[test]
fn source_sessions_retire_shared_task_activities_independently() {
    let project = Project::new();
    let mut first = project.source_session();
    first.start().unwrap();
    assert_independent_activity(first, project.source_session(), &project.facilities);
}

#[test]
fn source_and_pack_sessions_retire_shared_task_activities_independently() {
    let project = Project::new();
    let mut first = project.source_session();
    first.start().unwrap();
    let second = project.pack_session(&first);
    assert_independent_activity(first, second, &project.facilities);
}
