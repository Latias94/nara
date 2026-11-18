use std::sync::Arc;

use nara::{
    asset::{
        AssetImportSource, AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath,
        AssetPathScope, AssetProductRef, AssetRef, AssetSourceKind, AssetSourcePath,
        ImportedProductId, ProjectAssetDatabase, SourceAssetImporter,
    },
    asset_build::{ArtifactBuildObservation, ArtifactContentDigest},
    core::ByteLimit,
    fs::{DirectoryCapability, RelativePath},
    material::{Material2dAsset, Material2dDomain},
};
use two_level_game::region_material;

pub(super) fn prepare(
    root: &DirectoryCapability,
    scope: &AssetPathScope,
    database: &mut ProjectAssetDatabase,
    observations: &mut Vec<ArtifactBuildObservation>,
) -> Result<Material2dAsset, String> {
    let path = AssetPath::new(region_material::MATERIAL_PATH)
        .map_err(|error| format!("Material source path: {error}"))?;
    let source = scope.resolve(&path);
    let metadata_path = source.meta_path();
    let metadata = root
        .open_file(&RelativePath::new(&metadata_path).map_err(|error| error.to_string())?)
        .and_then(|file| {
            file.read_to_end_bounded(AssetMetaFileLimits::default().encoded_bytes().get() as u64)
        })
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let identity = metadata.identity();
    if identity.path() != &path
        || identity.source_kind() != &AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.into())
        || identity.stable_id() != region_material::material_product().source()
    {
        return Err(format!(
            "{metadata_path}: expected the game's shared region Material identity and path"
        ));
    }
    database
        .insert_identity(scope, path, identity.clone())
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let bytes = read_observed(
        root,
        &source,
        Material2dDomain::limits().source_bytes(),
        observations,
    )?;
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        read_observed(root, &scope.resolve(path), limit, observations)
    };
    let imported =
        Material2dDomain::prepare(&mut AssetImportSource::new(&metadata, bytes, &mut reader))
            .map_err(|error| format!("{source}: Material import failed: {error:?}"))?;
    let resolver = database.resolver(scope);
    imported.resolve_texture_references(|reference| {
        let record = resolver
            .resolve_ref(reference)
            .map_err(|error| format!("{source}: Material texture {reference:?}: {error}"))?;
        if record.source_kind() != &AssetSourceKind::Image {
            return Err(format!(
                "{source}: Material texture must resolve to an Image"
            ));
        }
        match reference {
            AssetRef::Product(product) => Ok(*product),
            AssetRef::Path(_) | AssetRef::StableId(_) => {
                AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                    .map_err(|error| format!("{source}: Material texture Product: {error}"))
            }
        }
    })
}

fn read_observed(
    root: &DirectoryCapability,
    path: &AssetSourcePath,
    limit: ByteLimit,
    observations: &mut Vec<ArtifactBuildObservation>,
) -> Result<Arc<[u8]>, String> {
    let bytes = root
        .open_file(&RelativePath::new(path.as_str()).map_err(|error| error.to_string())?)
        .and_then(|file| file.read_to_end_bounded(limit.get() as u64))
        .map_err(|error| format!("{path}: {error}"))?;
    observations.push(ArtifactBuildObservation::Raw {
        path: path.clone(),
        digest: Some(ArtifactContentDigest::of_bytes(&bytes)),
    });
    Ok(bytes.into())
}
