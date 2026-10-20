use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara_core::ItemLimit;
use nara_ecs::system::SystemState;
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
    assert!(target.shares_schema_snapshot(context.plan.schema_validation().snapshot()));
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
