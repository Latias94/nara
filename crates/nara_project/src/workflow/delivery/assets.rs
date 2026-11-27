use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read as _, Seek, Write},
    path::{Path, PathBuf},
};

use nara_asset::{
    AssetMeta, AssetRecord, AssetScanMode, AssetSourcePath, ProjectAssetDatabase,
    cook::{IMAGE_IMPORTER_VERSION, ImportCache, ImportCacheError, ImportCacheKey, cook_png},
    cooked::{
        MAX_NPAK_ENTRY_BYTES, MAX_NPAK_LZ4_BYTES, NpakCompression, NpakEntryKind, NpakError,
        NpakInput, write_npak,
    },
    source::ProjectAssetSource,
};
use nara_core::{AssetId, Vec4, vfs::DirAssetSource};
use nara_reflect::ComponentSchemaCatalog;
use nara_render::material::SpriteMaterialAsset;
use nara_scene::{
    authoring::{SceneDocument, SpriteSource, builtin_component_schemas, prefab::expand_prefabs},
    cook::{CookSceneError, cook_flattened_scene},
    cooked::CookedScene,
};

use super::{DeliveryError, DeliveryReport, check_stop};

const SCENE: &str = "nara.scene";
const PREFAB: &str = "nara.prefab";
const IMAGE: &str = "nara.image";
const MATERIAL: &str = "nara.material";
const SHADER: &str = "nara.shader";
const RAW: &str = "nara.raw";

const DOCUMENT_LIMIT: usize = 64 * 1024 * 1024;
const PNG_LIMIT: usize = 16 * 1024 * 1024;
const SHADER_LIMIT: usize = 1024 * 1024;
const MATERIAL_LIMIT: usize = 64 * 1024;

pub(super) struct Cooker {
    source: DirAssetSource,
    database: ProjectAssetDatabase,
    project_schemas: ComponentSchemaCatalog,
    catalog: ComponentSchemaCatalog,
    cache: ImportCache,
    platform: String,
    cache_hits: usize,
    cache_misses: usize,
}

impl Cooker {
    pub(super) fn open(
        project: &Path,
        asset_root: &str,
        project_schemas: ComponentSchemaCatalog,
    ) -> Result<Self, DeliveryError> {
        let root = AssetSourcePath::new(asset_root)
            .map_err(|_| super::ProjectCheckError::AssetDirectory(asset_root.to_owned()))?;
        let source = DirAssetSource::new(project)?;
        let database = ProjectAssetDatabase::scan(source.root(), &[root], AssetScanMode::Discover)?;
        let builtin = builtin_component_schemas()?;
        let custom = project_schemas
            .components()
            .iter()
            .filter(|schema| builtin.iter().all(|internal| internal.id() != schema.id()));
        let catalog =
            ComponentSchemaCatalog::from_components(builtin.iter().chain(custom).cloned())?;
        let cache = ImportCache::new(source.root())?;
        Ok(Self {
            source,
            database,
            project_schemas,
            catalog,
            cache,
            platform: host_platform(),
            cache_hits: 0,
            cache_misses: 0,
        })
    }

    pub(super) fn cook(
        &mut self,
        scenes: &[AssetId],
        stop: &mut impl FnMut() -> bool,
    ) -> Result<Pack, DeliveryError> {
        let mut pending: BTreeSet<_> = scenes.iter().copied().collect();
        for id in &pending {
            self.require_importer(*id, SCENE)?;
        }
        let mut pack = BTreeMap::new();
        while let Some(id) = pending.pop_first() {
            check_stop(stop)?;
            let (payload, dependencies, hit) = self.cook_asset(id)?;
            if hit {
                self.cache_hits += 1;
            } else {
                self.cache_misses += 1;
            }
            pack.insert(id, payload);
            pending.extend(dependencies.into_iter().filter(|id| !pack.contains_key(id)));
        }
        Ok(Pack(pack))
    }

    pub(super) fn report(
        &self,
        archive: PathBuf,
        executable: Option<PathBuf>,
        bytes: u64,
        assets: usize,
    ) -> DeliveryReport {
        DeliveryReport {
            archive,
            executable,
            assets,
            bytes,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
        }
    }

    fn cook_asset(&self, id: AssetId) -> Result<(Payload, Vec<AssetId>, bool), DeliveryError> {
        let asset = self.asset(id)?;
        let (kind, bytes, dependencies, hit) = match asset.meta.importer() {
            SCENE => {
                let (bytes, dependencies, hit) = self.cook_scene(asset)?;
                (NpakEntryKind::Scene, bytes, dependencies, hit)
            }
            IMAGE => {
                let source = self.read(asset, PNG_LIMIT)?;
                let (bytes, hit) =
                    self.cached(asset, source, IMAGE_IMPORTER_VERSION, |source| {
                        cook_png(&source, asset.meta).map_err(|error| asset.error(error))
                    })?;
                (NpakEntryKind::Image, bytes, Vec::new(), hit)
            }
            MATERIAL => {
                let source = self.read(asset, MATERIAL_LIMIT)?;
                let (bytes, hit) =
                    self.cached(asset, source, "nara.material.nsmt.v1", |source| {
                        let material = SpriteMaterialAsset::from_ron(asset.utf8(&source)?)
                            .map_err(|error| asset.error(error))?;
                        Ok(material.to_cooked().to_vec())
                    })?;
                let material =
                    SpriteMaterialAsset::from_cooked(&bytes).map_err(|error| asset.error(error))?;
                self.require_importer(material.shader(), SHADER)?;
                (NpakEntryKind::Material, bytes, vec![material.shader()], hit)
            }
            SHADER => {
                let source = self.read(asset, SHADER_LIMIT)?;
                let (bytes, hit) =
                    self.cached(asset, source, "nara.shader.sprite-wgsl.v1", |source| {
                        let shader = asset.utf8(&source)?;
                        SpriteMaterialAsset::new(id, Vec4::ONE)
                            .and_then(|material| material.bind(shader))
                            .map_err(|error| asset.error(error))?;
                        Ok(source)
                    })?;
                (NpakEntryKind::Shader, bytes, Vec::new(), hit)
            }
            RAW => {
                let path = asset.record.path().as_str().to_ascii_lowercase();
                if [".ron", ".meta", ".png"]
                    .iter()
                    .any(|suffix| path.ends_with(suffix))
                {
                    return Err(asset.error("authoring files require a cooked importer"));
                }
                let source = self.read(asset, MAX_NPAK_ENTRY_BYTES)?;
                let (bytes, hit) = self.cached(asset, source, "nara.raw.v1", Ok)?;
                (NpakEntryKind::Raw, bytes, Vec::new(), hit)
            }
            PREFAB => return Err(asset.error("prefabs are expanded into scenes during Cook")),
            importer => return Err(asset.error(format!("no cooker for importer '{importer}'"))),
        };
        Ok((Payload { kind, bytes }, dependencies, hit))
    }

    fn cook_scene(
        &self,
        asset: Source<'_>,
    ) -> Result<(Vec<u8>, Vec<AssetId>, bool), DeliveryError> {
        let source = self.read(asset, DOCUMENT_LIMIT)?;
        let scene = self.parse_document(asset, &source)?;
        let mut prefab_inputs = BTreeMap::new();
        let expanded = expand_prefabs(&scene, &self.catalog, |id| {
            let prefab = self.require_importer(id, PREFAB)?;
            let source = self.read(prefab, DOCUMENT_LIMIT)?;
            let mut fingerprint = blake3::Hasher::new();
            fingerprint.update(blake3::hash(&source).as_bytes());
            fingerprint.update(blake3::hash(prefab.meta.as_ron().as_bytes()).as_bytes());
            prefab_inputs.insert(id, fingerprint.finalize());
            self.parse_document(prefab, &source)
        })?;
        let mut fingerprint = blake3::Hasher::new();
        fingerprint.update(self.catalog.fingerprint().as_bytes());
        for (id, hash) in prefab_inputs {
            fingerprint.update(id.as_uuid().as_bytes());
            fingerprint.update(hash.as_bytes());
        }
        let version = format!("nara.scene.nscn.v1.{}", fingerprint.finalize().to_hex());
        let (bytes, hit) = self.cached(asset, source, &version, |_| {
            cook_flattened_scene(&expanded.document, &self.catalog)
                .map(|scene| scene.bytes)
                .map_err(|error| match error {
                    CookSceneError::Opaque {
                        component, version, ..
                    } => DeliveryError::MissingComponentSchema {
                        id: asset.id,
                        path: asset.record.path().to_string(),
                        component: component.to_string(),
                        version: version.0,
                    },
                    error => asset.error(error),
                })
        })?;
        let dependencies = CookedScene::parse(&bytes)
            .map_err(|error| asset.error(error))?
            .asset_dependencies()
            .to_vec();
        for row in &expanded.document.entities {
            if let Some(sprite) = row
                .typed::<SpriteSource>()
                .map_err(|error| asset.error(error))?
            {
                if let Some(image) = sprite.image {
                    self.require_importer(image, IMAGE)?;
                }
                if let Some(material) = sprite.material {
                    self.require_importer(material, MATERIAL)?;
                }
            }
        }
        Ok((bytes, dependencies, hit))
    }

    fn asset(&self, id: AssetId) -> Result<Source<'_>, DeliveryError> {
        let record = self
            .database
            .record_for_stable_id(id.into())
            .ok_or(DeliveryError::MissingAsset(id))?;
        let meta = record.meta().ok_or(DeliveryError::MissingAsset(id))?;
        Ok(Source { id, record, meta })
    }

    fn require_importer(
        &self,
        id: AssetId,
        expected: &'static str,
    ) -> Result<Source<'_>, DeliveryError> {
        let source = self.asset(id)?;
        if source.meta.importer() != expected {
            return Err(DeliveryError::AssetImporter {
                id,
                path: source.record.path().to_string(),
                expected,
                importer: source.meta.importer().to_owned(),
            });
        }
        Ok(source)
    }

    fn read(&self, source: Source<'_>, limit: usize) -> Result<Vec<u8>, DeliveryError> {
        let read = || -> io::Result<Vec<u8>> {
            let file =
                ProjectAssetSource::new(&self.source, &self.database).open_asset(source.id)?;
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    format!("source exceeds {limit} bytes"),
                ));
            }
            Ok(bytes)
        };
        read().map_err(|error| source.error(error))
    }

    fn parse_document(
        &self,
        asset: Source<'_>,
        bytes: &[u8],
    ) -> Result<SceneDocument, DeliveryError> {
        SceneDocument::parse_with(asset.utf8(bytes)?, self.project_schemas.components())
            .map_err(|error| asset.error(error))
    }

    fn cached(
        &self,
        asset: Source<'_>,
        source: Vec<u8>,
        version: &str,
        produce: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, DeliveryError>,
    ) -> Result<(Vec<u8>, bool), DeliveryError> {
        let key = ImportCacheKey::new(
            &source,
            asset.meta.as_ron().as_bytes(),
            version,
            &self.platform,
        );
        let mut miss = false;
        let bytes = self
            .cache
            .get_or_insert_with(key, || {
                miss = true;
                produce(source)
            })
            .map_err(|error| match error {
                ImportCacheError::Io(error) => asset.error(error),
                ImportCacheError::Produce(error) => error,
            })?;
        Ok((bytes, !miss))
    }
}

#[derive(Clone, Copy)]
struct Source<'a> {
    id: AssetId,
    record: &'a AssetRecord,
    meta: &'a AssetMeta,
}

impl Source<'_> {
    fn utf8<'a>(self, bytes: &'a [u8]) -> Result<&'a str, DeliveryError> {
        std::str::from_utf8(bytes).map_err(|error| self.error(error))
    }

    fn error(self, message: impl std::fmt::Display) -> DeliveryError {
        DeliveryError::Asset {
            id: self.id,
            path: self.record.path().to_string(),
            message: message.to_string(),
        }
    }
}

struct Payload {
    kind: NpakEntryKind,
    bytes: Vec<u8>,
}

pub(super) struct Pack(BTreeMap<AssetId, Payload>);

impl Pack {
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn write(&self, output: &mut (impl Seek + Write)) -> Result<(), NpakError> {
        let entries = self
            .0
            .iter()
            .map(|(&id, payload)| {
                let compression = if (1_024..=MAX_NPAK_LZ4_BYTES).contains(&payload.bytes.len()) {
                    NpakCompression::Lz4
                } else {
                    NpakCompression::None
                };
                NpakInput::new(id, payload.kind, &payload.bytes, compression)
            })
            .collect::<Vec<_>>();
        write_npak(output, &entries)
    }
}

pub(super) fn host_platform() -> String {
    let environment = if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "gnu") {
        "gnu"
    } else {
        "native"
    };
    format!(
        "{}-{}-{environment}",
        std::env::consts::ARCH,
        std::env::consts::OS
    )
}
