use super::*;

use nara_app::CoreStage;
use nara_core::ItemLimit;
use nara_ecs::bevy_ecs::system::SystemState;
use nara_identity::SpawnedSceneInstance;
use nara_project::ProjectPath;
use nara_scene::advanced::{SceneProductResource, SceneProductTransactionLimits};

use super::super::{
    ProjectSceneRequestError, ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
    commit_prepared_project_scene, scene_requests::hold_next_preparation,
};

type ProbeAction = Box<dyn FnOnce(&mut World) + Send + Sync>;

#[derive(Default)]
struct RequestProbe(Mutex<Option<ProbeAction>>);

struct RequestProbePlugin(Arc<RequestProbe>);

#[derive(Debug, Default, Resource)]
struct CommittedLevel(u32, Arc<()>);

impl CommittedLevel {
    fn new(value: u32) -> Self {
        Self(value, Arc::new(()))
    }
}

impl SceneProductResource for CommittedLevel {
    type Baseline = (u32, Arc<()>);

    fn capture_baseline(&self) -> Self::Baseline {
        (self.0, Arc::clone(&self.1))
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.0 == baseline.0 && Arc::ptr_eq(&self.1, &baseline.1)
    }
}

impl Plugin for RequestProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.project-scene-request"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<CommittedLevel>()?;
        let probe = Arc::clone(&self.0);
        app.add_systems(CoreStage::Update, move |world: &mut World| {
            let action = probe.0.lock().unwrap().take();
            if let Some(action) = action {
                action(world);
            }
        })?;
        Ok(())
    }
}

struct Fixture {
    _project: TestProject,
    loader: Arc<ProjectContentLoader>,
    probe: Arc<RequestProbe>,
    host: ProjectHost,
}

impl Fixture {
    fn new(with_context: bool) -> Self {
        let project = TestProject::new("hosted-project-scene-requests");
        let manifest_path = project.root.join(PROJECT_MANIFEST);
        let mut manifest = fs::read_to_string(&manifest_path).unwrap();
        manifest.push_str("\n[content]\nscenes = [\"b.scene.json\"]\n");
        fs::write(manifest_path, manifest).unwrap();
        for (path, entity) in [
            ("startup.scene.json", "level-a"),
            ("b.scene.json", "level-b"),
            ("undeclared.scene.json", "undeclared"),
        ] {
            let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                nara_identity::SceneEntityId::new(entity).unwrap(),
            )]);
            fs::write(
                project.root.join("scenes").join(path),
                scene.to_json_string().unwrap(),
            )
            .unwrap();
        }
        let root = project.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let probe = Arc::new(RequestProbe::default());
        let factory_probe = Arc::clone(&probe);
        let definition = PluginDefinition::infallible::<RequestProbePlugin, _>(
            PluginDefinitionId::new("nara.test.project-scene-request", 1),
            b"scene-requests-v1",
            move || RequestProbePlugin(Arc::clone(&factory_probe)),
        );
        let request =
            project_runtime_plugins(&candidate).insert_after::<GameplayCommandPlugin>(definition);
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = Arc::new(ProjectContentLoader::new(root).unwrap());
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        let mut host = ProjectHost::new(RuntimeClosePolicy::new(Duration::ZERO));
        let mut attempt = if with_context {
            host.begin_project_start(snapshot, plan, candidate, Arc::clone(&loader), Vec::new())
                .unwrap()
        } else {
            host.begin_start(snapshot, plan, Vec::new()).unwrap()
        };
        host.complete_start(&mut attempt).unwrap();
        drop(attempt);
        Self {
            _project: project,
            loader,
            probe,
            host,
        }
    }

    // Exercise only ordinary runtime drives; no test calls the private request poll directly.
    fn step<R: Send + 'static>(
        &mut self,
        action: impl FnOnce(&mut World) -> R + Send + Sync + 'static,
    ) -> R {
        let output = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&output);
        assert!(
            self.probe
                .0
                .lock()
                .unwrap()
                .replace(Box::new(move |world| {
                    *capture.lock().unwrap() = Some(action(world));
                }))
                .is_none()
        );
        self.host.drive_running_runtime(Duration::ZERO).unwrap();
        assert!(self.probe.0.lock().unwrap().is_none());
        let value = output.lock().unwrap().take().unwrap();
        value
    }

    fn request(&mut self, path: &str) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        let path = ProjectPath::new(path).unwrap();
        self.step(move |world| {
            let mut access = SystemState::<ProjectScenes>::new(world);
            access.get_mut(world).unwrap().request(path)
        })
    }

    fn wait_ready(&mut self, request: ProjectSceneRequestId) {
        self.wait_state(request, ProjectSceneRequestState::Ready);
    }

    fn wait_state(&mut self, request: ProjectSceneRequestId, expected: ProjectSceneRequestState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = self.step(move |world| {
                let mut access = SystemState::<ProjectScenes>::new(world);
                access.get_mut(world).unwrap().state(request)
            });
            if state == Some(expected) {
                break;
            }
            let pending = if expected == ProjectSceneRequestState::Cancelled {
                ProjectSceneRequestState::Cancelling
            } else {
                ProjectSceneRequestState::Preparing
            };
            assert_eq!(state, Some(pending));
            assert!(
                Instant::now() < deadline,
                "hosted Scene request did not reach {expected:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn close(&mut self) {
        close_host(&mut self.host);
        assert!(matches!(self.host.slot, ProjectHostSlot::Empty));
        assert_zero_budget(&self.loader);
    }
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE)
}

fn current(world: &mut World) -> (SceneDocument, SpawnedSceneInstance) {
    let mut access = SystemState::<CurrentScene>::new(world);
    let current = access.get_mut(world).unwrap();
    (current.source().clone(), current.receipt().clone())
}

fn assert_zero_budget(loader: &ProjectContentLoader) {
    let budget = loader.budget_snapshot();
    assert_eq!(budget.active_generations(), 0);
    assert_eq!(budget.active_reservations(), 0);
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(budget.active(kind), 0, "{kind:?}");
    }
}

#[test]
fn original_host_prepares_inspectable_scene_then_commits_and_acknowledges_b_to_a() {
    let mut fixture = Fixture::new(true);
    let original = fixture.step(current);
    let b = fixture.request("b.scene.json").unwrap();
    assert_eq!(
        fixture.request("startup.scene.json"),
        Err(ProjectSceneRequestError::Busy)
    );
    fixture.step(move |world| {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert!(!scenes.acknowledge(b));
    });
    fixture.wait_ready(b);
    let original_receipt = original.1.clone();
    let committed_b = fixture.step(move |world| {
        let target = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            let prepared = scenes.prepared(b).unwrap();
            assert_eq!(
                prepared.selected_scene_path().unwrap().as_str(),
                "b.scene.json"
            );
            assert_eq!(prepared.expected_instance(), &original_receipt);
            assert_eq!(
                prepared.scene().roots,
                [nara_identity::SceneEntityId::new("level-b").unwrap()]
            );
            prepared.scene().clone()
        };
        // Ready content remains data until the game explicitly accepts it.
        assert_eq!(current(world), original);
        let report = commit_prepared_project_scene(world, b, limits(), &[], |writer| {
            writer.replace_resource(CommittedLevel::new(1));
        });
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let receipt = report.instance.unwrap();
        assert_eq!(current(world), (target, receipt.clone()));
        assert_ne!(receipt.instance_id(), original_receipt.instance_id());
        assert_eq!(world.resource::<CommittedLevel>().0, 1);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert_eq!(scenes.state(b), Some(ProjectSceneRequestState::Committed));
        assert!(scenes.prepared(b).is_none());
        assert!(scenes.acknowledge(b));
        assert_eq!(scenes.state(b), None);
        assert!(!scenes.acknowledge(b));
        receipt
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let a = fixture.request("startup.scene.json").unwrap(); // Startup is an implicit declared root.
    assert_ne!(a, b);
    fixture.wait_ready(a);
    fixture.step(move |world| {
        let before = current(world);
        let rejected = commit_prepared_project_scene(world, b, limits(), &[], |_| {
            panic!("another request identity cannot configure a successor");
        });
        assert!(rejected.instance.is_none());
        assert!(report_has_code(
            &rejected.diagnostics,
            "project.scene.request-mismatch"
        ));
        assert_eq!(current(world), before);
        let target = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let mut scenes = access.get_mut(world).unwrap();
            assert!(!scenes.cancel(b));
            assert!(!scenes.acknowledge(b));
            let prepared = scenes.prepared(a).unwrap();
            assert_eq!(
                prepared.selected_scene_path().unwrap().as_str(),
                "startup.scene.json"
            );
            assert_eq!(prepared.expected_instance(), &committed_b);
            prepared.scene().clone()
        };
        let report = commit_prepared_project_scene(world, a, limits(), &[], |writer| {
            writer.replace_resource(CommittedLevel::new(2));
        });
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(current(world), (target, report.instance.unwrap()));
        assert_eq!(world.resource::<CommittedLevel>().0, 2);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert_eq!(scenes.state(a), Some(ProjectSceneRequestState::Committed));
        assert!(scenes.acknowledge(a));
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    fs::write(
        fixture._project.root.join("scenes/b.scene.json"),
        b"{ malformed",
    )
    .unwrap();
    let malformed = fixture.request("b.scene.json").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let terminal = fixture.step(move |world| {
            let (source, _) = current(world);
            assert_eq!(
                source.roots,
                [nara_identity::SceneEntityId::new("level-a").unwrap()]
            );
            assert_eq!(world.resource::<CommittedLevel>().0, 2);
            let mut access = SystemState::<ProjectScenes>::new(world);
            let mut scenes = access.get_mut(world).unwrap();
            assert!(scenes.diagnostics(a).is_none());
            match scenes.state(malformed) {
                Some(ProjectSceneRequestState::Preparing) => false,
                Some(ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Content(
                    kind,
                ))) => {
                    assert_eq!(
                        kind,
                        crate::project_content::ProjectContentErrorKind::SceneFormat
                    );
                    assert!(report_has_code(
                        scenes.diagnostics(malformed).unwrap(),
                        "project.content.scene-format-invalid"
                    ));
                    assert!(scenes.prepared(malformed).is_none());
                    assert!(scenes.acknowledge(malformed));
                    true
                }
                state => panic!("unexpected malformed Scene request state: {state:?}"),
            }
        });
        if terminal {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "malformed Scene request did not return its diagnostic"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    fixture.close();
}

#[test]
fn hosted_scene_request_rejects_undeclared_unavailable_and_stale_ready_inputs() {
    let mut unavailable = Fixture::new(false);
    assert_eq!(
        unavailable.request("b.scene.json"),
        Err(ProjectSceneRequestError::Unavailable)
    );
    unavailable.close();

    let mut fixture = Fixture::new(true);
    assert_eq!(
        fixture.request("undeclared.scene.json"),
        Err(ProjectSceneRequestError::UndeclaredScene)
    );
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let request = fixture.request("b.scene.json").unwrap();
    fixture.wait_ready(request);
    fixture.step(move |world| {
        let (source, old) = current(world);
        let restarted = crate::startup_scene::restart_current_scene_with_product(
            world,
            &old,
            limits(),
            &[],
            |_| {},
        );
        assert!(!restarted.diagnostics.has_errors());
        let current_receipt = restarted.instance.unwrap();
        assert_ne!(current_receipt, old);
        {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Rejected(
                    ProjectSceneRequestError::Stale
                ))
            );
            assert!(scenes.prepared(request).is_none());
        }
        let rejected = commit_prepared_project_scene(world, request, limits(), &[], |_| {
            panic!("a stale expected instance cannot configure a successor");
        });
        assert!(rejected.instance.is_none());
        assert!(report_has_code(
            &rejected.diagnostics,
            "project.scene.request-stale"
        ));
        assert_eq!(current(world), (source, current_receipt));
        assert_eq!(world.resource::<CommittedLevel>().0, 0);
        let mut access = SystemState::<ProjectScenes>::new(world);
        assert!(access.get_mut(world).unwrap().acknowledge(request));
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let stale_ready = fixture.request("b.scene.json").unwrap();
    fixture.wait_ready(stale_ready);
    let next = fixture.step(move |world| {
        let (source, old) = current(world);
        let restarted = crate::startup_scene::restart_current_scene_with_product(
            world,
            &old,
            limits(),
            &[],
            |_| {},
        );
        assert!(!restarted.diagnostics.has_errors());
        let receipt = restarted.instance.unwrap();
        let next = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let mut scenes = access.get_mut(world).unwrap();
            assert_eq!(
                scenes.state(stale_ready),
                Some(ProjectSceneRequestState::Rejected(
                    ProjectSceneRequestError::Stale
                ))
            );
            assert!(scenes.prepared(stale_ready).is_none());
            // Observation alone projects stale Ready input; direct acknowledgement must retire it
            // without requiring a rejected commit as an implicit protocol transition.
            assert!(scenes.acknowledge(stale_ready));
            assert_eq!(scenes.state(stale_ready), None);
            scenes
                .request(ProjectPath::new("b.scene.json").unwrap())
                .unwrap()
        };
        assert_eq!(current(world), (source, receipt));
        next
    });
    fixture.wait_ready(next);
    fixture.step(move |world| {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert!(scenes.cancel(next));
        assert!(scenes.acknowledge(next));
    });
    fixture.close();
}

#[test]
fn public_scene_cancel_waits_for_physical_return_then_acknowledges_and_reuses_the_slot() {
    let mut fixture = Fixture::new(true);
    let original = fixture.step(current);
    let (request, started, release) = fixture.step(|world| {
        let (started, release) = hold_next_preparation(world);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let request = access
            .get_mut(world)
            .unwrap()
            .request(ProjectPath::new("b.scene.json").unwrap())
            .unwrap();
        (request, started, release)
    });
    assert!(started.recv_timeout(Duration::from_secs(5)).is_ok());
    fixture.step(move |world| {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Preparing)
        );
        assert!(scenes.cancel(request));
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Cancelling)
        );
        assert!(!scenes.cancel(request));
        assert!(scenes.prepared(request).is_none());
        assert!(!scenes.acknowledge(request));
        assert_eq!(
            scenes.request(ProjectPath::new("startup.scene.json").unwrap()),
            Err(ProjectSceneRequestError::Busy)
        );
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 2);
    drop(release);
    fixture.wait_state(request, ProjectSceneRequestState::Cancelled);
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let ready_cancel = fixture.step(move |world| {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert!(!scenes.diagnostics(request).unwrap().has_errors());
        assert!(scenes.acknowledge(request));
        assert_eq!(scenes.state(request), None);
        scenes
            .request(ProjectPath::new("b.scene.json").unwrap())
            .unwrap()
    });
    assert_ne!(ready_cancel, request);
    fixture.wait_ready(ready_cancel);
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 2);
    let reuse = fixture.step(move |world| {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert!(scenes.prepared(ready_cancel).is_some());
        assert!(scenes.cancel(ready_cancel));
        assert_eq!(
            scenes.state(ready_cancel),
            Some(ProjectSceneRequestState::Cancelled)
        );
        assert!(scenes.prepared(ready_cancel).is_none());
        assert!(scenes.acknowledge(ready_cancel));
        scenes
            .request(ProjectPath::new("startup.scene.json").unwrap())
            .unwrap()
    });
    // The successor uses the already current revision, so cancellation released the target's
    // distinct generation synchronously without waiting for another worker poll.
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    fixture.wait_ready(reuse);
    fixture.step(move |world| {
        assert_eq!(current(world), original);
        assert_eq!(world.resource::<CommittedLevel>().0, 0);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        assert!(scenes.cancel(reuse));
        assert!(scenes.acknowledge(reuse));
    });
    fixture.close();
}

#[test]
fn hosted_scene_request_close_cancels_held_preparation_and_retires_without_late_publication() {
    let mut fixture = Fixture::new(true);
    let (request, started, release, original) = fixture.step(|world| {
        let original = current(world);
        let (started, release) = hold_next_preparation(world);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let request = access
            .get_mut(world)
            .unwrap()
            .request(ProjectPath::new("b.scene.json").unwrap())
            .unwrap();
        (request, started, release, original)
    });
    assert!(started.recv_timeout(Duration::from_secs(5)).is_ok());
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 2);
    fixture.step(move |world| {
        assert_eq!(current(world), original);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let scenes = access.get_mut(world).unwrap();
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Preparing)
        );
        assert!(scenes.prepared(request).is_none());
    });
    let failure = fixture.host.stop_running().unwrap_err();
    assert!(report_has_code(
        &failure.diagnostics,
        "project.run.cleanup-incomplete"
    ));
    assert!(matches!(
        fixture.host.slot,
        ProjectHostSlot::Cleaning { .. }
    ));
    assert!(fixture.host.with_running_world(|_| ()).is_err());
    assert_eq!(
        fixture.loader.budget_snapshot().active_generations(),
        2,
        "logical shutdown must retain current Scene and the physically held candidate"
    );
    drop(release);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match fixture.host.drive_cleanup_once() {
            CleanupDriveOutcome::Complete { .. } => break,
            CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete => {}
        }
        assert!(
            Instant::now() < deadline,
            "held Scene preparation did not retire after release"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(fixture.host.slot, ProjectHostSlot::Empty));
    assert!(fixture.host.with_running_world(|_| ()).is_err());
    assert!(!fixture.host.start_claim.any_active());
    assert_zero_budget(&fixture.loader);
}
