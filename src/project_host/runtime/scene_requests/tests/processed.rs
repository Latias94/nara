use nara_asset::{ImportedProductId, ProjectAssetDatabase, StableAssetId};
use nara_core::ByteLimit;
use nara_fs::ContentDigest;
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_scene::{
    ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SourceDocumentKey,
    SourceDocumentKind,
};

use super::*;
use crate::{
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentExpectation, RuntimeContentLimits,
        RuntimeContentMember, RuntimeContentTarget,
    },
    startup_scene::{SceneSource, replace_current_scene_with_product},
};

#[test]
fn processed_graph_preparation_reuses_exact_scene_work_cap_without_leaking_work() {
    let fixture = ProcessedFixture::new();
    let scene_limits = RuntimeSceneLimits::default();
    let work_limit = ByteLimit::new(scene_limits.working_bytes().get()).unwrap();
    let base = fixture.open(
        RuntimeContentLimits::default()
            .with_scene(scene_limits)
            .with_work_bytes(work_limit),
    );

    // Scene decode and graph indexing are sequential; their work promises must not overlap.
    // Repeating on the same base also catches graph work retained after preparation returns.
    for _ in 0..2 {
        let scene = base
            .acquire_startup_scene()
            .expect("the Scene promise fits the exact shared work cap");
        assert_eq!(scene.product(), startup_product());
        assert_eq!(scene.candidate().document().roots, [entity("startup")]);
        drop(scene);

        let content = base
            .prepare_scene_content(startup_product())
            .expect("graph preparation must reuse the released Scene work allowance");
        assert!(content.base().same_base(&base));
        assert_eq!(content.product(), startup_product());
        assert_eq!(
            content.scene.candidate().document().roots,
            [entity("startup")]
        );
        assert!(content.images.is_empty());
        assert!(content.tilesets.is_empty());
        drop(content);
    }
}

#[test]
fn processed_worker_uses_captured_bytes_and_original_definitions_without_source_io() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(RuntimeContentLimits::default());
    // This changes the physical member after capture. There is no source project or import cache.
    fs::write(
        fixture.root.join("later.runtime.json"),
        b"invalid replacement",
    )
    .unwrap();
    let context = Arc::new(Context::Processed { base: base.clone() });
    let pools = pools();
    let mut preparation = Preparation::start(context, later_product(), &pools, None).unwrap();
    let PreparedSceneContent::Processed(content) = finish(&mut preparation).unwrap() else {
        panic!("processed requests retain processed acquisitions")
    };
    assert!(base.same_base(content.base()));
    assert_eq!(content.product(), later_product());
    assert!(
        content
            .scene
            .candidate()
            .definitions()
            .matches(base.plan().schema_validation().bindings())
    );
    assert_eq!(
        content.scene.candidate().document().roots,
        [entity("later")]
    );
    assert_eq!(
        pools
            .stats()
            .for_kind(nara_tasks::TaskPoolKind::Io)
            .admitted,
        1
    );
    pools.shutdown_blocking().unwrap();
}

#[test]
fn processed_view_and_admission_require_exact_base_product_and_content_mode() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(RuntimeContentLimits::default());
    let other = fixture.open(RuntimeContentLimits::default());
    assert_eq!(base.catalog_digest(), other.catalog_digest());
    assert!(!base.same_base(&other));
    let source = SceneSource::from_hosted_runtime_content(base.acquire_startup_scene().unwrap());
    with_context_world(
        Arc::new(Context::Processed { base: base.clone() }),
        source,
        move |world| {
            world.insert_resource(pools());
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let source_view = CurrentScene::get(world).unwrap().source_view();
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.request(selected()),
                    Err(ProjectSceneRequestError::UndeclaredScene)
                );
                assert_eq!(
                    scenes.request(product(3)),
                    Err(ProjectSceneRequestError::UndeclaredScene),
                );
            });
            let request =
                ProjectScenes::scope(world, |scenes| scenes.request(later_product())).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            ProjectScenes::scope(world, |scenes| {
                let view = scenes.prepared(request).unwrap();
                assert_eq!(
                    view.selected_scene(),
                    &ProjectSceneSelection::Product(later_product())
                );
                assert_eq!(view.selected_scene_path(), None);
                assert_eq!(view.selected_scene_product(), Some(later_product()));
                assert_eq!(view.expected_instance(), &expected);
                assert_eq!(view.scene().roots, [entity("later")]);
            });
            let owner = world.resource::<RequestOwner>();
            let Some(Slot::Ready { admission, content }) = owner.slot.as_ref() else {
                panic!("the worker physically returned ready processed content")
            };
            assert!(api::content_matches_admission(content, admission));
            let foreign = PreparedSceneContent::Processed(
                other.prepare_scene_content(later_product()).unwrap(),
            );
            assert!(!api::content_matches_admission(&foreign, admission));
            let wrong_product = PreparedSceneContent::Processed(
                base.prepare_scene_content(startup_product()).unwrap(),
            );
            assert!(!api::content_matches_admission(&wrong_product, admission));
            let wrong_mode = Admission {
                context: Arc::clone(&admission.context),
                id: admission.id,
                expected: admission.expected.clone(),
                selection: selected().into(),
            };
            assert!(!api::content_matches_admission(content, &wrong_mode));
            let current = CurrentScene::get(world).unwrap();
            assert_eq!(current.receipt(), &expected);
            assert_eq!(current.source().roots, [entity("startup")]);
            assert_eq!(
                source_view.with_document(|document| document as *const SceneDocument),
                Some(current.source() as *const SceneDocument),
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.cancel(request));
                assert!(scenes.acknowledge(request));
            });
            // Equal catalog bytes do not authorize another independently admitted base.
            world.resource_mut::<RequestOwner>().context =
                Arc::new(Context::Processed { base: other });
            assert_eq!(
                ProjectScenes::scope(world, |scenes| scenes.request(later_product())),
                Err(ProjectSceneRequestError::Stale),
            );
        },
    );
}

#[test]
fn processed_cancellation_keeps_slot_and_original_worker_captures_until_return() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(RuntimeContentLimits::default());
    let source = SceneSource::from_hosted_runtime_content(base.acquire_startup_scene().unwrap());
    with_context_world(Arc::new(Context::Processed { base }), source, |world| {
        world.insert_resource(pools());
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        for phase in [TestPhase::BeforeRead, TestPhase::AfterPreparation] {
            let (gate, control) = TestGate::new(phase);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            let request =
                ProjectScenes::scope(world, |scenes| scenes.request(later_product())).unwrap();
            assert!(control.wait_until_started());
            ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(request)));
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelling)
                    );
                    assert!(!scenes.acknowledge(request));
                    assert_eq!(
                        scenes.request(later_product()),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
            }
            control.release();
            wait_for_request(world, request, ProjectSceneRequestState::Cancelled);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        }
    });

    let base = fixture.open(RuntimeContentLimits::default());
    let pools = pools();
    let context = Arc::new(Context::Processed { base });
    let captured = Arc::downgrade(&context);
    let (gate, control) = TestGate::new(TestPhase::AfterPreparation);
    let mut preparation = Preparation::start(context, later_product(), &pools, Some(gate)).unwrap();
    assert!(control.wait_until_started());
    preparation.cancel();
    assert!(preparation.poll().is_none());
    assert!(captured.upgrade().is_some());
    control.release();
    assert!(
        matches!(finish(&mut preparation), Err(failure) if failure.error == ProjectSceneRequestError::TaskUnavailable)
    );
    assert!(captured.upgrade().is_none());
    pools.shutdown_blocking().unwrap();
}

#[test]
fn processed_failure_uses_runtime_content_error_and_safe_runtime_diagnostics() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(
        RuntimeContentLimits::default()
            .with_scene(RuntimeSceneLimits::default().with_working_bytes(ByteLimit::ONE)),
    );
    let context = Arc::new(Context::Processed { base });
    let pools = pools();
    let mut preparation = Preparation::start(context, later_product(), &pools, None).unwrap();
    let failure = finish(&mut preparation).err().unwrap();
    assert_eq!(failure.error, ProjectSceneRequestError::RuntimeContent);
    let copy = failure.error;
    assert_eq!(copy.code(), "project.scene.runtime-content-rejected");
    assert!(failure.diagnostics.has_errors());
    assert!(failure.diagnostics.iter().all(|diagnostic| {
        diagnostic.code().as_str() == "project.scene.runtime-content-rejected"
    }));
    pools.shutdown_blocking().unwrap();
}

#[test]
fn processed_managed_travel_publishes_content_and_retains_request_authority() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(RuntimeContentLimits::default());
    let source = SceneSource::from_hosted_runtime_content(base.acquire_startup_scene().unwrap());
    with_context_world(
        Arc::new(Context::Processed { base: base.clone() }),
        source,
        move |world| {
            world.insert_resource(pools());
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let request =
                ProjectScenes::scope(world, |scenes| scenes.request(later_product())).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            begin_prepared_project_scene_travel(world, request, limits(), &[], |_| {}).unwrap();
            assert_eq!(
                ProjectScenes::scope(world, |scenes| scenes.state(request)),
                Some(ProjectSceneRequestState::Retiring),
            );
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            world.resource_mut::<nara_app::RealTime>().frame += 1;
            poll_project_scene_requests(world);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(scenes.acknowledge(request));
            });
            let current = world.resource::<CurrentSceneOwner>();
            assert_ne!(current.receipt().instance_id(), expected.instance_id());
            assert_eq!(current.source().roots, [entity("later")]);
            assert!(current.is_content_managed());
            assert!(!current.is_project_backed());
            assert!(current.hosted_runtime_base().unwrap().same_base(&base));
            assert!(!world.contains_resource::<ProjectAssetDatabase>());
            // The next request uses the same owner after successful processed publication.
            let next =
                ProjectScenes::scope(world, |scenes| scenes.request(startup_product())).unwrap();
            wait_for_request(world, next, ProjectSceneRequestState::Ready);
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.cancel(next));
                assert!(scenes.acknowledge(next));
            });
        },
    );
}

#[test]
fn processed_hosted_custody_requires_residency_and_preserves_independent_replacement() {
    let fixture = ProcessedFixture::new();
    let base = fixture.open(RuntimeContentLimits::default());
    let startup = base.acquire_startup_scene().unwrap();
    let later = base.acquire_scene(later_product()).unwrap();
    let independent = later.scene_source();
    assert!(!independent.is_project_backed());
    assert!(!independent.is_content_managed());
    let hosted = SceneSource::from_hosted_runtime_content(startup.clone());
    assert!(!hosted.is_project_backed());
    assert!(hosted.is_content_managed());
    let hosted_base = base.clone();
    let hosted_later = later.clone();
    with_context_world(
        Arc::new(Context::Processed { base: base.clone() }),
        hosted,
        move |world| {
            let current = world.resource::<CurrentSceneOwner>();
            assert!(current.is_content_managed());
            assert!(!current.is_project_backed());
            assert!(
                current
                    .hosted_runtime_base()
                    .unwrap()
                    .same_base(&hosted_base)
            );
            let expected = current.receipt().clone();
            let report = replace_current_scene_with_product(
                world,
                &expected,
                hosted_later.scene_source(),
                limits(),
                &[],
                |_| panic!("hosted content must be rejected before product configuration"),
            );
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.current.project-residency-required"
            }));
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        },
    );
    with_context_world(
        Arc::new(Context::Processed { base }),
        startup.scene_source(),
        move |world| {
            world.insert_resource(pools());
            let current = world.resource::<CurrentSceneOwner>();
            assert!(!current.is_content_managed());
            assert!(current.hosted_runtime_base().is_none());
            let expected = current.receipt().clone();
            assert_eq!(
                ProjectScenes::scope(world, |scenes| scenes.request(later_product())),
                Err(ProjectSceneRequestError::Unavailable),
            );
            let report = replace_current_scene_with_product(
                world,
                &expected,
                later.scene_source(),
                limits(),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let committed = report.instance.unwrap();
            assert_ne!(committed.instance_id(), expected.instance_id());
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &committed);
            let report = replace_current_scene_with_product(
                world,
                &committed,
                SceneSource::from_hosted_runtime_content(later.clone()),
                limits(),
                &[],
                |_| panic!("hosted targets must be rejected before product configuration"),
            );
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.current.project-residency-required"
            }));
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &committed);
        },
    );
}

fn entity(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn product(id: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{id:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn startup_product() -> AssetProductRef {
    product(1)
}

fn later_product() -> AssetProductRef {
    product(2)
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE)
}

struct ProcessedFixture {
    root: PathBuf,
    expected: RuntimeContentExpectation,
}

impl ProcessedFixture {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_processed_scene_request_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        let plan = resolve_runtime_configuration(
            &configuration.resolve().unwrap(),
            crate::ProductRecipe::new(),
        )
        .unwrap();
        let mut members = Vec::new();
        for (path, reference, name) in [
            ("startup.runtime.json", startup_product(), "startup"),
            ("later.runtime.json", later_product(), "later"),
        ] {
            let source =
                SourceDocumentKey::stable_asset(reference.source(), SourceDocumentKind::Scene);
            let expanded = ExpandedSceneCandidate::plain(
                source.clone(),
                Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                    entity(name),
                )])),
            );
            let encoded = RuntimeSceneCandidate::cook(
                &expanded,
                plan.schema_validation().bindings(),
                &ProjectAssetDatabase::default(),
                RuntimeSceneLimits::default(),
            )
            .unwrap()
            .to_json_bytes()
            .unwrap();
            fs::write(root.join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene { source },
                    vec![reference],
                    vec![],
                )
                .unwrap(),
            );
        }
        let target = RuntimeContentTarget::new("x86_64-pc-windows-msvc", "request-test").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: startup_product(),
                scenes: vec![startup_product(), later_product()],
                addressable_roots: vec![],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
        }
    }

    fn open(&self, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            crate::ProductRecipe::new(),
            limits,
        )
        .unwrap()
    }
}

impl Drop for ProcessedFixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_processed_scene_request_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
