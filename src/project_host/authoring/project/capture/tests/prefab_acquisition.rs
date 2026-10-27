//! Actual source-only acquisition through nominal conversion requests and physical files.

use std::convert::Infallible;

use nara_scene::{
    PrefabAuthoringSession, PrefabEntityTarget, PrefabExpansionOptions, PrefabOverlay,
    ScenePatchDocument, ScenePatchOperation, SourceDocumentKind,
};

use super::*;
use crate::project_host::authoring::{
    AdmittedAuthoringDocument, AuthoringDocumentKind, document::AuthoringDocumentSourceFact,
};

// A conservative bound for these fixed, small prepared-token fixtures.
const PREPARED_FIXTURE_BYTES: usize = 1024 * 1024;

fn instance(path: &str) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path(format!("{path}.prefab.json")).unwrap(),
        overrides: None,
    }
}

fn anchor(id: &str, path: &str) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
    entity.prefab = Some(instance(path));
    entity
}

fn open_prefab(
    snapshot: &AuthoringProjectSessionSnapshot,
    path: &str,
) -> (AuthoringDocumentSourceFact, PrefabAuthoringSession) {
    let AdmittedAuthoringDocument::Prefab {
        source, document, ..
    } = snapshot
        .capture_indexed_document(
            snapshot.source_index_snapshot().unwrap(),
            Arc::from(path),
            AuthoringDocumentKind::Prefab,
        )
        .unwrap()
        .prepare()
        .unwrap()
    else {
        panic!("the real Prefab source must retain its nominal document kind");
    };
    (
        source,
        PrefabAuthoringSession::from_published_authoring_document(document),
    )
}

#[test]
fn prefab_conversion_acquires_real_prefab_root_and_never_hydrates_effective_images() {
    let project = independent_project();
    let (snapshot, _, _) = open_second_scene(&project);
    let (source, mut session) = open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let revision = session.revision();
    let basis = session.authoring_schema_basis().unwrap();
    assert!(matches!(
        snapshot.capture_prefab_conversion_request(
            &source,
            &basis,
            SourceDocumentKind::Scene,
            Box::new([]),
        ),
        Err(AuthoringProjectSourceAdmissionError::NativeSource(
            crate::project_host::authoring::AuthoringDocumentAdmissionError::DocumentKindMismatch,
        )),
    ));
    // These files exist and would fail resource admission if conversion tried to hydrate them.
    fs::write(project.root.join("assets/leaf.png"), b"invalid image").unwrap();
    fs::write(
        project.root.join("assets/leaf.png.meta"),
        b"invalid metadata",
    )
    .unwrap();
    let mut request = session
        .request_complete_prefab_conversion(
            revision,
            &SceneEntityId::new("nested").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            &basis,
            SourceDocumentKind::Prefab,
            Box::new([]),
        )
        .unwrap();
    context.retain_root_input(session.document().retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let prepared = request
        .prepare_with_loader(None, |reference, working| {
            acquisition.load(reference, working)
        })
        .unwrap();
    assert_eq!(prepared.effective_asset_references().len(), 1);
    let captured = acquisition
        .finish_source_capture(PREPARED_FIXTURE_BYTES)
        .unwrap();
    assert_eq!(captured.prefabs().len(), 1);
    assert_eq!(captured.prefabs()[0].path().as_str(), "leaf.prefab.json");
    assert_eq!(captured.prefabs()[0].session_revision(), None);
    assert!(!captured.requires_recapture());
    let validation = captured.source_validation();
    assert_eq!(
        validation
            .sources
            .iter()
            .map(|fact| (fact.project_path().as_str(), fact.kind()))
            .collect::<Vec<_>>(),
        [
            ("prefabs/leaf.prefab.json", AuthoringSourceKind::Prefab),
            ("prefabs/outer.prefab.json", AuthoringSourceKind::Prefab),
        ],
    );
    validation.validate().unwrap();
    let report = session
        .commit_complete_prefab_conversion_with_loader(prepared, |reference| {
            Ok::<_, Infallible>(captured.resolve(reference))
        })
        .unwrap();
    assert!(report.report().applied);
    assert_ne!(session.revision(), revision);
}

#[test]
fn selected_scene_conversion_does_not_activate_independent_missing_sources_or_assets() {
    let project = independent_project();
    fs::write(
        project.root.join(SECOND_SCENE),
        SceneDocument::from_ordered_roots([
            anchor("anchor", "outer"),
            anchor("unrelated", "missing"),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let mut request = session
        .request_complete_prefab_conversion(
            session.revision(),
            &SceneEntityId::new("anchor").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            &session.authoring_schema_basis().unwrap(),
            SourceDocumentKind::Scene,
            Box::new([]),
        )
        .unwrap();
    context.retain_root_input(session.document().retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let prepared = request
        .prepare_with_loader(None, |reference, working| {
            acquisition.load(reference, working)
        })
        .unwrap();
    let captured = acquisition
        .finish_source_capture(PREPARED_FIXTURE_BYTES)
        .unwrap();
    assert_eq!(
        captured
            .prefabs()
            .iter()
            .map(|source| source.path().as_str())
            .collect::<Vec<_>>(),
        ["leaf.prefab.json", "outer.prefab.json"],
    );
    assert!(captured.resolve(&instance("missing").source).is_none());
    assert_eq!(prepared.effective_asset_references().len(), 2);
    assert!(!captured.requires_recapture());
    assert_eq!(
        prepared
            .document()
            .entities
            .iter()
            .find(|entity| entity.id.as_str() == "unrelated")
            .unwrap()
            .prefab,
        Some(instance("missing")),
    );
    captured.source_validation().validate().unwrap();
}

#[test]
fn finite_containing_prefab_demand_uses_the_actual_unsaved_root_working_copy() {
    let project = independent_project();
    fs::write(
        project.root.join("prefabs/outer.prefab.json"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("leaf").unwrap()),
            anchor("nested", "outer"),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let (snapshot, _, _) = open_second_scene(&project);
    let (source, session) = open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let mut session = session;
    let mut overlay = PrefabOverlay::new(source.source_key().clone());
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("nested").unwrap(),
    ));
    let declaration = PrefabInstance {
        source: instance("outer").source,
        overrides: Some(overlay),
    };
    let report = session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("nested").unwrap(),
            expected: Some(instance("outer")),
            value: Some(declaration),
        },
    ]));
    assert!(report.applied, "{:?}", report.diagnostics);
    let mut request = session
        .request_complete_prefab_conversion(
            session.revision(),
            &SceneEntityId::new("nested").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            &session.authoring_schema_basis().unwrap(),
            SourceDocumentKind::Prefab,
            vec![AuthoringPrefabSessionSnapshot::capture(&source, &session)].into_boxed_slice(),
        )
        .unwrap();
    context.retain_root_input(session.document().retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let prepared = request
        .prepare_with_loader(None, |reference, working| {
            acquisition.load(reference, working)
        })
        .unwrap();
    assert_eq!(prepared.entities().len(), 1);
    let captured = acquisition
        .finish_source_capture(PREPARED_FIXTURE_BYTES)
        .unwrap();
    assert_eq!(captured.prefabs().len(), 1);
    assert_eq!(
        captured.prefabs()[0].session_revision(),
        Some(session.revision()),
    );
    assert_eq!(
        captured
            .resolve(&instance("outer").source)
            .unwrap()
            .document(),
        session.document(),
    );
    captured.source_validation().validate().unwrap();
}

#[test]
fn selected_conversion_retains_unsaved_working_copy_and_file_participants_separately() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let (prefab_source, mut prefab_session) = open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let (unused_source, unused_session) = open_prefab(&snapshot, "prefabs/leaf.prefab.json");
    let report = prefab_session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("nested").unwrap(),
            expected: Some(instance("leaf")),
            value: Some(instance("alternate")),
        },
    ]));
    assert!(report.applied, "{:?}", report.diagnostics);
    let held = prefab_session.shared_document();
    let mut request = session
        .request_complete_prefab_conversion(
            session.revision(),
            &SceneEntityId::new("anchor").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            &session.authoring_schema_basis().unwrap(),
            SourceDocumentKind::Scene,
            vec![
                AuthoringPrefabSessionSnapshot::capture(&prefab_source, &prefab_session),
                AuthoringPrefabSessionSnapshot::capture(&unused_source, &unused_session)
                    .without_persistence(),
            ]
            .into_boxed_slice(),
        )
        .unwrap();
    context.retain_root_input(session.document().retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let prepared = request
        .prepare_with_loader(None, |reference, working| {
            acquisition.load(reference, working)
        })
        .unwrap();
    let captured = acquisition
        .finish_source_capture(PREPARED_FIXTURE_BYTES)
        .unwrap();
    assert_eq!(
        captured
            .prefabs()
            .iter()
            .map(|source| source.path().as_str())
            .collect::<Vec<_>>(),
        ["alternate.prefab.json", "outer.prefab.json"],
    );
    let selected = &captured.prefabs()[1];
    assert_eq!(selected.session_revision(), Some(prefab_session.revision()));
    assert_eq!(selected.native_source().key(), prefab_source.source_key());
    assert_eq!(
        selected.source().project_path().as_str(),
        prefab_source.relative_path()
    );
    assert!(std::ptr::eq(selected.document(), held.as_ref()));
    assert_eq!(captured.prefabs()[0].session_revision(), None);
    assert_eq!(prepared.effective_asset_references().len(), 2);
    assert_eq!(
        captured
            .resolve(&instance("outer").source)
            .unwrap()
            .document(),
        held.as_ref(),
    );
    captured.source_validation().validate().unwrap();
}

#[test]
fn selected_conversion_retains_path_and_stable_aliases_and_rejects_new_lookup_ambiguity() {
    let project = independent_project();
    let stable_id = StableAssetId::parse_str("a3ee3f72-c05a-4db8-b330-a765b37b965e").unwrap();
    let mut outer =
        PrefabDocument::from_ordered_roots([anchor("path", "leaf"), anchor("stable", "leaf")]);
    outer.entities[1].prefab.as_mut().unwrap().source = AssetRef::StableId(stable_id);
    fs::write(
        project.root.join("prefabs/outer.prefab.json"),
        outer.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/leaf.prefab.json.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new("leaf.prefab.json").unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    // Stable lookup enumerates both directories, including this otherwise unused canary.
    fs::write(
        project.root.join("assets/unused.png.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("0c3c39f0-9ac2-43fe-a8d6-74f3be852466").unwrap(),
                AssetPath::new("unused.png").unwrap(),
                AssetSourceKind::Image,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let mut request = session
        .request_complete_prefab_conversion(
            session.revision(),
            &SceneEntityId::new("anchor").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            &session.authoring_schema_basis().unwrap(),
            SourceDocumentKind::Scene,
            Box::new([]),
        )
        .unwrap();
    context.retain_root_input(session.document().retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let _prepared = request
        .prepare_with_loader(None, |reference, working| {
            acquisition.load(reference, working)
        })
        .unwrap();
    let captured = acquisition
        .finish_source_capture(PREPARED_FIXTURE_BYTES)
        .unwrap();
    assert_eq!(captured.prefabs().len(), 2);
    let path = captured.resolve(&instance("leaf").source).unwrap();
    let stable = captured.resolve(&AssetRef::StableId(stable_id)).unwrap();
    assert_eq!(path.key(), stable.key());
    assert!(std::ptr::eq(path.document(), stable.document()));
    captured.source_validation().validate().unwrap();
    fs::write(
        project.root.join("assets/new-conflict.png.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new("new-conflict.png").unwrap(),
                AssetSourceKind::Image,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        captured.source_validation().validate(),
        Err(AuthoringProjectSourceAdmissionError::AssetIdentity {
            source: AssetDatabaseError::DuplicateStableId { .. },
            ..
        }),
    ));
}
