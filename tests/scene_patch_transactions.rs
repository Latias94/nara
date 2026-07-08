use nara::{
    prelude::*,
    scene::{ScenePatchDocument, ScenePatchOperation},
};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestPosition {
    x: i32,
    y: Option<i32>,
}

#[test]
fn patch_adds_component_sets_field_reparents_and_inverse_restores() {
    let registry = test_registry();
    let player = scene_id("player");
    let enemy = scene_id("enemy");
    let mut scene = SceneDocument::new([
        SceneEntityRecord::new(player.clone()),
        SceneEntityRecord::new(enemy.clone()),
    ]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: player.clone(),
            component: position_type_id(),
            value: position_record(1, Some(2)),
        },
        ScenePatchOperation::SetField {
            entity: player.clone(),
            component: position_type_id(),
            path: ComponentFieldPath::from_fields(["x"]),
            value: ComponentValue::I64(7),
        },
        ScenePatchOperation::Reparent {
            entity: enemy.clone(),
            parent: Some(player.clone()),
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    assert!(!report.diagnostics.has_errors());
    assert_eq!(component_value(&scene, &player).field_i64("x").unwrap(), 7);
    assert_eq!(scene_entity(&scene, &enemy).parent.as_ref(), Some(&player));

    let inverse = report.inverse.unwrap();
    let inverse_report = inverse.apply_to_scene(&mut scene, &registry);

    assert!(inverse_report.applied);
    assert_eq!(scene, original);
}

#[test]
fn invalid_patch_operation_leaves_document_unchanged_with_operation_context() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: player.clone(),
            component: position_type_id(),
            value: position_record(1, None),
        },
        ScenePatchOperation::SetField {
            entity: player.clone(),
            component: position_type_id(),
            path: ComponentFieldPath::from_fields(["missing"]),
            value: ComponentValue::I64(9),
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert_eq!(scene, original);
    let diagnostic = report.diagnostics.diagnostics().first().unwrap();
    assert_eq!(diagnostic.context.operation_index, Some(1));
    assert_eq!(diagnostic.context.entity_id.as_deref(), Some("player"));
    assert_eq!(
        diagnostic.context.component_id.as_deref(),
        Some(position_type_id().as_str())
    );
    assert_eq!(diagnostic.context.field_path.as_deref(), Some("missing"));
}

#[test]
fn remove_entity_removes_subtree_and_inverse_restores_it() {
    let registry = test_registry();
    let root = scene_id("root");
    let child = scene_id("root/child");
    let grandchild = scene_id("root/child/grandchild");
    let mut scene = SceneDocument::new([
        SceneEntityRecord::new(root.clone())
            .with_component(position_type_id(), position_record(1, None)),
        SceneEntityRecord::new(child.clone()).with_parent(root.clone()),
        SceneEntityRecord::new(grandchild.clone()).with_parent(child.clone()),
    ]);
    let original = scene.clone();

    let report = ScenePatchDocument::new([ScenePatchOperation::RemoveEntity {
        entity: root.clone(),
    }])
    .apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    assert!(scene.entities.is_empty());

    let inverse_report = report
        .inverse
        .unwrap()
        .apply_to_scene(&mut scene, &registry);

    assert!(inverse_report.applied);
    assert_eq!(scene, original);
}

#[test]
fn remove_required_field_fails_before_mutating_document() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(position_type_id(), position_record(1, Some(2)))]);
    let original = scene.clone();

    let report = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: player,
        component: position_type_id(),
        path: ComponentFieldPath::from_fields(["x"]),
    }])
    .apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.patch-required-field-removal"
            && diagnostic.context.operation_index == Some(0)
    }));
}

#[test]
fn remove_optional_field_and_set_asset_ref_field_are_schema_checked() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(position_type_id(), position_record(1, Some(2)))]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::RemoveField {
            entity: player.clone(),
            component: position_type_id(),
            path: ComponentFieldPath::from_fields(["y"]),
        },
        ScenePatchOperation::SetAssetRefField {
            entity: player.clone(),
            component: position_type_id(),
            path: ComponentFieldPath::from_fields(["asset"]),
            asset_ref: AssetRef::path("textures/player.png").unwrap(),
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    let value = component_value(&scene, &player);
    assert!(value.get("y").is_none());
    assert_eq!(
        value.field("asset").unwrap().field_str("kind").unwrap(),
        "path"
    );
    assert_eq!(
        value.field("asset").unwrap().field_str("value").unwrap(),
        "textures/player.png"
    );
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let component_id = position_type_id();
    registry
        .register_serializable_component::<TestPosition, _, _>(
            component_id.clone(),
            ComponentSchemaVersion(1),
            |value| {
                let x = value.field_i64("x")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
                    y: value
                        .get("y")
                        .map(|value| {
                            let y = value
                                .as_i64()
                                .ok_or_else(|| ComponentCodecError::invalid_field("y", "i32"))?;
                            i32::try_from(y)
                                .map_err(|_| ComponentCodecError::invalid_field("y", "i32"))
                        })
                        .transpose()?,
                })
            },
            |position| {
                let mut fields = vec![("x", ComponentValue::I64(i64::from(position.x)))];
                if let Some(y) = position.y {
                    fields.push(("y", ComponentValue::I64(i64::from(y))));
                }
                Ok(ComponentValue::map(fields))
            },
        )
        .unwrap()
        .register_component_fields(
            &component_id,
            [
                ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["x"]),
                    ComponentValueKind::I64,
                ),
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldPath::from_fields(["y"]),
                    ComponentValueKind::I64,
                    ComponentValue::I64(0),
                ),
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldPath::from_fields(["asset"]),
                    ComponentValueKind::AssetRef,
                    ComponentValue::Null,
                ),
            ],
        )
        .unwrap();
    registry
}

fn scene_entity<'a>(scene: &'a SceneDocument, id: &SceneEntityId) -> &'a SceneEntityRecord {
    scene
        .entities
        .iter()
        .find(|entity| entity.id == *id)
        .unwrap()
}

fn component_value<'a>(scene: &'a SceneDocument, id: &SceneEntityId) -> &'a ComponentValue {
    &scene_entity(scene, id)
        .components
        .get(&position_type_id())
        .unwrap()
        .value
}

fn position_record(x: i32, y: Option<i32>) -> SceneComponentRecord {
    let mut fields = vec![("x", ComponentValue::I64(i64::from(x)))];
    if let Some(y) = y {
        fields.push(("y", ComponentValue::I64(i64::from(y))));
    }
    SceneComponentRecord::new(ComponentSchemaVersion(1), ComponentValue::map(fields))
}

fn position_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Position")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
