use nara::{
    asset::{
        AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath, AssetPathScope,
        AssetProductRef, AssetRef, AssetSourceKind, ImportedProductId, ProjectAssetDatabase,
    },
    asset_build::{ArtifactBuildObservation, ArtifactContentDigest},
    fs::{DirectoryCapability, RelativePath},
    tilemap::{TILESET_FORMAT_KIND, TileSetDescriptor, TileSetSource, TileSetSourceFileLimits},
};
use two_level_game::region_tileset;

pub(super) fn prepare(
    root: &DirectoryCapability,
    scope: &AssetPathScope,
    database: &mut ProjectAssetDatabase,
    observations: &mut Vec<ArtifactBuildObservation>,
) -> Result<TileSetDescriptor, String> {
    let path = AssetPath::new(region_tileset::TILESET_PATH)
        .map_err(|error| format!("TileSet source path: {error}"))?;
    let source = scope.resolve(&path);
    let metadata_path = source.meta_path();
    let metadata = root
        .open_file(&RelativePath::new(&metadata_path).map_err(|error| error.to_string())?)
        .and_then(|file| {
            file.read_to_end_bounded(AssetMetaFileLimits::default().encoded_bytes().get() as u64)
        })
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
        .map_err(|error| format!("{metadata_path}: {error}"))?
        .into_identity();
    if identity.path() != &path
        || identity.source_kind() != &AssetSourceKind::Other(TILESET_FORMAT_KIND.into())
        || identity.stable_id() != region_tileset::tileset_product().source()
    {
        return Err(format!(
            "{metadata_path}: expected the game's shared region TileSet identity and path"
        ));
    }
    database
        .insert_identity(scope, path, identity)
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let limits = TileSetSourceFileLimits::default();
    let bytes = root
        .open_file(&RelativePath::new(source.as_str()).map_err(|error| error.to_string())?)
        .and_then(|file| file.read_to_end_bounded(limits.encoded_bytes().get() as u64))
        .map_err(|error| format!("{source}: {error}"))?;
    let tileset = TileSetSource::decode_ron_bytes(&bytes, limits)
        .map_err(|error| format!("{source}: TileSet decode failed: {error}"))?;
    observations.push(ArtifactBuildObservation::Raw {
        path: source.clone(),
        digest: Some(ArtifactContentDigest::of_bytes(&bytes)),
    });
    let resolver = database.resolver(scope);
    let record = resolver
        .resolve_ref(&tileset.image)
        .map_err(|error| format!("{source}: TileSet Image {:?}: {error}", tileset.image))?;
    if record.source_kind() != &AssetSourceKind::Image {
        return Err(format!(
            "{source}: TileSet dependency must resolve to an Image"
        ));
    }
    let image = match tileset.image {
        AssetRef::Product(product) => product,
        AssetRef::Path(_) | AssetRef::StableId(_) => {
            AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| format!("{source}: TileSet Image Product: {error}"))?
        }
    };
    TileSetDescriptor::new(region_tileset::tileset_product(), image, tileset.atlas)
        .and_then(|descriptor| {
            descriptor
                .with_sampler(tileset.sampler)
                .with_alpha_mode(tileset.alpha_mode)
                .with_tint(tileset.tint)
        })
        .map_err(|error| format!("{source}: TileSet descriptor: {error}"))
}
