use nara::{diagnostic::DiagnosticValueRef, prelude::*, tooling_prelude::*};

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLabel {
    text: String,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct HiddenLabel;

#[derive(Clone, Debug, PartialEq, Component)]
struct MigratingPosition {
    x: i64,
}

#[test]
fn inspector_model_lists_scene_entities_schema_fields_and_live_snapshot() {
    let registry = test_registry();
    let player = scene_id("player");
    let child = scene_id("player/child");
    let document = SceneDocument::new([
        labeled_entity(player.clone(), "Player"),
        SceneEntityRecord::new(child.clone()).with_parent(player.clone()),
    ]);
    let mut session = SceneAuthoringSession::new(document);
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));
    let mut world = World::new();
    world.spawn_empty();
    assert!(session.sync_world(&mut world, &registry).synced);
    let snapshot = WorldIdentitySnapshot::capture_default(&world).unwrap();

    let model = inspector.model(&session, &registry, Some(&snapshot));

    let world_snapshot = model.world_snapshot.as_ref().unwrap();
    assert_eq!(world_snapshot.total_entity_count, 3);
    assert_eq!(world_snapshot.identified_entity_count, 2);
    assert_eq!(world_snapshot.runtime_only_entity_count, 1);
    assert_eq!(world_snapshot.returned_locator_count, 2);
    assert_eq!(world_snapshot.locators.len(), 2);
    assert_eq!(world_snapshot.omitted_locator_count, 0);
    assert_eq!(model.history.undo_depth, 0);
    assert!(!model.live_dirty);
    assert!(!model.diagnostics.has_errors());
    assert_eq!(model.entities.len(), 2);
    assert!(model.entities.iter().any(|row| {
        row.id == player
            && row.selected
            && row.inspectable_component_count == 1
            && row.live_locator.is_some()
    }));
    assert!(model.entities.iter().any(|row| {
        row.id == child && row.parent.as_ref() == Some(&player) && row.live_locator.is_some()
    }));

    let entity_view = model.selected_entity_view.unwrap();
    assert_eq!(entity_view.id, player);
    assert_eq!(entity_view.components.len(), 1);
    let component = &entity_view.components[0];
    assert_eq!(component.component, label_type_id());
    assert_eq!(component.document_version, ComponentSchemaVersion(1));
    assert_eq!(component.schema_version, ComponentSchemaVersion(1));
    assert_eq!(component.fields.len(), 2);
    assert!(
        component
            .fields
            .iter()
            .all(|field| { field.capabilities.contains(&ComponentCapability::Inspect) })
    );
    assert!(
        component
            .fields
            .iter()
            .all(|field| field.id.as_str() != "secret")
    );

    let text = field(&component.fields, "text");
    assert_eq!(text.state, SceneInspectorFieldState::Present);
    assert_eq!(text.value.as_ref().unwrap().as_str(), Some("Player"));

    let note = field(&component.fields, "note");
    assert_eq!(note.state, SceneInspectorFieldState::Missing);
    assert!(!note.required);
}

#[test]
fn inspector_model_omits_components_without_observation_eligibility() {
    let registry = visibility_registry();
    let player = scene_id("player");
    let document = SceneDocument::new([labeled_entity(player.clone(), "Player")
        .with_component(
            hidden_type_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("hidden", ComponentValue::Bool(true))]),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.test.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("unknown", ComponentValue::Bool(true))]),
            ),
        )]);
    let session = SceneAuthoringSession::new(document);
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let model = inspector.model(&session, &registry, None);

    let row = model
        .entities
        .iter()
        .find(|row| row.id == player)
        .expect("selected entity should have an inspector row");
    assert_eq!(row.inspectable_component_count, 1);
    let entity = model
        .selected_entity_view
        .expect("selected entity should have a detail view");
    assert_eq!(entity.components.len(), 1);
    assert_eq!(entity.components[0].component, label_type_id());
}

#[test]
fn inspector_model_does_not_project_an_unfrozen_registry_candidate() {
    let registry = building_test_registry();
    let player = scene_id("player");
    let session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let model = inspector.model(&session, &registry, None);

    assert!(model.diagnostics.has_errors());
    assert_eq!(model.entities[0].inspectable_component_count, 0);
    assert!(
        model
            .selected_entity_view
            .expect("selected entity should retain its structural view")
            .components
            .is_empty()
    );
}

#[test]
fn inspector_read_only_field_command_is_failure_atomic() {
    let registry = visibility_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let before = session.document().clone();
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SetField {
            entity: player.clone(),
            component: label_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("read_only"),
            value: ComponentValue::String("rejected".to_owned()),
        },
    );

    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    assert_eq!(session.document(), &before);
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus::default()
    );
    assert_eq!(inspector.selected_entity(), Some(&player));
}

#[test]
fn inspector_set_field_command_applies_patch_and_selects_target() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SetField {
            entity: player.clone(),
            component: label_type_id(),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("text"),
            value: ComponentValue::String("Hero".to_string()),
        },
    );

    assert!(report.applied);
    assert!(report.patch.is_some());
    assert!(report.patch_report.as_ref().unwrap().applied);
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(inspector.selected_entity(), Some(&player));
    assert_eq!(document_label(session.document(), &player), "Hero");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 1,
            redo_depth: 0,
        }
    );
    assert!(session.is_live_dirty());
}

#[test]
fn inspector_selection_rejects_missing_entities_without_losing_selection() {
    let registry = test_registry();
    let player = scene_id("player");
    let missing = scene_id("missing");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();
    assert!(
        inspector
            .apply_command(
                &mut session,
                &registry,
                SceneInspectorCommand::SelectEntity {
                    entity: Some(player.clone()),
                },
            )
            .applied
    );

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SelectEntity {
            entity: Some(missing),
        },
    );

    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    let diagnostic = report.diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "tooling.inspector-missing-entity"
    );
    let entity = diagnostic_field(diagnostic, "entity");
    assert_eq!(entity.class(), DiagnosticFieldClass::Public);
    assert_eq!(entity.value(), DiagnosticValueRef::Identifier("missing"));
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(inspector.selected_entity(), Some(&player));
}

#[test]
fn inspector_failed_patch_command_does_not_enter_history() {
    let registry = test_registry();
    let player = scene_id("player");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([labeled_entity(
        player.clone(),
        "Player",
    )]));
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::RemoveField {
            entity: player.clone(),
            component: label_type_id(),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("text"),
        },
    );

    assert!(!report.applied);
    assert!(report.patch.is_some());
    assert!(
        report
            .patch_report
            .as_ref()
            .unwrap()
            .diagnostics
            .has_errors()
    );
    assert_eq!(report.selected_entity.as_ref(), Some(&player));
    assert_eq!(document_label(session.document(), &player), "Player");
    assert_eq!(
        session.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 0,
        }
    );
}

#[test]
fn inspector_migrates_legacy_component_for_display_and_atomic_edit() {
    let registry = migrating_position_registry();
    let player = scene_id("player");
    let legacy = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("x", ComponentValue::I64(7))]),
    );
    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(player.clone())
            .with_component(position_type_id(), legacy.clone())]));
    let mut inspector = SceneInspectorState::new();
    inspector.select_entity(Some(player.clone()));

    let model = inspector.model(&session, &registry, None);
    assert!(!model.diagnostics.has_errors());
    let component = &model.selected_entity_view.unwrap().components[0];
    assert_eq!(component.document_version, ComponentSchemaVersion::ONE);
    assert_eq!(component.schema_version, ComponentSchemaVersion(2));
    let x = field(&component.fields, "x");
    assert_eq!(x.path, ComponentFieldPath::from_fields(["x2"]));
    assert_eq!(x.state, SceneInspectorFieldState::Present);
    assert_eq!(x.value, Some(ComponentValue::I64(7)));

    let report = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SetField {
            entity: player.clone(),
            component: position_type_id(),
            component_version: ComponentSchemaVersion(2),
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(9),
        },
    );

    assert!(report.applied);
    assert_eq!(report.patch.as_ref().unwrap().operations.len(), 2);
    let current = document_component(session.document(), &player, &position_type_id());
    assert_eq!(current.version, ComponentSchemaVersion(2));
    assert_eq!(current.value.field_i64("x2").unwrap(), 9);
    assert_eq!(session.history_status().undo_depth, 1);

    let undo = session.undo(&registry);
    assert!(undo.applied);
    assert_eq!(
        document_component(session.document(), &player, &position_type_id()),
        &legacy
    );

    let before = session.document().clone();
    let rejected = inspector.apply_command(
        &mut session,
        &registry,
        SceneInspectorCommand::SetField {
            entity: player,
            component: position_type_id(),
            component_version: ComponentSchemaVersion(2),
            field: ComponentFieldId::new("x"),
            value: ComponentValue::String("wrong kind".to_owned()),
        },
    );
    assert!(!rejected.applied);
    assert_eq!(session.document(), &before);
    assert_eq!(session.history_status().undo_depth, 0);
}

fn test_registry() -> ComponentRegistry {
    let mut registry = building_test_registry();
    registry.freeze().unwrap();
    registry
}

fn building_test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let label_id = label_type_id();
    let schema = ComponentSchema::new(label_id, "Label", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("text"),
                "Text",
                ComponentFieldPath::from_fields(["text"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("note"),
                "Note",
                ComponentFieldPath::from_fields(["note"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("secret"),
                "Secret",
                ComponentFieldPath::from_fields(["secret"]),
                ComponentValueKind::String,
            )
            .with_capabilities([ComponentCapability::Scene]),
        ]);
    registry
        .register_persistent_component_with_codec::<TestLabel, _, _>(
            schema,
            |value| {
                Ok(TestLabel {
                    text: value.field_str("text")?.to_string(),
                })
            },
            |label| {
                Ok(ComponentValue::map([(
                    "text",
                    ComponentValue::String(label.text.clone()),
                )]))
            },
        )
        .unwrap();
    registry
}

fn visibility_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let visible_id = label_type_id();
    registry
        .register_persistent_component_with_codec::<TestLabel, _, _>(
            ComponentSchema::new(visible_id, "Label", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("text"),
                        "Text",
                        ComponentFieldPath::from_fields(["text"]),
                        ComponentValueKind::String,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::optional(
                        ComponentFieldId::new("read_only"),
                        "Read only",
                        ComponentFieldPath::from_fields(["read_only"]),
                        ComponentValueKind::String,
                    )
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]),
                ]),
            |value| {
                Ok(TestLabel {
                    text: value.field_str("text")?.to_owned(),
                })
            },
            |label| {
                Ok(ComponentValue::map([(
                    "text",
                    ComponentValue::String(label.text.clone()),
                )]))
            },
        )
        .unwrap();

    let hidden_id = hidden_type_id();
    registry
        .register_persistent_component_with_codec::<HiddenLabel, _, _>(
            ComponentSchema::new(hidden_id, "Hidden label", ComponentSchemaVersion::ONE)
                .with_capabilities([ComponentCapability::Scene])
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("hidden"),
                    "Hidden",
                    ComponentFieldPath::from_fields(["hidden"]),
                    ComponentValueKind::Bool,
                )
                .with_capabilities([ComponentCapability::Scene])]),
            |_value| Ok(HiddenLabel),
            |_label| {
                Ok(ComponentValue::map([(
                    "hidden",
                    ComponentValue::Bool(true),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrating_position_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<MigratingPosition, _, _>(
            ComponentSchema::new(
                position_type_id(),
                "Migrating position",
                ComponentSchemaVersion(2),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("x"),
                "Horizontal position",
                ComponentFieldPath::from_fields(["x2"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            |value| {
                Ok(MigratingPosition {
                    x: value.field_i64("x2")?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(position.x),
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

fn labeled_entity(id: SceneEntityId, label: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id).with_component(
        label_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([
                ("text", ComponentValue::String(label.to_string())),
                (
                    "secret",
                    ComponentValue::String("must-not-be-observed".to_owned()),
                ),
            ]),
        ),
    )
}

fn field<'a>(fields: &'a [SceneInspectorFieldView], id: &str) -> &'a SceneInspectorFieldView {
    fields.iter().find(|field| field.id.as_str() == id).unwrap()
}

fn diagnostic_field<'a>(diagnostic: &'a Diagnostic, key: &str) -> &'a DiagnosticField {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .unwrap()
}

fn document_label(document: &SceneDocument, id: &SceneEntityId) -> String {
    document
        .entities
        .iter()
        .find(|entity| entity.id == *id)
        .and_then(|entity| entity.components.get(&label_type_id()))
        .and_then(|component| component.value.field_str("text").ok())
        .unwrap()
        .to_string()
}

fn document_component<'a>(
    document: &'a SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
) -> &'a SceneComponentRecord {
    document
        .entities
        .iter()
        .find(|record| record.id == *entity)
        .and_then(|record| record.components.get(component))
        .unwrap()
}

fn label_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Label")
}

fn hidden_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.HiddenLabel")
}

fn position_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.MigratingPosition")
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}
