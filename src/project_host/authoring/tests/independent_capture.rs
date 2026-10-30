//! Independent Scene capture follows the working document without widening startup custody.

use std::{
    thread,
    time::{Duration, Instant},
};

use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason,
    },
    PrefabAuthoringSession, PrefabProjectionStep, SceneAuthoringSession,
};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    document::{AuthoringDocumentDataError, AuthoringDocumentSourceFact},
    project::{AuthoringDocumentCapture, AuthoringPrefabSessionSnapshot},
};

const SECOND_SCENE: &str = "scenes/second.scene.json";
const OUTER_PREFAB: &str = "prefabs/outer.prefab.json";

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("nara.test.independent-capture").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.IndependentImage")
}

fn instance(name: &str) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
        overrides: None,
    }
}

fn anchor(name: &str, prefab: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(SceneEntityId::new(name).unwrap());
    record.prefab = Some(instance(prefab));
    record
}

fn image_entity(name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(SceneEntityId::new(name).unwrap()).with_component(
        component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "image",
                ComponentValue::map([
                    ("kind", ComponentValue::String("path".into())),
                    ("value", ComponentValue::String(format!("{name}.png"))),
                ]),
            )]),
        )
        .with_owner(owner()),
    )
}

pub(in crate::project_host::authoring) fn independent_project() -> TestAuthoringProject {
    let project = TestAuthoringProject::new(
        "independent-document-capture",
        asset_schema_bundle(owner(), component()),
    );
    project.write_startup_manifest();
    for directory in ["scenes", "prefabs", "assets", "scripts"] {
        fs::create_dir_all(project.root.join(directory)).unwrap();
    }
    for (path, document) in [
        (
            "scenes/startup.scene.json",
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("startup").unwrap(),
            )]),
        ),
        (
            SECOND_SCENE,
            SceneDocument::from_ordered_roots([anchor("anchor", "outer")]),
        ),
    ] {
        fs::write(project.root.join(path), document.to_json_string().unwrap()).unwrap();
    }
    for (name, document) in [
        (
            "outer",
            PrefabDocument::from_ordered_roots([image_entity("outer"), anchor("nested", "leaf")]),
        ),
        (
            "leaf",
            PrefabDocument::from_ordered_roots([image_entity("leaf")]),
        ),
        (
            "alternate",
            PrefabDocument::from_ordered_roots([image_entity("alternate")]),
        ),
    ] {
        fs::write(
            project.root.join(format!("prefabs/{name}.prefab.json")),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }
    for (name, stable_id) in [
        ("outer", "7ef3f24f-5b03-4f93-ac4c-1343af327d40"),
        ("leaf", "9c25f182-dd94-4425-b915-2ca11ca894f9"),
        ("alternate", "fc4ef7e9-89de-4614-ab10-b5594bf274ac"),
    ] {
        let path = AssetPath::new(format!("{name}.png")).unwrap();
        fs::write(
            project.root.join("assets").join(path.as_str()),
            name.as_bytes(),
        )
        .unwrap();
        fs::write(
            project.root.join("assets").join(path.meta_path()),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str(stable_id).unwrap(),
                    path,
                    AssetSourceKind::Image,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    }
    fs::write(
        project.root.join("prefabs/unused.prefab.json"),
        b"invalid unused Prefab canary",
    )
    .unwrap();
    fs::write(project.root.join("assets/unused.png"), b"unused").unwrap();
    fs::write(
        project.root.join("assets/unused.png.meta"),
        b"invalid unused metadata canary",
    )
    .unwrap();
    project
}

pub(in crate::project_host::authoring) fn open_second_scene(
    project: &TestAuthoringProject,
) -> (
    AuthoringProjectSessionSnapshot,
    AuthoringDocumentSourceFact,
    SceneAuthoringSession,
) {
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(project.capability(), None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(
        &schema.bundle().owners()[0],
        &Default::default(),
    )
    .unwrap();
    let mut snapshot = PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap();
    let worker = ItemLimit::new(1).unwrap();
    let pools =
        TaskPools::try_new(TaskPoolConfig::threaded(worker, worker, worker).unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while snapshot.source_index_has_pending_work() {
        snapshot.advance_source_index(&pools);
        assert!(
            Instant::now() < deadline,
            "the independent document source index did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let index = snapshot
        .source_index_snapshot()
        .expect("the actual project source index must publish");
    assert!(pools.shutdown_blocking().unwrap().is_complete());
    assert_eq!(
        index.entry(SECOND_SCENE).unwrap().open_capability(),
        ProjectSourceOpenCapability::Scene,
    );
    let AdmittedAuthoringDocument::Scene {
        source, document, ..
    } = snapshot
        .capture_indexed_document(index, Arc::from(SECOND_SCENE), AuthoringDocumentKind::Scene)
        .unwrap()
        .prepare()
        .unwrap()
    else {
        panic!("the indexed Scene request changed document kind");
    };
    assert_eq!(
        document.schema_basis(),
        snapshot.fork_startup_scene().schema_basis(),
    );
    (
        snapshot,
        source,
        SceneAuthoringSession::from_published_authoring_document(document),
    )
}

fn open_prefab(
    snapshot: &AuthoringProjectSessionSnapshot,
    relative_path: &str,
) -> (AuthoringDocumentSourceFact, PrefabAuthoringSession) {
    let index = snapshot.source_index_snapshot().unwrap();
    assert_eq!(
        index.entry(relative_path).unwrap().open_capability(),
        ProjectSourceOpenCapability::Prefab,
    );
    let AdmittedAuthoringDocument::Prefab {
        source, document, ..
    } = snapshot
        .capture_indexed_document(
            index,
            Arc::from(relative_path),
            AuthoringDocumentKind::Prefab,
        )
        .unwrap()
        .prepare()
        .unwrap()
    else {
        panic!("the indexed Prefab request changed document kind");
    };
    assert_eq!(
        document.schema_basis(),
        snapshot.fork_startup_scene().schema_basis(),
    );
    (
        source,
        PrefabAuthoringSession::from_published_authoring_document(document),
    )
}

fn assert_closure(capture: &AuthoringDocumentCapture, prefabs: &[&str], assets: &[&str]) {
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert!(
        !capture.diagnostics().has_errors(),
        "{:?}",
        capture.diagnostics()
    );
    assert_eq!(
        capture
            .prefabs()
            .iter()
            .map(|prefab| prefab.path().as_str())
            .collect::<Vec<_>>(),
        prefabs,
    );
    assert_eq!(
        capture
            .assets()
            .iter()
            .map(|asset| asset.path().as_str())
            .collect::<Vec<_>>(),
        assets
            .iter()
            .map(|path| format!("assets/{path}"))
            .collect::<Vec<_>>(),
    );
    let database = capture.asset_database();
    assert_eq!(database.len(), assets.len());
    for asset in capture.assets() {
        assert_eq!(database.record_for_path(asset.path()), Some(asset.record()));
    }
}

#[test]
fn indexed_scene_captures_its_nested_files_without_widening_startup_or_accumulating_usage() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let startup_usage = snapshot.usage();
    let startup_facts = snapshot.source_facts().cloned().collect::<Vec<_>>();
    let startup_database = snapshot.asset_database();
    let startup_scene = snapshot.fork_startup_scene();
    assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(snapshot.prefab_sources().count(), 0);
    assert_eq!(snapshot.asset_sources().count(), 0);
    assert!(startup_database.is_empty());
    assert_eq!(
        startup_facts
            .iter()
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>(),
        ["scenes/startup.scene.json"],
    );
    let index = snapshot.source_index_snapshot().unwrap();
    assert_eq!(
        index
            .entry("prefabs/unused.prefab.json")
            .unwrap()
            .open_capability(),
        ProjectSourceOpenCapability::Prefab,
    );
    assert_eq!(
        index.entry("assets/unused.png.meta").unwrap().kind(),
        ProjectSourceKind::AssetMeta,
    );

    let mut first = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &first,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        first
            .source_facts()
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>(),
        [
            "assets/leaf.png",
            "assets/leaf.png.meta",
            "assets/outer.png",
            "assets/outer.png.meta",
            "prefabs/leaf.prefab.json",
            "prefabs/outer.prefab.json",
            SECOND_SCENE,
        ],
    );
    for fact in first.source_facts() {
        let bytes = fs::read(project.root.join(fact.project_path().as_str())).unwrap();
        assert_eq!(fact.digest(), ContentDigest::of_bytes(&bytes));
        assert_eq!(fact.encoded_bytes(), bytes.len());
    }
    for asset in first.assets() {
        assert_eq!(
            asset.bytes(),
            fs::read(project.root.join(asset.path().as_str())).unwrap(),
        );
    }
    let preview = first
        .take_preview()
        .expect("the independent Scene must retain its expanded native preview");
    assert!(first.take_preview().is_none());
    let candidate = &preview.candidate;
    assert_eq!(candidate.source_key(), source.source_key());
    assert_ne!(
        candidate.source_key(),
        snapshot.startup_native_source().key()
    );
    let anchor_id = SceneEntityId::new("anchor").unwrap();
    assert_eq!(candidate.document().roots, [anchor_id.clone()]);
    assert_eq!(candidate.document().entities.len(), 4);
    assert_eq!(
        candidate.provenance().len(),
        candidate.document().entities.len()
    );
    assert_eq!(
        candidate.provenance()[&anchor_id].local_entity_id(),
        &anchor_id,
    );
    assert!(
        candidate.provenance()[&anchor_id]
            .projection_path()
            .is_none()
    );
    assert!(
        candidate
            .document()
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none() && entity.components.is_empty())
    );
    let outer = first
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "outer.prefab.json")
        .unwrap();
    let leaf = first
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "leaf.prefab.json")
        .unwrap();
    let leaf_steps = [
        PrefabProjectionStep::SourceEntity {
            source_document: AssetRef::Path(outer.path().clone()),
            canonical_source: outer.native_source().key().clone(),
            entity: SceneEntityId::new("nested").unwrap(),
        },
        PrefabProjectionStep::SourceEntity {
            source_document: AssetRef::Path(leaf.path().clone()),
            canonical_source: leaf.native_source().key().clone(),
            entity: SceneEntityId::new("leaf").unwrap(),
        },
    ];
    let mut leaf_projections = candidate.provenance().iter().filter(|(_, provenance)| {
        provenance
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &anchor_id && path.steps() == leaf_steps)
    });
    let (leaf_id, leaf_provenance) = leaf_projections
        .next()
        .expect("the nested leaf must retain its exact typed source ancestry");
    assert!(leaf_projections.next().is_none());
    assert_eq!(
        leaf_provenance.local_entity_id(),
        &SceneEntityId::new("leaf").unwrap(),
    );
    assert_eq!(preview.omissions.len(), 2);
    let mut omitted_entities = Vec::new();
    for omission in &preview.omissions {
        assert_eq!(
            omission.reason(),
            AuthoringPreviewOmissionReason::NativeBindingUnavailable,
        );
        let AuthoringPreviewOmissionLocation::ProjectedEntityComponent {
            entity,
            provenance,
            component: omitted_component,
        } = omission.location()
        else {
            panic!("an inherited Image component needs a projected omission");
        };
        assert_eq!(omitted_component, &component());
        assert_eq!(candidate.provenance().get(entity), Some(provenance));
        let retained = candidate
            .document()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .expect("a missing native binding must omit the component and retain its entity");
        assert!(retained.components.is_empty());
        assert!(!omitted_entities.contains(entity));
        omitted_entities.push(entity.clone());
    }
    assert!(omitted_entities.contains(leaf_id));

    let second = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &second,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(first.usage(), second.usage());
    assert_eq!(
        first.source_facts().collect::<Vec<_>>(),
        second.source_facts().collect::<Vec<_>>(),
    );
    assert_eq!(snapshot.usage(), startup_usage);
    assert_eq!(
        snapshot.source_facts().cloned().collect::<Vec<_>>(),
        startup_facts
    );
    assert_eq!(snapshot.prefab_sources().count(), 0);
    assert_eq!(snapshot.asset_sources().count(), 0);
    assert!(snapshot.asset_database().is_empty());
    assert!(Arc::ptr_eq(&startup_database, &snapshot.asset_database()));
    assert_eq!(
        snapshot.fork_startup_scene().document(),
        startup_scene.document()
    );
}

#[test]
fn unsaved_exact_instance_replacement_changes_the_capture_and_undo_restores_it() {
    let project = independent_project();
    let (snapshot, source, mut session) = open_second_scene(&project);
    let disk = fs::read(project.root.join(SECOND_SCENE)).unwrap();
    let original = session.shared_document();
    let content = session.content_state();
    let basis = session.authoring_schema_basis();
    let before = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &before,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: SceneEntityId::new("anchor").unwrap(),
        expected: Some(instance("outer")),
        value: Some(instance("alternate")),
    }]);
    let report = session.apply_authoring_patch(&patch);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_ne!(session.document(), original.as_ref());
    assert_ne!(session.content_state(), content);
    assert_eq!(session.authoring_schema_basis(), basis);

    let edited = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(&edited, &["alternate.prefab.json"], &["alternate.png"]);
    assert_eq!(
        edited
            .source_facts()
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>(),
        [
            "assets/alternate.png",
            "assets/alternate.png.meta",
            "prefabs/alternate.prefab.json",
            SECOND_SCENE,
        ],
    );
    assert_eq!(fs::read(project.root.join(SECOND_SCENE)).unwrap(), disk);
    assert_eq!(source.digest(), ContentDigest::of_bytes(&disk));
    let edited_root = edited
        .source_facts()
        .find(|fact| fact.project_path().as_str() == SECOND_SCENE)
        .unwrap();
    assert_eq!(edited_root.digest(), source.digest());
    assert_eq!(edited_root.identity(), source.identity());

    let undone = session.undo_authoring();
    assert!(undone.applied, "{:?}", undone.diagnostics);
    assert_eq!(session.document(), original.as_ref());
    assert_eq!(session.content_state(), content);
    let restored = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &restored,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(restored.usage(), before.usage());
    assert_eq!(
        restored.source_facts().collect::<Vec<_>>(),
        before.source_facts().collect::<Vec<_>>(),
    );
    assert_eq!(fs::read(project.root.join(SECOND_SCENE)).unwrap(), disk);
}

#[test]
fn independent_capture_rejects_a_fresh_schema_basis_even_when_files_and_shape_match() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (fresh_snapshot, fresh_source, fresh_session) = open_second_scene(&project);
    assert_eq!(source.source_key(), fresh_source.source_key());
    assert_eq!(source.digest(), fresh_source.digest());
    assert_eq!(source.relative_path(), fresh_source.relative_path());
    assert_ne!(source.identity(), fresh_source.identity());
    assert_eq!(
        snapshot.schema_fingerprint_hex(),
        fresh_snapshot.schema_fingerprint_hex(),
    );
    assert_eq!(session.document(), fresh_session.document());
    assert_eq!(
        session.authoring_readiness(),
        fresh_session.authoring_readiness()
    );
    assert_ne!(
        session.authoring_schema_basis(),
        fresh_session.authoring_schema_basis()
    );
    assert_closure(
        &snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap(),
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_closure(
        &fresh_snapshot
            .capture_scene_document(&fresh_source, &fresh_session, &[])
            .unwrap(),
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    let error = snapshot
        .capture_scene_document(&source, &fresh_session, &[])
        .err()
        .expect("an equivalent fresh Schema basis must not replace the original authority");
    assert!(matches!(
        error,
        AuthoringProjectSourceAdmissionError::SchemaBasisMismatch
    ));
}

#[test]
fn demanded_malformed_files_and_changed_root_or_schema_receipts_are_typed_refusals() {
    for (relative_path, expected_kind) in [
        ("prefabs/leaf.prefab.json", AuthoringSourceKind::Prefab),
        ("assets/leaf.png.meta", AuthoringSourceKind::AssetMetadata),
        (SECOND_SCENE, AuthoringSourceKind::Scene),
        (PROJECT_MANIFEST_PATH, AuthoringSourceKind::ProjectManifest),
        (BUNDLE_PATH, AuthoringSourceKind::SchemaBundle),
    ] {
        let project = independent_project();
        let (snapshot, source, session) = open_second_scene(&project);
        let startup_usage = snapshot.usage();
        let document = session.shared_document();
        assert_closure(
            &snapshot
                .capture_scene_document(&source, &session, &[])
                .unwrap(),
            &["leaf.prefab.json", "outer.prefab.json"],
            &["leaf.png", "outer.png"],
        );
        let path = project.root.join(relative_path);
        let bytes = if matches!(
            expected_kind,
            AuthoringSourceKind::Prefab | AuthoringSourceKind::AssetMetadata
        ) {
            b"{".to_vec()
        } else {
            // Whitespace preserves document meaning while invalidating the exact file receipt.
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b' ');
            bytes
        };
        fs::write(path, bytes).unwrap();
        let error = snapshot
            .capture_scene_document(&source, &session, &[])
            .err()
            .expect("a demanded malformed file or changed receipt must refuse capture");
        let expected_refusal = match (expected_kind, &error) {
            (
                AuthoringSourceKind::Prefab,
                AuthoringProjectSourceAdmissionError::SourceFormat {
                    kind: AuthoringSourceKind::Prefab,
                    path,
                    ..
                },
            ) => path.as_str() == relative_path,
            (
                AuthoringSourceKind::AssetMetadata,
                AuthoringProjectSourceAdmissionError::AssetMetadataFormat { path, .. },
            ) => path.as_str() == "assets/leaf.png",
            (
                kind @ (AuthoringSourceKind::Scene
                | AuthoringSourceKind::ProjectManifest
                | AuthoringSourceKind::SchemaBundle),
                AuthoringProjectSourceAdmissionError::SourceChanged {
                    kind: observed_kind,
                    path,
                },
            ) => *observed_kind == kind && path.as_str() == relative_path,
            _ => false,
        };
        assert!(
            expected_refusal,
            "unexpected refusal for {relative_path}: {error:?}"
        );
        assert_eq!(snapshot.usage(), startup_usage);
        assert!(Arc::ptr_eq(&document, &session.shared_document()));
    }
}

#[test]
fn open_prefab_edits_change_nested_demand_while_held_snapshots_and_disk_stay_unchanged() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (prefab_source, mut prefab_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let scene_revision = session.revision();
    let scene_document = session.shared_document();
    let scene_disk = fs::read(project.root.join(SECOND_SCENE)).unwrap();
    let prefab_disk = fs::read(project.root.join(OUTER_PREFAB)).unwrap();
    let original_document = prefab_session.shared_document();
    let original_content = prefab_session.content_state();
    let held = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    assert_eq!(held.source_key(), prefab_source.source_key());
    assert_eq!(held.revision(), prefab_session.revision());
    let before = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&held))
        .unwrap();
    assert_closure(
        &before,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        before.prefabs()[1].session_revision(),
        Some(held.revision())
    );
    assert!(std::ptr::eq(
        before.prefabs()[1].document(),
        original_document.as_ref(),
    ));
    assert!(matches!(
        before.prefabs()[1].publish_document(),
        Err(AuthoringDocumentDataError::SessionSnapshot)
    ));

    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: SceneEntityId::new("nested").unwrap(),
        expected: Some(instance("leaf")),
        value: Some(instance("alternate")),
    }]);
    let report = prefab_session.apply_authoring_patch(&patch);
    assert!(report.applied, "{:?}", report.diagnostics);
    let edited_document = prefab_session.shared_document();
    let edited = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    assert_ne!(edited.revision(), held.revision());
    assert_eq!(edited.revision().source_id(), held.revision().source_id());
    assert!(edited.revision().generation() > held.revision().generation());
    let selected = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&edited))
        .unwrap();
    assert_closure(
        &selected,
        &["alternate.prefab.json", "outer.prefab.json"],
        &["alternate.png", "outer.png"],
    );
    let row = &selected.prefabs()[1];
    assert_eq!(row.session_revision(), Some(prefab_session.revision()));
    assert_eq!(row.session_revision(), Some(edited.revision()));
    assert!(std::ptr::eq(row.document(), edited_document.as_ref()));
    assert_eq!(row.source().digest(), prefab_source.digest());
    assert_eq!(row.source().identity(), prefab_source.identity());
    assert!(matches!(
        row.publish_document(),
        Err(AuthoringDocumentDataError::SessionSnapshot)
    ));
    assert_eq!(session.revision(), scene_revision);
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        scene_disk
    );
    assert_eq!(
        fs::read(project.root.join(OUTER_PREFAB)).unwrap(),
        prefab_disk
    );

    let still_held = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&held))
        .unwrap();
    assert_closure(
        &still_held,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        still_held.prefabs()[1].session_revision(),
        Some(held.revision())
    );
    assert!(std::ptr::eq(
        still_held.prefabs()[1].document(),
        original_document.as_ref(),
    ));
    assert_eq!(still_held.usage(), before.usage());
    let from_disk = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &from_disk,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert!(
        from_disk
            .prefabs()
            .iter()
            .all(|row| row.session_revision().is_none())
    );
    assert_eq!(
        from_disk.prefabs()[1].document(),
        original_document.as_ref()
    );
    assert_eq!(
        from_disk.prefabs()[1]
            .publish_document()
            .unwrap()
            .document(),
        original_document.as_ref(),
    );

    let undone = prefab_session.undo_authoring();
    assert!(undone.applied, "{:?}", undone.diagnostics);
    assert_eq!(prefab_session.document(), original_document.as_ref());
    assert_eq!(prefab_session.content_state(), original_content);
    let restored_input = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    let restored = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&restored_input))
        .unwrap();
    assert_closure(
        &restored,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        restored.prefabs()[1].session_revision(),
        Some(prefab_session.revision())
    );
    assert_ne!(restored_input.revision(), held.revision());
    assert!(restored_input.revision().generation() > edited.revision().generation());
    let still_edited = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&edited))
        .unwrap();
    assert_closure(
        &still_edited,
        &["alternate.prefab.json", "outer.prefab.json"],
        &["alternate.png", "outer.png"],
    );
    assert_eq!(
        still_edited.prefabs()[1].session_revision(),
        Some(edited.revision())
    );
    assert!(std::ptr::eq(
        still_edited.prefabs()[1].document(),
        edited_document.as_ref(),
    ));
    assert_eq!(session.revision(), scene_revision);
    assert!(Arc::ptr_eq(&scene_document, &session.shared_document()));
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        scene_disk
    );
    assert_eq!(
        fs::read(project.root.join(OUTER_PREFAB)).unwrap(),
        prefab_disk
    );
}

#[test]
fn only_demanded_open_prefabs_activate_schema_basis_validation() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (fresh_snapshot, _, _) = open_second_scene(&project);
    let (unused_source, unused_session) =
        open_prefab(&fresh_snapshot, "prefabs/alternate.prefab.json");
    assert_ne!(
        unused_session.authoring_schema_basis(),
        session.authoring_schema_basis()
    );
    let unused = AuthoringPrefabSessionSnapshot::capture(&unused_source, &unused_session);
    let baseline = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    let with_unused = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&unused))
        .unwrap();
    assert_closure(
        &with_unused,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(with_unused.usage(), baseline.usage());
    assert_eq!(
        with_unused.source_facts().collect::<Vec<_>>(),
        baseline.source_facts().collect::<Vec<_>>(),
    );
    assert!(
        with_unused
            .prefabs()
            .iter()
            .all(|row| row.session_revision().is_none())
    );

    let (demanded_source, _) = open_prefab(&snapshot, OUTER_PREFAB);
    let (_, demanded_session) = open_prefab(&fresh_snapshot, OUTER_PREFAB);
    assert_eq!(
        demanded_source.source_key(),
        baseline.prefabs()[1].native_source().key(),
    );
    assert_eq!(
        demanded_session.document(),
        baseline.prefabs()[1].document()
    );
    let demanded = AuthoringPrefabSessionSnapshot::capture(&demanded_source, &demanded_session);
    let error = snapshot
        .capture_scene_document(&source, &session, &[unused, demanded])
        .err()
        .expect("an actually demanded Prefab must use the original shared Schema basis");
    assert!(matches!(
        error,
        AuthoringProjectSourceAdmissionError::SchemaBasisMismatch
    ));
}

#[test]
fn duplicate_canonical_prefab_sessions_are_ambiguous_even_at_the_same_generation() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (first_source, first_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let (second_source, second_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let first = AuthoringPrefabSessionSnapshot::capture(&first_source, &first_session);
    let second = AuthoringPrefabSessionSnapshot::capture(&second_source, &second_session);
    assert_eq!(first.source_key(), second.source_key());
    assert_eq!(
        first.revision().generation(),
        second.revision().generation()
    );
    assert_ne!(first.revision().source_id(), second.revision().source_id());
    for input in [&first, &second] {
        let selected = snapshot
            .capture_scene_document(&source, &session, std::slice::from_ref(input))
            .unwrap();
        assert_closure(
            &selected,
            &["leaf.prefab.json", "outer.prefab.json"],
            &["leaf.png", "outer.png"],
        );
        assert_eq!(
            selected.prefabs()[1].session_revision(),
            Some(input.revision())
        );
    }
    let error = snapshot
        .capture_scene_document(&source, &session, &[first, second])
        .err()
        .expect("two selected sessions with the same canonical source must be ambiguous");
    assert!(
        matches!(
            &error,
            AuthoringProjectSourceAdmissionError::AmbiguousPrefabSession { path }
                if path.as_str() == OUTER_PREFAB
        ),
        "{error:?}"
    );
}

#[test]
fn selected_prefab_session_rejects_a_stale_file_receipt_while_disk_capture_reads_current_bytes() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (prefab_source, prefab_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let scene_revision = session.revision();
    let startup_usage = snapshot.usage();
    let held = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    assert_closure(
        &snapshot
            .capture_scene_document(&source, &session, std::slice::from_ref(&held))
            .unwrap(),
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    let mut bytes = fs::read(project.root.join(OUTER_PREFAB)).unwrap();
    bytes.push(b' ');
    fs::write(project.root.join(OUTER_PREFAB), &bytes).unwrap();
    assert_ne!(ContentDigest::of_bytes(&bytes), prefab_source.digest());
    let error = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&held))
        .err()
        .expect("a selected working copy must retain an exact current file receipt");
    assert!(
        matches!(
            &error,
            AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: AuthoringSourceKind::Prefab,
                path,
            } if path.as_str() == OUTER_PREFAB
        ),
        "{error:?}"
    );
    let from_disk = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_closure(
        &from_disk,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    let row = &from_disk.prefabs()[1];
    assert_eq!(row.session_revision(), None);
    assert_eq!(row.source().digest(), ContentDigest::of_bytes(&bytes));
    assert_eq!(row.document(), prefab_session.document());
    assert!(row.publish_document().is_ok());
    assert_eq!(session.revision(), scene_revision);
    assert_eq!(snapshot.usage(), startup_usage);
}

#[test]
fn owned_request_keeps_root_and_prefab_snapshots_after_both_sessions_change_and_drop() {
    let project = independent_project();
    let (snapshot, source, mut session) = open_second_scene(&project);
    let (prefab_source, mut prefab_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let scene_revision = session.revision();
    let prefab_revision = prefab_session.revision();
    let scene_disk = fs::read(project.root.join(SECOND_SCENE)).unwrap();
    let prefab_disk = fs::read(project.root.join(OUTER_PREFAB)).unwrap();
    let original_prefab = prefab_session.shared_document();
    let input = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    let mut expected = snapshot
        .capture_scene_document(&source, &session, std::slice::from_ref(&input))
        .unwrap();
    let expected_preview = expected.take_preview().unwrap();
    let inputs = snapshot
        .admit_source_capture_inputs(
            0,
            AuthoringPrefabSessionSnapshot::capture_retained_bytes(&prefab_source, &prefab_session)
                .unwrap(),
        )
        .unwrap();
    let request = snapshot
        .capture_scene_request(&source, &session, inputs, vec![input].into_boxed_slice())
        .unwrap();
    let prefab_edit = prefab_session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("nested").unwrap(),
            expected: Some(instance("leaf")),
            value: Some(instance("alternate")),
        },
    ]));
    assert!(prefab_edit.applied, "{:?}", prefab_edit.diagnostics);
    let root_edit = session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected: Some(instance("outer")),
            value: Some(instance("leaf")),
        },
    ]));
    assert!(root_edit.applied, "{:?}", root_edit.diagnostics);
    let edited_input = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    assert_closure(
        &snapshot
            .capture_scene_document(&source, &session, &[edited_input])
            .unwrap(),
        &["leaf.prefab.json"],
        &["leaf.png"],
    );
    assert_ne!(session.revision(), scene_revision);
    assert_ne!(prefab_session.revision(), prefab_revision);
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        scene_disk
    );
    assert_eq!(
        fs::read(project.root.join(OUTER_PREFAB)).unwrap(),
        prefab_disk
    );
    drop(session);
    drop(prefab_session);
    drop(snapshot);

    let mut prepared = request.prepare().unwrap();
    assert_eq!(prepared.root_revision(), Some(scene_revision));
    assert_closure(
        &prepared,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        prepared.prefabs()[1].session_revision(),
        Some(prefab_revision)
    );
    assert!(std::ptr::eq(
        prepared.prefabs()[1].document(),
        original_prefab.as_ref(),
    ));
    assert_eq!(prepared.usage(), expected.usage());
    assert_eq!(
        prepared.source_facts().collect::<Vec<_>>(),
        expected.source_facts().collect::<Vec<_>>(),
    );
    let preview = prepared.take_preview().unwrap();
    assert_eq!(preview.candidate.source_key(), source.source_key());
    assert_eq!(preview.candidate, expected_preview.candidate);
    assert_eq!(preview.omissions, expected_preview.omissions);
    assert_eq!(source.digest(), ContentDigest::of_bytes(&scene_disk));
    assert_eq!(
        prefab_source.digest(),
        ContentDigest::of_bytes(&prefab_disk)
    );
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        scene_disk
    );
    assert_eq!(
        fs::read(project.root.join(OUTER_PREFAB)).unwrap(),
        prefab_disk
    );
}

#[test]
fn scene_request_rejects_same_basis_wrong_source_even_when_documents_are_identical() {
    const OTHER_SCENE: &str = "scenes/equivalent.scene.json";
    let project = independent_project();
    let bytes = fs::read(project.root.join(SECOND_SCENE)).unwrap();
    fs::write(project.root.join(OTHER_SCENE), &bytes).unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let AdmittedAuthoringDocument::Scene {
        source: other_source,
        document,
        ..
    } = snapshot
        .capture_indexed_document(
            snapshot.source_index_snapshot().unwrap(),
            Arc::from(OTHER_SCENE),
            AuthoringDocumentKind::Scene,
        )
        .unwrap()
        .prepare()
        .unwrap()
    else {
        panic!("the equivalent indexed Scene request changed document kind");
    };
    let other_session = SceneAuthoringSession::from_published_authoring_document(document);
    assert_eq!(session.document(), other_session.document());
    assert_eq!(
        session.authoring_schema_basis(),
        other_session.authoring_schema_basis()
    );
    assert_eq!(source.digest(), other_source.digest());
    assert_ne!(source.source_key(), other_source.source_key());
    for (receipt, working) in [(&source, &other_session), (&other_source, &session)] {
        let error = snapshot
            .capture_scene_request(
                receipt,
                working,
                snapshot.admit_source_capture_inputs(0, 0).unwrap(),
                Box::new([]),
            )
            .err()
            .expect("equal document content cannot authorize a different containing source");
        assert!(matches!(
            error,
            AuthoringProjectSourceAdmissionError::DocumentSourceMismatch
        ));
    }
    let admitted = snapshot
        .capture_scene_request(
            &other_source,
            &other_session,
            snapshot.admit_source_capture_inputs(0, 0).unwrap(),
            Box::new([]),
        )
        .unwrap()
        .prepare()
        .unwrap();
    assert_closure(
        &admitted,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert!(admitted.source_facts().any(|fact| {
        fact.project_path().as_str() == OTHER_SCENE && fact.digest() == other_source.digest()
    }));
}

#[test]
fn selected_prefab_rejects_same_basis_wrong_source_even_when_documents_are_identical() {
    const OTHER_PREFAB: &str = "prefabs/equivalent.prefab.json";
    let project = independent_project();
    let bytes = fs::read(project.root.join(OUTER_PREFAB)).unwrap();
    fs::write(project.root.join(OTHER_PREFAB), &bytes).unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let (prefab_source, prefab_session) = open_prefab(&snapshot, OUTER_PREFAB);
    let (other_source, other_session) = open_prefab(&snapshot, OTHER_PREFAB);
    assert_eq!(prefab_session.document(), other_session.document());
    assert_eq!(
        prefab_session.authoring_schema_basis(),
        other_session.authoring_schema_basis(),
    );
    assert_eq!(prefab_source.digest(), other_source.digest());
    assert_ne!(prefab_source.source_key(), other_source.source_key());
    let mismatched = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &other_session);
    assert_eq!(mismatched.source_key(), prefab_source.source_key());
    let error = snapshot
        .capture_scene_document(&source, &session, &[mismatched])
        .err()
        .expect("the selected Prefab session locator must match its actual source receipt");
    assert!(
        matches!(
            &error,
            AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: AuthoringSourceKind::Prefab,
                path,
            } if path.as_str() == OUTER_PREFAB
        ),
        "{error:?}"
    );
    let correct = AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session);
    let admitted = snapshot
        .capture_scene_document(&source, &session, &[correct])
        .unwrap();
    assert_closure(
        &admitted,
        &["leaf.prefab.json", "outer.prefab.json"],
        &["leaf.png", "outer.png"],
    );
    assert_eq!(
        admitted.prefabs()[1].session_revision(),
        Some(prefab_session.revision()),
    );
}
