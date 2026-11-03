use std::{
    fs, thread,
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    advanced_prelude::ItemLimit,
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    fs::{ContentDigest, RelativePath},
    project::ProjectManifest,
    project_host::{
        AuthoringSourceOperationLimits, EditorPrefabExtractionId, EditorPrefabExtractionOutcome,
        EditorPrefabExtractionPhase, EditorPrefabExtractionRequest, ProjectContentLoader,
        ProjectContentMemberKind, ProjectContentSnapshot, StartupContentClosureReceipt,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::ComponentTypeId,
    scene::{
        PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance, SceneDocument,
        SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
        SourceDocumentKind,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentKind, EditorExternalPlayState,
        EditorFrameModel, EditorNavigationAction, EditorPlayOperation, EditorPlayOperationResult,
        EditorPrefabExtractionDisposition,
    },
};
use nara_editor::{EditorApplication, EditorApplicationState};

use super::{current_frame, supervised_play_project::CleanRoomSupervisedPlayProject};

const STARTUP: &str = "scenes/startup.scene.json";
const PREFAB: &str = "prefabs/startup-camera.prefab.json";
const PREFAB_META: &str = "prefabs/startup-camera.prefab.json.meta";
const EXTRACTED: &str = "prefabs/extracted-camera.prefab.json";
const EXTRACTED_META: &str = "prefabs/extracted-camera.prefab.json.meta";
const PREFAB_ID: &str = "87990186-1e3d-47b7-bae7-512148909fc8";
const EXTRACTED_ID: &str = "24edf2c6-3e52-40fc-8e1e-eb8c979755c6";
const WAIT: Duration = Duration::from_secs(30);
const GUARDED: [&str; 9] = [
    STARTUP,
    "scenes/startup.scene.json.meta",
    PREFAB_META,
    "schema/authoring.bundle.json",
    "Cargo.toml",
    "Cargo.lock",
    "src/lib.rs",
    "src/bin/headless.rs",
    "src/bin/desktop.rs",
];

pub(super) struct ExtractionEvidence {
    startup: StartupContentClosureReceipt,
    saved: Vec<(String, Vec<u8>)>,
}

pub(super) fn prepare_startup_prefab(project: &CleanRoomSupervisedPlayProject) {
    let mut startup = scene(&fs::read(project.root().join(STARTUP)).unwrap());
    let camera = startup
        .entities
        .iter_mut()
        .find(|entity| entity.id == camera_id())
        .unwrap();
    assert!(camera.prefab.is_none());
    assert!(camera.components.contains_key(&camera_component()));
    let prefab = PrefabDocument::from_ordered_roots([camera.clone()]);
    fs::write(
        project.root().join(PREFAB),
        prefab.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.root().join(PREFAB_META),
        AssetMeta::new(metadata(PREFAB_ID, "startup-camera.prefab.json"), 0_u32)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut anchor = SceneEntityRecord::new(camera_id());
    anchor.prefab = Some(PrefabInstance::root(
        AssetRef::StableId(stable_id(PREFAB_ID)),
        source_key(PREFAB_ID),
        PrefabEntityTarget::source(camera_id()),
    ));
    *camera = anchor;
    fs::write(
        project.root().join(STARTUP),
        startup.to_json_string().unwrap(),
    )
    .unwrap();
}

pub(super) fn extract_and_check(
    project: &CleanRoomSupervisedPlayProject,
    application: &mut EditorApplication,
) -> ExtractionEvidence {
    settle(application);
    let initial = current_frame(application);
    let startup = initial.workspace().active_scene.as_ref().unwrap();
    let startup_document = startup.document;
    let before_runtime = load_startup(project);
    let original_prefab = fs::read(project.root().join(PREFAB)).unwrap();
    let manifest = fs::read(project.root().join("nara.toml")).unwrap();
    let guarded: Vec<_> = GUARDED
        .into_iter()
        .map(|path| {
            (
                path.to_owned(),
                fs::read(project.root().join(path)).unwrap(),
            )
        })
        .collect();
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: startup_document,
            entity: Some(camera_id()),
        }),
    );
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
            document: startup_document,
            base_revision: startup.revision,
            entity: camera_id(),
        }),
    );
    settle(application);
    let opened = current_frame(application);
    let document = opened.workspace().active_document.unwrap();
    assert_ne!(document, startup_document);
    let selected = opened.workspace().document(document).unwrap();
    assert_eq!(selected.kind, EditorDocumentKind::Prefab);
    assert_eq!(
        selected.source_binding.as_ref().unwrap().locator(),
        "startup-camera.prefab.json"
    );
    let documents = opened.workspace().documents.clone();
    assert_eq!(documents.len(), 2);
    let limits =
        AuthoringSourceOperationLimits::default().with_open_handles(ItemLimit::new(128).unwrap());
    let operation = application
        .begin_prefab_extraction_with_limits(
            EditorPrefabExtractionRequest::new(
                document,
                camera_id(),
                metadata(EXTRACTED_ID, "extracted-camera.prefab.json"),
            ),
            limits,
        )
        .expect("the real source-owned startup camera must admit extraction");
    wait_for_extraction(application, operation, EditorPrefabExtractionPhase::Review);
    let view = application.prefab_extraction_view().unwrap();
    assert_eq!(view.limits, limits);
    assert!(view.writes_fenced && !view.work_pending && view.can_revise());
    let review = application.prefab_extraction_review(operation).unwrap();
    assert_eq!(review.source_count(), 2);
    assert_eq!(
        review.documents().collect::<Vec<_>>(),
        [
            (startup_document, EditorPrefabExtractionDisposition::Guard),
            (document, EditorPrefabExtractionDisposition::ReplaceSource),
        ],
    );
    let mut outputs: Vec<_> = review
        .outputs()
        .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
        .collect();
    outputs.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        outputs
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [EXTRACTED, EXTRACTED_META, PREFAB],
    );
    assert_eq!(prefab(&outputs[0].1), prefab(&original_prefab));
    let mut rebound = SceneEntityRecord::new(camera_id());
    rebound.prefab = Some(PrefabInstance::root(
        AssetRef::StableId(stable_id(EXTRACTED_ID)),
        source_key(EXTRACTED_ID),
        PrefabEntityTarget::source(camera_id()),
    ));
    assert_eq!(
        prefab(&outputs[2].1),
        PrefabDocument::from_ordered_roots([rebound])
    );
    assert_eq!(
        fs::read(project.root().join(PREFAB)).unwrap(),
        original_prefab
    );
    assert_eq!(
        fs::read(project.root().join("nara.toml")).unwrap(),
        manifest
    );
    assert!(!project.root().join(EXTRACTED).exists());
    assert!(!project.root().join(EXTRACTED_META).exists());
    assert_saved(project, &guarded);
    assert_eq!(current_frame(application).workspace().documents, documents);

    application.confirm_prefab_extraction(operation).unwrap();
    wait_for_extraction(
        application,
        operation,
        EditorPrefabExtractionPhase::Terminal,
    );
    let view = application.prefab_extraction_view().unwrap();
    assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Adopted));
    assert!(!view.writes_fenced && !view.work_pending);
    assert_saved(project, &outputs);
    assert_saved(project, &guarded);
    assert_ne!(
        fs::read(project.root().join(PREFAB)).unwrap(),
        original_prefab
    );
    assert_eq!(
        ProjectManifest::parse_toml_bytes(&fs::read(project.root().join("nara.toml")).unwrap())
            .manifest,
        ProjectManifest::parse_toml_bytes(&manifest).manifest,
    );
    let adopted = current_frame(application);
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert_eq!(adopted.workspace().documents.len(), documents.len());
    for original in &documents {
        let installed = adopted.workspace().document(original.document).unwrap();
        assert_eq!(
            installed.authoring_schema_basis,
            original.authoring_schema_basis
        );
        assert_eq!(installed.source_binding, original.source_binding);
        assert!(!installed.dirty);
        assert_eq!(installed.revision, installed.saved_revision);
        if original.document == startup_document {
            assert_eq!(installed, original);
        } else {
            assert_ne!(installed.revision, original.revision);
            assert_eq!(installed.history.undo_depth, 0);
            assert_eq!(installed.history.redo_depth, 0);
        }
    }
    let after_runtime = load_startup(project);
    assert_eq!(
        before_runtime.compiled_scene().roots(),
        after_runtime.compiled_scene().roots()
    );
    assert_eq!(
        before_runtime.compiled_scene().entities(),
        after_runtime.compiled_scene().entities()
    );
    assert_ne!(
        before_runtime.content_digest(),
        after_runtime.content_digest()
    );
    let camera = after_runtime
        .compiled_scene()
        .entities()
        .iter()
        .find(|entity| entity.id() == &camera_id())
        .unwrap();
    assert!(camera.components().contains_key(&camera_component()));
    assert!(
        camera
            .components()
            .contains_key(&ComponentTypeId::new("nara.transform.Transform2d"))
    );
    for stable in [PREFAB_ID, EXTRACTED_ID] {
        assert!(
            after_runtime
                .prefabs()
                .iter()
                .any(|source| source.source_key() == &source_key(stable))
        );
    }

    let checked = check_project(
        ProjectCheckRequest::new(project.capability(), starter_recipe(project))
            .with_profile("desktop"),
    )
    .expect("the actual starter recipe must strictly admit the extracted saved startup generation");
    let startup = checked.startup_content().clone();
    for (path, kind) in [
        (STARTUP, ProjectContentMemberKind::Scene),
        (PREFAB, ProjectContentMemberKind::Prefab),
        (EXTRACTED, ProjectContentMemberKind::Prefab),
        (PREFAB_META, ProjectContentMemberKind::AssetMetadata),
        (EXTRACTED_META, ProjectContentMemberKind::AssetMetadata),
    ] {
        let member = startup
            .members()
            .iter()
            .find(|member| member.path().as_str() == path)
            .expect(
                "strict startup admission must consume both real Prefab generations and metadata",
            );
        assert_eq!(member.kind(), kind);
        assert_eq!(
            member.digest(),
            ContentDigest::of_bytes(&fs::read(project.root().join(path)).unwrap())
        );
        assert!(checked.members().contains(member));
    }
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: startup_document,
        }),
    );
    settle(application);
    assert_eq!(
        current_frame(application).workspace().active_document,
        Some(startup_document)
    );
    outputs.extend(guarded);
    outputs.push((
        "nara.toml".to_owned(),
        fs::read(project.root().join("nara.toml")).unwrap(),
    ));
    let evidence = ExtractionEvidence {
        startup,
        saved: outputs,
    };
    evidence.assert_saved_generation(project);
    evidence
}

impl ExtractionEvidence {
    pub(super) fn assert_saved_generation(&self, project: &CleanRoomSupervisedPlayProject) {
        assert_saved(project, &self.saved);
        for member in self.startup.members() {
            assert_eq!(
                member.digest(),
                ContentDigest::of_bytes(
                    &fs::read(project.root().join(member.path().as_str())).unwrap()
                ),
                "the checked startup generation changed: {}",
                member.path().as_str(),
            );
        }
    }

    pub(super) fn assert_running_generation(
        &self,
        project: &CleanRoomSupervisedPlayProject,
        frame: &EditorFrameModel,
    ) {
        self.assert_saved_generation(project);
        let external = frame.play().external().unwrap();
        assert_eq!(external.state(), EditorExternalPlayState::Running);
        let generation = external
            .generation()
            .expect("the actual Play child has a generation");
        assert_ne!(generation, 0);
        assert_eq!(
            frame.play().result(),
            Some(EditorPlayOperationResult::Applied {
                operation: EditorPlayOperation::Play,
                generation: Some(generation),
            })
        );
        eprintln!(
            "extracted_starter: generation={generation} startup_digest={:?} members={:?}",
            self.startup.content_digest(),
            self.startup.members(),
        );
    }
}

fn starter_recipe(project: &CleanRoomSupervisedPlayProject) -> ProductRecipe {
    assert_eq!(
        fs::read(project.root().join("src/lib.rs"))
            .unwrap()
            .as_slice(),
        include_bytes!("../../../starters/desktop-2d/v1/src/lib.rs").as_slice(),
        "strict Check uses the unchanged canonical starter's actual empty product recipe",
    );
    ProductRecipe::new()
}

fn load_startup(project: &CleanRoomSupervisedPlayProject) -> ProjectContentSnapshot {
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, Some("desktop")).unwrap();
    let plan = resolve_product_recipe(&candidate, starter_recipe(project)).unwrap();
    ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap()
}

fn dispatch(application: &mut EditorApplication, action: EditorAction) {
    let frame = current_frame(application);
    assert!(
        application
            .submit_and_drive(
                EditorActionBatch::new(frame.revision(), [action]),
                Duration::ZERO,
            )
            .update()
            .actions_observed()
    );
}

fn settle(application: &mut EditorApplication) {
    let deadline = Instant::now() + WAIT;
    loop {
        application.advance(Duration::ZERO);
        let frame = current_frame(application);
        if !frame.project_browser().has_pending_work()
            && !frame.image_import_work_pending()
            && !frame.source_asset_import_work_pending()
            && !frame.authoring_retirement_pending()
            && !frame.source_tool().has_pending_work()
            && !frame.scene_preview_work_pending()
            && !frame.authoring_operation_work_pending()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "starter extraction background work did not settle"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_extraction(
    application: &mut EditorApplication,
    operation: EditorPrefabExtractionId,
    phase: EditorPrefabExtractionPhase,
) {
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.prefab_extraction_view().unwrap();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.refusal,
            None,
            "{view:?}; {:?}",
            application.prefab_extraction_diagnostics()
        );
        if view.phase == phase {
            return;
        }
        assert_ne!(
            view.phase,
            EditorPrefabExtractionPhase::Terminal,
            "{view:?}; {:?}",
            application.prefab_extraction_diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "starter extraction did not reach {phase:?}: {view:?}"
        );
        application.advance(Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
}

fn assert_saved(project: &CleanRoomSupervisedPlayProject, saved: &[(String, Vec<u8>)]) {
    for (path, bytes) in saved {
        assert_eq!(
            &fs::read(project.root().join(path)).unwrap(),
            bytes,
            "{path}"
        );
    }
}

fn scene(bytes: &[u8]) -> SceneDocument {
    SceneDocumentCandidate::decode_json_bytes(bytes)
        .unwrap()
        .into_composition_source()
        .unwrap()
}

fn prefab(bytes: &[u8]) -> PrefabDocument {
    PrefabDocumentCandidate::decode_json_bytes(bytes)
        .unwrap()
        .into_composition_source()
        .unwrap()
}

fn camera_id() -> SceneEntityId {
    SceneEntityId::new("main-camera").unwrap()
}

fn camera_component() -> ComponentTypeId {
    ComponentTypeId::new("nara.render.Camera2d")
}

fn stable_id(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn source_key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(stable_id(value), SourceDocumentKind::Prefab)
}

fn metadata(value: &str, alias: &str) -> AssetMetaIdentity {
    AssetMetaIdentity::new(
        stable_id(value),
        AssetPath::new(alias).unwrap(),
        AssetSourceKind::Prefab,
    )
}
