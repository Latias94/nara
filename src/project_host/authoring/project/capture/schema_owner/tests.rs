use std::{fs, sync::Arc};

use nara_asset::{AssetPath, AssetRef};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{borrow_prepared_prefab_schema_source, borrow_prepared_scene_schema_source},
    PrefabAuthoringSession, PrefabInstance, SceneAuthoringSession, SceneComponentRecord,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
    ScenePlacementOwner, SchemaReclassificationWork,
};

use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionHost, AuthoringSchemaOwnerPreparation,
        document::AuthoringDocumentSourceFact, project_authoring_adapters,
        tests::independent_capture::independent_project,
    },
    file_publication::FilePublicationOutcome,
};

fn installed_owner() -> ComponentSchemaOwnerSnapshot {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.installed.owner").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.Installed"),
                    "Installed",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::from_fields(["value"]),
                    ComponentValueKind::U64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
    .owners()[0]
        .clone()
}

#[test]
fn actual_capture_uses_prepared_dirty_prefab_and_attaches_only_the_same_nominal_successor() {
    let project = independent_project();
    let mut startup = SceneEntityRecord::new(SceneEntityId::new("root").unwrap());
    startup.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([startup])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let outer_path = project.root.join("prefabs/outer.prefab.json");
    let mut outer = nara_scene::PrefabDocumentCandidate::decode_json_str(
        &fs::read_to_string(&outer_path).unwrap(),
    )
    .unwrap()
    .into_composition_source()
    .unwrap();
    outer.entities[0].components.insert(
        ComponentTypeId::new("test.Installed"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::U64(7))]),
        )
        .with_owner(installed_owner().owner_id().clone()),
    );
    fs::write(&outer_path, outer.to_json_string().unwrap()).unwrap();
    let root = crate::project_tooling::host_fs::writable_directory(&project.root).unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    let snapshot = PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap();
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(snapshot.fork_startup_scene());
    let scene_source = AuthoringDocumentSourceFact::from_captured_source(
        snapshot.startup_source(),
        snapshot.startup_native_source(),
    )
    .unwrap();
    let prefab = snapshot
        .prefab(&AssetPath::new("outer.prefab.json").unwrap())
        .unwrap();
    let prefab_source =
        AuthoringDocumentSourceFact::from_captured_source(prefab.source(), prefab.native_source())
            .unwrap();
    let mut prefab_session = PrefabAuthoringSession::from_published_authoring_document(
        prefab.publish_document().unwrap(),
    );
    let dirty = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("unsaved").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    assert!(prefab_session.apply_authoring_patch(&dirty).applied);
    assert!(scene.apply_authoring_patch(&dirty).applied);
    let original_prefab = prefab_session.shared_document();
    let old_basis = scene.authoring_schema_basis().unwrap();
    let AuthoringSchemaOwnerPreparation::Prepared {
        mut owner,
        mut publication,
    } = snapshot.schema.prepare_owner(&installed_owner()).unwrap()
    else {
        panic!("missing metadata must prepare");
    };
    let successor_basis = owner.schema_basis().clone();
    let mut work = SchemaReclassificationWork::default();
    let prepared_scene = scene
        .prepare_schema_reclassification(&successor_basis, &mut work)
        .unwrap();
    let prepared_prefab = prefab_session
        .prepare_schema_reclassification(&successor_basis, &mut work)
        .unwrap();
    let open_prefabs = [AuthoringPrefabSessionSnapshot::capture_prepared(
        &prefab_source,
        borrow_prepared_prefab_schema_source(&prefab_session, &prepared_prefab).unwrap(),
    )
    .unwrap()];
    let capture = owner
        .capture_scene(
            &scene_source,
            borrow_prepared_scene_schema_source(&scene, &prepared_scene).unwrap(),
            &open_prefabs,
        )
        .unwrap();
    assert!(capture.composition_failure().is_none());
    assert!(!capture.diagnostics().has_errors());
    let captured = capture
        .capture
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "outer.prefab.json")
        .unwrap();
    assert_eq!(captured.session_revision(), Some(prefab_session.revision()));
    assert!(Arc::ptr_eq(
        captured.shared_source_document(),
        &original_prefab
    ));
    assert!(
        captured
            .document()
            .entities
            .iter()
            .any(|entity| entity.id.as_str() == "unsaved")
    );
    assert_eq!(scene.authoring_schema_basis(), Some(old_basis));
    assert_eq!(snapshot.authoring_bundle().owners().len(), 1);
    let AuthoringSchemaOwnerPreparation::Prepared {
        owner: mut independent,
        ..
    } = snapshot.schema.prepare_owner(&installed_owner()).unwrap()
    else {
        panic!("an independent preparation must retain its own basis");
    };
    assert_ne!(independent.schema_basis(), &successor_basis);
    let independent_scene = scene
        .prepare_schema_reclassification(
            independent.schema_basis(),
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    let independent_capture = independent
        .capture_scene(
            &scene_source,
            borrow_prepared_scene_schema_source(&scene, &independent_scene).unwrap(),
            &[],
        )
        .unwrap();
    owner.validate().unwrap();
    let FilePublicationOutcome::Published(accepted) = publication.publish(&owner.bytes()) else {
        panic!("actual bundle publication must succeed");
    };
    let successor = owner.attach(accepted).unwrap();
    assert_eq!(successor.basis, successor_basis);
    assert!(independent_capture.attach(&successor).is_err());
    assert!(
        successor.usage.retained_candidate_bytes()
            > successor.schema.usage().retained_candidate_bytes()
    );
    assert!(
        successor.usage.aggregate_encoded_bytes()
            > successor.schema.usage().aggregate_encoded_bytes()
    );
    let capture = capture
        .attach(&successor)
        .unwrap_or_else(|_| panic!("same nominal successor must attach"));
    capture.source_validation().validate().unwrap();
    assert!(
        capture
            .authority
            .bundle()
            .owner(installed_owner().owner_id())
            .is_some()
    );
    assert!(scene.commit_schema_reclassification(prepared_scene).is_ok());
    assert!(
        prefab_session
            .commit_schema_reclassification(prepared_prefab)
            .is_ok()
    );
    assert!(Arc::ptr_eq(
        &original_prefab,
        &prefab_session.shared_document()
    ));
}
