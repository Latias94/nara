#![cfg(feature = "authoring")]

use bevy_ecs::component::Component;
use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    Bytes, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PackedF32, PackedI32, PackedU16, PackedU32, Schema,
};
use nara_scene::{
    authoring::{
        ColliderSource, RawRon, SceneAsset, SceneComponent, SceneComponentError, SceneDocument,
        SceneDocumentError, SceneEntity, SpriteSource, builtin_component_schemas,
    },
    spatial::{Transform2d, Visibility},
};

const DOCUMENT: &str = "01234567-89ab-cdef-0123-456789abcdef";
const ROOT: &str = "c62cea0c-0539-4cef-93d5-71c8ff7ee454";
const CHILD: &str = "916da482-3796-4722-a414-98a0cf318a28";
const EXTERNAL: &str = "4f527041-4b8b-48ce-82d0-db2ca0e76d04";
const IMAGE: &str = "0193a100-1111-7000-8000-000000000001";
const PREFAB: &str = "0193a100-2222-7000-8000-000000000002";

fn document_id() -> DocumentId {
    DOCUMENT.parse().unwrap()
}

fn entity_id(id: &str) -> EntityUuid {
    id.parse().unwrap()
}

fn entity_component(payload: &str, component: &str) -> String {
    format!(
        r#"Scene(version: 1, id: "{DOCUMENT}", entities: [
            (id: "{ROOT}", name: "Root", components: {{"{component}": {payload}}}),
        ])"#,
    )
}

#[test]
fn parses_named_or_unnamed_documents_with_forward_parent_links() {
    let body = format!(
        r#"(
            version: 1,
            id: "{DOCUMENT}",
            entities: [
                (id: "{CHILD}", name: "路灯", parent: Some("{ROOT}"), components: {{
                    "nara.Visibility": (mode: "hidden"),
                }}),
                (id: "{ROOT}", name: "Street"),
            ],
        )"#,
    );
    for prefix in ["", "Scene", "/* source header */\nPrefab"] {
        let document = SceneDocument::parse(&format!("{prefix}{body}")).unwrap();
        assert_eq!(document.id, document_id());
        assert_eq!(document.entities[0].name, "路灯");
        assert_eq!(document.entities[0].parent, Some(entity_id(ROOT)));
        assert_eq!(
            document.entities[0].typed().unwrap(),
            Some(Visibility::Hidden)
        );
        assert_eq!(document.entities[1].parent, None);
    }
    assert!(SceneDocument::parse(&format!("AssetMeta{body}")).is_err());
}

#[test]
fn rejects_legacy_documents_invalid_identity_and_duplicate_keys() {
    let nil = "00000000-0000-0000-0000-000000000000";
    for source in [
        format!(r#"(id: "{DOCUMENT}")"#),
        format!(r#"(version: 1, id: "{nil}")"#),
        format!(r#"(version: 1, id: "{DOCUMENT}", entities: [(id: "{nil}", name: "Nil")])"#),
        format!(
            r#"(version: 1, id: "{DOCUMENT}", entities: [(id: "{ROOT}", name: "Nil parent", parent: Some("{nil}"))])"#
        ),
        format!(
            r#"(version: 1, id: "{DOCUMENT}", entities: [(id: "{ROOT}", name: "Duplicate", components: {{
                "game.Future": Cast,
                "game.Future": AnotherCast(2),
            }})])"#,
        ),
        format!(
            r#"(version: 1, id: "{DOCUMENT}", assets: {{
                "texture": (id: "{IMAGE}", hint: "one.png"),
                "texture": (id: "{IMAGE}", hint: "two.png"),
            }})"#,
        ),
    ] {
        assert!(
            matches!(
                SceneDocument::parse(&source),
                Err(SceneDocumentError::Parse(_))
            ),
            "{source}",
        );
    }
    let unsupported = format!(r#"Scene(version: 2, id: "{DOCUMENT}")"#);
    assert!(matches!(
        SceneDocument::parse(&unsupported),
        Err(SceneDocumentError::UnsupportedVersion(2))
    ));
    let verbose = entity_component(
        r#"(version: 1, value: (type: "map", value: {"mode": "hidden"}))"#,
        Visibility::COMPONENT_ID,
    );
    assert!(matches!(
        SceneDocument::parse(&verbose),
        Err(SceneDocumentError::InvalidComponentValue { .. })
    ));
    let named_record = entity_component("MisnamedEnum(mode: \"hidden\")", Visibility::COMPONENT_ID);
    assert!(matches!(
        SceneDocument::parse(&named_record),
        Err(SceneDocumentError::InvalidComponentValue { .. })
    ));
    let extensions = format!("#![enable(implicit_some)] Scene(version: 1, id: \"{DOCUMENT}\")");
    assert!(matches!(
        SceneDocument::parse(&extensions),
        Err(SceneDocumentError::Extensions)
    ));
    let orphan = format!(
        r#"(version: 1, id: "{DOCUMENT}", entities: [(id: "{ROOT}", name: "Orphan",
            component_versions: {{"game.Future": 2}})])"#,
    );
    assert!(matches!(
        SceneDocument::parse(&orphan),
        Err(SceneDocumentError::OrphanComponentVersion { .. })
    ));
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
fn unknown_enum_payload_survives_an_unrelated_edit_byte_for_byte() {
    let payload = concat!(
        "/* leading */ FutureCast(\r\n",
        "  // Inner enum and CRLF are author data.\r\n",
        "  branch: Arcane(Phase6({1: [0xA, 1_024u64, -0.0]})),\r\n",
        "  marker: r#\"no \\\"reformat\\\" 路灯\"#,\r\n",
        ") /* trailing */ // closing note\r\n",
    );
    let source = format!(
        r#"Scene(version: 1, id: "{DOCUMENT}", entities: [
            (id: "{ROOT}", name: "Unknown", components: {{"game.Future": {payload}}},
                component_versions: {{"game.Future": 17}}),
            (id: "{CHILD}", name: "Child", parent: Some("{ROOT}")),
        ])"#,
    );
    let future = ComponentTypeId::new("game.Future");
    let mut document = SceneDocument::parse(&source).unwrap();
    let SceneComponent::Unknown(unknown) = &document.entities[0].components[&future] else {
        panic!("future component must be opaque");
    };
    assert_eq!(unknown.type_id, future);
    assert_eq!(unknown.raw.as_ron().as_bytes(), payload.as_bytes());
    assert_eq!(unknown.version.get(), 17);
    let expected = document.entities[0].components[&future].clone();

    document.entities[1].name = "Edited".into();
    document.entities[1].set(&Visibility::Visible).unwrap();
    let saved = document.to_ron().unwrap();
    assert!(saved.contains(payload));
    document.entities.reverse();
    assert_eq!(document.to_ron().unwrap(), saved);
    let reopened = SceneDocument::parse(&saved).unwrap();
    assert_eq!(reopened.to_ron().unwrap(), saved);
    assert_eq!(reopened.entities[0].id, entity_id(CHILD));
    assert_eq!(reopened.entities[1].components[&future], expected);
}

#[derive(Component, Debug, PartialEq, Schema)]
#[schema(crate = "nara_reflect", id = "game.Health", version = 2, default)]
struct Health {
    title: String,
    hit_points: u16,
    armor: Option<u16>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            title: "Player".into(),
            hit_points: 25,
            armor: Some(7),
        }
    }
}

#[test]
fn merges_schema_defaults_and_writes_field_declaration_order_and_sparse_versions() {
    let health_id = ComponentTypeId::new(Health::COMPONENT_ID);
    let schemas = ComponentSchemaCatalog::from_components(
        builtin_component_schemas()
            .unwrap()
            .into_iter()
            .chain([Health::type_schema().unwrap()]),
    )
    .unwrap();
    let mut entity = SceneEntity::new(entity_id(ROOT), "Typed");
    assert_eq!(entity.typed::<Health>().unwrap(), None);
    let health = Health {
        title: "Boss".into(),
        hit_points: u16::MAX,
        armor: None,
    };
    entity.set(&health).unwrap();
    entity.set(&Visibility::Inherited).unwrap();
    entity.set(&ColliderSource::default()).unwrap();
    entity.set(&Transform2d::from_xy(13.0, -2.0)).unwrap();

    let mut document = SceneDocument::new(document_id());
    document.entities.push(entity);
    assert!(matches!(
        document.to_ron(),
        Err(SceneDocumentError::MissingComponentSchema { component, .. }) if component == health_id
    ));
    let saved = document.to_ron_with(schemas.components()).unwrap();
    assert!(saved.contains(r#""game.Health": (title: "Boss", hit_points: 65535, armor: None)"#));
    assert!(saved.contains(r#""nara.ColliderSource": ()"#));
    assert!(saved.contains(r#""nara.Transform2d": (translation: (13.0, -2.0))"#));
    assert!(saved.contains(r#""nara.Visibility": ()"#));
    assert!(saved.contains("component_versions:"));
    assert!(saved.contains(r#""game.Health": 2"#));
    assert!(!saved.contains("scale:"));
    assert!(!saved.contains(r#""nara.Visibility": 1"#));
    assert!(saved.find(r#""game.Health""#).unwrap() < saved.find(r#""nara.Visibility""#).unwrap());

    let loaded = SceneDocument::parse_with(&saved, schemas.components()).unwrap();
    assert_eq!(loaded.entities[0].typed::<Health>().unwrap(), Some(health));
    assert_eq!(
        loaded.entities[0].typed::<ColliderSource>().unwrap(),
        Some(ColliderSource::default())
    );
    assert_eq!(
        loaded.entities[0].typed::<Visibility>().unwrap(),
        Some(Visibility::Inherited)
    );
    assert_eq!(loaded.to_ron_with(schemas.components()).unwrap(), saved);

    let mut default_source =
        saved.replace(r#"(title: "Boss", hit_points: 65535, armor: None)"#, "()");
    let default_loaded = SceneDocument::parse_with(&default_source, schemas.components()).unwrap();
    assert_eq!(
        default_loaded.entities[0].typed::<Health>().unwrap(),
        Some(Health::default())
    );
    default_source = default_source.replace(r#""game.Health": 2"#, r#""game.Health": 3"#);
    let opaque = SceneDocument::parse_with(&default_source, schemas.components()).unwrap();
    assert_eq!(opaque.entities[0].components[&health_id].version().get(), 3);
    assert!(matches!(
        opaque.entities[0].typed::<Health>(),
        Err(SceneComponentError::VersionMismatch { .. })
    ));
    assert_eq!(
        opaque.to_ron_with(schemas.components()).unwrap(),
        default_source
    );
}

#[test]
fn refresh_materializes_only_valid_matching_payloads_including_added_components() {
    let source = format!(
        r#"Scene(version: 1, id: "{DOCUMENT}", entities: [
            (id: "{ROOT}", name: "Good", components: {{"game.Health": (hit_points: 80)}},
                component_versions: {{"game.Health": 2}}),
            (id: "{CHILD}", name: "Bad", components: {{"game.Health": (hit_points: "bad")}},
                component_versions: {{"game.Health": 2}}),
            (id: "{EXTERNAL}", name: "Prefab", prefab: "{PREFAB}",
                added_components: {{"game.Health": (title: "Extra")}},
                added_component_versions: {{"game.Health": 2}}),
        ])"#,
    );
    let mut document = SceneDocument::parse(&source).unwrap();
    let schema = Health::type_schema().unwrap();
    let before = document.clone();
    assert!(matches!(
        document.resolve_unknown_components(&[schema.clone(), schema.clone()]),
        Err(SceneDocumentError::DuplicateSchema { .. })
    ));
    assert_eq!(document, before);
    let errors = document
        .resolve_unknown_components(std::slice::from_ref(&schema))
        .unwrap();
    assert!(matches!(
        errors.as_slice(),
        [SceneDocumentError::InvalidComponentValue { entity, .. }] if *entity == entity_id(CHILD)
    ));
    assert_eq!(
        document.entities[0]
            .typed::<Health>()
            .unwrap()
            .unwrap()
            .hit_points,
        80
    );
    assert_eq!(document.entities[1], before.entities[1]);
    let added = &document.entities[2]
        .prefab
        .as_ref()
        .unwrap()
        .added_components;
    assert_eq!(
        added[&ComponentTypeId::new(Health::COMPONENT_ID)]
            .typed::<Health>()
            .unwrap()
            .title,
        "Extra"
    );
    let saved = document.to_ron_with(&[schema]).unwrap();
    assert!(saved.contains(r#"(hit_points: "bad")"#));
}

#[test]
fn asset_aliases_decode_to_stable_ids_and_prefab_edits_are_flat() {
    let source = format!(
        r#"Prefab(version: 1, id: "{DOCUMENT}", assets: {{
            "z_texture": (id: "{IMAGE}", hint: "textures/renamed.png"),
            "a_texture": (id: "{IMAGE}", hint: "textures/current.png"),
            "lamp": (id: "{PREFAB}", hint: "prefabs/lamp.prefab.ron"),
        }}, entities: [
            (id: "{ROOT}", name: "Lamp", prefab: "@lamp", components: {{
                "nara.SpriteSource": (image: "@z_texture", tint: (0.3, 0.2, 0.1, 1.0)),
            }}, overrides: {{"nara.Transform2d.translation": (6.0, 8.0)}},
                added_components: {{"nara.Visibility": (mode: "hidden")}},
                removed_components: ["game.Z", "game.A"]),
        ])"#,
    );
    let document = SceneDocument::parse(&source).unwrap();
    let image_id = IMAGE.parse::<AssetId>().unwrap();
    let entity = &document.entities[0];
    let sprite = entity.typed::<SpriteSource>().unwrap().unwrap();
    assert_eq!(sprite.image, Some(image_id));
    assert_eq!(
        entity.components[&ComponentTypeId::new(SpriteSource::COMPONENT_ID)]
            .value()
            .unwrap()
            .get("image"),
        Some(&ComponentValue::AssetReference(image_id)),
    );
    let prefab = entity.prefab.as_ref().unwrap();
    assert_eq!(prefab.asset, PREFAB.parse().unwrap());
    assert_eq!(
        prefab.overrides["nara.Transform2d.translation"].as_ron(),
        "(6.0, 8.0)"
    );
    assert_eq!(
        prefab.added_components[&ComponentTypeId::new(Visibility::COMPONENT_ID)]
            .typed::<Visibility>()
            .unwrap(),
        Visibility::Hidden
    );
    let saved = document.to_ron().unwrap();
    assert!(saved.contains(r#"image: "@a_texture""#));
    assert!(saved.contains("tint: (0.3, 0.2, 0.1, 1.0)"));
    assert!(saved.contains(r#"prefab: "@lamp""#));
    assert!(saved.find(r#""a_texture""#).unwrap() < saved.find(r#""z_texture""#).unwrap());
    assert!(saved.find(r#""game.A""#).unwrap() < saved.find(r#""game.Z""#).unwrap());
    assert_eq!(SceneDocument::parse(&saved).unwrap(), document);

    assert!(SceneDocument::parse(&source.replace("@z_texture", "@typo")).is_err());
    assert!(
        SceneDocument::parse(&source.replace("textures/current.png", "../current.png")).is_err()
    );
    assert!(matches!(
        SceneDocument::parse(&source.replace(r#"prefab: "@lamp", "#, "")),
        Err(SceneDocumentError::OrphanPrefabEdits { .. })
    ));

    let direct = entity_component(
        &format!(r#"(image: "{IMAGE}")"#),
        SpriteSource::COMPONENT_ID,
    );
    assert!(
        SceneDocument::parse(&direct)
            .unwrap()
            .to_ron()
            .unwrap()
            .contains(IMAGE)
    );
}

#[test]
fn non_identifier_schema_paths_use_quoted_keys() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("game.Key"),
        "Key",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("name"),
        "Name",
        ComponentFieldPath::from_fields(["wire-key"]),
        ComponentValueKind::String,
    )]);
    let schemas = [schema];
    let source = entity_component(r#"{"wire-key": "name"}"#, "game.Key");
    let saved = SceneDocument::parse_with(&source, &schemas)
        .unwrap()
        .to_ron_with(&schemas)
        .unwrap();
    assert!(saved.contains(r#""game.Key": {"wire-key": "name"}"#));
    let duplicate = entity_component(r#"{"wire-key": "left", "wire-key": "right"}"#, "game.Key");
    assert!(matches!(
        SceneDocument::parse_with(&duplicate, &schemas),
        Err(SceneDocumentError::InvalidComponentValue { .. })
    ));
}

#[derive(Component, Debug, Default, PartialEq, Schema)]
#[schema(crate = "nara_reflect", id = "game.Tiles", version = 1, default)]
struct Tiles {
    cells: PackedU16,
    big: PackedU32,
    offsets: PackedI32,
    weights: PackedF32,
    metadata: Bytes,
}

#[test]
fn packed_arrays_are_plain_numeric_values_with_typed_widths() {
    let schemas = [Tiles::type_schema().unwrap()];
    let source = entity_component(
        "(cells: [0,65535], big: [4294967295], offsets: [-2147483648], weights: [0.125,2.5], metadata: [0,255])",
        Tiles::COMPONENT_ID,
    );
    let document = SceneDocument::parse_with(&source, &schemas).unwrap();
    let typed: Tiles = document.entities[0].typed().unwrap().unwrap();
    assert_eq!(typed.cells.0, [0, u16::MAX]);
    assert_eq!(typed.big.0, [u32::MAX]);
    assert_eq!(typed.offsets.0, [i32::MIN]);
    assert_eq!(typed.weights.0, [0.125, 2.5]);
    assert_eq!(typed.metadata.0, [0, u8::MAX]);
    let saved = document.to_ron_with(&schemas).unwrap();
    assert!(saved.contains("cells: [0,65535]"));
    assert!(saved.contains("weights: [0.125,2.5]"));
    assert_eq!(
        SceneDocument::parse_with(&saved, &schemas).unwrap(),
        document
    );
    for invalid in ["(cells: [65536])", "(weights: [NaN])"] {
        assert!(matches!(
            SceneDocument::parse_with(&entity_component(invalid, Tiles::COMPONENT_ID), &schemas),
            Err(SceneDocumentError::InvalidComponentValue { .. })
        ));
    }
}

#[test]
fn compact_10000_cells_are_at_least_ten_times_smaller_than_v0_typed_pretty_json() {
    #[derive(serde::Serialize)]
    struct V0Cell {
        r#type: &'static str,
        value: u16,
    }

    let cells: Vec<u16> = (0..10_000).map(|index| (index % 32) as u16).collect();
    let legacy_cells: Vec<_> = cells
        .iter()
        .map(|&value| V0Cell {
            r#type: "u32",
            value,
        })
        .collect();
    let legacy = serde_json::to_vec_pretty(&legacy_cells).unwrap();
    let mut entity = SceneEntity::new(entity_id(ROOT), "Tilemap");
    entity
        .set(&Tiles {
            cells: PackedU16(cells.clone()),
            ..Tiles::default()
        })
        .unwrap();
    let mut document = SceneDocument::new(document_id());
    document.entities.push(entity);
    let schemas = [Tiles::type_schema().unwrap()];
    let saved = document.to_ron_with(&schemas).unwrap();
    eprintln!(
        "10,000 representative cells (0..31 repeating): {} bytes v0-style pretty JSON cell array, {} bytes complete compact scene RON ({:.2}x)",
        legacy.len(),
        saved.len(),
        legacy.len() as f64 / saved.len() as f64,
    );
    assert!(legacy.len() >= saved.len() * 10);
    let loaded = SceneDocument::parse_with(&saved, &schemas).unwrap();
    assert_eq!(
        loaded.entities[0]
            .typed::<Tiles>()
            .unwrap()
            .unwrap()
            .cells
            .0,
        cells
    );
    assert_eq!(loaded.to_ron_with(&schemas).unwrap(), saved);
}

#[cfg(any(unix, windows))]
#[test]
fn saves_named_files_atomically_after_validation() {
    let directory = tempfile::tempdir().unwrap();
    let scene_path = directory.path().join("relay.nara.scene.ron");
    let prefab_path = directory.path().join("lamp.prefab.ron");
    let mut document = SceneDocument::new(document_id());
    document
        .entities
        .push(SceneEntity::new(entity_id(ROOT), "Original"));
    document.assets.insert(
        "texture".into(),
        SceneAsset {
            id: IMAGE.parse().unwrap(),
            hint: AssetPath::new("texture.png").unwrap(),
        },
    );
    document.write(&scene_path).unwrap();
    assert_eq!(SceneDocument::read(&scene_path).unwrap(), document);

    document.entities[0].name = "Replacement".into();
    document.write(&scene_path).unwrap();
    document.write(&prefab_path).unwrap();
    let saved = std::fs::read_to_string(&scene_path).unwrap();
    assert!(saved.starts_with("Scene("));
    assert!(
        std::fs::read_to_string(&prefab_path)
            .unwrap()
            .starts_with("Prefab(")
    );
    assert_eq!(SceneDocument::read(&prefab_path).unwrap(), document);

    document.entities[0].parent = Some(entity_id(EXTERNAL));
    assert!(matches!(
        document.write(&scene_path),
        Err(SceneDocumentError::MissingParent { .. })
    ));
    assert_eq!(std::fs::read_to_string(&scene_path).unwrap(), saved);

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

#[test]
fn raw_ron_constructor_accepts_one_value_and_serializes_as_ron() {
    let raw = RawRon::from_ron("Changed(1, true)").unwrap();
    assert_eq!(ron::to_string(&raw).unwrap(), raw.as_ron());
    assert!(RawRon::from_ron("Changed(1) Other").is_err());
}
