use super::*;
use nara_core::Vec2;
use nara_ecs::Component;
use nara_reflect::{PersistentComponent, PersistentComponentProvider};

#[derive(Component, PersistentComponent)]
#[nara(
    id = "game.Math",
    version = 1,
    alias = "Math",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Math {
    #[nara(id = "origin", alias = "Origin")]
    origin: Vec2,
    #[nara(id = "route", alias = "Route")]
    route: Vec<Vec2>,
}

#[test]
fn vector_data_authoring_requires_matching_catalog_and_complete_closed_values() {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.math").unwrap(),
        ComponentSchemaCatalog {
            components: vec![Math::persistent_component_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new())
        .expect("Vec2 and Vec<Vec2> are closed data shapes");
    let valid = Math {
        origin: Vec2::ZERO,
        route: vec![Vec2::ONE],
    }
    .__encode_persistent_component()
    .unwrap();
    let document = |value| {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("actor").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("game.Math"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
        )])
    };
    let candidate = |value| {
        SceneDocumentCandidate::decode_json_bytes(
            document(value).to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
    };
    for (field, invalid) in [
        (
            "origin",
            ComponentValue::map([("x", ComponentValue::f64(0.0).unwrap())]),
        ),
        (
            "route",
            ComponentValue::List(vec![ComponentValue::map([
                ("x", ComponentValue::f64(f64::MAX).unwrap()),
                ("y", ComponentValue::f64(0.0).unwrap()),
            ])]),
        ),
    ] {
        let mut value = valid.clone();
        value
            .replace_path(&ComponentFieldPath::from_fields([field]), invalid)
            .unwrap();
        assert!(
            candidate(value)
                .admit_authoring(source(), &bundle, &[adapter.clone()])
                .is_err()
        );
    }
    let mut changed = bundle.owners()[0].current().clone();
    changed.components[0].fields[0]
        .aliases
        .push("Other schema".to_owned());
    let changed = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        bundle.owners()[0].owner_id().clone(),
        changed,
        None,
    )])
    .unwrap();
    let published = candidate(valid)
        .admit_authoring(source(), &changed, &[adapter])
        .unwrap()
        .publish();
    assert_eq!(
        published.record_facts()[0].readiness(),
        AuthoringRecordReadiness::KnownUnbound
    );
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let before = session.document().clone();
    let outcome =
        session.apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: SceneEntityId::new("actor").unwrap(),
            component: ComponentTypeId::new("game.Math"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("origin"),
            value: ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(2.0).unwrap()),
            ]),
        }]));
    assert!(!outcome.applied);
    assert_eq!(session.document(), &before);
}
