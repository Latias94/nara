use nara::{
    asset::{
        AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetSourceKind, AssetSourcePath,
        ProjectAssetDatabase, RuntimeAssetCodec,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest,
        ArtifactSourceRequest, build_image_import,
    },
    core::FormatVersion,
    fs::{DirectoryCapability, RelativePath},
    image::ImageRuntimeCodec,
    project::{
        EffectiveProjectSettings, NativeDocumentIdentity, ProjectAuthoringState, ProjectManifest,
    },
    project_host::resolve_runtime_configuration,
    project_tooling::host_fs,
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentLimits, CookedContentRequest, PreparedRuntimeContent,
    },
    runtime_content::{RuntimeContentMemberKind, RuntimeContentTarget},
    scene::{
        SceneCompilationOptions, SceneDocumentCandidate, SourceDocumentKey, SourceDocumentKind,
    },
};
use two_level_game::{cooked_recipe, levels::LevelId};

const DOCUMENT_BYTES: u64 = 1024 * 1024;
const METADATA_BYTES: u64 = 64 * 1024;
const COOKER_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn prepare(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
) -> Result<PreparedRuntimeContent, String> {
    let settings = project_settings(root)?;
    let configuration = settings
        .runtime_configuration()
        .map_err(|error| format!("nara.toml runtime configuration: {error}"))?;
    let recipe = cooked_recipe();
    let plan = resolve_runtime_configuration(&configuration, recipe.clone())
        .map_err(|error| format!("cooked game composition: {error}"))?;
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            target: RuntimeContentTarget::new(triple, "two-level-shipping")
                .map_err(|error| format!("cooked content target: {error}"))?,
            configuration: configuration.to_configuration(),
            startup_scene: LevelId::STARTUP.scene_product(),
            scenes: LevelId::ALL
                .into_iter()
                .map(LevelId::scene_product)
                .collect(),
            addressable_roots: Vec::new(),
        },
        recipe,
        CookedContentLimits::default(),
    )
    .map_err(|error| format!("cooked content assembly: {error}"))?;

    let scope = AssetPathScope::under(
        AssetSourcePath::new(settings.paths.assets.as_str())
            .map_err(|error| format!("assets root: {error}"))?,
    );
    let definition = image_import_definition()?;
    let assets = root
        .open_directory(
            &RelativePath::new(settings.paths.assets.as_str())
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("opening the original assets root: {error}"))?;
    let store = ArtifactBuildStore::open(cache.share(), ArtifactBuildLimits::default())
        .map_err(|error| format!("Image import cache: {error}"))?;
    let mut database = ProjectAssetDatabase::default();
    for level in LevelId::ALL {
        let path = AssetPath::new(match level {
            LevelId::Meadow => "textures/meadow.png",
            LevelId::Cavern => "textures/cavern.png",
        })
        .map_err(|error| format!("Image source path: {error}"))?;
        let source = scope.resolve(&path);
        let metadata_path = source.meta_path();
        let metadata = read(root, &metadata_path, METADATA_BYTES)?;
        let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
            .map_err(|error| format!("{metadata_path}: {error}"))?
            .into_identity();
        if identity.path() != &path
            || identity.source_kind() != &AssetSourceKind::Image
            || identity.stable_id() != level.image_product().source()
        {
            return Err(format!(
                "{metadata_path}: expected this level's Image identity and path"
            ));
        }
        database
            .insert_identity(&scope, path.clone(), identity)
            .map_err(|error| format!("{metadata_path}: {error}"))?;
        let request =
            ArtifactSourceRequest::new(assets.share(), scope.clone(), path, definition.clone());
        let imported = build_image_import(&store, &request, |_| Ok(()))
            .map_err(|error| format!("{source}: Image import failed: {error}"))?
            .into_group();
        if imported.seed().source != level.image_product()
            || imported.seed().location.as_ref() != Some(&source)
            || imported.members().len() != 1
            || imported.members()[0].products.as_slice() != [level.image_product()]
        {
            return Err(format!(
                "{source}: imported group must supply only this level's Image Product"
            ));
        }
        builder
            .add_group(&imported)
            .map_err(|error| format!("{source}: {error}"))?;
    }

    for level in LevelId::ALL {
        let source = scene_source(&settings, level)?;
        let bytes = read(root, source.as_str(), DOCUMENT_BYTES)?;
        let document = SceneDocumentCandidate::decode_json_bytes(&bytes)
            .map_err(|error| format!("{source}: {error}"))?
            .into_composition_source()
            .map_err(|error| format!("{source}: {error}"))?;
        if document
            .entities
            .iter()
            .any(|entity| entity.prefab.is_some())
        {
            return Err(format!(
                "{source}: Prefab instances are outside this game's cooked inventory"
            ));
        }
        let compilation = plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                &document,
                SceneCompilationOptions::default(),
                Some(database.resolver(&scope)),
                |_, _| Err::<_, String>(format!("{source}: Prefab loading is not supported")),
            )?;
        if compilation.diagnostics.has_errors() {
            return Err(format!(
                "{source}: Scene compilation failed: {:?}",
                compilation.diagnostics
            ));
        }
        let compiled = compilation
            .candidate
            .ok_or_else(|| format!("{source}: Scene compilation produced no candidate"))?
            .bind_source(SourceDocumentKey::project_location(
                settings.project.source_universe,
                source.clone(),
                SourceDocumentKind::Scene,
            ));
        builder
            .add_scene(level.scene_product(), &compiled, database.resolver(&scope))
            .map_err(|error| format!("{source}: Scene cooking failed: {error}"))?;
    }

    let prepared = builder
        .finish()
        .map_err(|error| format!("cooked content closure: {error}"))?;
    let catalog = prepared.base().catalog();
    if catalog.members().len() != 4 {
        return Err("cooked content must contain exactly two Scenes and two Images".to_owned());
    }
    for level in LevelId::ALL {
        let scene = catalog
            .member_for_product(level.scene_product())
            .ok_or_else(|| format!("missing cooked {level:?} Scene"))?;
        let expected_source = SourceDocumentKey::project_location(
            settings.project.source_universe,
            scene_source(&settings, level)?,
            SourceDocumentKind::Scene,
        );
        if scene.products() != [level.scene_product()]
            || !matches!(scene.kind(), RuntimeContentMemberKind::Scene { source } if source == &expected_source)
        {
            return Err(format!("cooked {level:?} Scene identity changed"));
        }
        let image = catalog
            .member_for_product(level.image_product())
            .ok_or_else(|| format!("missing cooked {level:?} Image"))?;
        if image.products() != [level.image_product()]
            || !matches!(image.kind(), RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND && *version == FormatVersion::ONE)
        {
            return Err(format!("cooked {level:?} Image identity or format changed"));
        }
    }
    Ok(prepared)
}

fn project_settings(root: &DirectoryCapability) -> Result<EffectiveProjectSettings, String> {
    let bytes = read(root, "nara.toml", DOCUMENT_BYTES)?;
    let loaded = ProjectManifest::parse_toml_bytes(&bytes);
    if loaded.has_errors() {
        return Err(format!("nara.toml: {:?}", loaded.diagnostics));
    }
    let settings = loaded
        .manifest
        .ok_or_else(|| "nara.toml: project manifest is absent".to_owned())?
        .resolve_profile(None)
        .map_err(|error| format!("nara.toml: {error}"))?;
    if settings.paths.assets.as_str() != "assets"
        || settings.paths.scenes.as_str() != "scenes"
        || settings.startup.default_scene.as_ref() != Some(&LevelId::STARTUP.scene_path())
        || settings.content.scenes.as_slice() != [LevelId::Cavern.scene_path()]
        || settings.project.native_document_identity != NativeDocumentIdentity::ProjectLocation
        || !matches!(settings.authoring_state, ProjectAuthoringState::Ready)
    {
        return Err(
            "nara.toml: expected the ready two-level game's assets/scenes roots, Meadow startup, Cavern content root and project_location identity policy"
                .to_owned(),
        );
    }
    Ok(settings)
}

fn scene_source(
    settings: &EffectiveProjectSettings,
    level: LevelId,
) -> Result<AssetSourcePath, String> {
    AssetSourcePath::new(format!("{}/{}", settings.paths.scenes, level.scene_path()))
        .map_err(|error| format!("Scene source path: {error}"))
}

fn read(root: &DirectoryCapability, path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let relative = RelativePath::new(path).map_err(|error| format!("{path}: {error}"))?;
    root.open_file(&relative)
        .and_then(|file| file.read_to_end_bounded(limit))
        .map_err(|error| format!("{path}: {error}"))
}

fn image_import_definition() -> Result<ArtifactBuildDefinition, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("locating the cooker executable: {error}"))?;
    let file = host_fs::readable_file(&executable)
        .map_err(|error| format!("opening the cooker executable: {error}"))?;
    let digest = file
        .digest(COOKER_BYTES)
        .map_err(|error| format!("hashing the cooker executable: {error}"))?;
    Ok(ArtifactBuildDefinition {
        id: "two-level-game.image-import".to_owned(),
        revision: FormatVersion::ONE,
        implementation: ArtifactContentDigest::from_content(digest),
    })
}
