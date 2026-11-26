use std::{
    collections::{HashMap, hash_map::Entry},
    error::Error,
    path::Path,
};

use game_components::Spin;
use nara::{
    asset::{AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase, add_asset},
    core::{
        AssetId, EntityUuid,
        vfs::{AssetPath, AssetSource, DirAssetSource},
    },
    prelude::{
        App, Camera2d, ChildOf, Color, DefaultPlugins, Name, Query, RegisterSchema, Res, Schema,
        Sprite, Transform2d, Update, VirtualTime, Visibility, World,
    },
    reflect::{ComponentSchema, ComponentSchemaCatalog},
    render::image::{ImageAsset, ImageColorSpace},
    scene::authoring::{
        SceneDocument, SceneEntity, SpriteSource, builtin_component_schemas, prefab::expand_prefabs,
    },
};

const ASSET_DIRECTORY: &str = "assets";
const ENTRY_SCENE: &str = "01a219c4-2965-7784-9173-9c9863ec5284";

type GameResult<T> = Result<T, Box<dyn Error>>;

fn main() -> GameResult<()> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(env!("CARGO_PKG_NAME")))?;
    app.register_schema::<Spin>()?;
    load_scene(app.world_mut()?, Path::new(env!("CARGO_MANIFEST_DIR")))?;
    app.add_systems(Update, spin)?;
    app.run()?;
    Ok(())
}

fn load_scene(world: &mut World, project_root: &Path) -> GameResult<()> {
    let assets = AuthoredAssets {
        files: DirAssetSource::new(project_root)?,
        database: ProjectAssetDatabase::scan(
            project_root,
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
    let scene = assets.document(ENTRY_SCENE.parse()?, AssetSourceKind::Scene, &schemas)?;
    let scene = expand_prefabs(&scene, &catalog, |id| {
        assets.document(id, AssetSourceKind::Prefab, &schemas)
    })?
    .document;
    let rows = scene
        .entities
        .into_iter()
        .map(AuthoredEntity::decode)
        .collect::<GameResult<Vec<_>>>()?;
    let mut images = HashMap::new();
    for id in rows.iter().filter_map(|row| row.sprite.as_ref()?.image) {
        if let Entry::Vacant(slot) = images.entry(id) {
            let image = assets.read(id, AssetSourceKind::Image, |bytes| {
                Ok(ImageAsset::from_png(bytes, ImageColorSpace::Srgb)?)
            })?;
            slot.insert(image);
        }
    }
    let handles = images
        .into_iter()
        .map(|(id, image)| Ok((id, add_asset(world, image)?)))
        .collect::<GameResult<HashMap<_, _>>>()?;

    world.spawn((
        Camera2d {
            clear_color: Some(Color::rgb(0.04, 0.06, 0.1)),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    let mut entities = HashMap::with_capacity(rows.len());
    let mut children = Vec::new();
    for row in rows {
        let mut spawned = world.spawn((Name::new(row.name), row.transform, row.visibility));
        entities.insert(row.id, spawned.id());
        if let Some(parent) = row.parent {
            children.push((spawned.id(), parent));
        }
        if let Some(source) = row.sprite {
            let sprite = match source.image {
                Some(id) => {
                    Sprite::from_texture(handles[&id], source.size).with_tint(source.tint())
                }
                None => Sprite::from_color(source.size, source.tint()),
            };
            spawned.insert(sprite.with_layer(source.layer));
        }
        if let Some(spin) = row.spin {
            spawned.insert(spin);
        }
    }
    for (child, parent) in children {
        let parent = entities
            .get(&parent)
            .copied()
            .ok_or_else(|| format!("missing parent entity {parent}"))?;
        world.entity_mut(child).insert(ChildOf(parent));
    }
    Ok(())
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
        self.read(id, kind, |bytes| {
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
        decode: impl FnOnce(&[u8]) -> GameResult<T>,
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
        let bytes = self
            .files
            .read_bytes(&path)
            .map_err(|error| format!("could not read {kind} asset {id} at {path}: {error}"))?;
        decode(&bytes).map_err(|error| {
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

fn spin(time: Res<VirtualTime>, mut sprites: Query<(&mut Transform2d, &Spin)>) {
    for (mut transform, spin) in &mut sprites {
        transform.rotation += spin.radians_per_second * time.delta_seconds();
    }
}
