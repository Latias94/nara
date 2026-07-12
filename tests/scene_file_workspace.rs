#![cfg(feature = "serde")]

use nara::{
    ecs::Component,
    prelude::*,
    reflect::{
        ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentSchema, ComponentValueKind,
    },
    tooling::EditorWorkspace,
    tooling_prelude::EditorWorkspaceCommand,
};

#[derive(Debug, Clone, PartialEq, Eq, Component)]
struct MigratingText(String);

#[test]
fn canonical_scene_file_publishes_through_a_validated_workspace_session() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let mut workspace = EditorWorkspace::new();

    let invalid = SceneDocument::new([SceneEntityRecord::new(scene_id("invalid")).with_component(
        ComponentTypeId::new("nara.test.Unknown"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    )]);
    let invalid_candidate =
        SceneDocumentCandidate::decode_json_bytes(invalid.to_json_string().unwrap().as_bytes())
            .unwrap();
    let error =
        SceneAuthoringSession::try_from_file_candidate(invalid_candidate, &registry).unwrap_err();
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "scene.unknown-component")
    );
    assert!(workspace.is_empty());

    let encoded = SceneDocument::default().to_ron_string().unwrap();
    let candidate = SceneDocumentCandidate::decode_ron_bytes(encoded.as_bytes()).unwrap();
    let session = SceneAuthoringSession::try_from_file_candidate(candidate, &registry).unwrap();
    let source_revision = session.revision();

    let report = workspace.open_scene_session("main", session).unwrap();

    let document = report.opened_document.unwrap();
    assert!(report.applied);
    assert_eq!(workspace.active_document(), Some(document));
    assert_eq!(
        workspace.scene(document).unwrap().revision(),
        source_revision
    );
}

#[test]
fn migrated_scene_file_stays_dirty_until_the_upgraded_source_is_saved() {
    let registry = migrating_text_registry();
    let scene = SceneDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        migrating_text_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("legacy", ComponentValue::String("before".to_owned()))]),
        ),
    )]);
    let encoded = scene.to_json_string().unwrap();
    let session = SceneAuthoringSession::try_from_file_candidate(
        SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
        &registry,
    )
    .unwrap();
    let mut workspace = EditorWorkspace::new();

    let opened = workspace.open_scene_session("main", session).unwrap();
    let document = opened.opened_document.unwrap();

    assert_eq!(opened.dirty, Some(true));
    let slot = workspace.scene(document).unwrap();
    assert!(slot.is_dirty());
    assert!(slot.session().source_upgrade_required());

    let saved = workspace.apply_command(
        &registry,
        EditorWorkspaceCommand::MarkSaved {
            document: Some(document),
        },
    );
    assert!(saved.applied);
    let slot = workspace.scene(document).unwrap();
    assert!(!slot.is_dirty());
    assert!(!slot.session().source_upgrade_required());
}

fn migrating_text_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<MigratingText, _, _>(
            ComponentSchema::new(
                migrating_text_id(),
                "Migrating text",
                ComponentSchemaVersion(2),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("text"),
                "Text",
                ComponentFieldPath::from_fields(["current"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            |value| Ok(MigratingText(value.field_str("current")?.to_owned())),
            |value| {
                Ok(ComponentValue::map([(
                    "current",
                    ComponentValue::String(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry
        .register_component_migration(
            &migrating_text_id(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let legacy = fields
                    .remove("legacy")
                    .ok_or_else(|| ComponentCodecError::missing_field("legacy"))?;
                fields.insert("current".to_owned(), legacy);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrating_text_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.MigratingText")
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
