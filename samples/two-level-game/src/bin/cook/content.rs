use super::input::RuntimeKind;
use nara::{
    asset::{
        AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetSourceKind, AssetSourcePath,
        ProjectAssetDatabase, RuntimeAssetCodec, SourceAssetBuild, SourceAssetImporter,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, build_image_import, build_source,
    },
    core::FormatVersion,
    fs::{DirectoryCapability, RelativePath},
    image::ImageRuntimeCodec,
    input::{ActionMapAsset, ActionMapFileCandidate, ActionMapFileLimits, ActionMapRuntimeCodec},
    project::{
        EffectiveProjectSettings, NativeDocumentIdentity, ProjectAuthoringState, ProjectManifest,
    },
    project_host::resolve_runtime_configuration,
    project_tooling::host_fs,
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentLimits, CookedContentRequest, PreparedRuntimeContent,
    },
    runtime_content::{
        RuntimeContentDependencyRole, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        SceneCompilationOptions, SceneDocumentCandidate, SourceDocumentKey, SourceDocumentKind,
    },
};
use nara_sprite_animation::{SpriteAnimationCodec, SpriteAnimationDomain};
use two_level_game::{
    animation::{ATLAS_PATH, CLIPS_PATH, atlas_product, clips_product},
    input::{INPUT_PATH, input_product},
    levels::LevelId,
    sound_bank::{BANK_PATH, SoundBankCodec, SoundBankDomain, bank_product},
};

const DOCUMENT_BYTES: u64 = 1024 * 1024;
const METADATA_BYTES: u64 = 64 * 1024;
const COOKER_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn prepare(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
    runtime: RuntimeKind,
) -> Result<(PreparedRuntimeContent, Vec<ArtifactBuildObservation>), String> {
    if runtime == RuntimeKind::AudioDesktop && !triple.contains("-windows-") {
        return Err("audio-desktop Player requires an explicit Windows target".into());
    }
    let settings = project_settings(root, runtime)?;
    let projection = settings
        .source_runtime_projection()
        .map_err(|error| format!("nara.toml runtime configuration: {error}"))?;
    let selected = projection
        .input_requirement()
        .ok_or("nara.toml: the game's Player requires an authored startup Input selection")?;
    if selected.as_str() != INPUT_PATH {
        return Err(format!("nara.toml: expected input.action_map={INPUT_PATH}"));
    }
    let input_bytes = read(
        root,
        selected.as_str(),
        ActionMapFileLimits::default().encoded_bytes().get() as u64,
    )?;
    let input = ActionMapFileCandidate::decode_json_bytes(&input_bytes)
        .map_err(|error| format!("{selected}: {error}"))?;
    if input.product() != input_product() {
        return Err(format!(
            "{selected}: expected the game's intrinsic primary Input Product"
        ));
    }
    let metadata_path = format!("{selected}.meta");
    match root
        .open_file_exact(&RelativePath::new(&metadata_path).map_err(|error| error.to_string())?)
    {
        Ok(file) => {
            let bytes = file
                .read_to_end_bounded(
                    nara::asset::AssetMetaFileLimits::default()
                        .encoded_bytes()
                        .get() as u64,
                )
                .map_err(|error| error.to_string())?;
            let identity = AssetMetaIdentityCandidate::decode_json_bytes(&bytes)
                .map_err(|error| format!("{metadata_path}: {error}"))?
                .into_identity();
            if identity.stable_id() != input.product().source()
                || identity.source_kind() != &AssetSourceKind::Other("input-action-map".into())
                || identity.path().as_str() != selected.as_str()
            {
                return Err(format!(
                    "{metadata_path}: conflicts with the intrinsic Input identity"
                ));
            }
        }
        Err(nara::fs::FsError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{metadata_path}: {error}")),
    }
    let (product, definition) = input.into_parts();
    let input =
        ActionMapAsset::new(product, definition).map_err(|error| format!("{selected}: {error}"))?;
    let mut configuration = projection.runtime().to_configuration();
    configuration.input.action_map = Some(input.product());
    let configuration = configuration
        .resolve()
        .map_err(|error| format!("nara.toml processed runtime configuration: {error}"))?;
    let recipe = runtime.recipe()?;
    let plan = resolve_runtime_configuration(&configuration, recipe.clone())
        .map_err(|error| format!("cooked game composition: {error}"))?;
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            target: RuntimeContentTarget::new(triple, runtime.content_profile())
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
    builder
        .add_input(&input)
        .map_err(|error| format!("{selected}: Input cooking failed: {error}"))?;

    let scope = AssetPathScope::under(
        AssetSourcePath::new(settings.paths.assets.as_str())
            .map_err(|error| format!("assets root: {error}"))?,
    );
    let definition = import_definition("two-level-game.image-import", FormatVersion::ONE)?;
    let assets = root
        .open_directory(
            &RelativePath::new(settings.paths.assets.as_str())
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("opening the original assets root: {error}"))?;
    let store = ArtifactBuildStore::open(cache.share(), ArtifactBuildLimits::default())
        .map_err(|error| format!("asset import cache: {error}"))?;
    let mut database = ProjectAssetDatabase::default();
    let mut observations = vec![ArtifactBuildObservation::Raw {
        path: AssetSourcePath::new(selected.as_str()).map_err(|error| error.to_string())?,
        digest: Some(ArtifactContentDigest::of_bytes(&input_bytes)),
    }];
    for (path, product) in LevelId::ALL
        .into_iter()
        .map(|level| {
            let path = match level {
                LevelId::Meadow => "textures/meadow.png",
                LevelId::Cavern => "textures/cavern.png",
            };
            (path, level.image_product())
        })
        .chain([(ATLAS_PATH, atlas_product())])
    {
        let path = AssetPath::new(path).map_err(|error| format!("Image source path: {error}"))?;
        let source = scope.resolve(&path);
        let metadata_path = source.meta_path();
        let metadata = read(root, &metadata_path, METADATA_BYTES)?;
        let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
            .map_err(|error| format!("{metadata_path}: {error}"))?
            .into_identity();
        if identity.path() != &path
            || identity.source_kind() != &AssetSourceKind::Image
            || identity.stable_id() != product.source()
        {
            return Err(format!(
                "{metadata_path}: expected the game's declared Image identity and path"
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
        if imported.seed().source != product
            || imported.seed().location.as_ref() != Some(&source)
            || imported.members().len() != 1
            || imported.members()[0].products.as_slice() != [product]
        {
            return Err(format!(
                "{source}: imported group must supply only its declared Image Product"
            ));
        }
        builder
            .add_group(&imported)
            .map_err(|error| format!("{source}: {error}"))?;
        observations.extend_from_slice(imported.observations());
    }

    let path =
        AssetPath::new(BANK_PATH).map_err(|error| format!("SoundBank source path: {error}"))?;
    let source = scope.resolve(&path);
    let metadata_path = source.meta_path();
    let metadata = read(root, &metadata_path, METADATA_BYTES)?;
    let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
        .map_err(|error| format!("{metadata_path}: {error}"))?
        .into_identity();
    if identity.path() != &path
        || identity.source_kind() != &AssetSourceKind::Other(SoundBankDomain::SOURCE_KIND.into())
        || identity.stable_id() != bank_product().source()
    {
        return Err(format!(
            "{metadata_path}: expected the game's SoundBank identity and path"
        ));
    }
    database
        .insert_identity(&scope, path.clone(), identity)
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let definition = import_definition(
        SoundBankDomain::BUILD_ID,
        FormatVersion::new(SoundBankDomain::BUILD_REVISION)
            .ok_or("SoundBank build revision must be nonzero")?,
    )?;
    let request = ArtifactSourceRequest::new(assets.share(), scope.clone(), path, definition);
    let imported = build_source::<SoundBankDomain>(&store, &request, |_| Ok(()))
        .map_err(|error| format!("{source}: SoundBank import failed: {error}"))?
        .into_group();
    if imported.seed().source != bank_product()
        || imported.seed().location.as_ref() != Some(&source)
        || imported.members().len() != 1
        || imported.members()[0].products.as_slice() != [bank_product()]
    {
        return Err(format!(
            "{source}: imported group must supply only the game's SoundBank Product"
        ));
    }
    builder
        .add_group(&imported)
        .map_err(|error| format!("{source}: {error}"))?;
    observations.extend_from_slice(imported.observations());

    let path = AssetPath::new(CLIPS_PATH)
        .map_err(|error| format!("Sprite animation source path: {error}"))?;
    let source = scope.resolve(&path);
    let metadata_path = source.meta_path();
    let metadata = read(root, &metadata_path, METADATA_BYTES)?;
    let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata)
        .map_err(|error| format!("{metadata_path}: {error}"))?
        .into_identity();
    if identity.path() != &path
        || identity.source_kind()
            != &AssetSourceKind::Other(SpriteAnimationDomain::SOURCE_KIND.into())
        || identity.stable_id() != clips_product().source()
    {
        return Err(format!(
            "{metadata_path}: expected the game's Sprite animation identity and path"
        ));
    }
    database
        .insert_identity(&scope, path.clone(), identity)
        .map_err(|error| format!("{metadata_path}: {error}"))?;
    let definition = import_definition(
        SpriteAnimationDomain::BUILD_ID,
        FormatVersion::new(SpriteAnimationDomain::BUILD_REVISION)
            .ok_or("Sprite animation build revision must be nonzero")?,
    )?;
    let request = ArtifactSourceRequest::new(assets.share(), scope.clone(), path, definition);
    let imported = build_source::<SpriteAnimationDomain>(&store, &request, |_| Ok(()))
        .map_err(|error| format!("{source}: Sprite animation import failed: {error}"))?
        .into_group();
    if imported.seed().source != clips_product()
        || imported.seed().location.as_ref() != Some(&source)
        || imported.members().len() != 1
        || imported.members()[0].products.as_slice() != [clips_product()]
        || imported.members()[0].format.as_str() != SpriteAnimationCodec::RUNTIME_KIND
        || imported.members()[0].format_version.get() != SpriteAnimationCodec::RUNTIME_VERSION
        || !imported.members()[0].dependencies.is_empty()
    {
        return Err(format!(
            "{source}: imported group must supply only the game's Sprite animation Product and format"
        ));
    }
    builder
        .add_group(&imported)
        .map_err(|error| format!("{source}: {error}"))?;
    observations.extend_from_slice(imported.observations());

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
    if catalog.members().len() != 8 {
        return Err(
            "cooked content must contain exactly two Scenes, three Images, one SoundBank, one Sprite animation set and one global Input"
                .to_owned(),
        );
    }
    let input_member = catalog
        .member_for_product(input_product())
        .ok_or("missing cooked global Input")?;
    if catalog.configuration().input.action_map != Some(input_product())
        || prepared.base().plan().settings().input().action_map != Some(input.product())
        || input_member.products() != [input_product()]
        || !input_member.dependencies().is_empty()
        || !matches!(input_member.kind(), RuntimeContentMemberKind::Asset { format, version }
            if format.as_str() == ActionMapRuntimeCodec::RUNTIME_KIND
                && version.get() == ActionMapRuntimeCodec::RUNTIME_VERSION)
    {
        return Err("cooked global Input selection, identity or format changed".into());
    }
    let acquired_input = prepared
        .base()
        .acquire_startup_input()
        .map_err(|error| format!("cooked global Input acquisition: {error}"))?
        .ok_or("cooked content has no selected global Input")?;
    if acquired_input.product() != input.product()
        || acquired_input.definition().contexts() != input.definition().contexts()
        || acquired_input.definition().bindings() != input.definition().bindings()
    {
        return Err(
            "cooked global Input differs from its original authored Product or definition".into(),
        );
    }
    let bank = catalog
        .member_for_product(bank_product())
        .ok_or("missing cooked game SoundBank")?;
    if bank.products() != [bank_product()]
        || !matches!(bank.kind(), RuntimeContentMemberKind::Asset { format, version }
            if format.as_str() == SoundBankCodec::RUNTIME_KIND
                && version.get() == SoundBankCodec::RUNTIME_VERSION)
    {
        return Err("cooked SoundBank identity or format changed".into());
    }
    let clips = catalog
        .member_for_product(clips_product())
        .ok_or("missing cooked explorer Sprite animation set")?;
    if clips.products() != [clips_product()]
        || !clips.dependencies().is_empty()
        || !matches!(clips.kind(), RuntimeContentMemberKind::Asset { format, version }
            if format.as_str() == SpriteAnimationCodec::RUNTIME_KIND
                && version.get() == SpriteAnimationCodec::RUNTIME_VERSION)
    {
        return Err("cooked Sprite animation identity or format changed".into());
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
            || scene
                .dependencies()
                .iter()
                .any(|dependency| dependency.product() == input_product())
            || [atlas_product(), bank_product(), clips_product()]
                .into_iter()
                .any(|product| {
                    !scene.dependencies().iter().any(|dependency| {
                        dependency.product() == product
                            && dependency.role() == RuntimeContentDependencyRole::Required
                    })
                })
        {
            return Err(format!(
                "cooked {level:?} Scene identity or required shared atlas, SoundBank or Sprite animation changed"
            ));
        }
    }
    for product in LevelId::ALL
        .into_iter()
        .map(LevelId::image_product)
        .chain([atlas_product()])
    {
        let image = catalog
            .member_for_product(product)
            .ok_or_else(|| format!("missing cooked Image {product}"))?;
        if image.products() != [product]
            || !matches!(image.kind(), RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND && *version == FormatVersion::ONE)
        {
            return Err(format!("cooked Image {product} identity or format changed"));
        }
    }
    Ok((prepared, observations))
}

fn project_settings(
    root: &DirectoryCapability,
    runtime: RuntimeKind,
) -> Result<EffectiveProjectSettings, String> {
    let bytes = read(root, "nara.toml", DOCUMENT_BYTES)?;
    let loaded = ProjectManifest::parse_toml_bytes(&bytes);
    if loaded.has_errors() {
        return Err(format!("nara.toml: {:?}", loaded.diagnostics));
    }
    let settings = loaded
        .manifest
        .ok_or_else(|| "nara.toml: project manifest is absent".to_owned())?
        .resolve_profile(match runtime {
            RuntimeKind::Headless => None,
            RuntimeKind::Desktop | RuntimeKind::AudioDesktop => Some("desktop"),
        })
        .map_err(|error| format!("nara.toml: {error}"))?;
    if settings.window.enabled != (runtime != RuntimeKind::Headless) {
        return Err(format!(
            "nara.toml: the {} Player requires window.enabled={}",
            runtime.name(),
            runtime != RuntimeKind::Headless
        ));
    }
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

fn import_definition(id: &str, revision: FormatVersion) -> Result<ArtifactBuildDefinition, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("locating the cooker executable: {error}"))?;
    let file = host_fs::readable_file(&executable)
        .map_err(|error| format!("opening the cooker executable: {error}"))?;
    let digest = file
        .digest(COOKER_BYTES)
        .map_err(|error| format!("hashing the cooker executable: {error}"))?;
    Ok(ArtifactBuildDefinition {
        id: id.to_owned(),
        revision,
        implementation: ArtifactContentDigest::from_content(digest),
    })
}
