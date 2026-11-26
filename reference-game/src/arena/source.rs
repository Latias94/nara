use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    path::Path,
};

use nara::{
    core::{
        AssetId,
        vfs::{AssetPath, AssetSource, DirAssetSource},
    },
    prelude::App,
    reflect::{ComponentSchema, ComponentSchemaCatalog, component_registry},
    scene::authoring::{
        SceneDocument, SpriteSource, builtin_component_schemas, prefab::expand_prefabs,
    },
};
use nara_asset::{
    AssetScanMode, AssetSourceKind, AssetSourcePath, Handle, ProjectAssetDatabase,
    image::{ImageAsset, ImageSourceDirectory, load_image},
};

use crate::register_game_schema;

const ENTRY_SCENE_ID: &str = "01a219c4-2965-7297-b503-da50fb749023";

pub(crate) type TextureHandles = BTreeMap<AssetId, Handle<ImageAsset>>;

pub(crate) fn development_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn load(
    app: &mut App,
    project_root: &Path,
    snapshot: Option<&Path>,
) -> Result<(SceneDocument, TextureHandles), Box<dyn Error>> {
    register_game_schema(app)?;
    let schemas = component_registry(app.world())
        .map(ComponentSchemaCatalog::components)
        .ok_or("could not read the Night Relay Schema catalog")?;
    let source = DirAssetSource::new(project_root)?;
    let assets = ProjectAssetDatabase::scan(
        source.root(),
        &[AssetSourcePath::new("assets")?],
        AssetScanMode::ReadOnly,
    )?;
    let scene = match snapshot {
        Some(path) => SceneDocument::read_with(path, schemas)
            .map_err(|error| format!("could not load {}: {error}", path.display()))?,
        None => read_document(
            &source,
            &assets,
            ENTRY_SCENE_ID.parse()?,
            AssetSourceKind::Scene,
            schemas,
        )?,
    };
    let catalog = ComponentSchemaCatalog::from_components(
        builtin_component_schemas()?
            .into_iter()
            .chain(schemas.iter().cloned()),
    )?;
    let scene = expand_prefabs(&scene, &catalog, |id| {
        read_document(&source, &assets, id, AssetSourceKind::Prefab, schemas)
    })?
    .document;

    app.insert_resource(ImageSourceDirectory::new(source.root())?)?;
    let world = app.world_mut()?;
    let mut textures = BTreeMap::new();
    for entity in &scene.entities {
        if let Some(id) = entity
            .typed::<SpriteSource>()?
            .and_then(|sprite| sprite.image)
            && let Entry::Vacant(slot) = textures.entry(id)
        {
            let path = asset_path(&assets, id, AssetSourceKind::Image)?;
            let texture = load_image(world, path.as_str())
                .map_err(|error| format!("could not load {path}: {error}"))?;
            slot.insert(texture);
        }
    }
    Ok((scene, textures))
}

fn read_document(
    source: &DirAssetSource,
    assets: &ProjectAssetDatabase,
    id: AssetId,
    kind: AssetSourceKind,
    schemas: &[ComponentSchema],
) -> Result<SceneDocument, String> {
    let path = asset_path(assets, id, kind)?;
    let read = || -> Result<_, Box<dyn Error>> {
        let bytes = source.read_bytes(&AssetPath::new(path.as_str())?)?;
        Ok(SceneDocument::parse_with(
            std::str::from_utf8(&bytes)?,
            schemas,
        )?)
    };
    read().map_err(|error| format!("could not load {path}: {error}"))
}

fn asset_path(
    assets: &ProjectAssetDatabase,
    id: AssetId,
    kind: AssetSourceKind,
) -> Result<&AssetSourcePath, String> {
    let record = assets
        .record_for_stable_id(id.into())
        .ok_or_else(|| format!("could not find {kind} asset {id}"))?;
    if record.source_kind() != &kind {
        return Err(format!(
            "asset {id} at {} is {}, expected {kind}",
            record.path(),
            record.source_kind()
        ));
    }
    Ok(record.path())
}
