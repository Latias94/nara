#![cfg(any(unix, windows))]

use std::fs;

use nara_core::{DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{
    EditorCommand, EditorDocument, EditorProject, EditorRegistry, EditorSession, HierarchyModel,
    InspectorModel, InspectorValue,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind,
};
use nara_scene::{
    authoring::{SceneComponent, SceneDocument, SceneEntity, builtin_component_schemas},
    spatial::Transform2d,
};

fn id(n: u64) -> EntityUuid {
    format!("fc46193e-2e80-4a44-934a-{n:012x}").parse().unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::new(
        "97270f99-fd97-483f-a605-a1e7acaea745"
            .parse::<DocumentId>()
            .unwrap(),
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("night_relay.RunRules")
}

fn field() -> ComponentFieldPath {
    ComponentFieldPath::from_fields(["shield"])
}

fn registry() -> EditorRegistry {
    let schema = ComponentSchema::new(component(), "Run Rules", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("shield"),
            "Shield",
            field(),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    EditorRegistry::new(
        builtin_component_schemas().unwrap(),
        ComponentSchemaCatalog::from_components([schema]).unwrap(),
    )
    .unwrap()
}

fn actor(n: u64, value: u64) -> SceneEntity {
    let mut row = SceneEntity::new(id(n), format!("Actor {n}"));
    row.components.insert(
        component(),
        SceneComponent {
            version: ComponentSchemaVersion::ONE,
            value: ComponentValue::map([("shield", ComponentValue::U64(value))]),
        },
    );
    row.set(&Transform2d::from_translation(Vec2::new(n as f32, 0.0)))
        .unwrap();
    row
}

fn edit(value: u64) -> EditorCommand {
    EditorCommand::SetField {
        entities: vec![id(1), id(2)],
        component: component(),
        path: field(),
        value: ComponentValue::U64(value),
    }
}

fn value(session: &EditorSession) -> InspectorValue {
    InspectorModel::for_session(session)
        .components
        .into_iter()
        .find(|entry| entry.id == component())
        .unwrap()
        .fields
        .remove(0)
        .value
}

#[test]
fn mixed_fields_gestures_and_unknown_components_survive_save_and_reopen() {
    let unknown = ComponentTypeId::new("third_party.CustomComponent");
    let staying = SceneComponent {
        version: ComponentSchemaVersion(27),
        value: ComponentValue::String("保留原值".into()),
    };
    let mut original = document();
    let mut first = actor(1, 2);
    first.components.insert(unknown.clone(), staying.clone());
    original.entities = vec![actor(2, 3), first];

    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("night-relay.nara.scene.ron");
    original.write(&path).unwrap();
    let mut session = EditorSession::new(EditorDocument::open(&path).unwrap(), registry());
    session.selection_mut().replace([id(1), id(2)]);
    assert_eq!(value(&session), InspectorValue::Mixed);
    assert!(!session.is_dirty());

    session.begin_gesture().unwrap();
    assert!(session.apply(edit(10)).unwrap());
    assert!(session.apply(edit(12)).unwrap());
    assert!(session.end_gesture());
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(12))
    );
    assert!(session.undo().unwrap());
    assert!(!session.can_undo());
    assert!(!session.is_dirty());
    assert_eq!(value(&session), InspectorValue::Mixed);
    assert!(session.redo().unwrap());

    session.begin_gesture().unwrap();
    session.apply(edit(20)).unwrap();
    assert!(session.cancel_gesture());
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(12))
    );
    session.save().unwrap();
    assert!(!session.is_dirty());

    let loaded = EditorDocument::open(&path).unwrap();
    assert_eq!(loaded.scene(), session.document().scene());
    assert_eq!(loaded.scene().entities[0].components[&unknown], staying);
    assert_eq!(
        loaded.scene().entities[0]
            .typed::<Transform2d>()
            .unwrap()
            .unwrap()
            .translation,
        Vec2::X
    );
    session
        .apply(EditorCommand::RemoveComponent {
            entities: vec![id(1)],
            component: unknown.clone(),
        })
        .unwrap();
    assert!(
        !session.document().scene().entities[0]
            .components
            .contains_key(&unknown)
    );
    assert!(session.undo().unwrap());
    assert_eq!(
        session.document().scene().entities[0].components[&unknown],
        staying
    );
}

#[test]
fn hierarchy_reparent_delete_and_undo_are_atomic() {
    let mut scene = document();
    let mut child = actor(2, 3);
    child.parent = Some(id(1));
    scene.entities = vec![actor(1, 2), child, actor(3, 4)];
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry());
    let batch = EditorCommand::Batch(vec![
        EditorCommand::Rename {
            entity: id(1),
            name: "changed".into(),
        },
        EditorCommand::Reparent {
            entities: vec![id(1)],
            parent: Some(id(2)),
        },
    ]);
    assert!(session.apply(batch).is_err());
    assert_eq!(session.document().scene().entities[0].name, "Actor 1");
    assert!(!session.can_undo());

    session.selection_mut().replace([id(2), id(3)]);
    session.apply(EditorCommand::Delete(vec![id(1)])).unwrap();
    assert_eq!(session.selection().entities(), &[id(3)]);
    assert_eq!(session.document().scene().entities.len(), 1);
    assert!(session.undo().unwrap());
    let rows = HierarchyModel::default().rows(session.document().scene());
    assert_eq!(
        rows.iter()
            .map(|row| (row.entity, row.depth))
            .collect::<Vec<_>>(),
        [(id(1), 0), (id(2), 1), (id(3), 0)]
    );
    assert!(session.redo().unwrap());
    assert_eq!(session.document().scene().entities.len(), 1);
}

#[test]
fn project_settings_and_asset_documents_round_trip_on_the_same_mount() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::write(
        root.join("nara.toml"),
        "[project]\nname = 'Night Relay'\nversion = '0.1'\n",
    )
    .unwrap();
    fs::create_dir(root.join("assets")).unwrap();
    let path = AssetPath::new("night-relay.nara.scene.ron").unwrap();
    document()
        .write(root.join("assets").join(path.as_str()))
        .unwrap();

    let mut project = EditorProject::open(root).unwrap();
    assert_eq!(project.assets().unwrap().entries()[0].path, path);
    project.set_entry_scene(Some(path.clone())).unwrap();
    let reopened = EditorProject::open(root).unwrap();
    assert_eq!(reopened.entry_scene(), Some(&path));
    assert_eq!(reopened.open_document(&path).unwrap().scene(), &document());
}
