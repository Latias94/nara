#![allow(dead_code)]

use std::{error::Error, fmt, fs::File, io, path::Path};

#[cfg(feature = "desktop")]
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    app::PluginDefinition,
    asset::{AssetRef, AssetSourceKind, StableAssetId},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    image::ImageImportLimits,
    prelude::{Component, ComponentRegistry, Transform2d, Vec2, World},
    project_host::{
        ProjectContentLoader, ProjectContentSnapshot, ProjectSettingsCandidate, RuntimePlan,
        built_in_schema_providers, ingest_project_manifest, resolve_runtime_plan,
    },
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        ScenePatchDocument, ScenePatchOperation,
    },
    tilemap::TilemapPlugin,
};
use nara::{gameplay::GameplayCommandPlugin, project_host::project_runtime_plugins};
use nara_reference_game::{
    EnemyRole, InitialHealth, InitialVelocity2d, PlayerRole, REFERENCE_GAME_SCHEMA_PROVIDER,
    ReferenceGamePlugin, ReferenceWavePlugin, WaveSpawn, Weapon,
    advanced_project_outcome_plugin_definition,
};

#[cfg(feature = "desktop")]
use nara::app::{RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeState};
#[cfg(feature = "desktop")]
use nara_reference_game::ReferenceDesktopPlugin;

pub const TEXTURE_ID: &str = "f840a555-9fca-4ceb-ac1b-e03b55d2f492";

fn game_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceGamePlugin>()
}

fn wave_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceWavePlugin>()
}

#[cfg(feature = "desktop")]
fn desktop_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceDesktopPlugin>()
}

#[cfg(feature = "desktop")]
pub fn stop_runtime(mut runtime: RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.drive(Duration::ZERO).unwrap();
        match runtime.state() {
            RuntimeState::Stopped => return,
            RuntimeState::CloseIncomplete => {
                panic!("desktop test runtime close became incomplete")
            }
            _ if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            state => panic!("desktop test runtime remained in {state:?} past its deadline"),
        }
    }
}

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
    let (candidate, plan, root) =
        try_candidate_plan_and_root_with(root, ImageImportLimits::default(), false)?;
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

#[cfg(feature = "desktop")]
pub fn desktop_candidate_plan_and_root()
-> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    let root = try_project_root_capability().expect("reference project root should be authorized");
    let manifest_path = RelativePath::new("nara.toml").unwrap();
    let manifest = root.open_file(&manifest_path).unwrap();
    let candidate = ingest_project_manifest(&manifest, Some("desktop")).unwrap();
    drop(manifest);

    let request = project_runtime_plugins(&candidate)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(game_plugin())
        .insert_after::<ReferenceGamePlugin>(wave_plugin())
        .insert_after::<ReferenceWavePlugin>(desktop_plugin());
    let mut providers = built_in_schema_providers();
    providers.push(REFERENCE_GAME_SCHEMA_PROVIDER);
    let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();
    (candidate, plan, root)
}

pub fn headless_wave_candidate_plan_and_root()
-> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    headless_wave_candidate_plan_and_root_with_test_plugin(None)
}

pub fn headless_wave_candidate_plan_and_root_with(
    test_plugin: PluginDefinition,
) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    headless_wave_candidate_plan_and_root_with_test_plugin(Some(test_plugin))
}

fn headless_wave_candidate_plan_and_root_with_test_plugin(
    test_plugin: Option<PluginDefinition>,
) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    let root = try_project_root_capability().expect("reference project root should be authorized");
    let manifest_path = RelativePath::new("nara.toml").unwrap();
    let manifest = root.open_file(&manifest_path).unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    drop(manifest);

    let request = project_runtime_plugins(&candidate)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(game_plugin())
        .insert_after::<ReferenceGamePlugin>(wave_plugin());
    let request = match test_plugin {
        Some(definition) => request.insert_after::<ReferenceWavePlugin>(definition),
        None => request,
    };
    let mut providers = built_in_schema_providers();
    providers.push(REFERENCE_GAME_SCHEMA_PROVIDER);
    let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();
    (candidate, plan, root)
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
    let request = project_runtime_plugins(candidate)
        .insert_after::<GameplayCommandPlugin>(game_plugin())
        .insert_after::<ReferenceGamePlugin>(advanced_project_outcome_plugin_definition())
        .configure(nara::image::plugin(image_limits));
    if include_tilemap {
        request
    } else {
        request.disable::<TilemapPlugin>()
    }
}

pub fn expected_startup_scene(plan: &RuntimePlan) -> SceneDocument {
    let registry = plan.schema_validation().registry();
    let sprite_id = ComponentTypeId::new("nara.sprite.Sprite");
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let player_role_id = ComponentTypeId::new("reference_game.PlayerRole");
    let health_id = ComponentTypeId::new("reference_game.InitialHealth");
    let velocity_id = ComponentTypeId::new("reference_game.InitialVelocity2d");
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let anchor = enemy_anchor("enemy-anchor", None, registry);
    let second_anchor = enemy_anchor("enemy-anchor-2", Some((9.0, 5)), registry);
    let third_anchor = enemy_anchor("enemy-anchor-3", Some((13.0, 9)), registry);
    let player = SceneEntityRecord::new(scene_id("player"))
        .with_component(
            sprite_id.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(
                    &AssetRef::path("textures/tiny-dungeon.png").unwrap(),
                    (1.8, 1.8),
                    (0.58, 0.84, 1.0, 1.0),
                    (0.0, 8.0 / 11.0),
                    (1.0 / 12.0, 0.090_909_090_909_090_93),
                    20,
                ),
            ),
        )
        .with_component(
            transform_id.clone(),
            component_record(Transform2d::IDENTITY, &transform_id, registry),
        )
        .with_component(
            health_id.clone(),
            component_record(InitialHealth { hit_points: 20 }, &health_id, registry),
        )
        .with_component(
            velocity_id.clone(),
            component_record(
                InitialVelocity2d {
                    velocity: Vec2::ZERO,
                },
                &velocity_id,
                registry,
            ),
        )
        .with_component(
            player_role_id.clone(),
            component_record(PlayerRole {}, &player_role_id, registry),
        );
    let weapon = SceneEntityRecord::new(scene_id("player-weapon"))
        .with_parent(scene_id("player"))
        .with_component(
            sprite_id,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, weapon_sprite_value()),
        )
        .with_component(
            transform_id.clone(),
            component_record(
                Transform2d::from_translation(Vec2::new(1.2, 0.0)),
                &transform_id,
                registry,
            ),
        )
        .with_component(
            weapon_id.clone(),
            component_record(Weapon::fixture(), &weapon_id, registry),
        );
    SceneDocument::new([anchor, second_anchor, third_anchor, player, weapon])
}

fn enemy_anchor(
    id: &str,
    override_values: Option<(f64, u64)>,
    registry: &ComponentRegistry,
) -> SceneEntityRecord {
    let overrides = override_values.map_or_else(ScenePatchDocument::default, |(x, spawn_tick)| {
        ScenePatchDocument::new([
            ScenePatchOperation::SetField {
                entity: scene_id("enemy"),
                component: ComponentTypeId::new("nara.transform.Transform2d"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("translation.x"),
                value: ComponentValue::f64(x).unwrap(),
            },
            ScenePatchOperation::SetField {
                entity: scene_id("enemy"),
                component: ComponentTypeId::new("reference_game.WaveSpawn"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("tick"),
                value: ComponentValue::U64(spawn_tick),
            },
        ])
    });
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let mut anchor = SceneEntityRecord::new(scene_id(id)).with_component(
        transform_id.clone(),
        component_record(Transform2d::IDENTITY, &transform_id, registry),
    );
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides,
    });
    anchor
}

pub fn expected_enemy_prefab(plan: &RuntimePlan) -> PrefabDocument {
    let registry = plan.schema_validation().registry();
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let enemy_role_id = ComponentTypeId::new("reference_game.EnemyRole");
    let health_id = ComponentTypeId::new("reference_game.InitialHealth");
    let velocity_id = ComponentTypeId::new("reference_game.InitialVelocity2d");
    let wave_spawn_id = ComponentTypeId::new("reference_game.WaveSpawn");
    let image = AssetRef::path("textures/tiny-dungeon.png").unwrap();
    let enemy = SceneEntityRecord::new(scene_id("enemy"))
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(
                    &image,
                    (1.65, 1.65),
                    (1.0, 0.74, 0.74, 1.0),
                    (2.0 / 12.0, 9.0 / 11.0),
                    (1.0 / 12.0, 0.090_909_090_909_090_93),
                    10,
                ),
            ),
        )
        .with_component(
            transform_id.clone(),
            component_record(
                Transform2d::from_translation(Vec2::new(5.0, 0.0)),
                &transform_id,
                registry,
            ),
        )
        .with_component(
            enemy_role_id.clone(),
            component_record(EnemyRole {}, &enemy_role_id, registry),
        )
        .with_component(
            health_id.clone(),
            component_record(InitialHealth { hit_points: 10 }, &health_id, registry),
        )
        .with_component(
            velocity_id.clone(),
            component_record(
                InitialVelocity2d {
                    velocity: Vec2::new(-0.5, 0.0),
                },
                &velocity_id,
                registry,
            ),
        )
        .with_component(
            wave_spawn_id.clone(),
            component_record(WaveSpawn::fixture(), &wave_spawn_id, registry),
        );
    PrefabDocument::new([enemy])
}

fn weapon_sprite_value() -> ComponentValue {
    ComponentValue::map([
        (
            "material",
            ComponentValue::map([(
                "tint",
                ComponentValue::map([
                    ("r", ComponentValue::f64(1.0).unwrap()),
                    ("g", ComponentValue::f64(0.86).unwrap()),
                    ("b", ComponentValue::f64(0.22).unwrap()),
                    ("a", ComponentValue::f64(1.0).unwrap()),
                ]),
            )]),
        ),
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(0.9).unwrap()),
                ("y", ComponentValue::f64(0.3).unwrap()),
            ]),
        ),
    ])
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

fn sprite_value(
    image: &AssetRef,
    size: (f64, f64),
    tint: (f64, f64, f64, f64),
    texture_min: (f64, f64),
    texture_size: (f64, f64),
    sort_key: i64,
) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(size.0).unwrap()),
                ("y", ComponentValue::f64(size.1).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                ("image", asset_ref_value(image)),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(tint.0).unwrap()),
                        ("g", ComponentValue::f64(tint.1).unwrap()),
                        ("b", ComponentValue::f64(tint.2).unwrap()),
                        ("a", ComponentValue::f64(tint.3).unwrap()),
                    ]),
                ),
                (
                    "sampler",
                    ComponentValue::map([
                        ("min_filter", ComponentValue::String("nearest".to_owned())),
                        ("mag_filter", ComponentValue::String("nearest".to_owned())),
                        (
                            "mipmap_filter",
                            ComponentValue::String("nearest".to_owned()),
                        ),
                        (
                            "address_mode_u",
                            ComponentValue::String("clamp_to_edge".to_owned()),
                        ),
                        (
                            "address_mode_v",
                            ComponentValue::String("clamp_to_edge".to_owned()),
                        ),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(sort_key)),
        (
            "texture_region",
            ComponentValue::map([
                (
                    "min",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(texture_min.0).unwrap()),
                        ("y", ComponentValue::f64(texture_min.1).unwrap()),
                    ]),
                ),
                (
                    "size",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(texture_size.0).unwrap()),
                        ("y", ComponentValue::f64(texture_size.1).unwrap()),
                    ]),
                ),
            ]),
        ),
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
    let root = host_directory(path).map_err(|_| ProjectContentFixtureError::OpenProjectRoot)?;
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
