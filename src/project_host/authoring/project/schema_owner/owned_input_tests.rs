use std::{fs, sync::Arc, thread};

use nara_core::ItemLimit;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerChange, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, PersistentComponentSchemaOwnerId,
};
use nara_scene::{__private::borrow_prepared_scene_schema_source, SchemaReclassificationWork};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, project_authoring_adapters,
    tests::{
        TestAuthoringProject,
        independent_capture::{independent_project, open_second_scene},
    },
};

pub(super) fn installed_owner() -> ComponentSchemaOwnerSnapshot {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.owned-project-input").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.OwnedProjectInput"),
                    "Owned project input",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
    .owners()[0]
        .clone()
}

pub(super) fn open_project(project: &TestAuthoringProject) -> AuthoringProjectSessionSnapshot {
    let root = crate::project_tooling::host_fs::writable_directory(&project.root).unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap()
}

#[test]
fn cloned_input_prepares_on_its_actual_root_after_the_live_project_is_dropped() {
    fn assert_owned_input<T: Clone + Send + Sync + 'static>() {}
    assert_owned_input::<AuthoringSchemaOwnerProjectInput>();

    let project = independent_project();
    let snapshot = open_project(&project);
    let input = snapshot.schema_owner_input();
    let owned = input.clone();
    let root_identity = snapshot.project_root().identity();
    let basis = snapshot.startup_scene.schema_basis();
    let startup = Arc::downgrade(&snapshot.startup_scene);
    assert!(input.matches(&snapshot));
    assert!(owned.matches(&snapshot));
    assert!(Arc::ptr_eq(&input.incumbent, &snapshot.schema));
    assert!(Arc::ptr_eq(&input.incumbent, &owned.incumbent));
    assert_eq!(owned.schema_basis(), &basis);
    drop(snapshot);
    drop(input);
    assert!(startup.upgrade().is_none());

    // There are no live documents or Workspace participants in this worker request.
    thread::spawn(move || {
        assert_eq!(owned.project_root().identity(), root_identity);
        assert_eq!(owned.schema_basis(), &basis);
        let AuthoringSchemaOwnerPreparation::Prepared { mut owner, .. } = owned
            .prepare_owner(&installed_owner(), ComponentSchemaOwnerChange::Add)
            .unwrap()
        else {
            panic!("the missing owner must prepare from the retained actual authority");
        };
        assert!(Arc::ptr_eq(owner.incumbent(), &owned.incumbent));
        let successor_basis = owner.schema_basis().clone();
        let prepared = owner.prepare_project_input(&owned, &[]).unwrap();
        assert_eq!(prepared.startup.document.schema_basis(), successor_basis);
        assert_eq!(owner.schema_basis(), &successor_basis);
        assert_eq!(prepared.readiness(), AuthoringProjectReadiness::Complete);
        prepared.validate(&mut owner).unwrap();
        owner.validate().unwrap();
    })
    .join()
    .unwrap();
}

#[test]
fn input_rejects_foreign_and_independently_readmitted_project_authority() {
    let project = independent_project();
    let snapshot = open_project(&project);
    let readmitted = open_project(&project);
    let foreign_project = independent_project();
    let foreign = open_project(&foreign_project);
    let input = snapshot.schema_owner_input();
    assert!(input.matches(&snapshot));
    assert!(
        snapshot
            .project_root()
            .same_object(&readmitted.project_root())
            .unwrap()
    );
    assert_eq!(
        snapshot.authoring_bundle_digest(),
        readmitted.authoring_bundle_digest()
    );
    assert!(!input.matches(&readmitted));
    assert!(!input.matches(&foreign));

    let AuthoringSchemaOwnerPreparation::Prepared { mut owner, .. } = input
        .prepare_owner(&installed_owner(), ComponentSchemaOwnerChange::Add)
        .unwrap()
    else {
        panic!("the missing owner must prepare");
    };
    let usage = owner.budget().usage();
    for other in [&readmitted, &foreign] {
        assert!(matches!(
            owner.prepare_project_input(&other.schema_owner_input(), &[]),
            Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)
        ));
        assert_eq!(owner.budget().usage(), usage);
    }
    owner.prepare_project_input(&input, &[]).unwrap();
}

#[test]
fn current_capture_validation_keeps_saved_project_budget_and_original_source_observations() {
    let project = independent_project();
    let (snapshot, source, scene) = open_second_scene(&project);
    let input = snapshot.schema_owner_input();
    let AuthoringSchemaOwnerPreparation::Prepared { mut owner, .. } = input
        .prepare_owner(&installed_owner(), ComponentSchemaOwnerChange::Add)
        .unwrap()
    else {
        panic!("the missing owner must prepare");
    };
    let saved = owner.prepare_project_input(&input, &[]).unwrap();
    let saved_work = owner.budget().usage().decode_work();
    let mut work = SchemaReclassificationWork::default();
    let prepared_scene = scene
        .prepare_schema_reclassification(
            owner.schema_basis(),
            owner.reclassification_kind(),
            &mut work,
        )
        .unwrap();
    let capture = owner
        .capture_scene(
            &source,
            borrow_prepared_scene_schema_source(&scene, &prepared_scene).unwrap(),
            &[],
        )
        .unwrap();
    assert!(owner.budget().usage().decode_work() > saved_work);
    saved.validate(&mut owner).unwrap();
    let before = owner.budget().usage().decode_work();
    capture.validate(&mut owner).unwrap();
    let once = owner.budget().usage().decode_work();
    assert!(once > before);
    capture.validate(&mut owner).unwrap();
    assert!(owner.budget().usage().decode_work() > once);

    let AuthoringSchemaOwnerPreparation::Prepared {
        owner: mut other, ..
    } = input
        .prepare_owner(&installed_owner(), ComponentSchemaOwnerChange::Add)
        .unwrap()
    else {
        panic!("the independently prepared owner must remain unpublished");
    };
    let other_usage = other.budget().usage();
    assert!(matches!(
        capture.validate(&mut other),
        Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)
    ));
    assert_eq!(other.budget().usage(), other_usage);

    // Only the current Scene demands this asset; the saved startup has no such dependency.
    let asset_path = project.root.join("assets/outer.png");
    let original = fs::read(&asset_path).unwrap();
    let mut changed = original.clone();
    changed.push(b' ');
    fs::write(&asset_path, changed).unwrap();
    saved.validate(&mut owner).unwrap();
    let before_drift = owner.budget().usage().decode_work();
    assert!(matches!(
        capture.validate(&mut owner),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == "assets/outer.png"
    ));
    assert!(owner.budget().usage().decode_work() > before_drift);
    fs::write(&asset_path, original).unwrap();
    capture.validate(&mut owner).unwrap();

    let mut budget = owner.budget();
    let maximum = budget.usage().decode_work() + 1;
    budget.limits.decode_work = ItemLimit::new(maximum).unwrap();
    owner.retain_capture_budget(budget);
    assert!(matches!(
        capture.validate(&mut owner),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert!(owner.budget().usage().decode_work() > maximum);
    assert!(matches!(
        saved.validate(&mut owner),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: observed }
        )) if observed == maximum
    ));
}
