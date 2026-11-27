use std::collections::HashMap;

use game_components::Spin;
use nara::{
    asset::{
        AssetMeta, AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
        image::decode_png_source,
    },
    core::{
        AssetId, EntityUuid,
        vfs::{AssetPath, AssetSource, DirAssetSource},
    },
    prelude::{ChildOf, Name, Schema, Transform2d, Visibility, World},
    reflect::{ComponentSchema, ComponentSchemaCatalog},
    scene::{
        authoring::{SceneDocument, SceneEntity, prefab::expand_prefabs},
        components::{SpriteSource, builtin_component_schemas},
    },
};

use super::{GameResult, bind_sprites};

const ASSET_DIRECTORY: &str = "assets";

pub(super) fn load_scene(world: &mut World, entry: AssetId) -> GameResult<()> {
    let root = std::env::current_dir()?;
    if !root.join("nara.toml").is_file() {
        return Err(format!(
            "run the development game from its project directory (current directory: {})",
            root.display()
        )
        .into());
    }
    let assets = AuthoredAssets {
        files: DirAssetSource::new(&root)?,
        database: ProjectAssetDatabase::scan(
            &root,
            &[AssetSourcePath::new(ASSET_DIRECTORY)?],
            AssetScanMode::ReadOnly,
        )?,
    };
    let schemas = [Spin::type_schema()?];
    let catalog = ComponentSchemaCatalog::from_components(
        builtin_component_schemas()?
            .into_iter()
            .chain(schemas.iter().cloned()),
    )?;
    let scene = assets.document(entry, AssetSourceKind::Scene, &schemas)?;
    let scene = expand_prefabs(&scene, &catalog, |id| {
        assets.document(id, AssetSourceKind::Prefab, &schemas)
    })?
    .document;
    let rows = scene
        .entities
        .into_iter()
        .map(AuthoredEntity::decode)
        .collect::<GameResult<Vec<_>>>()?;

    let mut entities = HashMap::with_capacity(rows.len());
    let mut hierarchy = Vec::with_capacity(rows.len());
    for row in rows {
        let mut spawned = world.spawn((Name::new(row.name), row.transform, row.visibility));
        if let Some(sprite) = row.sprite {
            spawned.insert(sprite);
        }
        if let Some(spin) = row.spin {
            spawned.insert(spin);
        }
        let entity = spawned.id();
        entities.insert(row.id, entity);
        hierarchy.push((entity, row.parent));
    }
    for &(child, parent) in &hierarchy {
        if let Some(parent) = parent {
            let parent = entities
                .get(&parent)
                .copied()
                .ok_or_else(|| format!("missing parent entity {parent}"))?;
            world.entity_mut(child).insert(ChildOf(parent));
        }
    }
    bind_sprites(
        world,
        hierarchy.into_iter().map(|(entity, _)| entity),
        |id| {
            assets.read(id, AssetSourceKind::Image, |bytes, meta| {
                Ok(decode_png_source(bytes, meta)?)
            })
        },
    )
}

struct AuthoredAssets {
    files: DirAssetSource,
    database: ProjectAssetDatabase,
}

impl AuthoredAssets {
    fn document(
        &self,
        id: AssetId,
        kind: AssetSourceKind,
        schemas: &[ComponentSchema],
    ) -> GameResult<SceneDocument> {
        self.read(id, kind, |bytes, _| {
            Ok(SceneDocument::parse_with(
                std::str::from_utf8(bytes)?,
                schemas,
            )?)
        })
    }

    fn read<T>(
        &self,
        id: AssetId,
        kind: AssetSourceKind,
        decode: impl FnOnce(&[u8], &AssetMeta) -> GameResult<T>,
    ) -> GameResult<T> {
        let record = self
            .database
            .record_for_stable_id(id.into())
            .ok_or_else(|| format!("missing {kind} asset {id}"))?;
        let path = AssetPath::new(record.path().as_str())?;
        if record.source_kind() != &kind {
            return Err(format!(
                "expected {kind} asset {id} at {path}, found {}",
                record.source_kind()
            )
            .into());
        }
        let meta = record
            .meta()
            .ok_or_else(|| format!("missing sidecar for {kind} asset {id} at {path}"))?;
        let bytes = self
            .files
            .read_bytes(&path)
            .map_err(|error| format!("could not read {kind} asset {id} at {path}: {error}"))?;
        decode(&bytes, meta).map_err(|error| {
            format!("could not decode {kind} asset {id} at {path}: {error}").into()
        })
    }
}

struct AuthoredEntity {
    id: EntityUuid,
    name: String,
    parent: Option<EntityUuid>,
    transform: Transform2d,
    visibility: Visibility,
    sprite: Option<SpriteSource>,
    spin: Option<Spin>,
}

impl AuthoredEntity {
    fn decode(entity: SceneEntity) -> GameResult<Self> {
        let mut row = Self {
            id: entity.id,
            name: entity.name,
            parent: entity.parent,
            transform: Transform2d::default(),
            visibility: Visibility::default(),
            sprite: None,
            spin: None,
        };
        for (kind, component) in entity.components {
            let result = match kind.as_str() {
                Transform2d::COMPONENT_ID => component.typed().map(|value| row.transform = value),
                Visibility::COMPONENT_ID => component.typed().map(|value| row.visibility = value),
                SpriteSource::COMPONENT_ID => {
                    component.typed().map(|value| row.sprite = Some(value))
                }
                Spin::COMPONENT_ID => component.typed().map(|value| row.spin = Some(value)),
                _ => {
                    return Err(
                        format!("entity {} uses unsupported component {kind}", row.id).into(),
                    );
                }
            };
            result.map_err(|error| format!("entity {} component {kind}: {error}", row.id))?;
        }
        Ok(row)
    }
}
