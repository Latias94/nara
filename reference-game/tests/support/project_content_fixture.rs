#![allow(dead_code)]

use std::{fs::File, path::Path};

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

pub fn load_project_content() -> LoadedProjectContent {
    let (candidate, plan, root) = candidate_plan_and_root(ImageImportLimits::default(), false);
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    LoadedProjectContent {
        candidate,
        plan,
        loader,
        snapshot,
    }
}

pub fn candidate_plan_and_root(
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
    let root = project_root_capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_reference_plan(&candidate, image_limits, include_tilemap);
    drop(manifest);
    (candidate, plan, root)
}

pub fn resolve_reference_plan(
    candidate: &ProjectSettingsCandidate,
    image_limits: ImageImportLimits,
    include_tilemap: bool,
) -> RuntimePlan {
    let request = reference_runtime_plugins(candidate, image_limits, include_tilemap);
    let mut providers = built_in_schema_providers();
    providers.push(REFERENCE_GAME_SCHEMA_PROVIDER);
    resolve_runtime_plan(candidate, request, providers).unwrap()
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

fn project_root_capability() -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(Path::new(env!("CARGO_MANIFEST_DIR"))),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .unwrap()
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> File {
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
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}

pub fn texture_stable_id() -> StableAssetId {
    StableAssetId::parse_str(TEXTURE_ID).unwrap()
}

pub fn image_source_kind() -> AssetSourceKind {
    AssetSourceKind::Image
}
