use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, SceneCompilationFailureKind, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord,
};

#[derive(Component)]
struct Payload(ComponentValue);

#[test]
fn migrated_invalid_default_parent_refuses_once_without_changing_source_or_previous_output() {
    let component = ComponentTypeId::new("test.OwnedMigrationDefaults");
    let schema = |version| {
        ComponentSchema::new(component.clone(), "Owned migration defaults", version)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([
                ComponentFieldSchema::required(
                    ComponentFieldId::new("items"),
                    "Items",
                    ComponentFieldPath::from_fields(["items"]),
                    ComponentValueShape::list(ComponentValueKind::U64.into()),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("first"),
                    "First",
                    ComponentFieldPath::from_fields(["a_valid"]),
                    ComponentValueKind::U64,
                    ComponentValue::U64(7),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("amount"),
                    "Amount",
                    ComponentFieldPath::from_fields(["settings", "amount"]),
                    ComponentValueKind::U64,
                    ComponentValue::U64(9),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ])
    };
    let mut previous = ComponentRegistryBuilder::new();
    previous
        .register_persistent_component_with_codec::<Payload, _, _>(
            schema(ComponentSchemaVersion::ONE),
            |value| Ok(Payload(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    let previous = previous.freeze().unwrap();
    let migrations = Arc::new(AtomicUsize::new(0));
    let decodes = Arc::new(AtomicUsize::new(0));
    let version = ComponentSchemaVersion::new(2).unwrap();
    let mut receiving = ComponentRegistryBuilder::new();
    receiving
        .register_persistent_component_with_codec::<Payload, _, _>(
            schema(version),
            {
                let decodes = Arc::clone(&decodes);
                move |value| {
                    decodes.fetch_add(1, Ordering::SeqCst);
                    Ok(Payload(value.clone()))
                }
            },
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    receiving
        .register_component_migration(&component, ComponentSchemaVersion::ONE, version, {
            let migrations = Arc::clone(&migrations);
            move |mut value| {
                migrations.fetch_add(1, Ordering::SeqCst);
                let ComponentValue::Map(fields) = &mut value else {
                    unreachable!()
                };
                fields.remove("a_valid");
                fields.insert("settings".into(), ComponentValue::Bool(false));
                Ok(value)
            }
        })
        .unwrap();
    let receiving = receiving.freeze().unwrap();
    let original = ComponentValue::map([
        (
            "items",
            ComponentValue::List(vec![ComponentValue::U64(1); 257]),
        ),
        ("a_valid", ComponentValue::U64(7)),
        (
            "settings",
            ComponentValue::map([("amount", ComponentValue::U64(9))]),
        ),
    ]);
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        component.clone(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, original.clone()),
    )]);
    let before = source.clone();
    let resolver = InMemoryPrefabSourceResolver::new();
    let compiled = source.compile(&previous, &resolver).candidate.unwrap();
    let failed = source.compile(&receiving, &resolver);
    assert!(failed.candidate.is_none());
    assert_eq!(
        failed.failure.as_ref().unwrap().reason(),
        &SceneCompilationFailureKind::Component
    );
    let diagnostic = failed
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-default-materialization-failed"
        })
        .unwrap();
    assert_eq!(
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == "codec-error-kind")
            .unwrap()
            .value(),
        DiagnosticValueRef::Identifier("invalid-field"),
    );
    assert_eq!(migrations.load(Ordering::SeqCst), 1);
    assert_eq!(decodes.load(Ordering::SeqCst), 0);
    assert_eq!(source, before);
    assert_eq!(
        compiled.entities()[0].components()[&component].value(),
        &original
    );
    drop(source);
    assert_eq!(
        compiled.entities()[0].components()[&component].value(),
        &original
    );
}
