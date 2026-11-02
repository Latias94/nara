use nara::{
    advanced_prelude::*,
    core::{ByteLimit, ItemLimit},
    diagnostic::DiagnosticValueRef,
    scene::{
        PrefabEntityTarget, SceneEntityPlacement, ScenePatchApplyLimits, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement, ScenePlacementParent,
    },
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

fn root_last() -> ScenePlacement {
    ScenePlacement::last(ScenePlacementParent::Root)
}

fn child_last(parent: SceneEntityId) -> ScenePlacement {
    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
        parent,
    )))
}

#[test]
fn patch_adds_component_sets_field_moves_entity_and_inverse_restores() {
    let registry = test_registry();
    let player = scene_id("player");
    let enemy = scene_id("enemy");
    let mut scene = SceneDocument::from_ordered_roots([
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
        ScenePatchOperation::SetPlacements {
            expected: scene.placements.clone(),
            value: vec![
                SceneEntityPlacement::root(player.clone()),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(enemy.clone()),
                    child_last(player.clone()),
                ),
            ],
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    assert!(!report.diagnostics.has_errors());
    assert_eq!(component_value(&scene, &player).field_i64("x").unwrap(), 7);
    let hierarchy = scene.local_hierarchy(Default::default()).unwrap();
    assert_eq!(hierarchy.children(&player).unwrap(), [enemy.clone()]);
    assert_eq!(hierarchy.roots(), [player.clone()]);

    let inverse = report.inverse.unwrap();
    let inverse_report = inverse.apply_to_scene(&mut scene, &registry);

    assert!(inverse_report.applied);
    assert_eq!(scene, original);
}

#[test]
fn stable_field_id_patch_resolves_current_path_after_rename() {
    let registry = renamed_field_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
        .with_component(
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
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())]);
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
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
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
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
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
                DiagnosticFieldClass::Sensitive,
                DiagnosticValueRef::Redacted,
            )
    }));
}

#[test]
fn set_field_without_edit_capability_fails_before_mutating_document() {
    let registry = readonly_field_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
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
                DiagnosticFieldClass::Sensitive,
                DiagnosticValueRef::Redacted,
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
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())]),
            ScenePatchOperation::AddComponent {
                entity: player.clone(),
                component: position_type_id(),
                value: readonly.clone(),
            },
        ),
        (
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
                .with_component(position_type_id(), readonly.clone())]),
            ScenePatchOperation::RemoveComponent {
                entity: player.clone(),
                component: position_type_id(),
            },
        ),
        (
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
                .with_component(position_type_id(), readonly.clone())]),
            ScenePatchOperation::ReplaceComponent {
                entity: player.clone(),
                component: position_type_id(),
                value: readonly.clone(),
            },
        ),
        (
            SceneDocument::from_ordered_roots([]),
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(player.clone())
                    .with_component(position_type_id(), readonly.clone()),
            },
        ),
        (
            SceneDocument::new(
                [
                    SceneEntityPlacement::root(player.clone()),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(child.clone()),
                        child_last(player.clone()),
                    ),
                ],
                [
                    SceneEntityRecord::new(player.clone()),
                    SceneEntityRecord::new(child.clone())
                        .with_component(position_type_id(), readonly),
                ],
            ),
            ScenePatchOperation::RemoveEntity {
                entity: child.clone(),
            },
        ),
    ];

    for (mut scene, operation) in cases {
        let original = scene.clone();
        let placements = match &operation {
            ScenePatchOperation::AddEntity { entity } => Some(
                scene
                    .placements
                    .iter()
                    .cloned()
                    .chain([SceneEntityPlacement::root(entity.id.clone())])
                    .collect(),
            ),
            ScenePatchOperation::RemoveEntity { entity } => Some(
                scene
                    .placements
                    .iter()
                    .filter(|entry| entry.target != PrefabEntityTarget::source(entity.clone()))
                    .cloned()
                    .collect(),
            ),
            _ => None,
        };
        let mut patch = ScenePatchDocument::new([operation]);
        if let Some(value) = placements {
            patch.operations.push(ScenePatchOperation::SetPlacements {
                expected: scene.placements.clone(),
                value,
            });
        }
        let report = patch.apply_to_scene(&mut scene, &registry);

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
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(root.clone())
            .with_component(position_type_id(), position_record(1, None)),
        SceneEntityRecord::new(child.clone()),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::SetField {
            entity: root.clone(),
            component: position_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(2),
        },
        ScenePatchOperation::SetField {
            entity: root,
            component: position_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(1),
        },
    ]);

    let exact_limits =
        ScenePatchApplyLimits::default().with_validation_work(ItemLimit::new(18).unwrap());
    let mut exact = original.clone();
    let exact_report = patch.apply_to_scene_with_limits(&mut exact, &registry, exact_limits);
    assert!(exact_report.applied);
    assert_eq!(exact, original);

    let rejected_limits =
        ScenePatchApplyLimits::default().with_validation_work(ItemLimit::new(17).unwrap());
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
    let mut scene =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("invalid"))
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
    assert_eq!(
        diagnostic.summary().as_str(),
        "Source declarations exceed the replay data or depth budget"
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
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(root.clone()).with_component(
            blob_type_id(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, payload),
        ),
        SceneEntityRecord::new(child.clone()),
    ]);
    let mut patch = ScenePatchDocument::set_placement(
        &original.placements,
        PrefabEntityTarget::source(child),
        child_last(root),
    );
    let moved = match &patch.operations[0] {
        ScenePatchOperation::SetPlacements { value, .. } => value.clone(),
        _ => unreachable!("set_placement constructs a complete vector replacement"),
    };
    patch.operations.push(ScenePatchOperation::SetPlacements {
        expected: moved,
        value: original.placements.clone(),
    });

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
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
        .with_component(
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
fn explicit_subtree_removal_batch_and_inverse_restore_exact_document() {
    let registry = test_registry();
    let root = scene_id("root");
    let child = scene_id("root/child");
    let grandchild = scene_id("root/child/grandchild");
    let mut scene = SceneDocument::new(
        [
            SceneEntityPlacement::root(root.clone()),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(child.clone()),
                child_last(root.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(grandchild.clone()),
                child_last(child.clone()),
            ),
        ],
        [
            SceneEntityRecord::new(root.clone())
                .with_component(position_type_id(), position_record(1, None)),
            SceneEntityRecord::new(child.clone()),
            SceneEntityRecord::new(grandchild.clone()),
        ],
    );
    let original = scene.clone();

    let report = ScenePatchDocument::new([
        ScenePatchOperation::RemoveEntity {
            entity: root.clone(),
        },
        ScenePatchOperation::RemoveEntity {
            entity: child.clone(),
        },
        ScenePatchOperation::RemoveEntity {
            entity: grandchild.clone(),
        },
        ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: vec![],
        },
    ])
    .apply_to_scene(&mut scene, &registry);

    assert!(report.applied);
    assert!(scene.placements.is_empty());
    assert!(scene.entities.is_empty());

    let inverse = report
        .inverse
        .expect("successful subtree removal must produce an inverse");
    assert!(matches!(
        inverse.operations.as_slice(),
        [
            ScenePatchOperation::SetPlacements { expected, value },
            ScenePatchOperation::RestoreEntity { entity: restored_grandchild },
            ScenePatchOperation::RestoreEntity { entity: restored_child },
            ScenePatchOperation::RestoreEntity { entity: restored_root },
        ] if expected.is_empty() && value == &original.placements
            && restored_root == scene_entity(&original, &root)
            && restored_child == scene_entity(&original, &child)
            && restored_grandchild == scene_entity(&original, &grandchild)
    ));
    let stale_inverse = inverse.clone();
    let inverse_report = inverse.apply_to_scene(&mut scene, &registry);
    assert!(inverse_report.applied);
    assert_eq!(scene, original);

    let redo_report = inverse_report
        .inverse
        .expect("successful restore must produce a redo patch")
        .apply_to_scene(&mut scene, &registry);
    assert!(redo_report.applied);
    assert!(scene.placements.is_empty());
    assert!(scene.entities.is_empty());

    let second_inverse_report = redo_report
        .inverse
        .expect("successful redo must remain exactly invertible")
        .apply_to_scene(&mut scene, &registry);
    assert!(second_inverse_report.applied);
    assert_eq!(scene, original);

    let mut recreated = SceneDocument::from_ordered_roots([SceneEntityRecord::new(root.clone())]);
    let recreated_before = recreated.clone();
    let stale_report = stale_inverse.apply_to_scene(&mut recreated, &registry);
    assert!(!stale_report.applied);
    assert_eq!(recreated, recreated_before);
    assert!(stale_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-placement-context-mismatch"
    }));
    let duplicate = ScenePatchDocument::new([ScenePatchOperation::RestoreEntity {
        entity: scene_entity(&original, &root).clone(),
    }]);
    let duplicate_report = duplicate.apply_to_scene(&mut recreated, &registry);
    assert!(!duplicate_report.applied);
    assert_eq!(recreated, recreated_before);
    assert!(duplicate_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-duplicate-entity"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(0),
            )
    }));
}

#[test]
fn subtree_remove_and_restore_share_the_same_validation_work_budget() {
    let registry = test_registry();
    let ids = (0..6)
        .map(|index| scene_id(&format!("node-{index}")))
        .collect::<Vec<_>>();
    let records = ids.iter().cloned().map(SceneEntityRecord::new);
    let placements = std::iter::once(SceneEntityPlacement::root(ids[0].clone())).chain(
        ids.windows(2).map(|pair| {
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(pair[1].clone()),
                child_last(pair[0].clone()),
            )
        }),
    );
    let original = SceneDocument::new(placements, records);
    // Source plus all six record changes and the final vector replacement cost 71 items.
    let limits = ScenePatchApplyLimits::default()
        .with_validation_work(ItemLimit::new(71).expect("test limit is non-zero"));
    let mut scene = original.clone();

    let patch = ScenePatchDocument::new(
        ids.iter()
            .cloned()
            .map(|entity| ScenePatchOperation::RemoveEntity { entity })
            .chain([ScenePatchOperation::SetPlacements {
                expected: original.placements.clone(),
                value: vec![],
            }]),
    );
    let short = limits.with_validation_work(ItemLimit::new(70).unwrap());
    let rejected = patch.apply_to_scene_with_limits(&mut scene, &registry, short);
    assert!(!rejected.applied);
    assert_eq!(scene, original);
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            && diagnostic_has_field(
                diagnostic,
                "observed",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(71),
            )
            && diagnostic_has_field(
                diagnostic,
                "maximum",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(70),
            )
    }));
    let removed = patch.apply_to_scene_with_limits(&mut scene, &registry, limits);

    assert!(removed.applied, "{:?}", removed.diagnostics);
    let inverse = removed
        .inverse
        .expect("removal must retain every record and placement");
    assert_eq!(inverse.operations.len(), 7);
    let empty = scene.clone();
    let rejected = inverse.apply_to_scene_with_limits(&mut scene, &registry, short);
    assert!(!rejected.applied);
    assert_eq!(scene, empty);
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            && diagnostic_has_field(
                diagnostic,
                "observed",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(71),
            )
            && diagnostic_has_field(
                diagnostic,
                "maximum",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(70),
            )
    }));
    let restored = inverse.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(restored.applied, "{:?}", restored.diagnostics);
    assert_eq!(scene, original);
}

#[test]
fn invalid_restore_entity_closure_is_failure_atomic() {
    let registry = test_registry();
    let existing = scene_id("existing");
    let restored = scene_id("restored");
    let missing = scene_id("missing");
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(existing)]);
    let original = scene.clone();
    let mut placements = scene.placements.clone();
    placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(restored.clone()),
        child_last(missing),
    ));
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::RestoreEntity {
            entity: SceneEntityRecord::new(restored),
        },
        ScenePatchOperation::SetPlacements {
            expected: scene.placements.clone(),
            value: placements,
        },
    ]);

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-placement-target"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(1),
            )
    }));
}

#[test]
fn ordered_insert_anchors_and_inverse_preserve_exact_order() {
    let registry = test_registry();
    let alpha = scene_id("alpha");
    let omega = scene_id("omega");
    let a = scene_id("alpha/a");
    let c = scene_id("alpha/c");
    let original = SceneDocument::new(
        [
            SceneEntityPlacement::root(alpha.clone()),
            SceneEntityPlacement::root(omega.clone()),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(a.clone()),
                child_last(alpha.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(c.clone()),
                child_last(alpha.clone()),
            ),
        ],
        [
            SceneEntityRecord::new(alpha.clone()),
            SceneEntityRecord::new(omega.clone()),
            SceneEntityRecord::new(a.clone()),
            SceneEntityRecord::new(c.clone()),
        ],
    );
    let middle = scene_id("middle");
    let between = scene_id("between");
    let first = scene_id("first");
    let b = scene_id("alpha/b");
    let d = scene_id("alpha/d");
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(middle.clone()),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(b.clone()),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(between.clone()),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(first.clone()),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(d.clone()),
        },
        ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: original
                .placements
                .iter()
                .cloned()
                .chain([
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(middle.clone()),
                        ScenePlacement::before(
                            ScenePlacementParent::Root,
                            PrefabEntityTarget::source(omega.clone()),
                        ),
                    ),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(b.clone()),
                        ScenePlacement::before(
                            ScenePlacementParent::Entity(PrefabEntityTarget::source(alpha.clone())),
                            PrefabEntityTarget::source(c.clone()),
                        ),
                    ),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(between.clone()),
                        ScenePlacement::after(
                            ScenePlacementParent::Root,
                            PrefabEntityTarget::source(middle.clone()),
                        ),
                    ),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(first.clone()),
                        ScenePlacement::first(ScenePlacementParent::Root),
                    ),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(d.clone()),
                        child_last(alpha.clone()),
                    ),
                ])
                .collect(),
        },
    ]);
    let mut scene = original.clone();

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied, "{:?}", report.diagnostics);
    let hierarchy = scene.local_hierarchy(Default::default()).unwrap();
    assert_eq!(
        hierarchy.roots(),
        [first, alpha.clone(), middle, between, omega]
    );
    assert_eq!(hierarchy.children(&alpha).unwrap(), [a, b, c, d]);
    let changed = scene.clone();

    let inverse_report = report
        .inverse
        .expect("successful ordered inserts must produce an inverse")
        .apply_to_scene(&mut scene, &registry);
    assert!(inverse_report.applied, "{:?}", inverse_report.diagnostics);
    assert_eq!(scene, original);

    let redo_report = inverse_report
        .inverse
        .expect("ordered insert inverse must produce an exact redo")
        .apply_to_scene(&mut scene, &registry);
    assert!(redo_report.applied, "{:?}", redo_report.diagnostics);
    assert_eq!(scene, changed);
}

#[test]
fn ordered_moves_reorder_across_owners_and_inverse_exactly() {
    let registry = test_registry();
    let left = scene_id("left");
    let right = scene_id("right");
    let loose = scene_id("loose");
    let a = scene_id("left/a");
    let b = scene_id("left/b");
    let c = scene_id("left/c");
    let d = scene_id("right/d");
    let e = scene_id("right/e");
    let left_owner = ScenePlacementParent::Entity(PrefabEntityTarget::source(left.clone()));
    let right_owner = ScenePlacementParent::Entity(PrefabEntityTarget::source(right.clone()));
    let original = SceneDocument::new(
        [
            SceneEntityPlacement::root(left.clone()),
            SceneEntityPlacement::root(right.clone()),
            SceneEntityPlacement::root(loose.clone()),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(a.clone()),
                child_last(left.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(b.clone()),
                child_last(left.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(c.clone()),
                child_last(left.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(d.clone()),
                child_last(right.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(e.clone()),
                child_last(right.clone()),
            ),
        ],
        [
            SceneEntityRecord::new(left.clone()),
            SceneEntityRecord::new(right.clone()),
            SceneEntityRecord::new(loose.clone()),
            SceneEntityRecord::new(a.clone()),
            SceneEntityRecord::new(b.clone()),
            SceneEntityRecord::new(c.clone()),
            SceneEntityRecord::new(d.clone()),
            SceneEntityRecord::new(e.clone()),
        ],
    );
    let mut placements = original.placements.clone();
    placements[5].placement = ScenePlacement::first(left_owner.clone());
    placements[4].placement =
        ScenePlacement::after(right_owner, PrefabEntityTarget::source(e.clone()));
    placements[2].placement = ScenePlacement::before(
        ScenePlacementParent::Root,
        PrefabEntityTarget::source(right.clone()),
    );
    placements[6].placement =
        ScenePlacement::after(left_owner, PrefabEntityTarget::source(a.clone()));
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
        expected: original.placements.clone(),
        value: placements,
    }]);
    let mut scene = original.clone();

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied, "{:?}", report.diagnostics);
    let hierarchy = scene.local_hierarchy(Default::default()).unwrap();
    assert_eq!(hierarchy.roots(), [left.clone(), loose, right.clone()]);
    assert_eq!(hierarchy.children(&left).unwrap(), [c, a, d]);
    assert_eq!(hierarchy.children(&right).unwrap(), [e, b]);

    let inverse_report = report
        .inverse
        .expect("successful ordered moves must produce an inverse")
        .apply_to_scene(&mut scene, &registry);
    assert!(inverse_report.applied, "{:?}", inverse_report.diagnostics);
    assert_eq!(scene, original);
}

#[test]
fn later_operations_can_anchor_to_earlier_candidate_changes() {
    let registry = test_registry();
    let root = scene_id("root");
    let first = scene_id("first");
    let second = scene_id("second");
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(root.clone())]);
    let with_first = vec![
        original.placements[0].clone(),
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(first.clone()),
            ScenePlacement::after(
                ScenePlacementParent::Root,
                PrefabEntityTarget::source(root.clone()),
            ),
        ),
    ];
    let mut with_second = with_first.clone();
    with_second.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(second.clone()),
        ScenePlacement::after(
            ScenePlacementParent::Root,
            PrefabEntityTarget::source(first.clone()),
        ),
    ));
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(first.clone()),
        },
        ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: with_first.clone(),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(second.clone()),
        },
        ScenePatchOperation::SetPlacements {
            expected: with_first,
            value: with_second.clone(),
        },
        ScenePatchOperation::SetPlacements {
            expected: with_second,
            value: vec![
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(root.clone()),
                    ScenePlacement::after(
                        ScenePlacementParent::Root,
                        PrefabEntityTarget::source(second.clone()),
                    ),
                ),
                SceneEntityPlacement::root(first.clone()),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(second.clone()),
                    ScenePlacement::after(
                        ScenePlacementParent::Root,
                        PrefabEntityTarget::source(first.clone()),
                    ),
                ),
            ],
        },
    ]);
    let mut scene = original.clone();

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(
        scene.local_hierarchy(Default::default()).unwrap().roots(),
        [first, second, root]
    );
    let inverse_report = report
        .inverse
        .expect("candidate-relative operations must produce an inverse")
        .apply_to_scene(&mut scene, &registry);
    assert!(inverse_report.applied, "{:?}", inverse_report.diagnostics);
    assert_eq!(scene, original);
}

#[test]
fn invalid_ordered_placement_contexts_reject_the_whole_batch() {
    let registry = test_registry();
    let root = scene_id("root");
    let child = scene_id("root/child");
    let sibling = scene_id("sibling");
    let parent = scene_id("parent");
    let other = scene_id("parent/other");
    let source = SceneDocument::new(
        [
            SceneEntityPlacement::root(root.clone()),
            SceneEntityPlacement::root(sibling.clone()),
            SceneEntityPlacement::root(parent.clone()),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(child.clone()),
                child_last(root.clone()),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(other.clone()),
                child_last(parent.clone()),
            ),
        ],
        [
            SceneEntityRecord::new(root.clone()),
            SceneEntityRecord::new(child.clone()),
            SceneEntityRecord::new(sibling.clone()),
            SceneEntityRecord::new(parent.clone()),
            SceneEntityRecord::new(other),
        ],
    );
    let mut stale = source.placements.clone();
    stale[0].placement = ScenePlacement::first(ScenePlacementParent::Root);
    let cases = [
        (
            ScenePatchDocument::set_placement(
                &source.placements,
                PrefabEntityTarget::source(sibling.clone()),
                ScenePlacement::before(
                    ScenePlacementParent::Root,
                    PrefabEntityTarget::source(sibling.clone()),
                ),
            ),
            "scene.patch-local-layout-invalid",
            false,
        ),
        (
            ScenePatchDocument::set_placement(
                &source.placements,
                PrefabEntityTarget::source(sibling.clone()),
                ScenePlacement::after(
                    ScenePlacementParent::Root,
                    PrefabEntityTarget::source(scene_id("retired-anchor")),
                ),
            ),
            "scene.invalid-placement-target",
            true,
        ),
        (
            ScenePatchDocument::set_placement(
                &source.placements,
                PrefabEntityTarget::source(root.clone()),
                child_last(child.clone()),
            ),
            "scene.patch-local-layout-invalid",
            false,
        ),
        (
            ScenePatchDocument::set_placement(
                &stale,
                PrefabEntityTarget::source(child.clone()),
                root_last(),
            ),
            "scene.patch-placement-context-mismatch",
            true,
        ),
        (
            ScenePatchDocument::set_placement(
                &source.placements,
                PrefabEntityTarget::source(child.clone()),
                child_last(scene_id("retired-parent")),
            ),
            "scene.invalid-placement-target",
            true,
        ),
    ];

    for (index, (mut patch, code, indexed)) in cases.into_iter().enumerate() {
        let temporary = scene_id(&format!("temporary-{index}"));
        let ScenePatchOperation::SetPlacements { value, .. } = &mut patch.operations[0] else {
            unreachable!("set_placement constructs a complete vector replacement")
        };
        value.push(SceneEntityPlacement::root(temporary.clone()));
        patch.operations.insert(
            0,
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(temporary),
            },
        );
        let mut scene = source.clone();
        let report = patch.apply_to_scene(&mut scene, &registry);

        assert!(!report.applied, "case {index} unexpectedly applied");
        assert_eq!(scene, source, "case {index} partially mutated the document");
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == code
                && (!indexed
                    || diagnostic_has_field(
                        diagnostic,
                        "operation-index",
                        DiagnosticFieldClass::Public,
                        DiagnosticValueRef::Unsigned(1),
                    ))
        }));
    }
}

#[test]
fn anchor_removed_earlier_in_a_batch_fails_without_publication() {
    let registry = test_registry();
    let root = scene_id("root");
    let first = scene_id("first");
    let tail = scene_id("tail");
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(root.clone())]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(first.clone()),
        },
        ScenePatchOperation::RemoveEntity {
            entity: root.clone(),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(tail.clone()),
        },
        ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: vec![
                SceneEntityPlacement::root(first),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(tail),
                    ScenePlacement::after(
                        ScenePlacementParent::Root,
                        PrefabEntityTarget::source(root),
                    ),
                ),
            ],
        },
    ]);
    let mut scene = original.clone();

    let report = patch.apply_to_scene(&mut scene, &registry);

    assert!(!report.applied);
    assert_eq!(scene, original);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-placement-target"
            && diagnostic_has_field(
                diagnostic,
                "operation-index",
                DiagnosticFieldClass::Public,
                DiagnosticValueRef::Unsigned(3),
            )
    }));
}

#[test]
fn remove_required_field_fails_before_mutating_document() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
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
    let mut scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(player.clone())
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

fn test_registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
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
    registry.freeze().unwrap()
}

fn readonly_field_registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
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
    registry.freeze().unwrap()
}

fn renamed_field_registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
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
    registry.freeze().unwrap()
}

fn migrated_field_registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
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
    registry.freeze().unwrap()
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
