//! Independent public Session consumer of the project's explicit data proposal.

#[allow(dead_code)]
#[path = "authoring_project.rs"]
mod project;

#[cfg(any(windows, target_os = "linux"))]
#[path = "authored_upgrade_session/process_recovery.rs"]
mod process_recovery;
#[path = "authored_upgrade_session/product.rs"]
mod product;

use std::{
    fs,
    time::{Duration, Instant},
};

use nara_asset::AssetSourcePath;
use nara_engine::project_host::{
    AuthoredSchemaUpgradeHistoryPolicy, AuthoredSchemaUpgradeProposal,
    AuthoredSchemaUpgradeSourceProposal, EditorAuthoredSchemaUpgradeError,
    EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeOutcome,
    EditorAuthoredSchemaUpgradePhase, EditorAuthoredUpgradeWorkingSource, EditorProjectSession,
};
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
use nara_scene::{AuthoringPropertyEdit, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneFileEncoding};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
    EditorDocumentOpenResult, EditorDocumentQueryBasis, EditorNavigationAction,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
};

use super::*;

const WAIT: Duration = Duration::from_secs(30);
const STARTUP: &str = "scenes/startup.scene.json";
const INACTIVE: &str = "scenes/inactive.scene.json";
const PREFAB: &str = "prefabs/enemy.prefab.json";
const LEAF: &str = "prefabs/leaf.prefab.json";
const PROFILE_SCENE: &str = "dormant/scenes/closed.scene.json";
const PROFILE_PREFAB: &str = "dormant/prefabs/closed.prefab.json";
const SOURCES: [&str; 6] = [
    STARTUP,
    INACTIVE,
    PREFAB,
    LEAF,
    PROFILE_SCENE,
    PROFILE_PREFAB,
];
const TRACKED: [&str; 8] = [
    "nara.toml",
    "schema/authoring.bundle.json",
    STARTUP,
    INACTIVE,
    PREFAB,
    LEAF,
    PROFILE_SCENE,
    PROFILE_PREFAB,
];

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "the public operation must settle: {:?}; {:?}",
            editor.authored_schema_upgrade_view(),
            editor.authored_schema_upgrade_diagnostics(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn source_key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "f1390a84-11df-49b0-a901-d7a045141039".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn linked(path: &str, overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path(path).unwrap(),
        arguments: None,
        overrides: overlay,
    }
}

fn stats_entity(name: &str, next: bool, whole: i64, milli: i64) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(component(), record(next, whole, milli, 7))
}

fn leaf_document(next: bool) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([
        stats_entity("leaf", next, 8, 8000),
        stats_entity("armor-only", next, 9, 9000),
        stats_entity("empty", next, 10, 10000),
        stats_entity("replaced", next, 11, 11000),
    ])
}

fn enemy_document(next: bool, dirty: bool) -> PrefabDocument {
    let mut overlay = PrefabOverlay::new(source_key(LEAF, SourceDocumentKind::Prefab));
    // Both Scene instances replace this nested declaration. Its -9 pin remains authored
    // data and must still be upgraded, even though their effective values do not use it.
    overlay.components = vec![
        properties(next, target("leaf"), vec![health_pin(next, -9, -9000)]),
        properties(next, target("armor-only"), vec![armor_pin(24)]),
        properties(next, target("empty"), Vec::new()),
        set(
            PrefabComponentSetMode::Replace,
            target("replaced"),
            record(next, 77, 77000, 6),
        ),
    ];
    let mut nested = SceneEntityRecord::new(id("nested-anchor"));
    nested.prefab = Some(linked("leaf.prefab.json", Some(overlay)));
    PrefabDocument::from_ordered_roots([
        stats_entity(
            "enemy",
            next,
            if dirty { 109 } else { 105 },
            if dirty { 109000 } else { 105000 },
        )
        .with_component(ComponentTypeId::new(UNKNOWN), unknown_record()),
        nested,
    ])
}

fn scene_document(next: bool, dirty: bool) -> SceneDocument {
    let mut inner = PrefabOverlay::new(source_key(LEAF, SourceDocumentKind::Prefab));
    inner.components.push(properties(
        next,
        target("leaf"),
        vec![health_pin(next, 14, 14000)],
    ));
    let mut outer = PrefabOverlay::new(source_key(PREFAB, SourceDocumentKind::Prefab));
    outer.instances.push(PrefabInstanceClaim {
        target: target("nested-anchor"),
        value: Some(linked("leaf.prefab.json", Some(inner))),
    });
    let mut added = PrefabAddition::new(id("added"));
    added
        .components
        .insert(component(), record(next, -3, -3000, 4));
    let mut beneath_addition = PrefabOverlay::new(source_key(LEAF, SourceDocumentKind::Prefab));
    // An equal-to-source pin must stay explicit, including under an added instance.
    beneath_addition.components.push(properties(
        next,
        target("leaf"),
        vec![health_pin(next, 8, 8000)],
    ));
    added.prefab = Some(linked("leaf.prefab.json", Some(beneath_addition)));
    outer.additions.push(added);
    outer.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    let mut first = SceneEntityRecord::new(id("first-instance"));
    first.prefab = Some(linked("enemy.prefab.json", Some(outer.clone())));
    let mut second = SceneEntityRecord::new(id("second-instance"));
    second.prefab = Some(linked("enemy.prefab.json", Some(outer)));
    SceneDocument::from_ordered_roots([
        stats_entity(
            "enemy",
            next,
            if dirty { 109 } else { 105 },
            if dirty { 109000 } else { 105000 },
        )
        .with_component(ComponentTypeId::new(UNKNOWN), unknown_record()),
        first,
        second,
    ])
}

fn profile_prefab(next: bool) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([stats_entity("profile-enemy", next, 17, 17000)
        .with_component(ComponentTypeId::new(UNKNOWN), unknown_record())])
}

fn profile_scene(next: bool) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("profile-instance"));
    anchor.prefab = Some(linked("closed.prefab.json", None));
    SceneDocument::from_ordered_roots([stats_entity("profile-enemy", next, 19, 19000), anchor])
}

fn fixture_project() -> project::Project {
    let builtin = AuthoringSchemaBundle::from_trusted_provider_definitions([
        SCENE_COMPONENTS_SCHEMA_PROVIDER,
    ])
    .unwrap();
    let bundle = AuthoringSchemaBundle::new(
        builtin
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::new(
                owner(OWNER),
                v1_catalog(),
                None,
            )]),
    )
    .unwrap();
    let project = project::Project::new(&bundle, &scene_document(false, false), "json");
    for directory in ["dormant/scenes", "dormant/prefabs"] {
        fs::create_dir_all(project.root().join(directory)).unwrap();
    }
    let manifest_path = project.root().join("nara.toml");
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(concat!(
        "\n[profiles.dormant.paths]\n",
        "scenes = 'dormant/scenes'\nprefabs = 'dormant/prefabs'\n",
        "[profiles.dormant.startup]\ndefault_scene = 'closed.scene.json'\n",
    ));
    fs::write(manifest_path, manifest).unwrap();
    for (path, encoded) in [
        (
            INACTIVE,
            scene_document(false, false).to_json_string().unwrap(),
        ),
        (
            PREFAB,
            enemy_document(false, false).to_json_string().unwrap(),
        ),
        (LEAF, leaf_document(false).to_json_string().unwrap()),
        (
            PROFILE_SCENE,
            profile_scene(false).to_json_string().unwrap(),
        ),
        (
            PROFILE_PREFAB,
            profile_prefab(false).to_json_string().unwrap(),
        ),
    ] {
        fs::write(project.root().join(path), encoded).unwrap();
    }
    project
}

fn open_document(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    wait(editor, |editor| {
        editor
            .current_frame()
            .project_browser()
            .entry(path)
            .is_some()
    });
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: path.into(),
                },
            )],
        ))
        .unwrap();
    let request = match report.outcomes() {
        [
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
                request,
            }),
        ] => *request,
        other => panic!("public source Open must prepare: {other:?}"),
    };
    wait(editor, |editor| {
        editor.current_frame().document_open().result().is_some()
    });
    let document = match editor.current_frame().document_open().result().unwrap() {
        EditorDocumentOpenResult::Opened {
            request: completed,
            document,
        } if completed == request => document,
        other => panic!("the actual source must open: {other:?}"),
    };
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::AcknowledgeOpen { request },
            )],
        ))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.current_frame().document_open().result().is_none());
    document
}

fn open_dirty(project: &project::Project) -> (EditorProjectSession, [EditorDocumentId; 3]) {
    let mut editor = project.open();
    let startup = editor.current_frame().workspace().active_document.unwrap();
    let inactive = open_document(&mut editor, INACTIVE);
    let prefab = open_document(&mut editor, PREFAB);
    let documents = [startup, inactive, prefab];
    for document in documents {
        for health in [109, 113] {
            let basis = editor.document_query_basis(document).unwrap();
            let prepared = editor
                .prepare_authoring_property_edit(
                    &basis,
                    &[AuthoringPropertyEdit {
                        entity: id("enemy"),
                        component: component(),
                        component_version: V1,
                        property: root("health"),
                        value: ComponentValue::I64(health),
                    }],
                )
                .unwrap();
            project::applied(editor.commit_authoring_edit(prepared));
            editor.drive_editor_frame(Duration::ZERO);
        }
        project::replay(&mut editor, document, false);
        project::assert_history(&editor, document, 1, 1);
        project::assert_value(
            &editor,
            document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::I64(109),
        );
    }
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SetActiveDocument { document: startup },
            )],
        ))
        .unwrap();
    assert!(
        matches!(report.outcomes(), [EditorActionOutcome::Workspace(report)] if report.applied)
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(startup)
    );
    (editor, documents)
}

fn capture(editor: &mut EditorProjectSession) -> EditorAuthoredSchemaUpgradeId {
    let operation = editor.begin_authored_schema_upgrade().unwrap();
    assert!(editor.authored_schema_upgrade_view().writes_fenced);
    wait(editor, |editor| {
        !matches!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::Draining
                | EditorAuthoredSchemaUpgradePhase::Capturing
        )
    });
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorAuthoredSchemaUpgradePhase::AwaitingProposal
    );
    assert!(view.writes_fenced && !view.work_pending);
    assert_eq!(
        editor
            .authored_schema_upgrade_inputs(operation)
            .unwrap()
            .source_count(),
        SOURCES.len()
    );
    operation
}

fn proposed(
    editor: &EditorProjectSession,
    operation: EditorAuthoredSchemaUpgradeId,
) -> AuthoredSchemaUpgradeProposal {
    let inputs = editor.authored_schema_upgrade_inputs(operation).unwrap();
    let bundle = AuthoringSchemaBundle::new(inputs.schema_bundle().owners().iter().map(|owner| {
        if owner.owner_id().as_str() == OWNER {
            AuthoringSchemaBundleOwner::new(
                owner.owner_id().clone(),
                successor_catalog(owner.current()).unwrap(),
                Some(owner.current().clone()),
            )
        } else {
            AuthoringSchemaBundleOwner::from(owner)
        }
    }))
    .unwrap();
    let sources = inputs
        .sources()
        .map(|source| {
            assert_eq!(source.encoding, SceneFileEncoding::Json);
            let (working, successor) = match source.working {
                EditorAuthoredUpgradeWorkingSource::Scene(document) => (
                    document.to_json_string().unwrap().into_bytes(),
                    plan_scene(document)
                        .unwrap()
                        .into_document()
                        .to_json_string()
                        .unwrap(),
                ),
                EditorAuthoredUpgradeWorkingSource::Prefab(document) => (
                    document.to_json_string().unwrap().into_bytes(),
                    plan_prefab(document)
                        .unwrap()
                        .into_document()
                        .to_json_string()
                        .unwrap(),
                ),
                EditorAuthoredUpgradeWorkingSource::Saved(bytes) => {
                    let successor = match source.key.kind() {
                        SourceDocumentKind::Scene => {
                            let document = SceneDocumentCandidate::decode_json_bytes(bytes)
                                .unwrap()
                                .into_composition_source()
                                .unwrap();
                            plan_scene(&document)
                                .unwrap()
                                .into_document()
                                .to_json_string()
                                .unwrap()
                        }
                        SourceDocumentKind::Prefab => {
                            let document = PrefabDocumentCandidate::decode_json_bytes(bytes)
                                .unwrap()
                                .into_composition_source()
                                .unwrap();
                            plan_prefab(&document)
                                .unwrap()
                                .into_document()
                                .to_json_string()
                                .unwrap()
                        }
                    };
                    // Closed sources use their exact captured bytes, without re-encoding.
                    (bytes.to_vec(), successor)
                }
            };
            AuthoredSchemaUpgradeSourceProposal {
                key: source.key.clone(),
                project_path: source.project_path.into(),
                reviewed_working: working.into_boxed_slice(),
                successor: successor.into_bytes().into_boxed_slice(),
            }
        })
        .collect();
    AuthoredSchemaUpgradeProposal {
        successor_bundle: bundle
            .to_json_string()
            .unwrap()
            .into_bytes()
            .into_boxed_slice(),
        component: component(),
        field: ComponentFieldId::new("health"),
        successor_property: health_property_v2(),
        sources,
    }
}

fn settle_preparation(editor: &mut EditorProjectSession) {
    wait(editor, |editor| {
        editor.authored_schema_upgrade_view().phase != EditorAuthoredSchemaUpgradePhase::Preparing
    });
    assert!(!editor.authored_schema_upgrade_view().work_pending);
}

fn assert_review(
    editor: &EditorProjectSession,
    operation: EditorAuthoredSchemaUpgradeId,
    documents: [EditorDocumentId; 3],
    disk: &[Vec<u8>; 8],
) {
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorAuthoredSchemaUpgradePhase::Review,
        "{:?}",
        editor.authored_schema_upgrade_diagnostics(),
    );
    assert!(view.writes_fenced && !view.work_pending);
    assert_eq!(view.refusal, None);
    let review = editor.authored_schema_upgrade_review(operation).unwrap();
    assert_eq!(review.source_count(), SOURCES.len());
    assert_eq!(review.document_count(), documents.len());
    assert_eq!(review.component(), &component());
    assert_eq!(review.field(), &ComponentFieldId::new("health"));
    assert_eq!(review.successor_property(), &health_property_v2());
    assert_eq!(
        review.history_policy(),
        AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory,
    );
    assert_eq!(review.sources().len(), SOURCES.len());
    let mut paths: Vec<_> = review.sources().map(|source| source.project_path).collect();
    let mut expected_paths = SOURCES;
    paths.sort_unstable();
    expected_paths.sort_unstable();
    assert_eq!(paths.as_slice(), expected_paths.as_slice());
    for source in review.sources() {
        let index = TRACKED
            .iter()
            .position(|path| *path == source.project_path)
            .unwrap();
        assert_eq!(source.original_disk, disk[index].as_slice());
        let document = documents
            .into_iter()
            .zip([STARTUP, INACTIVE, PREFAB])
            .find_map(|(document, path)| (path == source.project_path).then_some(document));
        assert_eq!(source.document, document);
        if document.is_some() {
            assert_ne!(source.reviewed_working, source.original_disk);
        } else {
            assert_eq!(source.reviewed_working, source.original_disk);
        }
        assert_ne!(source.reviewed_working, source.successor);
        let kind = if [PREFAB, LEAF, PROFILE_PREFAB].contains(&source.project_path) {
            SourceDocumentKind::Prefab
        } else {
            SourceDocumentKind::Scene
        };
        assert_eq!(source.key, &source_key(source.project_path, kind));
        // Expected v2 trees use literal milli values, independently of the project command.
        // Complete equality also checks topology, opaque unknown records and dormant pins.
        match source.project_path {
            STARTUP | INACTIVE | PROFILE_SCENE => {
                let actual = SceneDocumentCandidate::decode_json_bytes(source.successor)
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let expected = if source.project_path == PROFILE_SCENE {
                    profile_scene(true)
                } else {
                    scene_document(true, true)
                };
                assert_eq!(actual, expected, "{}", source.project_path);
            }
            PREFAB | LEAF | PROFILE_PREFAB => {
                let actual = PrefabDocumentCandidate::decode_json_bytes(source.successor)
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let expected = match source.project_path {
                    PREFAB => enemy_document(true, true),
                    LEAF => leaf_document(true),
                    PROFILE_PREFAB => profile_prefab(true),
                    _ => unreachable!(),
                };
                assert_eq!(actual, expected, "{}", source.project_path);
            }
            other => panic!("unexpected reviewed source: {other}"),
        }
    }
    for (bundle, version) in [
        (review.schema_bundle(), V1),
        (review.successor_schema_bundle(), V2),
    ] {
        assert_eq!(
            bundle
                .owner(&owner(OWNER))
                .unwrap()
                .schema(&component())
                .unwrap()
                .version(),
            version,
        );
    }
}

fn assert_unchanged(
    editor: &EditorProjectSession,
    project: &project::Project,
    before: &[EditorDocumentQueryBasis; 3],
    disk: &[Vec<u8>; 8],
) {
    for basis in before {
        assert_eq!(editor.document_query_basis(basis.document).unwrap(), *basis);
        project::assert_history(editor, basis.document, 1, 1);
        project::assert_value(
            editor,
            basis.document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::I64(109),
        );
    }
    assert_eq!(
        &TRACKED.map(|path| fs::read(project.root().join(path)).unwrap()),
        disk
    );
}

fn cancel_and_close(
    editor: &mut EditorProjectSession,
    operation: EditorAuthoredSchemaUpgradeId,
    project: &project::Project,
    before: &[EditorDocumentQueryBasis; 3],
    disk: &[Vec<u8>; 8],
) {
    editor.cancel_authored_schema_upgrade(operation).unwrap();
    wait(editor, |editor| {
        !editor.authored_schema_upgrade_view().work_pending
    });
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Terminal);
    assert_eq!(
        view.outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Cancelled)
    );
    assert!(!view.writes_fenced);
    assert_unchanged(editor, project, before, disk);
    for basis in before {
        project::replay(editor, basis.document, false);
        project::assert_history(editor, basis.document, 0, 2);
        project::assert_value(
            editor,
            basis.document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::I64(105),
        );
        project::replay(editor, basis.document, true);
        project::assert_history(editor, basis.document, 1, 1);
        project::assert_value(
            editor,
            basis.document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::I64(109),
        );
        project::replay(editor, basis.document, true);
        project::assert_history(editor, basis.document, 2, 0);
        project::assert_value(
            editor,
            basis.document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::I64(113),
        );
    }
    assert_eq!(
        &TRACKED.map(|path| fs::read(project.root().join(path)).unwrap()),
        disk
    );
    project::close(editor);
}

#[test]
fn independent_public_session_reviews_complete_dirty_and_closed_sources_and_cancels_without_saving()
{
    let project = fixture_project();
    let (mut editor, documents) = open_dirty(&project);
    let before = documents.map(|document| editor.document_query_basis(document).unwrap());
    let disk = TRACKED.map(|path| fs::read(project.root().join(path)).unwrap());
    let operation = capture(&mut editor);
    let proposal = proposed(&editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal)
        .unwrap();
    settle_preparation(&mut editor);
    assert_review(&editor, operation, documents, &disk);
    assert_unchanged(&editor, &project, &before, &disk);
    cancel_and_close(&mut editor, operation, &project, &before, &disk);
}

#[test]
fn independent_public_session_refuses_incomplete_and_malformed_proposals_and_retries_the_original_capture()
 {
    let project = fixture_project();
    let (mut editor, documents) = open_dirty(&project);
    let before = documents.map(|document| editor.document_query_basis(document).unwrap());
    let disk = TRACKED.map(|path| fs::read(project.root().join(path)).unwrap());
    let operation = capture(&mut editor);
    let captured: Vec<_> = editor
        .authored_schema_upgrade_inputs(operation)
        .unwrap()
        .sources()
        .map(|source| {
            (
                source.key.clone(),
                source.project_path.to_owned(),
                source.original_disk.to_vec(),
            )
        })
        .collect();

    for incomplete in [true, false] {
        let mut proposal = proposed(&editor, operation);
        if incomplete {
            let mut sources = proposal.sources.into_vec();
            assert!(sources.pop().is_some());
            proposal.sources = sources.into_boxed_slice();
            assert_eq!(
                editor.prepare_authored_schema_upgrade(operation, proposal),
                Err(EditorAuthoredSchemaUpgradeError::Preparation),
                "incomplete membership must be refused before task custody",
            );
        } else {
            proposal.sources[0].successor = b"{".to_vec().into_boxed_slice();
            editor
                .prepare_authored_schema_upgrade(operation, proposal)
                .unwrap();
            settle_preparation(&mut editor);
        }
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.phase,
            EditorAuthoredSchemaUpgradePhase::AwaitingProposal
        );
        assert_eq!(
            view.refusal,
            Some(EditorAuthoredSchemaUpgradeError::Preparation)
        );
        assert!(view.writes_fenced && !view.work_pending);
        assert!(editor.authored_schema_upgrade_diagnostics().is_some());
        assert!(matches!(
            editor.authored_schema_upgrade_review(operation),
            Err(EditorAuthoredSchemaUpgradeError::Operation),
        ));
        let still_captured: Vec<_> = editor
            .authored_schema_upgrade_inputs(operation)
            .unwrap()
            .sources()
            .map(|source| {
                (
                    source.key.clone(),
                    source.project_path.to_owned(),
                    source.original_disk.to_vec(),
                )
            })
            .collect();
        assert_eq!(still_captured, captured);
        assert_unchanged(&editor, &project, &before, &disk);
    }

    // No new begin/capture operation: correction consumes the retained original inputs.
    let proposal = proposed(&editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal)
        .unwrap();
    settle_preparation(&mut editor);
    assert_review(&editor, operation, documents, &disk);
    assert_unchanged(&editor, &project, &before, &disk);
    cancel_and_close(&mut editor, operation, &project, &before, &disk);
}
