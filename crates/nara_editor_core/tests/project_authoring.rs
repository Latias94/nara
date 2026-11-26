#![cfg(any(unix, windows))]

use std::{collections::BTreeMap, fs, path::Path};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_editor_core::{EditorCommand, EditorProject, EditorRegistry, EditorSession, ProjectError};
use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::authoring::{
    PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneEntity,
    builtin_component_schemas,
};

fn project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("nara.toml"),
        "[project]\nname = 'Test'\nversion = '0.1.0'\n",
    )
    .unwrap();
    fs::create_dir(project.path().join("assets")).unwrap();
    project
}

fn document(index: u64) -> SceneDocument {
    let mut document = SceneDocument::new(
        format!("ad25c4d9-7626-4a99-adeb-{index:012x}")
            .parse::<DocumentId>()
            .unwrap(),
    );
    document.entities.push(SceneEntity::new(
        format!("c1dd808f-dda4-42fb-b4a6-{index:012x}")
            .parse::<EntityUuid>()
            .unwrap(),
        "Root",
    ));
    document
}

fn sidecar(path: &Path, asset: AssetId, importer: &str) {
    let meta = path.with_file_name(format!(
        "{}.meta",
        path.file_name().unwrap().to_str().unwrap()
    ));
    fs::write(
        meta,
        format!("AssetMeta(id: \"{asset}\", importer: \"{importer}\", settings: ())"),
    )
    .unwrap();
}

#[test]
fn saving_a_moved_document_resolves_its_sidecar_again() {
    let directory = project();
    let assets = directory.path().join("assets");
    let path = AssetPath::new("first.scene.ron").unwrap();
    let original = assets.join(path.as_str());
    let renamed = assets.join("moved.scene.ron");
    document(1).write(&original).unwrap();
    let mut project = EditorProject::open(directory.path()).unwrap();
    let id = project.asset_reference(&path).unwrap().id;
    project.set_entry_scene(Some(path.clone())).unwrap();
    let loaded = project.open_document(&path).unwrap();
    let mut session = EditorSession::new(loaded, EditorRegistry::default());

    fs::rename(&original, &renamed).unwrap();
    fs::rename(
        assets.join("first.scene.ron.meta"),
        assets.join("moved.scene.ron.meta"),
    )
    .unwrap();
    document(2).write(&original).unwrap();
    let other = fs::read(&original).unwrap();
    let entity = session.document().scene().entities[0].id;
    session
        .apply(EditorCommand::Rename {
            entity,
            name: "Edited".into(),
        })
        .unwrap();
    let destination = project
        .document_save_path(id, session.document().scene().id)
        .unwrap();
    session.save_as(destination).unwrap();
    assert_eq!(
        SceneDocument::read(renamed).unwrap().entities[0].name,
        "Edited"
    );
    assert_eq!(fs::read(original).unwrap(), other);
    let reopened = EditorProject::open(directory.path()).unwrap();
    assert_eq!(
        reopened.entry_scene().map(AssetPath::as_str),
        Some("moved.scene.ron")
    );
}

#[test]
fn publishing_a_prefab_never_overwrites_another_identity_at_its_old_path() {
    let directory = project();
    let path = directory.path().join("assets/piece.prefab.ron");
    let source = document(1);
    source.write(&path).unwrap();
    let mut prefab = source.clone();
    prefab.entities[0].name = "Changed".into();
    let id = "01951a29-2e18-7000-9000-000000000001".parse().unwrap();
    sidecar(&path, id, "nara.prefab");
    let project = EditorProject::open(directory.path()).unwrap();
    sidecar(
        &path,
        "01951a29-2e18-7000-9000-000000000002".parse().unwrap(),
        "nara.prefab",
    );
    let expected = BTreeMap::from([(id, source.clone())]);
    assert!(matches!(
        project.write_prefab(id, &prefab, &expected, &ComponentSchemaCatalog::default()),
        Err(ProjectError::MissingPrefab(found)) if found == id,
    ));
    assert_eq!(SceneDocument::read(path).unwrap(), source);
}

#[test]
fn prefab_publish_checks_overrides_in_the_already_saved_scene() {
    let directory = project();
    let assets = directory.path().join("assets");
    let path = assets.join("piece.prefab.ron");
    let type_id = ComponentTypeId::new("test.Tiles");
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("cells"),
        "Cells",
        ComponentFieldPath::from_fields(["cells"]),
        ComponentValueKind::PackedU16,
    );
    let schemas = builtin_component_schemas()
        .unwrap()
        .into_iter()
        .chain([
            ComponentSchema::new(type_id.clone(), "Tiles", ComponentSchemaVersion::ONE)
                .with_fields([field]),
        ]);
    let catalog = ComponentSchemaCatalog::from_components(schemas).unwrap();
    let tiles = |cells| {
        SceneComponent::known(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("cells", ComponentValue::PackedU16(cells))]),
        )
    };

    let mut source = document(1);
    source.entities[0]
        .components
        .insert(type_id.clone(), tiles(vec![7, 8]));
    source.write_with(&path, catalog.components()).unwrap();
    let original = fs::read(&path).unwrap();
    let id = "01951a29-2e18-7000-9000-000000000001".parse().unwrap();
    sidecar(&path, id, "nara.prefab");
    let mut dependent = document(2);
    dependent.assets.insert(
        "piece".into(),
        SceneAsset {
            id,
            hint: AssetPath::new("piece.prefab.ron").unwrap(),
        },
    );
    let mut instance = PrefabInstance::new(id);
    instance
        .overrides
        .insert("test.Tiles.cells[1]".into(), RawRon::from_ron("9").unwrap());
    dependent.entities[0].prefab = Some(instance);
    dependent.write(assets.join("saved.scene.ron")).unwrap();
    let project = EditorProject::open(directory.path()).unwrap();

    let mut shortened = source.clone();
    shortened.entities[0]
        .components
        .insert(type_id, tiles(vec![7]));
    let expected = BTreeMap::from([(id, source)]);
    assert!(matches!(
        project.write_prefab(id, &shortened, &expected, &catalog),
        Err(ProjectError::PrefabDependent { .. }),
    ));
    assert_eq!(fs::read(path).unwrap(), original);
}
