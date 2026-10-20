#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;
#[path = "support/reference_project_copy.rs"]
mod reference_project_copy;

use std::fs;

use nara::{
    prelude::{ComponentRegistry, Vec2},
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentFieldPath, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponentProvider,
    },
    scene::{SceneAuthoringSession, SceneDocument, SceneDocumentCandidate},
};
use nara_reference_game::{InitialVelocity2d, REFERENCE_GAME_SCHEMA_PROVIDER, project_recipe};
use project_content_fixture::try_load_project_content_from_path;
use reference_project_copy::ReferenceProjectCopy;

const LEGACY: &[u8] = include_bytes!("fixtures/initial-velocity-v1.scene.json");

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    REFERENCE_GAME_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    registry
}

#[test]
fn saved_opaque_vec2_predecessor_migrates_explicitly_and_reopens_as_current() {
    let registry = registry();
    let candidate = SceneDocumentCandidate::decode_json_bytes(LEGACY).unwrap();
    let mut session = SceneAuthoringSession::new(SceneDocument::default());
    session
        .try_replace_file_candidate(candidate, &registry)
        .unwrap();
    assert!(session.source_upgrade_required());
    let id = ComponentTypeId::new("reference_game.InitialVelocity2d");
    let record = &session.document().entities[0].components[&id];
    assert_eq!(record.version, ComponentSchemaVersion::new(2).unwrap());
    assert_eq!(
        InitialVelocity2d::__decode_persistent_component(&record.value)
            .unwrap()
            .velocity,
        Vec2::new(1.5, -2.0)
    );
    let migrated = session.document().clone();
    let saved = migrated.to_json_string().unwrap();
    let mut reopened = SceneAuthoringSession::new(SceneDocument::default());
    reopened
        .try_replace_file_candidate(
            SceneDocumentCandidate::decode_json_bytes(saved.as_bytes()).unwrap(),
            &registry,
        )
        .unwrap();
    assert!(!reopened.source_upgrade_required());
    assert_eq!(reopened.document(), &migrated);

    for invalid in [
        ComponentValue::map([("x", ComponentValue::f64(1.0).unwrap())]),
        ComponentValue::map([
            ("x", ComponentValue::f64(f64::MAX).unwrap()),
            ("y", ComponentValue::f64(0.0).unwrap()),
        ]),
        ComponentValue::map([
            ("x", ComponentValue::f64(1.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
            ("extra", ComponentValue::Bool(true)),
        ]),
    ] {
        let mut legacy = migrated.entities[0].components[&id].value.clone();
        legacy
            .replace_path(&ComponentFieldPath::from_fields(["velocity"]), invalid)
            .unwrap();
        assert!(
            registry
                .migrate_component_value(&id, ComponentSchemaVersion::ONE, &legacy)
                .is_err()
        );
    }
}

#[test]
fn strict_check_admits_saved_vec2_predecessor_without_rewriting_source() {
    let project = ReferenceProjectCopy::new("vec2-predecessor-check");
    let id = ComponentTypeId::new("reference_game.InitialVelocity2d");
    let legacy: serde_json::Value = serde_json::from_slice(LEGACY).unwrap();
    let mut authored: serde_json::Value =
        serde_json::from_slice(&fs::read(project.file("scenes/startup.scene.json")).unwrap())
            .unwrap();
    // Keep the complete production startup closure; replace only the saved predecessor record.
    let player = authored["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == "player")
        .unwrap();
    player["components"][id.as_str()] =
        legacy["payload"]["entities"][0]["components"][id.as_str()].clone();
    let saved = serde_json::to_string_pretty(&authored).unwrap();
    fs::write(project.file("scenes/startup.scene.json"), &saved).unwrap();

    let closure = check_project(ProjectCheckRequest::new(
        project.capability(),
        project_recipe().unwrap(),
    ))
    .expect("strict Check must admit the explicit saved-data migration");
    assert!(!closure.members().is_empty());
    assert_eq!(
        fs::read_to_string(project.file("scenes/startup.scene.json")).unwrap(),
        saved
    );
    let loaded = try_load_project_content_from_path(project.path()).unwrap();
    assert!(loaded.snapshot.source_upgrade_required());
    let player = loaded
        .snapshot
        .scene()
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "player")
        .unwrap();
    let migrated = &player.components[&id];
    assert_eq!(migrated.version, ComponentSchemaVersion::new(2).unwrap());
    assert_eq!(
        InitialVelocity2d::__decode_persistent_component(&migrated.value)
            .unwrap()
            .velocity,
        Vec2::new(1.5, -2.0)
    );
}
