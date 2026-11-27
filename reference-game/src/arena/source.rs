use std::{
    collections::{BTreeMap, HashMap, btree_map::Entry},
    error::Error,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use nara::{
    core::{AssetId, vfs::DirAssetSource},
    prelude::*,
    reflect::{ComponentSchemaCatalog, component_registry},
    render::material::SpriteMaterialAsset,
    scene::{
        authoring::{
            SceneDocument, SceneEntity, builtin_component_schemas, prefab::expand_prefabs,
        },
        components::{ColliderSource, SpriteSource},
    },
};
use nara_asset::{
    AssetRecord, AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
    image::decode_png_source, source::ProjectAssetSource,
};

use crate::{
    ENTRY_SCENE_ID, ExtractionGate, PatrolDrone, Pilot, PowerCell, RunRules, register_game_schema,
};

use super::{Arena, ArenaPlan, SceneRow};

#[cfg(all(feature = "desktop", feature = "hot-reload"))]
mod snapshot;
#[cfg(all(feature = "desktop", feature = "hot-reload"))]
pub(crate) use snapshot::SceneLayers;

const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MATERIAL_BYTES: u64 = 64 * 1024;
const MAX_SHADER_BYTES: u64 = 1024 * 1024;

pub(crate) struct SourceProject {
    source: DirAssetSource,
    project_schemas: ComponentSchemaCatalog,
    catalog: ComponentSchemaCatalog,
}

pub(crate) struct SourceContent {
    pub scene: SceneDocument,
    #[cfg(feature = "desktop")]
    pub images: BTreeMap<AssetId, ImageAsset>,
    #[cfg(feature = "desktop")]
    pub materials: BTreeMap<AssetId, nara::render::material::Material2dBinding>,
}

pub(crate) fn development_root() -> Result<PathBuf, Box<dyn Error>> {
    let is_project = |root: &Path| root.join("nara.toml").is_file() && root.join("assets").is_dir();
    let root = match std::env::var_os("NARA_PROJECT_ROOT") {
        Some(root) => PathBuf::from(root),
        None => {
            let current = std::env::current_dir()?;
            let reference = current.join("reference-game");
            if !is_project(&current) && is_project(&reference) {
                reference
            } else {
                current
            }
        }
    };
    if !is_project(&root) {
        return Err(format!(
            "{} is not the Night Relay project directory. Run from the project directory or set NARA_PROJECT_ROOT",
            root.display()
        )
        .into());
    }
    Ok(root.canonicalize()?)
}

#[cfg(test)]
pub(crate) fn test_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

impl SourceProject {
    pub fn new(app: &mut App, project_root: &Path) -> Result<Self, Box<dyn Error>> {
        register_game_schema(app)?;
        let project_schemas = component_registry(app.world())
            .ok_or("could not read the Night Relay Schema catalog")?
            .clone();
        let catalog = ComponentSchemaCatalog::from_components(
            builtin_component_schemas()?
                .into_iter()
                .chain(project_schemas.components().iter().cloned()),
        )?;
        Ok(Self {
            source: DirAssetSource::new(project_root)?,
            project_schemas,
            catalog,
        })
    }

    #[cfg(all(feature = "desktop", feature = "hot-reload"))]
    pub fn watch_directory(&self) -> PathBuf {
        self.source.root().join("assets")
    }

    pub fn assets(&self) -> Result<ProjectAssetDatabase, Box<dyn Error>> {
        Ok(ProjectAssetDatabase::scan(
            self.source.root(),
            &[AssetSourcePath::new("assets")?],
            AssetScanMode::ReadOnly,
        )?)
    }

    pub fn load(
        &self,
        assets: &ProjectAssetDatabase,
        snapshot: Option<&Path>,
    ) -> Result<SourceContent, Box<dyn Error>> {
        let scene = match snapshot {
            Some(path) => self.read_snapshot(assets, path)?,
            None => self.read_entry(assets)?,
        };
        self.prepare_content(assets, scene)
    }

    pub fn read_snapshot(
        &self,
        assets: &ProjectAssetDatabase,
        path: &Path,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let read = || -> Result<SceneDocument, Box<dyn Error>> {
            let bytes = read_bounded(File::open(path)?, MAX_DOCUMENT_BYTES)?;
            Ok(SceneDocument::parse_with(
                std::str::from_utf8(&bytes)?,
                self.project_schemas.components(),
            )?)
        };
        let scene =
            read().map_err(|error| format!("could not load {}: {error}", path.display()))?;
        self.expand(assets, &scene)
    }

    pub fn read_entry(
        &self,
        assets: &ProjectAssetDatabase,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let scene = self.read_document(assets, ENTRY_SCENE_ID.parse()?, AssetSourceKind::Scene)?;
        self.expand(assets, &scene)
    }

    pub fn prepare_content(
        &self,
        assets: &ProjectAssetDatabase,
        mut scene: SceneDocument,
    ) -> Result<SourceContent, Box<dyn Error>> {
        scene.entities.sort_unstable_by_key(|row| row.id);
        let mut images = BTreeMap::new();
        let mut materials = BTreeMap::new();
        for source in scene
            .entities
            .iter()
            .filter_map(|row| row.typed::<SpriteSource>().transpose())
        {
            let source = source?;
            if let Some(id) = source.image
                && let Entry::Vacant(entry) = images.entry(id)
            {
                let record = asset_record(assets, id, AssetSourceKind::Image)?;
                let meta = record
                    .meta()
                    .ok_or_else(|| format!("missing metadata for {}", record.path()))?;
                let bytes = self.read_asset(assets, id, record.path(), MAX_IMAGE_BYTES)?;
                entry.insert(
                    decode_png_source(&bytes, meta)
                        .map_err(|error| format!("could not load {}: {error}", record.path()))?,
                );
            }
            if let Some(id) = source.material
                && let Entry::Vacant(entry) = materials.entry(id)
            {
                let path =
                    asset_record(assets, id, AssetSourceKind::Other("nara.material".into()))?
                        .path();
                let bytes = self.read_asset(assets, id, path, MAX_MATERIAL_BYTES)?;
                let material = SpriteMaterialAsset::from_ron(std::str::from_utf8(&bytes)?)
                    .map_err(|error| format!("could not load {path}: {error}"))?;
                let shader = asset_record(
                    assets,
                    material.shader(),
                    AssetSourceKind::Other("nara.shader".into()),
                )?
                .path();
                let bytes = self.read_asset(assets, material.shader(), shader, MAX_SHADER_BYTES)?;
                entry.insert(
                    material
                        .bind(std::str::from_utf8(&bytes)?)
                        .map_err(|error| format!("could not bind {path} with {shader}: {error}"))?,
                );
            }
        }
        Ok(SourceContent {
            scene,
            #[cfg(feature = "desktop")]
            images,
            #[cfg(feature = "desktop")]
            materials,
        })
    }

    fn expand(
        &self,
        assets: &ProjectAssetDatabase,
        scene: &SceneDocument,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        Ok(expand_prefabs(scene, &self.catalog, |id| {
            self.read_document(assets, id, AssetSourceKind::Prefab)
        })?
        .document)
    }

    fn read_document(
        &self,
        assets: &ProjectAssetDatabase,
        id: AssetId,
        kind: AssetSourceKind,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let path = asset_record(assets, id, kind)?.path();
        let bytes = self.read_asset(assets, id, path, MAX_DOCUMENT_BYTES)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| format!("could not load {path}: {error}"))?;
        Ok(
            SceneDocument::parse_with(text, self.project_schemas.components())
                .map_err(|error| format!("could not load {path}: {error}"))?,
        )
    }

    fn read_asset(
        &self,
        assets: &ProjectAssetDatabase,
        id: AssetId,
        path: &AssetSourcePath,
        limit: u64,
    ) -> Result<Vec<u8>, String> {
        let project = ProjectAssetSource::new(&self.source, assets);
        project
            .open_asset(id)
            .and_then(|file| read_bounded(file, limit))
            .map_err(|error| format!("could not load {path}: {error}"))
    }
}

fn read_bounded(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("source exceeds {limit} bytes"),
        ));
    }
    Ok(bytes)
}

fn asset_record(
    assets: &ProjectAssetDatabase,
    id: AssetId,
    kind: AssetSourceKind,
) -> Result<&AssetRecord, String> {
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
    Ok(record)
}

pub(crate) struct SourceScene<'a> {
    document: &'a SceneDocument,
    plan: ArenaPlan,
    sprites: Vec<Option<SpriteSource>>,
}

impl<'a> SourceScene<'a> {
    pub fn prepare(
        document: &'a SceneDocument,
        rules_override: Option<RunRules>,
    ) -> Result<Self, String> {
        document.validate().map_err(|error| error.to_string())?;
        let plan = ArenaPlan::prepare(document.entities.iter().map(read_row), rules_override)?;
        let sprites = document
            .entities
            .iter()
            .map(component)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            document,
            plan,
            sprites,
        })
    }

    #[cfg(all(feature = "desktop", feature = "hot-reload"))]
    pub fn arena(&self) -> &Arena {
        &self.plan.arena
    }

    pub fn spawn(self, world: &mut World) -> (Arena, RunRules, Vec<Entity>) {
        let mut ids = HashMap::with_capacity(self.document.entities.len());
        let mut entities = Vec::with_capacity(self.document.entities.len());
        for (row, sprite) in self.document.entities.iter().zip(self.sprites) {
            let mut entity = world.spawn(Name::new(row.name.as_str()));
            if let Some(sprite) = sprite {
                entity.insert(sprite);
            }
            let entity = entity.id();
            ids.insert(row.id, entity);
            entities.push(entity);
        }
        for (row, &entity) in self.document.entities.iter().zip(&entities) {
            if let Some(parent) = row.parent {
                world.entity_mut(entity).insert(ChildOf(ids[&parent]));
            }
        }
        let (arena, rules) = self.plan.bind(world, &entities);
        (arena, rules, entities)
    }
}

fn read_row(source: &SceneEntity) -> Result<SceneRow<'_>, String> {
    if source.prefab.is_some() {
        return Err(format!("{} is an unresolved prefab instance", source.name));
    }
    Ok(SceneRow {
        name: &source.name,
        is_child: source.parent.is_some(),
        pose: component::<Transform2d>(source)?,
        visibility: component::<Visibility>(source)?,
        collider: component::<ColliderSource>(source)?,
        pilot: component::<Pilot>(source)?.is_some(),
        cell_order: component::<PowerCell>(source)?.map(|cell| cell.order),
        drone_order: component::<PatrolDrone>(source)?.map(|drone| drone.order),
        gate: component::<ExtractionGate>(source)?.is_some(),
        rules: component::<RunRules>(source)?,
    })
}

fn component<T: Schema>(source: &SceneEntity) -> Result<Option<T>, String> {
    source.typed::<T>().map_err(|error| {
        format!(
            "{} ({}) {}: {error}",
            source.name,
            source.id,
            T::COMPONENT_ID
        )
    })
}
