use std::{
    collections::{HashMap, hash_map::Entry},
    io::{self, Read as _},
    path::PathBuf,
};

use nara_asset::{
    AssetRecord, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
    image::{ImageAsset, ImageColorSpace, ImageContentHash, decode_png_source},
};
use nara_core::{
    AssetId,
    vfs::{AssetPath, DirAssetSource},
};
use nara_render::material::{Material2dBinding, SpriteMaterialAsset};
use nara_scene::authoring::{SceneDocument, SpriteSource};

use crate::{ViewportError, scene::typed};

const MAX_PNG_READ: u64 = 16 * 1024 * 1024 + 1;
const MAX_MATERIAL_READ: u64 = 64 * 1024 + 1;
const MAX_SHADER_READ: u64 = 1024 * 1024 + 1;

/// Immutable preview inputs resolved against one project index and prepared without a GPU.
#[derive(Clone, Default)]
pub struct PreviewAssets {
    pub(crate) root: Option<PathBuf>,
    pub(crate) images: HashMap<AssetId, ImageAsset>,
    pub(crate) materials: HashMap<AssetId, PreparedMaterial>,
}

#[derive(Clone)]
pub(crate) struct PreparedMaterial {
    source: SpriteMaterialAsset,
    pub(crate) binding: Material2dBinding,
}

impl PreviewAssets {
    /// Reads only the sources referenced by the expanded preview. Compatible unchanged material
    /// inputs reuse their validated generation across reloads.
    pub fn load(
        document: &SceneDocument,
        source: &DirAssetSource,
        database: &ProjectAssetDatabase,
        previous: &Self,
    ) -> Result<Self, ViewportError> {
        let mut assets = Self {
            root: Some(source.root().to_path_buf()),
            ..Self::default()
        };
        let same_root = previous.root == assets.root;
        for row in &document.entities {
            let Some(sprite) = typed::<SpriteSource>(row)? else {
                continue;
            };
            if let Some(id) = sprite.image
                && let Entry::Vacant(entry) = assets.images.entry(id)
            {
                let record = image_record(database, id)?;
                let image = read_png(source, record)?;
                let image = previous
                    .images
                    .get(&id)
                    .filter(|cached| same_root && cached.content_hash() == image.content_hash())
                    .cloned()
                    .unwrap_or(image);
                entry.insert(image);
            }
            if let Some(id) = sprite.material
                && let Entry::Vacant(entry) = assets.materials.entry(id)
            {
                let path = material_path(database, id, "nara.material")?;
                let text = read_text(source, path, MAX_MATERIAL_READ)?;
                let material = SpriteMaterialAsset::from_ron(&text).map_err(|source| {
                    ViewportError::Material {
                        path: path.to_string(),
                        source,
                    }
                })?;
                let shader_path = material_path(database, material.shader(), "nara.shader")?;
                let shader = read_text(source, shader_path, MAX_SHADER_READ)?;
                let binding = if let Some(cached) = previous.materials.get(&id).filter(|cached| {
                    same_root
                        && cached.source == material
                        && cached.binding.generation().shader().user_source() == shader
                }) {
                    cached.binding.clone()
                } else {
                    material
                        .bind(&shader)
                        .map_err(|source| ViewportError::Material {
                            path: shader_path.to_string(),
                            source,
                        })?
                };
                entry.insert(PreparedMaterial {
                    source: material,
                    binding,
                });
            }
        }
        Ok(assets)
    }

    pub fn image_hash(&self, id: AssetId) -> Option<ImageContentHash> {
        Some(self.images.get(&id)?.content_hash())
    }

    pub fn material(&self, id: AssetId) -> Option<&Material2dBinding> {
        Some(&self.materials.get(&id)?.binding)
    }

    /// Compares rendered inputs. A source move with identical content does not require a draw.
    pub fn same_content(&self, other: &Self) -> bool {
        self.root == other.root
            && self.images.len() == other.images.len()
            && self.materials.len() == other.materials.len()
            && self
                .images
                .iter()
                .all(|(&id, asset)| other.image_hash(id) == Some(asset.content_hash()))
            && self
                .materials
                .iter()
                .all(|(&id, asset)| other.material(id) == Some(&asset.binding))
    }
}

fn image_record(
    database: &ProjectAssetDatabase,
    id: AssetId,
) -> Result<&AssetRecord, ViewportError> {
    let record = database
        .record_for_stable_id(id.into())
        .ok_or(ViewportError::MissingImageAsset { id })?;
    if record.source_kind() != &AssetSourceKind::Image {
        return Err(ViewportError::InvalidImageAssetKind {
            id,
            path: record.path().to_string(),
            kind: record.source_kind().clone(),
        });
    }
    Ok(record)
}

fn material_path<'a>(
    database: &'a ProjectAssetDatabase,
    id: AssetId,
    importer: &'static str,
) -> Result<&'a AssetSourcePath, ViewportError> {
    database.record_for_stable_id(id.into())
        .filter(|record| matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == importer))
        .map(AssetRecord::path)
        .ok_or(ViewportError::MaterialSource { id, importer })
}

fn read_png(source: &DirAssetSource, record: &AssetRecord) -> Result<ImageAsset, ViewportError> {
    let path = record.path();
    let bytes =
        read_bytes(source, path, MAX_PNG_READ).map_err(|source| ViewportError::ReadImage {
            path: path.to_string(),
            source,
        })?;
    let decoded = match record.meta() {
        Some(meta) => decode_png_source(&bytes, meta),
        None => ImageAsset::from_png(&bytes, ImageColorSpace::Srgb).map_err(Into::into),
    };
    decoded.map_err(|source| ViewportError::DecodeImage {
        path: path.to_string(),
        source,
    })
}

fn read_text(
    source: &DirAssetSource,
    path: &AssetSourcePath,
    limit: u64,
) -> Result<String, ViewportError> {
    read_bytes(source, path, limit)
        .and_then(|bytes| {
            if bytes.len() as u64 == limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source exceeds its size limit",
                ));
            }
            String::from_utf8(bytes).map_err(io::Error::other)
        })
        .map_err(|source| ViewportError::ReadMaterial {
            path: path.to_string(),
            source,
        })
}

fn read_bytes(source: &DirAssetSource, path: &AssetSourcePath, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    source
        .open(&AssetPath::new(path.as_str())?)?
        .take(limit)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
