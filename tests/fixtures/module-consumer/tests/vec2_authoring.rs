use std::collections::BTreeSet;

use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::Component;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponent,
    PersistentComponentProvider, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringRecordReadiness,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};

const COMPONENT: &str = "consumer.math.Motion";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.math.Motion",
    version = 1,
    alias = "Motion",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Motion {
    #[nara(id = "position", alias = "Position")]
    position: Vec2,
    #[nara(id = "speed", alias = "Speed")]
    speed: f32,
    #[nara(id = "route", alias = "Route")]
    route: Vec<Vec2>,
}

fn vector(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

fn patch(field: &str, value: ComponentValue) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: SceneEntityId::new("actor").unwrap(),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value,
    }])
}

#[test]
fn derived_vectors_edit_undo_save_and_reopen_without_a_native_registry() {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("consumer.math").unwrap(),
        ComponentSchemaCatalog {
            components: vec![Motion::persistent_component_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new())
        .expect("derived math fields must support the data-only adapter");
    assert!(adapter.contribution().is_none());
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/motion.scene.json").unwrap());
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("actor").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            Motion {
                position: Vec2::ZERO,
                speed: 2.0,
                route: vec![Vec2::X],
            }
            .__encode_persistent_component()
            .unwrap(),
        ),
    )]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source.clone(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();
    assert_eq!(
        published.record_facts()[0].readiness(),
        AuthoringRecordReadiness::Available
    );
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);

    for (field, replacement) in [
        ("position", vector(12.5, -6.0)),
        ("speed", ComponentValue::f64(3.5).unwrap()),
        (
            "route",
            ComponentValue::List(vec![vector(4.0, 5.0), vector(-2.0, 0.0)]),
        ),
    ] {
        let before = session.document().clone();
        let outcome = session.apply_authoring_patch(&patch(field, replacement));
        assert!(outcome.applied, "{:?}", outcome.diagnostics);
        let after = session.document().clone();
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &before);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &after);
    }

    let accepted = session.document().clone();
    let revision = session.revision();
    let history = session.history_status();
    for invalid in [
        ComponentValue::map([("x", ComponentValue::f64(1.0).unwrap())]),
        ComponentValue::map([
            ("x", ComponentValue::f64(1.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
            ("extra", ComponentValue::Bool(true)),
        ]),
        vector(f64::MAX, 0.0),
    ] {
        for (field, replacement) in [
            ("position", invalid.clone()),
            ("route", ComponentValue::List(vec![invalid.clone()])),
        ] {
            assert!(
                !session
                    .apply_authoring_patch(&patch(field, replacement))
                    .applied
            );
            assert_eq!(session.document(), &accepted);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.history_status(), history);
        }
    }
    let saved = session.document().to_json_string().unwrap();
    let reopened = SceneDocumentCandidate::decode_json_bytes(saved.as_bytes())
        .unwrap()
        .admit_authoring(source, &bundle, &[adapter])
        .unwrap()
        .publish();
    assert_eq!(reopened.document(), &accepted);
    assert_eq!(reopened.unavailable_manifest().total_count(), 0);
}
