use nara_scene::{
    PrefabDeclarationSource, PrefabEntityTarget, PrefabExpansionFailureKind, PrefabOverlay,
    PrefabOverlaySlot, ScenePatchDocument, ScenePatchOperation,
};

use super::*;

#[test]
fn actual_source_capture_retains_typed_failure_and_fences_its_schema_and_prefab_receipts() {
    let project = independent_project();
    let disk_scene = fs::read(project.root.join(SECOND_SCENE)).unwrap();
    let (snapshot, source, mut session) = open_second_scene(&project);
    let baseline = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    let key = baseline
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "outer.prefab.json")
        .unwrap()
        .native_source()
        .key()
        .clone();
    let expected = session.document().entities[0].prefab.clone();
    let mut replacement = expected.clone().unwrap();
    let mut overlay = PrefabOverlay::new(key);
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("missing-in-base").unwrap(),
    ));
    replacement.overrides = Some(overlay);
    let report = session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected,
            value: Some(replacement),
        },
    ]));
    assert!(report.applied, "{:?}", report.diagnostics);
    drop(baseline);
    let revision = session.revision();
    let mut capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Degraded);
    assert_eq!(capture.root_revision(), Some(revision));
    assert!(capture.take_preview().is_none());
    assert!(!capture.requires_recapture());
    assert!(capture.diagnostics().has_errors());
    let failure = capture.composition_failure().unwrap();
    assert_eq!(failure.reason(), &PrefabExpansionFailureKind::MissingTarget);
    let location = failure.location().unwrap();
    assert_eq!(location.source(), &PrefabDeclarationSource::Containing);
    assert_eq!(location.path().anchor.as_str(), "anchor");
    assert_eq!(
        failure.slot(),
        Some(PrefabOverlaySlot::Suppression { index: 0 })
    );
    let retained = composition_failure_retained_bytes(failure).unwrap();
    assert_eq!(
        retained,
        size_of::<nara_scene::PrefabExpansionFailure>()
            + crate::scene_retention::string_retained_bytes("anchor").unwrap(),
    );
    assert!(capture.usage().retained_candidate_bytes() >= retained);
    assert_eq!(capture.prefabs().len(), 1);
    assert_eq!(capture.prefabs()[0].path().as_str(), "outer.prefab.json");
    assert_eq!(capture.source_facts().len(), 2);
    capture.source_validation().validate().unwrap();

    for relative in ["prefabs/outer.prefab.json", "schemas/authoring.bundle.json"] {
        let path = project.root.join(relative);
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.push(b' ');
        fs::write(&path, changed).unwrap();
        assert!(matches!(
            capture.source_validation().validate(),
            Err(AuthoringProjectSourceAdmissionError::SourceChanged { .. }),
        ));
        fs::write(path, original).unwrap();
        capture.source_validation().validate().unwrap();
    }
    assert_eq!(session.revision(), revision);
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        disk_scene
    );
}

#[test]
fn missing_source_capture_keeps_analysis_and_recovers_only_after_demand_runs_again() {
    let project = independent_project();
    let path = project.root.join("prefabs/leaf.prefab.json");
    let leaf_bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let revision = session.revision();
    let mut missing = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(missing.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(missing.take_preview().is_none());
    assert!(missing.composition_failure().is_some());
    assert!(missing.requires_recapture());
    assert_eq!(missing.unresolved_dependencies.len(), 1);
    missing.source_validation().validate().unwrap();

    fs::write(path, &leaf_bytes).unwrap();
    // Existing receipts remain valid both before and after creating the missing file.
    missing.source_validation().validate().unwrap();
    let mut recovered = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert!(recovered.composition_failure().is_none());
    assert!(!recovered.requires_recapture());
    assert!(recovered.unresolved_dependencies.is_empty());
    assert_eq!(recovered.prefabs().len(), 2);
    assert!(recovered.take_preview().is_some());
    assert_eq!(recovered.root_revision(), Some(revision));
    assert_eq!(session.revision(), revision);
}
