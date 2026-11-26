#![cfg(feature = "authoring")]

use bevy_ecs::component::Component;
use nara_core::{DocumentId, EntityUuid, Vec2};
use nara_reflect::{
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    PersistentComponent, PersistentComponentProvider,
};
use nara_scene::{
    authoring::{
        ColliderSource, SceneComponent, SceneComponentError, SceneDocument, SceneDocumentError,
        SceneEntity, SpriteSource, builtin_component_schemas,
    },
    spatial::{Transform2d, Visibility},
};

const DOCUMENT: &str = "01234567-89ab-cdef-0123-456789abcdef";
const ROOT: &str = "c62cea0c-0539-4cef-93d5-71c8ff7ee454";
const CHILD: &str = "916da482-3796-4722-a414-98a0cf318a28";
const EXTERNAL: &str = "4f527041-4b8b-48ce-82d0-db2ca0e76d04";

fn document_id() -> DocumentId {
    DOCUMENT.parse().unwrap()
}

fn entity_id(id: &str) -> EntityUuid {
    id.parse().unwrap()
}

#[test]
fn parses_flat_ron_with_forward_parent_links() {
    let source = format!(
        r#"(
            id: "{DOCUMENT}",
            entities: [
                (id: "{CHILD}", name: "路灯", parent: Some("{ROOT}"), components: {{
                    "nara.Visibility": (version: 1, value: (
                        type: "map", value: {{"mode": (type: "string", value: "hidden")}},
                    )),
                }}),
                (id: "{ROOT}", name: "Street"),
            ],
        )"#,
    );
    let document = SceneDocument::parse(&source).unwrap();
    assert_eq!(document.id, document_id());
    assert_eq!(document.entities[0].name, "路灯");
    assert_eq!(document.entities[0].parent, Some(entity_id(ROOT)));
    assert_eq!(
        document.entities[0].typed().unwrap(),
        Some(Visibility::Hidden)
    );
    assert_eq!(document.entities[1].parent, None);
    assert!(document.entities[1].components.is_empty());
}

#[test]
fn rejects_nil_identity_and_duplicate_component_ids_in_ron() {
    let nil = "00000000-0000-0000-0000-000000000000";
    for source in [
        format!(r#"(id: "{nil}")"#),
        format!(r#"(id: "{DOCUMENT}", entities: [(id: "{nil}", name: "Nil")])"#),
        format!(
            r#"(id: "{DOCUMENT}", entities: [(id: "{ROOT}", name: "Nil parent", parent: Some("{nil}"))])"#
        ),
    ] {
        assert!(matches!(
            SceneDocument::parse(&source),
            Err(SceneDocumentError::Parse(_))
        ));
    }

    let duplicate = format!(
        r#"(id: "{DOCUMENT}", entities: [(id: "{ROOT}", name: "Duplicate", components: {{
            "game.Future": (version: 1, value: (type: "null")),
            "game.Future": (version: 2, value: (type: "bool", value: false)),
        }})])"#,
    );
    let Err(SceneDocumentError::Parse(error)) = SceneDocument::parse(&duplicate) else {
        panic!("duplicate component ID should produce a parse error");
    };
    assert!(
        error
            .to_string()
            .contains("duplicate component ID 'game.Future'")
    );
}

#[test]
fn validates_unique_entities_and_closed_acyclic_hierarchy() {
    let mut document = SceneDocument::new(document_id());
    document.entities = vec![
        SceneEntity::new(entity_id(ROOT), "Root"),
        SceneEntity::new(entity_id(CHILD), "Child"),
    ];
    document.entities[1].parent = Some(entity_id(ROOT));
    document.validate().unwrap();

    document.entities[0].parent = Some(entity_id(EXTERNAL));
    assert!(matches!(
        document.validate(),
        Err(SceneDocumentError::MissingParent { entity, parent })
            if entity == entity_id(ROOT) && parent == entity_id(EXTERNAL)
    ));
    for parent in [ROOT, CHILD] {
        document.entities[0].parent = Some(entity_id(parent));
        assert!(matches!(
            document.validate(),
            Err(SceneDocumentError::HierarchyCycle { .. })
        ));
    }

    document.entities[0].parent = None;
    document.entities[1].id = entity_id(ROOT);
    assert!(matches!(
        document.validate(),
        Err(SceneDocumentError::DuplicateEntity { entity }) if entity == entity_id(ROOT)
    ));
}

#[test]
fn unknown_payloads_keep_numeric_kinds_and_references_across_edits() {
    let mut document = SceneDocument::new(document_id());
    let id = ComponentTypeId::new("game.Future");
    let unknown = SceneComponent {
        version: ComponentSchemaVersion::new(17).unwrap(),
        value: ComponentValue::map([(
            "nested",
            ComponentValue::List(vec![
                ComponentValue::I64(i64::MIN),
                ComponentValue::U64(u64::MAX),
                ComponentValue::f64(1.000_000_059_604_644_8).unwrap(),
                ComponentValue::f64(-0.0).unwrap(),
                ComponentValue::EntityReference(entity_id(EXTERNAL)),
                ComponentValue::Null,
            ]),
        )]),
    };
    let mut root = SceneEntity::new(entity_id(ROOT), "Root");
    root.components.insert(id.clone(), unknown.clone());
    let mut child = SceneEntity::new(entity_id(CHILD), "Child");
    child.parent = Some(entity_id(ROOT));
    document.entities = vec![root, child];

    let before = document.to_ron().unwrap();
    document.entities.reverse();
    assert_eq!(document.to_ron().unwrap(), before);
    let mut read = SceneDocument::parse(&before).unwrap();
    assert_eq!(read.entities[0].id, entity_id(CHILD));
    read.entities[0].name = "Edited".into();
    read.entities[0].set(&Visibility::Visible).unwrap();
    let saved = read.to_ron().unwrap();
    let reopened = SceneDocument::parse(&saved).unwrap();
    assert_eq!(reopened.to_ron().unwrap(), saved);
    let retained = &reopened.entities[1].components[&id];
    assert_eq!(retained, &unknown);
    let Some(ComponentValue::List(values)) = retained.value.get("nested") else {
        panic!("unknown nested list was not retained");
    };
    assert_eq!(values[3].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
}

#[derive(Component, Debug, PartialEq, PersistentComponent)]
#[nara(
    crate = "nara_reflect",
    id = "game.Health",
    version = 2,
    alias = "Health",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Health {
    #[nara(id = "hit_points", alias = "Hit Points")]
    hit_points: u16,
}

#[test]
fn typed_values_use_provider_versions_and_builtin_creation_metadata() {
    let mut entity = SceneEntity::new(entity_id(ROOT), "Typed");
    assert_eq!(entity.typed::<Health>().unwrap(), None);
    let health = Health {
        hit_points: u16::MAX,
    };
    let transform = Transform2d::from_xy(13.0, -2.0);
    let collider = ColliderSource {
        radius: Some(2.0),
        ..Default::default()
    };
    entity.set(&health).unwrap();
    entity.set(&transform).unwrap();
    entity.set(&collider).unwrap();
    assert_eq!(entity.typed().unwrap(), Some(transform));
    assert_eq!(entity.typed().unwrap(), Some(collider));
    assert_eq!(entity.typed::<Health>().unwrap(), Some(health));

    let health_id = ComponentTypeId::new(Health::COMPONENT_ID);
    let record = entity.components.get_mut(&health_id).unwrap();
    record.version = ComponentSchemaVersion::ONE;
    assert!(matches!(
        entity.typed::<Health>(),
        Err(SceneComponentError::VersionMismatch { .. })
    ));

    let catalog = ComponentSchemaCatalog::from_components(
        builtin_component_schemas()
            .unwrap()
            .into_iter()
            .chain([Health::persistent_component_schema()]),
    )
    .unwrap();
    assert_eq!(catalog.components().len(), 5);
    for schema in catalog
        .components()
        .iter()
        .filter(|schema| schema.id() != &health_id)
    {
        let record = SceneComponent {
            version: schema.version(),
            value: schema.authoring_initial_value().unwrap().clone(),
        };
        match schema.id().as_str() {
            Transform2d::COMPONENT_ID => {
                assert_eq!(
                    record.typed::<Transform2d>().unwrap(),
                    Transform2d::IDENTITY
                )
            }
            Visibility::COMPONENT_ID => {
                assert_eq!(record.typed::<Visibility>().unwrap(), Visibility::Inherited)
            }
            SpriteSource::COMPONENT_ID => {
                assert_eq!(
                    record.typed::<SpriteSource>().unwrap(),
                    SpriteSource::default()
                )
            }
            ColliderSource::COMPONENT_ID => {
                assert_eq!(
                    record.typed::<ColliderSource>().unwrap(),
                    ColliderSource::default()
                )
            }
            other => panic!("unexpected built-in schema {other}"),
        }
    }

    entity.components.get_mut(&health_id).unwrap().version = ComponentSchemaVersion(0);
    let mut document = SceneDocument::new(document_id());
    document.entities.push(entity);
    assert!(matches!(
        document.to_ron(),
        Err(SceneDocumentError::InvalidComponentVersion { component, .. }) if component == health_id
    ));
}

#[test]
fn sprite_source_accepts_only_portable_png_paths_at_admission() {
    let mut entity = SceneEntity::new(entity_id(ROOT), "Sprite");
    let mut sprite = SpriteSource {
        size: Vec2::new(32.0, 16.0),
        ..Default::default()
    };
    assert_eq!(sprite.image_asset_path().unwrap(), None);
    sprite.image = Some("props/street-lamp.PNG".to_owned());
    entity.set(&sprite).unwrap();
    assert_eq!(
        sprite.image_asset_path().unwrap().unwrap().as_str(),
        "props/street-lamp.PNG"
    );
    let mut document = SceneDocument::new(document_id());
    document.entities.push(entity);
    let good = document.to_ron().unwrap();

    for invalid in [
        "../street-lamp.png",
        "C:/lamp.png",
        "props/CON.png",
        "lamp.jpg",
    ] {
        let malformed = good.replace("props/street-lamp.PNG", invalid);
        assert!(matches!(
            SceneDocument::parse(&malformed),
            Err(SceneDocumentError::InvalidComponentValue { .. })
        ));
        sprite.image = Some(invalid.to_owned());
        assert!(matches!(
            document.entities[0].set(&sprite),
            Err(SceneComponentError::Codec(_))
        ));
    }
    assert_eq!(document.to_ron().unwrap(), good);
}

#[cfg(any(unix, windows))]
#[test]
fn scene_and_prefab_saves_replace_files_atomically_after_validation() {
    let directory = tempfile::tempdir().unwrap();
    let scene_path = directory.path().join("relay.nara.scene.ron");
    let prefab_path = directory.path().join("lamp.nara.prefab.ron");
    let mut document = SceneDocument::new(document_id());
    document
        .entities
        .push(SceneEntity::new(entity_id(ROOT), "Original"));
    document.write(&scene_path).unwrap();
    assert_eq!(SceneDocument::read(&scene_path).unwrap(), document);

    document.entities[0].name = "Replacement".to_owned();
    document.write(&scene_path).unwrap();
    document.write(&prefab_path).unwrap();
    let saved = std::fs::read(&scene_path).unwrap();
    assert_eq!(std::fs::read(&prefab_path).unwrap(), saved);
    assert_eq!(SceneDocument::read(&prefab_path).unwrap(), document);

    document.entities[0].parent = Some(entity_id(EXTERNAL));
    assert!(matches!(
        document.write(&scene_path),
        Err(SceneDocumentError::MissingParent { .. })
    ));
    assert_eq!(std::fs::read(&scene_path).unwrap(), saved);

    document.entities[0].parent = None;
    let blocked = directory.path().join("blocked.nara.scene.ron");
    std::fs::create_dir(&blocked).unwrap();
    assert!(matches!(
        document.write(&blocked),
        Err(SceneDocumentError::Io { .. })
    ));
    assert!(blocked.is_dir());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 3);
}
