use std::{fs, sync::Arc};

use nara_core::ByteLimit;
use nara_reflect::AuthoringSchemaBundleOwner;
use nara_scene::{
    __private::borrow_prepared_scene_schema_source, SceneEntityId, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner, SchemaReclassificationWork,
};

use super::owned_input_tests::{installed_owner, open_project};
use super::*;
use crate::project_host::{
    authoring::tests::independent_capture::independent_project,
    file_publication::FilePublicationOutcome,
};

const BUNDLE: &str = "schemas/authoring.bundle.json";

#[test]
fn observed_successor_reprepares_dirty_source_and_attaches_without_another_write_or_basis() {
    let project = independent_project();
    let snapshot = open_project(&project);
    let source = crate::project_host::authoring::document::AuthoringDocumentSourceFact::from_captured_source(
        snapshot.startup_source(), snapshot.startup_native_source(),
    ).unwrap();
    let mut scene = nara_scene::SceneAuthoringSession::from_published_authoring_document(
        snapshot.fork_startup_scene(),
    );
    let input = snapshot.schema_owner_input();
    let AuthoringSchemaOwnerPreparation::Prepared {
        owner: original,
        mut publication,
    } = input.prepare_owner(&installed_owner()).unwrap()
    else {
        panic!("the owner must be missing");
    };
    assert!(Arc::ptr_eq(original.incumbent(), original.observation()));
    let old_successor_basis = original.schema_basis().clone();
    let FilePublicationOutcome::Published(accepted) = publication.publish(&original.bytes()) else {
        panic!("the first operation must actually publish");
    };
    let published_identity = accepted.source().identity();
    let published_digest = accepted.source().digest();
    // Keep the actual original publication alive separately from reconciliation preparation.
    let (evidence, published_leaf) = accepted.into_parts();
    let bytes = fs::read(project.root.join(BUNDLE)).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("after-publication").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    assert!(scene.apply_authoring_patch(&patch).applied);
    let revision = scene.revision();
    let content = scene.content_state();
    let document = scene.shared_document();

    let AuthoringSchemaOwnerReconciliation::Observed(mut owner) =
        input.prepare_observed_owner(&installed_owner()).unwrap()
    else {
        panic!("published metadata must produce an observed preparation");
    };
    assert!(input.matches(&snapshot));
    assert!(Arc::ptr_eq(owner.incumbent(), &input.incumbent));
    assert!(!Arc::ptr_eq(owner.incumbent(), owner.observation()));
    assert_eq!(owner.observation().bundle_identity(), published_identity);
    assert_eq!(owner.observation().bundle_digest(), published_digest);
    assert_ne!(owner.schema_basis(), &old_successor_basis);
    let basis = owner.schema_basis().clone();
    let saved = owner.prepare_project_input(&input, &[]).unwrap();
    let mut work = SchemaReclassificationWork::default();
    let prepared_scene = scene
        .prepare_schema_reclassification(&basis, &mut work)
        .unwrap();
    let capture = owner
        .capture_scene(
            &source,
            borrow_prepared_scene_schema_source(&scene, &prepared_scene).unwrap(),
            &[],
        )
        .unwrap();
    owner.validate().unwrap();
    saved.validate(&mut owner).unwrap();
    capture.validate(&mut owner).unwrap();
    let observed = owner.attach_observed().unwrap();
    let capture = capture.attach_observed(&observed).ok().unwrap();
    assert_eq!(capture.root_revision(), Some(revision));
    capture.source_validation().validate().unwrap();
    let successor = saved
        .attach_observed(observed)
        .unwrap()
        .into_session_snapshot()
        .unwrap();
    scene
        .commit_schema_reclassification(prepared_scene)
        .unwrap();
    assert_eq!(scene.authoring_schema_basis(), Some(basis.clone()));
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.content_state(), content);
    assert!(Arc::ptr_eq(&document, &scene.shared_document()));
    assert_eq!(successor.fork_startup_scene().schema_basis(), basis);
    assert_eq!(successor.schema.bundle_identity(), published_identity);
    assert_eq!(successor.authoring_bundle_digest(), published_digest);
    assert_eq!(fs::read(project.root.join(BUNDLE)).unwrap(), bytes);
    assert_eq!(published_leaf.identity(), published_identity);
    assert_eq!(
        evidence.receipt().published_identity(),
        Some(published_identity)
    );
    assert_eq!(
        evidence.receipt().verified_content_digest(),
        Some(published_digest)
    );
}

#[test]
fn observed_owner_refuses_changed_manifest_dropped_lineage_and_reviewed_leaf_drift() {
    let project = independent_project();
    let snapshot = open_project(&project);
    let input = snapshot.schema_owner_input();
    let installed = installed_owner();
    let AuthoringSchemaOwnerPreparation::Prepared { owner, .. } =
        input.prepare_owner(&installed).unwrap()
    else {
        panic!("the owner must be missing");
    };
    let successor = owner.bytes();
    // An owner-only bundle must not erase the incumbent's complete catalog or lineage.
    let dropped = AuthoringSchemaBundle::from_owner_snapshots([installed.clone()]).unwrap();
    fs::write(project.root.join(BUNDLE), dropped.to_json_string().unwrap()).unwrap();
    assert!(matches!(
        input.prepare_observed_owner(&installed),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    // A changed current catalog for the requested owner is not its exact installed snapshot.
    let mut changed = installed.current().clone();
    changed.components[0]
        .aliases
        .push("Changed installed schema".into());
    let changed = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        installed.owner_id().clone(),
        changed,
        None,
    )])
    .unwrap();
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(
        snapshot
            .authoring_bundle()
            .owners()
            .iter()
            .cloned()
            .chain(changed.owners().iter().cloned()),
    )
    .unwrap();
    fs::write(project.root.join(BUNDLE), bundle.to_json_string().unwrap()).unwrap();
    assert!(matches!(
        input.prepare_observed_owner(&installed),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    fs::write(project.root.join(BUNDLE), &successor).unwrap();
    let AuthoringSchemaOwnerReconciliation::Observed(mut observed) =
        input.prepare_observed_owner(&installed).unwrap()
    else {
        panic!("the exact successor must prepare");
    };
    let saved = observed.prepare_project_input(&input, &[]).unwrap();
    observed.validate().unwrap();
    let mut changed = successor.to_vec();
    changed.push(b' ');
    fs::write(project.root.join(BUNDLE), changed).unwrap();
    assert!(matches!(
        observed.validate(),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    assert!(matches!(
        saved.validate(&mut observed),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged {
            kind: AuthoringSourceKind::SchemaBundle,
            ..
        })
    ));
    fs::write(project.root.join(BUNDLE), &successor).unwrap();
    let manifest_path = project.root.join("nara.toml");
    let mut manifest = fs::read(&manifest_path).unwrap();
    manifest.push(b' ');
    fs::write(&manifest_path, manifest).unwrap();
    assert!(matches!(
        input.prepare_observed_owner(&installed),
        Err(AuthoringSchemaOwnerError::ManifestChanged)
    ));
}

#[test]
fn observed_preparation_falls_back_to_publication_and_retains_refused_input_reservations() {
    let project = independent_project();
    let snapshot = open_project(&project);
    let input = snapshot.schema_owner_input();
    let AuthoringSchemaOwnerReconciliation::Publication(
        AuthoringSchemaOwnerPreparation::Prepared { mut owner, .. },
    ) = input.prepare_observed_owner(&installed_owner()).unwrap()
    else {
        panic!("the original actual bundle must use ordinary publication");
    };
    assert!(Arc::ptr_eq(owner.incumbent(), owner.observation()));
    let before = owner.budget().usage().retained_candidate_bytes();
    owner.retain_operation_inputs(64).unwrap();
    assert_eq!(
        owner.budget().usage().retained_candidate_bytes(),
        before + 64
    );
    let mut budget = owner.budget();
    let maximum = budget.usage().retained_candidate_bytes() + 1;
    budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    owner.retain_capture_budget(budget);
    assert!(matches!(
        owner.retain_operation_inputs(2),
        Err(AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: observed
        }) if observed == maximum
    ));
    assert!(owner.budget().usage().retained_candidate_bytes() > maximum);
    assert!(owner.retain_operation_inputs(0).is_err());
    assert!(owner.attach_observed().is_err());
}
