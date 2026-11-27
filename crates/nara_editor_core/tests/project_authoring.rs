#![cfg(any(unix, windows))]

use std::{collections::BTreeMap, fs, path::Path};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_editor_core::{
    EditorCommand, EditorError, EditorProject, EditorRegistry, EditorSession, ProjectError,
    SelectionMode,
};
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
fn external_scene_write_is_undoable_and_preserves_existing_history() {
    let directory = project();
    let path = AssetPath::new("scene.scene.ron").unwrap();
    let native = directory.path().join("assets").join(path.as_str());
    let mut scene = document(1);
    let entity = scene.entities[0].id;
    scene.write(&native).unwrap();
    let project = EditorProject::open(directory.path()).unwrap();
    let id = project.asset_reference(&path).unwrap().id;
    let mut session = EditorSession::new(
        project.open_document(&path).unwrap(),
        EditorRegistry::default(),
    );
    session
        .selection_mut()
        .select(entity, SelectionMode::Replace, &[entity]);
    session
        .apply(EditorCommand::Rename {
            entity,
            name: "Saved edit".into(),
        })
        .unwrap();
    session.save().unwrap();
    session
        .apply(EditorCommand::Rename {
            entity,
            name: "Redo edit".into(),
        })
        .unwrap();
    session.undo().unwrap();
    assert!(!session.is_dirty());

    scene.entities[0].name = "External edit".into();
    scene.write(&native).unwrap();
    let disk = project.reload_document(id, &[]).unwrap();
    assert!(session.reload_sources(Some(disk), BTreeMap::new()).unwrap());
    assert_eq!(session.document().scene(), &scene);
    assert_eq!(session.selection().entities(), &[entity]);
    assert!(!session.is_dirty());
    assert!(session.can_undo() && session.can_redo());
    session.redo().unwrap();
    assert_eq!(session.document().scene().entities[0].name, "Redo edit");
    session.undo().unwrap();
    assert_eq!(session.document().scene(), &scene);
    session.undo().unwrap();
    assert_eq!(session.document().scene().entities[0].name, "Saved edit");
    session.undo().unwrap();
    assert_eq!(session.document().scene().entities[0].name, "Root");
}

#[test]
fn reload_recognizes_a_moved_scene_without_disturbing_a_dirty_edit_or_gesture() {
    let directory = project();
    let assets = directory.path().join("assets");
    let path = AssetPath::new("original.scene.ron").unwrap();
    let mut scene = document(1);
    let entity = scene.entities[0].id;
    scene.write(assets.join(path.as_str())).unwrap();
    let mut project = EditorProject::open(directory.path()).unwrap();
    let id = project.asset_reference(&path).unwrap().id;
    let mut session = EditorSession::new(
        project.open_document(&path).unwrap(),
        EditorRegistry::default(),
    );
    session
        .apply(EditorCommand::Rename {
            entity,
            name: "Local unsaved".into(),
        })
        .unwrap();
    let gesture = session.begin_gesture().unwrap();
    session
        .apply_gesture(
            &gesture,
            EditorCommand::Rename {
                entity,
                name: "Local gesture".into(),
            },
        )
        .unwrap();

    let renamed = assets.join("moved.scene.ron");
    fs::rename(assets.join(path.as_str()), &renamed).unwrap();
    fs::rename(
        assets.join("original.scene.ron.meta"),
        assets.join("moved.scene.ron.meta"),
    )
    .unwrap();
    scene.entities[0].name = "External edit".into();
    scene.write(&renamed).unwrap();
    project.rescan_assets().unwrap();
    assert!(matches!(
        session.snapshot_for_background(),
        Err(EditorError::GestureActive)
    ));
    let disk = project.reload_document(id, &[]).unwrap();
    assert!(matches!(
        session.reload_sources(Some(disk.clone()), BTreeMap::new()),
        Err(EditorError::GestureActive)
    ));
    assert!(session.owns_gesture(&gesture));
    assert_eq!(session.document().scene().entities[0].name, "Local gesture");
    session.cancel_gesture(&gesture);

    assert!(!session.reload_sources(Some(disk), BTreeMap::new()).unwrap());
    assert_eq!(
        session.document().path(),
        Some(renamed.canonicalize().unwrap().as_path())
    );
    assert_eq!(session.document().scene().entities[0].name, "Local unsaved");
    assert!(session.is_dirty());
    session.undo().unwrap();
    assert_eq!(session.document().scene().entities[0].name, "Root");
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
