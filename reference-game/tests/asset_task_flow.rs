use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    advanced_prelude::*, asset::AssetPlugin, hierarchy::HierarchyPlugin,
    reflect::ComponentRegistryPlugin, transform::TransformPlugin,
};

const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TEXTURE_BYTES: &[u8] = include_bytes!("../assets/textures/player.png");

#[test]
fn public_asset_task_flow_preserves_last_good_after_failed_reload() {
    let temp = TempAssetRoot::new();
    let texture_path = temp.path().join("textures").join("player.png");
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, TEXTURE_BYTES).unwrap();
    let record = AssetRecord::new(
        StableAssetId::parse_str(TEXTURE_ID).unwrap(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    );
    let limits = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(TEXTURE_BYTES.len()).unwrap());
    let mut app = App::new();
    app.add_plugins((
        DiagnosticsPlugin::default(),
        ComponentRegistryPlugin,
        TaskPlugin::default(),
        AssetPlugin,
        HierarchyPlugin,
        TransformPlugin,
        RenderPlugin,
        ImagePreparePlugin,
    ))
    .unwrap();
    app.add_plugin(
        ImagePlugin::with_limits(limits)
            .unwrap()
            .with_source_directory(image_source_directory(temp.path())),
    )
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();

    drive_until(&mut app, |app| {
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .is_some_and(|state| state.load_state() == &LoadState::Loaded)
    });
    let last_good_hash = app
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .source()
        .source_hash();
    let last_good_version = app
        .world()
        .resource::<AssetStates>()
        .version(handle.id())
        .unwrap();

    let mut oversized = TEXTURE_BYTES.to_vec();
    oversized.push(0);
    fs::write(&texture_path, oversized).unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();
    drive_until(&mut app, |app| {
        app.world().resource::<ImageReloadStats>().failed > 0
    });

    assert_eq!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash(),
        last_good_hash
    );
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .version(handle.id())
            .unwrap(),
        last_good_version
    );
    assert_eq!(app.world().resource::<ImageReloadStats>().pending, 0);
    app.shutdown_plugins().unwrap();
}

fn drive_until(app: &mut App, mut complete: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !complete(app) && Instant::now() < deadline {
        app.update().unwrap();
        thread::yield_now();
    }
    assert!(complete(app), "asset task flow exceeded the test deadline");
}

fn image_source_directory(path: &Path) -> ImageSourceDirectory {
    let directory = DirectoryCapability::from_host_handle(
        host_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory)
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        return fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}

struct TempAssetRoot {
    path: PathBuf,
}

impl TempAssetRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara_reference_asset_task_{}_{}",
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempAssetRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
