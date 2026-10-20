use std::{
    fs::{self, File},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_app::{App, CoreStage, RuntimeInstance};
use nara_asset::{
    AssetPath, AssetPlugin, AssetRecord, AssetServer, AssetSourceChanges, AssetSourceKind, Assets,
    Handle, ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ItemLimit, TimeLimit};
use nara_ecs::{Res, ResMut, Resource};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_hierarchy::HierarchyPlugin;
use nara_image::{
    ImageAsset, ImageImportBudgetHost, ImageImportLimits, ImageImporter, ImagePlugin,
    ImagePreparePlugin, ImageReloadStats, ImageSourceDirectory,
};
use nara_reflect::ComponentRegistryPlugin;
use nara_render::{RenderComponentsPlugin, RenderPlugin};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskKindConfig, TaskPlugin, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskShutdownPolicy, TaskSpawnRequest,
};
use nara_transform::TransformPlugin;

use super::domain::{
    self, Control, Domain, NativeImageSource, NativeState, Readiness, configuration_entity,
};

const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TEXTURE_PATH: &str = "textures/player.png";
const FIRST_IMAGE: &[u8] =
    include_bytes!("../../../../../reference-game/assets/textures/player.png");
const SECOND_IMAGE: &[u8] =
    include_bytes!("../../../../../reference-game/assets/textures/tiny-dungeon.png");
const IMAGE_METADATA: &[u8] =
    include_bytes!("../../../../../reference-game/assets/textures/player.png.meta");
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
enum SourceChange {
    Initial,
    Modified,
    Removed,
}

#[derive(Clone, Default, Resource)]
struct ImageControl(Arc<Mutex<Vec<SourceChange>>>);

fn publish_source_changes(control: Res<ImageControl>, mut changes: ResMut<AssetSourceChanges>) {
    for change in std::mem::take(&mut *control.0.lock().unwrap()) {
        let path = AssetPath::new(TEXTURE_PATH).unwrap();
        match change {
            SourceChange::Initial => changes.meta_modified(path),
            SourceChange::Modified => changes.modified(path),
            SourceChange::Removed => changes.removed(path),
        }
        .unwrap();
    }
}

pub struct ImageFixture {
    root: TempAssetRoot,
    source_changes: ImageControl,
    pub control: Control,
    pub budget: ImageImportBudgetHost,
    pub handle: Handle<ImageAsset>,
}

impl ImageFixture {
    fn start() -> (Self, RuntimeInstance) {
        let root = TempAssetRoot::new();
        let image_path = root.path.join(TEXTURE_PATH);
        fs::create_dir_all(image_path.parent().unwrap()).unwrap();
        fs::write(&image_path, FIRST_IMAGE).unwrap();
        fs::write(root.path.join("textures/player.png.meta"), IMAGE_METADATA).unwrap();
        let record = AssetRecord::new(
            StableAssetId::parse_str(TEXTURE_ID).unwrap(),
            AssetPath::new(TEXTURE_PATH).unwrap(),
            AssetSourceKind::Image,
        );
        let limits = ImageImportLimits::default();
        let budget = ImageImportBudgetHost::new(limits).unwrap();
        let source_changes = ImageControl::default();
        let control = Control::default();
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            TaskPlugin::new(task_config()),
            AssetPlugin,
            HierarchyPlugin,
            TransformPlugin,
            RenderComponentsPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
        ))
        .unwrap();
        app.add_plugin(
            ImagePlugin::with_budget_host(limits, budget.clone())
                .unwrap()
                .with_source_directory(image_source_directory(&root.path)),
        )
        .unwrap();
        app.insert_resource(source_changes.clone())
            .unwrap()
            .add_systems(CoreStage::First, publish_source_changes)
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
        app.insert_resource(NativeImageSource(handle)).unwrap();
        domain::install(&mut app, &control);
        let runtime = match RuntimeInstance::start(app) {
            Ok(runtime) => runtime,
            Err(mut failure) => {
                let error = format!("{failure:?}");
                failure.drive_retirement();
                drop(failure);
                panic!("image reconciliation startup failed: {error}");
            }
        };
        (
            Self {
                root,
                source_changes,
                control,
                budget,
                handle,
            },
            runtime,
        )
    }

    pub fn load_initial_image(&self, runtime: &mut RuntimeInstance) {
        let domain = runtime.world().resource::<Domain>();
        assert_eq!(
            domain.readiness,
            Readiness::Pending(domain.session.revision())
        );
        self.source_changes
            .0
            .lock()
            .unwrap()
            .push(SourceChange::Initial);
        drive_until(runtime, |runtime| {
            let domain = runtime.world().resource::<Domain>();
            domain.readiness == Readiness::Ready(domain.session.revision())
        });
        assert_eq!(runtime.world().resource::<ImageReloadStats>().applied, 1);
        let entity = configuration_entity(runtime.world()).unwrap();
        let native = runtime.world().get::<NativeState>(entity).unwrap();
        let snapshot = native.image.as_ref().unwrap();
        let image = runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(self.handle)
            .unwrap();
        assert!(!snapshot.resource().pixels().is_empty());
        assert_eq!(snapshot.resource().pixels(), image.pixels());
        assert_eq!(
            snapshot.resource().source_hash(),
            image.source().source_hash()
        );
        assert!(!snapshot.resource().retains_source_allocation());
        self.assert_import_work_released(runtime);
    }

    pub fn assert_import_work_released(&self, runtime: &RuntimeInstance) {
        let budget = self.budget.snapshot();
        assert_eq!(
            budget,
            runtime
                .world()
                .resource::<ImageImporter>()
                .budget_snapshot()
        );
        assert!(budget.accepted_reservations() > 0);
        assert!(budget.high_water_rgba_bytes() > 0);
        assert_eq!(budget.active_reservations(), 0);
        assert_eq!(budget.active_bytes(), 0);
        assert_eq!(
            budget.accepted_reservations(),
            budget.released_reservations()
        );
        assert_eq!(budget.total_reserved_bytes(), budget.total_released_bytes());
    }

    pub fn reload_second_image(&self) {
        self.reload(SECOND_IMAGE);
    }

    pub fn reload_first_image(&self) {
        self.reload(FIRST_IMAGE);
    }

    fn reload(&self, bytes: &[u8]) {
        fs::write(self.root.path.join(TEXTURE_PATH), bytes).unwrap();
        self.source_changes
            .0
            .lock()
            .unwrap()
            .push(SourceChange::Modified);
    }

    pub fn remove_image(&self) {
        fs::remove_file(self.root.path.join(TEXTURE_PATH)).unwrap();
        self.source_changes
            .0
            .lock()
            .unwrap()
            .push(SourceChange::Removed);
    }
}

pub fn with_runtime(exercise: impl FnOnce(&ImageFixture, &mut RuntimeInstance)) {
    let (fixture, mut runtime) = ImageFixture::start();
    let result = catch_unwind(AssertUnwindSafe(|| exercise(&fixture, &mut runtime)));
    let closed = runtime.finish().map_err(|owner| {
        let error = owner.to_string();
        drop(owner);
        error
    });
    closed.expect("the real image and native domain owners must close physically");
    let budget = fixture.budget.snapshot();
    assert_eq!(budget.active_reservations(), 0);
    assert_eq!(budget.active_bytes(), 0);
    assert_eq!(
        budget.accepted_reservations(),
        budget.released_reservations()
    );
    assert_eq!(budget.total_reserved_bytes(), budget.total_released_bytes());
    let effects = fixture.control.effects();
    assert!(effects.live.is_empty());
    assert_eq!(
        effects.released,
        effects
            .created
            .iter()
            .map(|(allocation, _)| *allocation)
            .collect::<Vec<_>>()
    );
    assert_eq!(effects.removed, effects.added);
    assert_eq!(effects.hook_owned_removed, effects.added);
    assert_eq!(
        effects.required_removed,
        effects
            .added
            .iter()
            .map(|(entity, _)| *entity)
            .collect::<Vec<_>>()
    );
    match result {
        Ok(()) => {}
        Err(failure) => resume_unwind(failure),
    }
}

pub fn drive_until(
    runtime: &mut RuntimeInstance,
    mut complete: impl FnMut(&RuntimeInstance) -> bool,
) {
    let deadline = Instant::now() + DEADLINE;
    while !complete(runtime) && Instant::now() < deadline {
        runtime.drive(Duration::ZERO).unwrap();
        thread::yield_now();
    }
    assert!(
        complete(runtime),
        "image reconciliation exceeded its finite deadline"
    );
}

pub fn assert_image_queued(runtime: &mut RuntimeInstance) {
    drive_until(runtime, |runtime| {
        runtime.world().resource::<ImageReloadStats>().pending > 0
            && runtime
                .world()
                .resource::<TaskPools>()
                .stats()
                .for_kind(TaskPoolKind::Io)
                .queued
                > 0
    });
    assert_eq!(
        runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .running,
        1
    );
}

pub struct IoGate {
    release: Option<mpsc::Sender<()>>,
    exited: mpsc::Receiver<()>,
    _task: TaskHandle<()>,
}

impl IoGate {
    pub fn hold(runtime: &RuntimeInstance) -> Self {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (exited_tx, exited_rx) = mpsc::channel();
        let task = runtime
            .world()
            .resource::<TaskPools>()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x49_4f_47_41_54_45)),
                move |_| {
                    let _ = started_tx.send(());
                    let _ = release_rx.recv_timeout(Duration::from_secs(30));
                    let _ = exited_tx.send(());
                },
            )
            .into_handle()
            .unwrap();
        started_rx.recv_timeout(DEADLINE).unwrap();
        Self {
            release: Some(release_tx),
            exited: exited_rx,
            _task: task,
        }
    }

    pub fn release(mut self) {
        self.release.take().unwrap().send(()).unwrap();
        self.exited.recv_timeout(DEADLINE).unwrap();
    }
}

fn task_config() -> TaskPoolConfig {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::new(8).unwrap());
    let timeout = TimeLimit::new(Duration::from_millis(20)).unwrap();
    TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(timeout, timeout, timeout),
    )
    .unwrap()
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

        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
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
            "nara_image_native_reconciliation_{}_{}",
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempAssetRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
