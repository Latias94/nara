#![allow(dead_code)]

use std::{error::Error, fmt, fs::File, io, path::Path};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    asset::{AssetRef, AssetSourceKind, StableAssetId},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    image::ImageImportLimits,
    prelude::{Component, ComponentRegistry, World},
    project_host::{
        ProjectContentLoader, ProjectContentSnapshot, ProjectSettingsCandidate, RuntimePlan,
        built_in_schema_providers, ingest_project_manifest, resolve_runtime_plan,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        ScenePatchDocument,
    },
    tilemap::TilemapPlugin,
};
use nara_reference_game::{Enemy, Player, REFERENCE_GAME_SCHEMA_PROVIDER, Weapon, runtime_plugins};

pub const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";

pub struct LoadedProjectContent {
    pub candidate: ProjectSettingsCandidate,
    pub plan: RuntimePlan,
    pub loader: ProjectContentLoader,
    pub snapshot: ProjectContentSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectContentFixtureError {
    OpenProjectRoot,
    AuthorizeProjectRoot,
    ParseManifestPath,
    OpenManifest,
    IngestManifest,
    ResolveRuntimePlan,
    CreateContentLoader,
    LoadContentSnapshot,
}

impl fmt::Display for ProjectContentFixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenProjectRoot => "project root directory could not be opened",
            Self::AuthorizeProjectRoot => "project root capability could not be authorized",
            Self::ParseManifestPath => "project manifest path is invalid",
            Self::OpenManifest => "project manifest could not be opened",
            Self::IngestManifest => "project manifest could not be ingested",
            Self::ResolveRuntimePlan => "project runtime plan could not be resolved",
            Self::CreateContentLoader => "project content loader could not be created",
            Self::LoadContentSnapshot => "project content snapshot could not be loaded",
        })
    }
}

impl Error for ProjectContentFixtureError {}

pub fn load_project_content() -> LoadedProjectContent {
    try_load_project_content().expect("reference project content should load")
}

pub fn project_root_capability() -> DirectoryCapability {
    try_project_root_capability().expect("reference project root should be authorized")
}

pub fn try_load_project_content() -> Result<LoadedProjectContent, ProjectContentFixtureError> {
    try_load_project_content_from_path(Path::new(env!("CARGO_MANIFEST_DIR")))
}

pub fn try_load_project_content_from_path(
    path: &Path,
) -> Result<LoadedProjectContent, ProjectContentFixtureError> {
    let root = try_project_root_capability_at(path)?;
    let (candidate, plan, root) = try_candidate_plan_and_root_with(
        root,
        ImageImportLimits::default(),
        false,
    )?;
    let loader = ProjectContentLoader::new(root)
        .map_err(|_| ProjectContentFixtureError::CreateContentLoader)?;
    let snapshot = loader
        .load(&candidate, &plan)
        .map_err(|_| ProjectContentFixtureError::LoadContentSnapshot)?;
    Ok(LoadedProjectContent {
        candidate,
        plan,
        loader,
        snapshot,
    })
}

pub fn candidate_plan_and_root(
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    try_candidate_plan_and_root(image_limits, include_tilemap)
        .expect("reference project plan should resolve")
}

pub fn try_candidate_plan_and_root(
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> Result<(ProjectSettingsCandidate, RuntimePlan, DirectoryCapability), ProjectContentFixtureError>
{
    let root = try_project_root_capability()?;
    try_candidate_plan_and_root_with(root, image_limits, include_tilemap)
}

fn try_candidate_plan_and_root_with(
    root: DirectoryCapability,
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> Result<(ProjectSettingsCandidate, RuntimePlan, DirectoryCapability), ProjectContentFixtureError>
{
    let manifest_path = RelativePath::new("nara.toml")
        .map_err(|_| ProjectContentFixtureError::ParseManifestPath)?;
    let manifest = root
        .open_file(&manifest_path)
        .map_err(|_| ProjectContentFixtureError::OpenManifest)?;
    let candidate = ingest_project_manifest(&manifest, None)
        .map_err(|_| ProjectContentFixtureError::IngestManifest)?;
    let plan = try_resolve_reference_plan(&candidate, image_limits, include_tilemap)?;
    drop(manifest);
    Ok((candidate, plan, root))
}

pub fn resolve_reference_plan(
    candidate: &ProjectSettingsCandidate,
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> RuntimePlan {
    try_resolve_reference_plan(candidate, image_limits, include_tilemap)
        .expect("reference runtime plan should resolve")
}

pub fn try_resolve_reference_plan(
    candidate: &ProjectSettingsCandidate,
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> Result<RuntimePlan, ProjectContentFixtureError> {
    let request = reference_runtime_plugins(candidate, image_limits, include_tilemap);
    let mut providers = built_in_schema_providers();
    providers.push(REFERENCE_GAME_SCHEMA_PROVIDER);
    resolve_runtime_plan(candidate, request, providers)
        .map_err(|_| ProjectContentFixtureError::ResolveRuntimePlan)
}

pub fn reference_runtime_plugins(
    candidate: &ProjectSettingsCandidate,
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> nara::project_host::ProjectRuntimePlugins {
    let request = runtime_plugins(candidate).configure(nara::image::plugin(image_limits));
    let request = if include_tilemap {
        request
    } else {
        request.disable::<TilemapPlugin>()
    };
    request
}

pub fn expected_startup_scene(plan: &RuntimePlan) -> SceneDocument {
    let registry = plan.schema_validation().registry();
    let player_id = ComponentTypeId::new("reference_game.Player");
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let mut anchor = SceneEntityRecord::new(scene_id("enemy-anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    let player = SceneEntityRecord::new(scene_id("player"))
        .with_component(
            player_id.clone(),
            component_record(Player::fixture(), &player_id, registry),
        )
        .with_component(
            weapon_id.clone(),
            component_record(Weapon::fixture(), &weapon_id, registry),
        );
    SceneDocument::new([anchor, player])
}

pub fn expected_enemy_prefab(plan: &RuntimePlan) -> PrefabDocument {
    let registry = plan.schema_validation().registry();
    let enemy_id = ComponentTypeId::new("reference_game.Enemy");
    let image = AssetRef::path("textures/player.png").unwrap();
    let enemy = SceneEntityRecord::new(scene_id("enemy"))
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
        )
        .with_component(
            enemy_id.clone(),
            component_record(Enemy::fixture(), &enemy_id, registry),
        );
    PrefabDocument::new([enemy])
}

pub fn scene_id(value: &str) -> nara::identity::SceneEntityId {
    nara::identity::SceneEntityId::new(value).unwrap()
}

fn component_record<T>(
    component: T,
    component_id: &ComponentTypeId,
    registry: &ComponentRegistry,
) -> SceneComponentRecord
where
    T: Component,
{
    let mut world = World::new();
    let entity = world.spawn(component).id();
    let value = registry
        .encode_component(component_id, &world, entity)
        .unwrap()
        .unwrap()
        .unwrap();
    SceneComponentRecord::new(registry.schema(component_id).unwrap().version(), value)
}

fn sprite_value(image: &AssetRef) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                ("image", asset_ref_value(image)),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(1.0).unwrap()),
                        ("g", ComponentValue::f64(1.0).unwrap()),
                        ("b", ComponentValue::f64(1.0).unwrap()),
                        ("a", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_owned())),
            ("value", ComponentValue::String(path.as_str().to_owned())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn try_project_root_capability() -> Result<DirectoryCapability, ProjectContentFixtureError> {
    try_project_root_capability_at(Path::new(env!("CARGO_MANIFEST_DIR")))
}

pub fn try_project_root_capability_at(
    path: &Path,
) -> Result<DirectoryCapability, ProjectContentFixtureError> {
    let root = host_directory(path)
        .map_err(|_| ProjectContentFixtureError::OpenProjectRoot)?;
    DirectoryCapability::from_host_handle(
        root,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .map_err(|_| ProjectContentFixtureError::AuthorizeProjectRoot)
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(unix)]
    {
        File::open(path)
    }
}

pub fn texture_stable_id() -> StableAssetId {
    StableAssetId::parse_str(TEXTURE_ID).unwrap()
}

pub fn image_source_kind() -> AssetSourceKind {
    AssetSourceKind::Image
}
