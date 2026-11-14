use std::{
    sync::{Arc, Mutex, OnceLock, atomic::AtomicUsize},
    time::{Duration, Instant},
};

use nara_app::{
    PluginDefinition, PluginDefinitionId, PluginPlanError, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeGeneration,
    RuntimeObligationLedger,
};
use nara_asset_watch::{AssetWatchEvent, AssetWatchPlugin, AssetWatchQueueSendError};
use nara_ecs::bevy_ecs::{
    error::BevyError,
    prelude::{On, Remove},
};
use nara_fs::RelativePath;
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::SceneDocument;

use super::*;
use crate::{
    ProductRecipe,
    project_host::{
        RuntimePlan, RuntimePlanError,
        advanced::{built_in_schema_providers, project_runtime_plugins, resolve_runtime_plan},
        ingest_project_manifest, resolve_product_recipe, resolve_runtime_configuration,
        runtime::{
            CleanupDriveOutcome, HostFault, ProjectHost, ProjectHostSlot,
            verify_processed_runtime_plan,
        },
        runtime_agreement::prepare_runtime_schema_agreement,
    },
    startup_scene::CurrentScene,
};

#[test]
fn plugin_build_acquires_no_native_worker_and_unbound_startup_refuses() {
    let project = ProjectFixture::new();
    let plan = project.plan(watch_recipe(project.configuration()));
    let candidate = plan
        .plugin_plan()
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let driver = candidate.world().resource::<SourceWatchDriver>();
    let observer = driver.retirement();
    assert_eq!(driver.status().state, ProjectSceneWatchState::Unbound);
    assert_eq!(driver.status().native, AssetWatchLifecycleStats::default());
    assert!(!driver.status().rescan_pending);
    let mut failure = candidate.complete_startup().err().unwrap();
    assert_eq!(
        failure.fault().detail().unwrap().code(),
        "project.scene.watch-source-host-required"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while failure.drive_retirement() != RuntimeCandidateRetirementState::Retired {
        assert!(
            Instant::now() < deadline,
            "unbound Runtime cleanup did not finish"
        );
        std::thread::yield_now();
    }
    assert_eq!(observer.stats(), AssetWatchLifecycleStats::default());
}

#[test]
fn processed_runtime_plan_rejects_source_watch_before_its_factory_runs() {
    let project = ProjectFixture::new();
    let candidate = project.candidate();
    let ordinary =
        resolve_runtime_configuration(candidate.runtime_configuration(), ProductRecipe::new())
            .unwrap();
    verify_processed_runtime_plan(&ordinary).unwrap();
    let factories = Arc::new(AtomicUsize::new(0));
    let factory_count = Arc::clone(&factories);
    let recipe = ProductRecipe::new()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(
            project.configuration(),
            move |configuration| {
                factory_count.fetch_add(1, Ordering::Relaxed);
                configuration.clone()
            },
        )
        .unwrap();
    let plan = resolve_runtime_configuration(candidate.runtime_configuration(), recipe).unwrap();
    assert_eq!(factories.load(Ordering::Relaxed), 0);
    let failure = verify_processed_runtime_plan(&plan).unwrap_err();
    let diagnostic = failure.diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "project.run.source-execution-not-allowed"
    );
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "plugin"
            && matches!(
                field.value(),
                nara_diagnostic::DiagnosticValueRef::Identifier(value)
                    if value == PROJECT_SCENE_WATCH_PLUGIN_ID.as_str()
            )
    }));
    assert_eq!(factories.load(Ordering::Relaxed), 0);
}

#[test]
fn stock_generic_route_conflicts_during_recipe_resolution_before_native_acquisition() {
    let project = ProjectFixture::new();
    let factories = Arc::new(AtomicUsize::new(0));
    let scene_count = Arc::clone(&factories);
    let stock_count = Arc::clone(&factories);
    let recipe = ProductRecipe::new()
        .add_plugin::<nara_asset::AssetSourcePlugin>()
        .unwrap()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(
            project.configuration(),
            move |configuration| {
                scene_count.fetch_add(1, Ordering::Relaxed);
                configuration.clone()
            },
        )
        .unwrap()
        .add_configured_plugin::<AssetWatchPlugin, _, _>(
            project.configuration(),
            move |configuration| {
                stock_count.fetch_add(1, Ordering::Relaxed);
                AssetWatchPlugin::new(configuration.root.clone(), AssetPathScope::ROOT)
            },
        )
        .unwrap();
    assert!(matches!(
        resolve_product_recipe(&project.candidate(), recipe),
        Err(RuntimePlanError::PluginPlan(PluginPlanError::Conflict { plugin, conflict }))
            if plugin == PROJECT_SCENE_WATCH_PLUGIN_ID
                && conflict == nara_asset_watch::ASSET_WATCH_PLUGIN_ID
    ));
    assert_eq!(factories.load(Ordering::Relaxed), 0);
}

#[test]
fn mismatched_root_refuses_binding_without_starting_native_coverage() {
    let project = ProjectFixture::new();
    let other = TestDirectory::new();
    let plan = project.plan(watch_recipe(
        ProjectSceneWatchPlugin::new(other.path.clone()).unwrap(),
    ));
    let mut candidate = plan
        .plugin_plan()
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let loader = Arc::new(ProjectContentLoader::new(project.capability()).unwrap());
    let settings = project.candidate();
    let inspected = Arc::new(OnceLock::new());
    let observation = Arc::clone(&inspected);
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                let error = bind_source_watch(world, &settings, &loader).unwrap_err();
                assert_eq!(
                    error.diagnostics.iter().next().unwrap().code().as_str(),
                    "project.scene.watch-root-mismatch"
                );
                ProjectScenes::scope(world, |scenes| {
                    assert!(!scenes.retry_source_watch());
                    let status = scenes.source_watch_status().unwrap();
                    assert_eq!(status.state, ProjectSceneWatchState::Unbound);
                    assert_eq!(status.native, AssetWatchLifecycleStats::default());
                    assert!(!status.rescan_pending);
                    assert!(
                        observation
                            .set(scenes.source_watch_retirement().unwrap())
                            .is_ok()
                    );
                });
            });
        })
        .map(|outcome| assert!(outcome.post_operation_fault.is_none()))
        .unwrap();
    let mut retirement = candidate.begin_retirement();
    let deadline = Instant::now() + Duration::from_secs(5);
    while retirement.drive_retirement() != RuntimeCandidateRetirementState::Retired {
        assert!(
            Instant::now() < deadline,
            "mismatched-root cleanup did not finish"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        inspected.get().unwrap().stats(),
        AssetWatchLifecycleStats::default()
    );
}

#[test]
fn source_host_binds_one_worker_before_initial_rescan_and_preserves_current_scene() {
    ProjectFixture::new()
        .with_hosted_world(|world| {
            let current = CurrentScene::get(world).unwrap().receipt().clone();
            let status = watch_status(world);
            assert_eq!(status.state, ProjectSceneWatchState::Watching);
            assert_eq!(status.native.started(), 1);
            assert_eq!(status.native.active(), 1);
            assert_eq!(status.restarts, 0);
            assert!(status.rescan_pending);
            assert!(!source_status(world).pending);
            assert!(source_status(world).request.is_none());
            assert!(!ProjectScenes::scope(world, |scenes| scenes.retry_source_watch()));
            let driver = world.resource::<SourceWatchDriver>();
            let binding = driver.binding.as_ref().unwrap();
            assert_eq!(
                binding.root.root(),
                driver.configuration.root.canonicalize().unwrap()
            );
            assert_eq!(
                coverage_paths(binding.root.root(), &binding.desired).unwrap(),
                vec![
                    AssetWatchPath::new(binding.root.root().to_owned(), false),
                    AssetWatchPath::new(binding.root.root().join("assets"), true),
                    AssetWatchPath::new(binding.root.root().join("scenes"), true),
                    AssetWatchPath::new(binding.root.root().join("prefabs"), true),
                    AssetWatchPath::new(binding.root.root().join("schema"), false),
                ]
            );
            poll_source_watch(world).unwrap();
            assert!(!watch_status(world).rescan_pending);
            assert!(watch_status(world).error.is_none());
            assert!(source_status(world).pending);
            assert!(source_status(world).request.is_none());
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
            assert!(!world.contains_resource::<nara_asset::AssetSourceChanges>());
            Ok(())
        })
        .unwrap();
}

#[test]
fn deterministic_queue_overflow_replaces_coverage_before_source_rescan() {
    assert_loss_replaces_before_rescan(Loss::Overflow);
}

#[test]
fn deterministic_translation_failure_replaces_coverage_before_source_rescan() {
    assert_loss_replaces_before_rescan(Loss::Translation);
}

#[test]
fn deterministic_broad_observation_replaces_coverage_before_source_rescan() {
    assert_loss_replaces_before_rescan(Loss::Broad);
}

#[derive(Clone, Copy)]
enum Loss {
    Overflow,
    Translation,
    Broad,
}

fn assert_loss_replaces_before_rescan(loss: Loss) {
    ProjectFixture::new()
        .with_hosted_world(move |world| {
            let current = CurrentScene::get(world).unwrap().receipt().clone();
            poll_source_watch(world).unwrap();
            poll_project_scene_requests(world);
            let initial = source_status(world);
            assert!(initial.request.is_some());
            assert!(!initial.pending);
            assert!(initial.error.is_none());
            let driver = world.resource::<SourceWatchDriver>();
            let root = driver.binding.as_ref().unwrap().root.root().to_owned();
            let old_sender = driver.current.as_ref().unwrap().queue.sender();
            match loss {
                Loss::Overflow => {
                    assert!(matches!(
                        old_sender.try_send_batch(vec![
                            AssetWatchEvent::modified(
                                root.join("ignored")
                            );
                            3
                        ]),
                        Err(AssetWatchQueueSendError::Full { .. })
                    ));
                    assert!(old_sender.stats().rescan_required());
                }
                Loss::Translation => {
                    old_sender
                        .try_send(AssetWatchEvent::modified(
                            root.parent().unwrap().join("outside"),
                        ))
                        .unwrap();
                }
                Loss::Broad => {
                    old_sender
                        .try_send(AssetWatchEvent::subtree(root.join("scenes")))
                        .unwrap();
                }
            }
            let generation = world.remove_resource::<RuntimeGeneration>().unwrap();
            poll_source_watch(world).unwrap();
            let replacement = watch_status(world);
            assert_eq!(replacement.state, ProjectSceneWatchState::Watching);
            assert_eq!(replacement.restarts, 1);
            assert_eq!(replacement.native.started(), 2);
            assert_eq!(replacement.native.active(), 1);
            assert_eq!(
                replacement.native.joined() + replacement.native.retiring() as u64,
                1
            );
            assert!(replacement.rescan_pending);
            assert!(replacement.error.is_some());
            assert_eq!(replacement.queue.retained_events(), 0);
            assert_eq!(replacement.queue.high_water_events(), 0);
            assert_eq!(replacement.queue.accepted_events(), 0);
            assert!(!replacement.queue.rescan_required());
            assert_eq!(source_status(world), initial);
            assert_eq!(
                old_sender.try_send(AssetWatchEvent::modified(root.join("ignored"))),
                Err(AssetWatchQueueSendError::Disconnected)
            );
            world.insert_resource(generation);
            poll_source_watch(world).unwrap();
            let recovered = watch_status(world);
            assert!(!recovered.rescan_pending);
            assert!(recovered.error.is_none());
            assert_eq!(recovered.native.started(), 2);
            assert_eq!(recovered.restarts, 1);
            assert!(source_status(world).pending);
            assert_eq!(source_status(world).request, initial.request);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
            Ok(())
        })
        .unwrap();
}

#[test]
fn replaced_root_keeps_rescan_pending_and_retry_explicit_until_original_directory_returns() {
    let project = ProjectFixture::new();
    let root = project.root.clone();
    let workspace = project.directory.path.clone();
    let retired = project.directory.path.join("retired-project");
    project
        .with_hosted_world(move |world| {
            let current = CurrentScene::get(world).unwrap().receipt().clone();
            assert!(!source_status(world).pending);
            let old_sender = world
                .resource::<SourceWatchDriver>()
                .current
                .as_ref()
                .unwrap()
                .queue
                .sender();
            old_sender
                .try_send(AssetWatchEvent::subtree(root.join("scenes")))
                .unwrap();
            #[cfg(windows)]
            retire_windows_coverage_before_directory_move(world, &workspace, &root, &retired);
            move_fixture_directory(&workspace, &root, &retired).unwrap();
            fs::create_dir(&root).unwrap();
            poll_source_watch(world).unwrap();
            let refused = watch_status(world);
            assert_eq!(refused.state, ProjectSceneWatchState::RetryRequired);
            assert!(refused.rescan_pending);
            assert!(
                refused
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("admitted directory")
            );
            assert_eq!(refused.native.started(), 1);
            assert_eq!(refused.native.active(), 0);
            assert_eq!(refused.restarts, 0);
            assert!(!source_status(world).pending);
            assert_eq!(
                old_sender.try_send(AssetWatchEvent::modified(root.join("ignored"))),
                Err(AssetWatchQueueSendError::Disconnected)
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.retry_source_watch());
                assert!(!scenes.retry_source_watch());
                assert_eq!(scenes.source_watch_status().unwrap().native.started(), 1);
            });
            poll_source_watch(world).unwrap();
            assert_eq!(
                watch_status(world).state,
                ProjectSceneWatchState::RetryRequired
            );
            assert_eq!(watch_status(world).native.started(), 1);
            assert!(!source_status(world).pending);
            fs::remove_dir(&root).unwrap();
            move_fixture_directory(&workspace, &retired, &root).unwrap();
            poll_source_watch(world).unwrap();
            assert_eq!(
                watch_status(world).state,
                ProjectSceneWatchState::RetryRequired
            );
            assert_eq!(watch_status(world).native.started(), 1);
            assert!(ProjectScenes::scope(world, |scenes| scenes.retry_source_watch()));
            poll_source_watch(world).unwrap();
            let recovered = watch_status(world);
            assert_eq!(recovered.state, ProjectSceneWatchState::Watching);
            assert_eq!(recovered.native.started(), 2);
            assert_eq!(recovered.native.active(), 1);
            assert_eq!(recovered.restarts, 1);
            assert!(!recovered.rescan_pending);
            assert!(recovered.error.is_none());
            assert!(source_status(world).pending);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
            Ok(())
        })
        .unwrap();
}

#[test]
fn deterministic_nested_parent_replacement_rebuilds_configured_native_coverage() {
    let project = ProjectFixture::new();
    let root = project.root.clone();
    let workspace = project.directory.path.clone();
    for child in ["assets", "scenes", "prefabs"] {
        fs::create_dir_all(root.join("content/source").join(child)).unwrap();
    }
    fs::copy(
        root.join("scenes/startup.scene.json"),
        root.join("content/source/scenes/startup.scene.json"),
    )
    .unwrap();
    let manifest = fs::read_to_string(root.join("nara.toml")).unwrap();
    let manifest = manifest
        .replace("assets = \"assets\"", "assets = \"content/source/assets\"")
        .replace("scenes = \"scenes\"", "scenes = \"content/source/scenes\"")
        .replace(
            "prefabs = \"prefabs\"",
            "prefabs = \"content/source/prefabs\"",
        );
    fs::write(root.join("nara.toml"), manifest).unwrap();
    project
        .with_hosted_world(move |world| {
            let current = CurrentScene::get(world).unwrap().receipt().clone();
            let driver = world.resource::<SourceWatchDriver>();
            let binding = driver.binding.as_ref().unwrap();
            let native_root = binding.root.root().to_owned();
            let paths = coverage_paths(&native_root, &binding.desired).unwrap();
            assert!(paths.contains(&AssetWatchPath::new(native_root.join("content"), false)));
            assert!(paths.contains(&AssetWatchPath::new(
                native_root.join("content/source"),
                false
            )));
            let old_sender = driver.current.as_ref().unwrap().queue.sender();
            old_sender
                .try_send(AssetWatchEvent::subtree(native_root.join("content")))
                .unwrap();
            #[cfg(windows)]
            retire_windows_coverage_before_directory_move(
                world,
                &workspace,
                &root.join("content"),
                &root.join("retired-content"),
            );
            move_fixture_directory(
                &workspace,
                &root.join("content"),
                &root.join("retired-content"),
            )
            .unwrap();
            for child in ["assets", "scenes", "prefabs"] {
                fs::create_dir_all(root.join("content/source").join(child)).unwrap();
            }
            fs::copy(
                root.join("retired-content/source/scenes/startup.scene.json"),
                root.join("content/source/scenes/startup.scene.json"),
            )
            .unwrap();
            poll_source_watch(world).unwrap();
            let replaced = watch_status(world);
            assert_eq!(replaced.state, ProjectSceneWatchState::Watching);
            assert_eq!(replaced.native.started(), 2);
            assert_eq!(replaced.native.active(), 1);
            assert_eq!(replaced.restarts, 1);
            assert!(replaced.error.is_none());
            assert!(!replaced.rescan_pending);
            assert!(source_status(world).pending);
            assert_eq!(
                old_sender.try_send(AssetWatchEvent::modified(native_root.join("ignored"))),
                Err(AssetWatchQueueSendError::Disconnected)
            );
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
            let driver = world.resource::<SourceWatchDriver>();
            let binding = driver.binding.as_ref().unwrap();
            let paths = coverage_paths(&native_root, &binding.desired).unwrap();
            for child in ["assets", "scenes", "prefabs"] {
                assert!(paths.contains(&AssetWatchPath::new(
                    native_root.join("content/source").join(child),
                    true,
                )));
            }
            assert!(paths.contains(&AssetWatchPath::new(native_root, false)));
            Ok(())
        })
        .unwrap();
}

#[test]
fn global_lifecycle_observer_refuses_scope_and_reports_a_classified_runtime_fault() {
    let failure = ProjectFixture::new()
        .with_hosted_world(|world| {
            let current = CurrentScene::get(world).unwrap().receipt().clone();
            let old_sender = world
                .resource::<SourceWatchDriver>()
                .current
                .as_ref()
                .unwrap()
                .queue
                .sender();
            let calls = Arc::new(AtomicUsize::new(0));
            let observed_calls = Arc::clone(&calls);
            let observer = world
                .add_observer(move |_: On<Remove<()>>| {
                    observed_calls.fetch_add(1, Ordering::Relaxed);
                })
                .id();
            world.flush();
            let refusal = poll_source_watch(world).unwrap_err();
            let status = watch_status(world);
            assert_eq!(status.state, ProjectSceneWatchState::RetryRequired);
            assert!(status.rescan_pending);
            assert!(
                status
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("lifecycle scope was refused")
            );
            assert_eq!(status.native.started(), 1);
            assert_eq!(status.native.active(), 0);
            assert_eq!(status.native.joined() + status.native.retiring() as u64, 1);
            assert!(!source_status(world).pending);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            assert_eq!(
                old_sender.try_send(AssetWatchEvent::modified(PathBuf::from("unused"))),
                Err(AssetWatchQueueSendError::Disconnected)
            );
            world.entity_mut(observer).despawn();
            world.flush();
            Err(refusal)
        })
        .unwrap_err();
    assert!(failure.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.scene.watch-lifecycle-unsupported"
    }));
}

fn watch_status(world: &mut World) -> ProjectSceneWatchStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_watch_status().unwrap())
}

fn move_fixture_directory(workspace: &Path, source: &Path, target: &Path) -> std::io::Result<()> {
    let workspace = workspace.canonicalize()?;
    if !source.canonicalize()?.starts_with(&workspace)
        || !target
            .parent()
            .unwrap()
            .canonicalize()?
            .starts_with(&workspace)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "directory control move must remain inside its own fixture workspace",
        ));
    }
    fs::rename(source, target)
}

#[cfg(windows)]
fn retire_windows_coverage_before_directory_move(
    world: &mut World,
    workspace: &Path,
    source: &Path,
    target: &Path,
) {
    let before = watch_status(world);
    let current = CurrentScene::get(world).unwrap().receipt().clone();
    let failure = move_fixture_directory(workspace, source, target).unwrap_err();
    assert_eq!(failure.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(source.is_dir());
    assert!(!target.exists());
    assert_eq!(watch_status(world), before);
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &current);
    let coverage = world
        .resource_mut::<SourceWatchDriver>()
        .current
        .take()
        .unwrap();
    drop(coverage);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let driver = world.resource::<SourceWatchDriver>();
        driver.native.poll_retired().unwrap();
        let stats = driver.native.stats();
        if stats.active() == 0 && stats.retiring() == 0 {
            assert_eq!(stats.started(), stats.joined());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original Windows watches did not retire"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        watch_status(world).state,
        ProjectSceneWatchState::RetryRequired
    );
    assert!(ProjectScenes::scope(world, |scenes| scenes.retry_source_watch()));
}

fn source_status(world: &mut World) -> crate::project_host::ProjectSceneSourceUpdateStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_update_status().unwrap())
}

fn watch_recipe(configuration: ProjectSceneWatchPlugin) -> ProductRecipe {
    ProductRecipe::new()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(configuration, Clone::clone)
        .unwrap()
}

type Inspection = Box<dyn FnOnce(&mut World) -> Result<(), BevyError> + Send + Sync>;

struct WatchInspectionPlugin(Arc<Mutex<Option<Inspection>>>);

impl Plugin for WatchInspectionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.scene-watch-inspection"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let inspection = Arc::clone(&self.0);
        app.add_systems(
            CoreStage::TaskUpdate,
            (move |world: &mut World| -> Result<(), BevyError> {
                let inspect = inspection.lock().unwrap().take();
                if let Some(inspect) = inspect {
                    inspect(world)?;
                }
                Ok(())
            })
            .before(poll_source_watch),
        )?;
        Ok(())
    }
}

struct ProjectFixture {
    directory: TestDirectory,
    root: PathBuf,
    authority: DirectoryCapability,
}

impl ProjectFixture {
    fn new() -> Self {
        let directory = TestDirectory::new();
        let root = directory.path.join("project");
        for child in [
            "assets",
            "scenes",
            "prefabs",
            "schema",
            "target/debug/incremental",
        ] {
            fs::create_dir_all(root.join(child)).unwrap();
        }
        let bundle =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1

[project]
source_universe = '832c0aec-c104-59bc-9f23-709e071fa8fd'
native_document_identity = 'project_location'
name = "Native Scene Watch Controls"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d", "asset-watch"]
[tasks.io]
workers = 1
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
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let authority = open_explicit_product_project_root(&root).unwrap();
        Self {
            directory,
            root,
            authority,
        }
    }

    fn capability(&self) -> DirectoryCapability {
        self.authority.share()
    }

    fn candidate(&self) -> ProjectSettingsCandidate {
        let root = self.capability();
        ingest_project_manifest(
            &root
                .open_file(&RelativePath::new("nara.toml").unwrap())
                .unwrap(),
            None,
        )
        .unwrap()
    }

    fn configuration(&self) -> ProjectSceneWatchPlugin {
        ProjectSceneWatchPlugin::new(self.root.clone())
            .unwrap()
            .with_queue_limits(AssetWatchQueueLimits::new(
                ItemLimit::new(2).unwrap(),
                ByteLimit::new(1024 * 1024).unwrap(),
            ))
    }

    fn plan(&self, recipe: ProductRecipe) -> RuntimePlan {
        resolve_product_recipe(&self.candidate(), recipe).unwrap()
    }

    fn with_hosted_world(
        &self,
        inspect: impl FnOnce(&mut World) -> Result<(), BevyError> + Send + Sync + 'static,
    ) -> Result<(), HostFault> {
        let observer = Arc::new(OnceLock::new());
        let observed = Arc::clone(&observer);
        let inspection: Inspection = Box::new(move |world| {
            let retirement =
                ProjectScenes::scope(world, |scenes| scenes.source_watch_retirement().unwrap());
            assert!(observed.set(retirement).is_ok());
            inspect(world)
        });
        let inspection = Arc::new(Mutex::new(Some(inspection)));
        let factory_inspection = Arc::clone(&inspection);
        let definition = PluginDefinition::infallible::<WatchInspectionPlugin, _>(
            PluginDefinitionId::new("nara.test.scene-watch-inspection", 1),
            b"scene-watch-inspection-v1",
            move || WatchInspectionPlugin(Arc::clone(&factory_inspection)),
        );
        let candidate = self.candidate();
        let request = project_runtime_plugins(&candidate)
            .with_recipe(watch_recipe(self.configuration()))
            .insert_after::<nara_gameplay::GameplayCommandPlugin>(definition);
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = Arc::new(ProjectContentLoader::new(self.capability()).unwrap());
        let agreement =
            prepare_runtime_schema_agreement(&loader.project_root(), &candidate, &plan).unwrap();
        let snapshot = loader
            .load_with_runtime_agreement(&candidate, &plan, agreement)
            .unwrap();
        let mut host = ProjectHost::new(RuntimeClosePolicy::default());
        let mut attempt = host
            .begin_project_start(snapshot, plan, candidate, Arc::clone(&loader), Vec::new())
            .unwrap();
        host.complete_start(&mut attempt).unwrap();
        drop(attempt);
        let drive = host.drive_running_runtime(Duration::ZERO);
        assert!(inspection.lock().unwrap().is_none());
        let stop = host.stop_running();
        assert!(
            stop.is_ok() || host.has_cleanup_owner(),
            "Host Stop failed: {stop:?}"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let CleanupDriveOutcome::Complete {
                failed,
                diagnostics,
                ..
            } = host.drive_cleanup_once()
            {
                assert!(!failed, "Scene watcher cleanup failed: {diagnostics:?}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Scene watcher Host cleanup did not finish"
            );
            std::thread::yield_now();
        }
        assert!(matches!(host.slot, ProjectHostSlot::Empty));
        let stats = observer.get().unwrap().stats();
        assert_eq!(stats.active(), 0);
        assert_eq!(stats.retiring(), 0);
        assert_eq!(stats.started(), stats.joined());
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        drive
    }
}
