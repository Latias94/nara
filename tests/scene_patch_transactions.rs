use nara::{
    core::{ByteLimit, ItemLimit},
    diagnostic::DiagnosticValueRef,
    prelude::*,
    scene::{ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation},
};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestPosition {
    x: i32,
    y: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestBlob(String);

fn diagnostic_has_field(
    diagnostic: &Diagnostic,
    key: &str,
    class: DiagnosticFieldClass,
    value: DiagnosticValueRef<'_>,
) -> bool {
    diagnostic.fields().iter().any(|field| {
        field.key().as_str() == key && field.class() == class && field.value() == value
    })
}

fn assert_diagnostic_field(
    diagnostic: &Diagnostic,
    key: &str,
    class: DiagnosticFieldClass,
    value: DiagnosticValueRef<'_>,
) {
    assert!(
        diagnostic_has_field(diagnostic, key, class, value),
        "missing diagnostic field {key}"
    );
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
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("x"),
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
fn stable_field_id_patch_resolves_current_path_after_rename() {
    let registry = renamed_field_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("renamed_x", ComponentValue::I64(1))]),
        ),
    )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player.clone(),
        component: position_type_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(9),
    }]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    assert_eq!(
        component_value(&scene, &player)
            .field_i64("renamed_x")
            .unwrap(),
        9
    );
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
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("missing"),
            value: ComponentValue::I64(9),
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert_eq!(scene, original);
    let diagnostic = report.diagnostics.iter().next().unwrap();
    assert_diagnostic_field(
        diagnostic,
        "operation-index",
        DiagnosticFieldClass::Public,
        DiagnosticValueRef::Unsigned(1),
    );
    assert_diagnostic_field(
        diagnostic,
        "entity-id",
        DiagnosticFieldClass::Public,
        DiagnosticValueRef::Identifier("player"),
    );
    assert_diagnostic_field(
        diagnostic,
        "component-id",
        DiagnosticFieldClass::Public,
        DiagnosticValueRef::Identifier(position_type_id().as_str()),
    );
    assert_diagnostic_field(
        diagnostic,
        "field-id",
        DiagnosticFieldClass::Public,
        DiagnosticValueRef::Identifier("missing"),
    );
}

#[test]
fn patch_new_uses_current_format_version() {
    let patch = ScenePatchDocument::new([]);

    assert_eq!(
        patch.format_version,
        ScenePatchDocument::CURRENT_FORMAT_VERSION
    );
    assert_eq!(
        ScenePatchDocument::default().format_version,
        ScenePatchDocument::CURRENT_FORMAT_VERSION
    );
}

#[test]
fn unsupported_patch_format_version_fails_before_mutating_document() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(position_type_id(), position_record(1, None))]);
    let original = scene.clone();
    let patch = ScenePatchDocument {
        format_version: ScenePatchDocument::CURRENT_FORMAT_VERSION + 1,
        operations: vec![ScenePatchOperation::SetField {
            entity: player,
            component: position_type_id(),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(9),
        }],
    };

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-unsupported-format-version"
    }));
}

#[test]
fn stale_patch_component_schema_version_fails_before_mutating_document() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(position_type_id(), position_record(1, None))]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: position_type_id(),
        component_version: ComponentSchemaVersion(0),
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(9),
    }]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-stale-component-schema-version"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(0),
            )
            && diagnostic_has_field(
                diagnostic,
                "entity-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("player"),
            )
            && diagnostic_has_field(
                diagnostic,
                "component-id",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier(position_type_id().as_str()),
            )
            && diagnostic_has_field(
                diagnostic,
                "field-path",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("f_x"),
            )
    }));
}

#[test]
fn set_field_without_edit_capability_fails_before_mutating_document() {
    let registry = readonly_field_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(position_type_id(), position_record(1, None))]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: position_type_id(),
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(9),
    }]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-field-capability-missing"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(0),
            )
            && diagnostic_has_field(
                diagnostic,
                "field-path",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("f_x"),
            )
    }));
}

#[test]
fn whole_component_and_entity_operations_require_edit_for_every_field() {
    let registry = readonly_field_registry();
    let player = scene_id("player");
    let child = scene_id("child");
    let readonly = position_record(1, None);
    let cases = [
        (
            SceneDocument::new([SceneEntityRecord::new(player.clone())]),
            ScenePatchOperation::AddComponent {
                entity: player.clone(),
                component: position_type_id(),
                value: readonly.clone(),
            },
        ),
        (
            SceneDocument::new([SceneEntityRecord::new(player.clone())
                .with_component(position_type_id(), readonly.clone())]),
            ScenePatchOperation::RemoveComponent {
                entity: player.clone(),
                component: position_type_id(),
            },
        ),
        (
            SceneDocument::new([SceneEntityRecord::new(player.clone())
                .with_component(position_type_id(), readonly.clone())]),
            ScenePatchOperation::ReplaceComponent {
                entity: player.clone(),
                component: position_type_id(),
                value: readonly.clone(),
            },
        ),
        (
            SceneDocument::new([]),
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(player.clone())
                    .with_component(position_type_id(), readonly.clone()),
            },
        ),
        (
            SceneDocument::new([
                SceneEntityRecord::new(player.clone()),
                SceneEntityRecord::new(child.clone())
                    .with_parent(player.clone())
                    .with_component(position_type_id(), readonly),
            ]),
            ScenePatchOperation::RemoveEntity {
                entity: player.clone(),
            },
        ),
    ];

    for (mut scene, operation) in cases {
        let original = scene.clone();
        let report = ScenePatchDocument::new([operation]).apply_to_scene(&mut scene, &registry);

        assert!(!report.applied);
        assert_eq!(scene, original);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.patch-component-capability-missing"
        }));
    }
}

#[test]
fn patch_combined_validation_work_budget_is_exact_and_failure_atomic() {
    let registry = test_registry();
    let root = scene_id("root");
    let child = scene_id("child");
    let original = SceneDocument::new([
        SceneEntityRecord::new(root.clone()),
        SceneEntityRecord::new(child.clone()),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::Reparent {
            entity: child.clone(),
            parent: Some(root),
        },
        ScenePatchOperation::Reparent {
            entity: child,
            parent: None,
        },
    ]);

    let exact_limits =
        ScenePatchApplyLimits::default().with_validation_work(ItemLimit::new(9).unwrap());
    let mut exact = original.clone();
    let exact_report = patch.apply_to_scene_with_limits(&mut exact, &registry, exact_limits);
    assert!(exact_report.applied);
    assert_eq!(exact, original);

    let rejected_limits =
        ScenePatchApplyLimits::default().with_validation_work(ItemLimit::new(8).unwrap());
    let mut rejected = original.clone();
    let rejected_report =
        patch.apply_to_scene_with_limits(&mut rejected, &registry, rejected_limits);
    assert!(!rejected_report.applied);
    assert_eq!(rejected, original);
    assert!(rejected_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
    }));
}

#[test]
fn patch_rejects_source_work_before_semantic_validation() {
    let registry = test_registry();
    let unknown_component = ComponentTypeId::new("nara.test.Unknown");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(scene_id("invalid"))
        .with_component(
            unknown_component,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
    let original = scene.clone();
    let limits = ScenePatchApplyLimits::default().with_validation_work(ItemLimit::new(2).unwrap());

    let report =
        ScenePatchDocument::default().apply_to_scene_with_limits(&mut scene, &registry, limits);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert_eq!(report.diagnostics.stats().observed_errors(), 1);
    let diagnostic = report.diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "scene.patch-validation-work-budget-exceeded"
    );
    assert_diagnostic_field(
        diagnostic,
        "budget-kind",
        DiagnosticFieldClass::Public,
        DiagnosticValueRef::Identifier("structural-items"),
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.unknown-component" })
    );
}

#[test]
fn patch_validation_value_budget_bounds_repeated_large_payload_work() {
    let registry = test_registry();
    let root = scene_id("root");
    let child = scene_id("child");
    let payload = ComponentValue::String("x".repeat(64 * 1024));
    let per_validation_bytes = payload.cost().logical_bytes();
    let original = SceneDocument::new([
        SceneEntityRecord::new(root.clone()).with_component(
            blob_type_id(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, payload),
        ),
        SceneEntityRecord::new(child.clone()),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::Reparent {
            entity: child.clone(),
            parent: Some(root),
        },
        ScenePatchOperation::Reparent {
            entity: child,
            parent: None,
        },
    ]);

    let exact_limits = ScenePatchApplyLimits::default().with_validation_value_bytes(
        ByteLimit::new(per_validation_bytes.saturating_mul(3)).unwrap(),
    );
    let mut exact = original.clone();
    let exact_report = patch.apply_to_scene_with_limits(&mut exact, &registry, exact_limits);
    assert!(exact_report.applied);
    assert_eq!(exact, original);

    let rejected_limits = ScenePatchApplyLimits::default().with_validation_value_bytes(
        ByteLimit::new(per_validation_bytes.saturating_mul(3).saturating_sub(1)).unwrap(),
    );
    let mut rejected = original.clone();
    let rejected_report =
        patch.apply_to_scene_with_limits(&mut rejected, &registry, rejected_limits);
    assert!(!rejected_report.applied);
    assert_eq!(rejected, original);
    assert!(rejected_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            && diagnostic_has_field(
                diagnostic,
                "budget-kind",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("value-bytes"),
            )
    }));

    let exact_node_limits =
        ScenePatchApplyLimits::default().with_validation_value_nodes(ItemLimit::new(3).unwrap());
    let mut exact_nodes = original.clone();
    assert!(
        patch
            .apply_to_scene_with_limits(&mut exact_nodes, &registry, exact_node_limits)
            .applied
    );

    let rejected_node_limits =
        ScenePatchApplyLimits::default().with_validation_value_nodes(ItemLimit::new(2).unwrap());
    let mut rejected_nodes = original.clone();
    let rejected_node_report =
        patch.apply_to_scene_with_limits(&mut rejected_nodes, &registry, rejected_node_limits);
    assert!(!rejected_node_report.applied);
    assert_eq!(rejected_nodes, original);
    assert!(rejected_node_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            && diagnostic_has_field(
                diagnostic,
                "budget-kind",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Identifier("value-nodes"),
            )
    }));
}

#[test]
fn field_patch_rejects_an_unmigrated_target_record() {
    let registry = migrated_field_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("x", ComponentValue::I64(1))]),
        ),
    )]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: position_type_id(),
        component_version: ComponentSchemaVersion(2),
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(9),
    }]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-target-component-version-mismatch"
    }));
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
        component_version: ComponentSchemaVersion(1),
        field: ComponentFieldId::new("x"),
    }])
    .apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-required-field-removal"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(0),
            )
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
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("y"),
        },
        ScenePatchOperation::SetAssetRefField {
            entity: player.clone(),
            component: position_type_id(),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("asset"),
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
    let schema = ComponentSchema::new(component_id, "Position", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("x"),
                "X",
                ComponentFieldPath::from_fields(["x"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("y"),
                "Y",
                ComponentFieldPath::from_fields(["y"]),
                ComponentValueKind::I64,
                ComponentValue::I64(0),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("asset"),
                "Asset",
                ComponentFieldPath::from_fields(["asset"]),
                ComponentValueKind::AssetRef,
                ComponentValue::Null,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
        ]);
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
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
        .unwrap();
    let blob_schema = ComponentSchema::new(blob_type_id(), "Blob", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::empty(),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    registry
        .register_persistent_component_with_codec::<TestBlob, _, _>(
            blob_schema,
            |value| {
                Ok(TestBlob(
                    value
                        .as_str()
                        .ok_or_else(|| ComponentCodecError::invalid_field("value", "string"))?
                        .to_owned(),
                ))
            },
            |blob| Ok(ComponentValue::String(blob.0.clone())),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn readonly_field_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let component_id = position_type_id();
    let schema = ComponentSchema::new(component_id, "Position", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("x"),
            "X",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::I64,
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])]);
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
            |value| {
                let x = value.field_i64("x")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
                    y: None,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn renamed_field_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = ComponentSchema::new(
        position_type_id(),
        "Renamed position",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("x"),
        "Horizontal position",
        ComponentFieldPath::from_fields(["renamed_x"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
            |value| {
                let x = value.field_i64("renamed_x")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("renamed_x", "i32"))?,
                    y: None,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "renamed_x",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrated_field_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = ComponentSchema::new(
        position_type_id(),
        "Migrated position",
        ComponentSchemaVersion(2),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("x"),
        "Horizontal position",
        ComponentFieldPath::from_fields(["x2"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
            |value| {
                let x = value.field_i64("x2")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                    y: None,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    registry
        .register_component_migration(
            &position_type_id(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let x = fields
                    .remove("x")
                    .ok_or_else(|| ComponentCodecError::missing_field("x"))?;
                fields.insert("x2".to_owned(), x);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
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

fn blob_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Blob")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
