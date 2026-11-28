#![cfg(feature = "runtime")]

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nara_app::App;
use nara_asset::{
    AssetAppExt, AssetError, AssetEventKind, AssetEvents, AssetLoadContext, AssetLoadError,
    AssetLoader, AssetPlugin, AssetServer, AssetStates, Assets, Handle, LoadState, NpakEntryKind,
    RuntimeAssetMode, RuntimeAssetSource, load_asset_sync, source::asset_id_path,
};
use nara_core::{AssetId, vfs::MemoryAssetSource};

const MAIN: &str = "0193a100-1111-7000-8000-000000000001";
const DEPENDENCY: &str = "0193a100-1111-7000-8000-000000000002";

#[derive(Debug, PartialEq, Eq)]
struct Text(Vec<u8>);

struct TextLoader(Arc<AtomicUsize>);

struct PanicOnDrop;

impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        panic!("test panic payload panicked while dropping");
    }
}

impl AssetLoader for TextLoader {
    type Asset = Text;
    type Error = AssetLoadError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Raw)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<Text, AssetLoadError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        match bytes {
            b"panic" => panic!("test loader panicked"),
            b"panic on drop" => std::panic::panic_any(PanicOnDrop),
            _ => (),
        }
        if bytes == b"broken" {
            return Err(AssetLoadError::Read {
                id: context.id(),
                source: io::Error::other("invalid document"),
            });
        }
        let dependency =
            context.read_dependency(DEPENDENCY.parse().unwrap(), Some(NpakEntryKind::Raw))?;
        Ok(Text([bytes, dependency.as_slice()].concat()))
    }
}

fn app(source: RuntimeAssetSource) -> (App, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.add_plugin(AssetPlugin).unwrap();
    app.set_asset_source(source).unwrap();
    app.register_asset_loader(TextLoader(hits.clone())).unwrap();
    (app, hits)
}

fn memory(main: &[u8]) -> RuntimeAssetSource {
    RuntimeAssetSource::from_memory(RuntimeAssetMode::Source, files(main))
}

fn files(main: &[u8]) -> MemoryAssetSource {
    let mut memory = MemoryAssetSource::new();
    memory
        .insert(&asset_id_path(MAIN.parse().unwrap()), main)
        .unwrap();
    memory
        .insert(&asset_id_path(DEPENDENCY.parse().unwrap()), b" world")
        .unwrap();
    memory
}

fn request(app: &mut App) -> Handle<Text> {
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .load_id(MAIN.parse().unwrap())
        .unwrap()
}

fn update_until_settled(app: &mut App, handle: Handle<Text>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update_with_delta(Duration::ZERO).unwrap();
        if app
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .is_some_and(|state| !matches!(state.load_state(), LoadState::Loading))
        {
            break;
        }
        assert!(Instant::now() < deadline, "asset never settled");
        std::thread::yield_now();
    }
}

#[test]
fn requests_share_a_handle_decode_and_dependency_edges() {
    let (mut app, hits) = app(memory(b"hello"));
    let handle = request(&mut app);
    assert_eq!(handle, request(&mut app));
    assert!(app.world().resource::<Assets<Text>>().get(handle).is_none());
    update_until_settled(&mut app, handle);
    assert_eq!(handle, request(&mut app));
    app.update_with_delta(Duration::ZERO).unwrap();

    assert_eq!(
        app.world().resource::<Assets<Text>>().get(handle),
        Some(&Text(b"hello world".to_vec()))
    );
    assert_eq!(hits.load(Ordering::Relaxed), 1);
    let state = app
        .world()
        .resource::<AssetStates>()
        .state(handle.id())
        .unwrap();
    assert_eq!(state.load_state(), &LoadState::Loaded);
    assert_eq!(
        state.dependencies(),
        &[DEPENDENCY.parse::<AssetId>().unwrap()]
    );
    assert_eq!(
        app.world()
            .resource::<AssetEvents>()
            .iter()
            .next()
            .unwrap()
            .kind(),
        AssetEventKind::Added
    );
}

#[test]
fn sync_completes_a_queued_request_and_keeps_failures_available() {
    let (mut app, hits) = app(memory(b"broken"));
    let handle = request(&mut app);
    let result = load_asset_sync::<Text>(app.world_mut().unwrap(), MAIN.parse().unwrap());
    assert!(result.unwrap_err().to_string().contains("invalid document"));
    assert_eq!(handle, request(&mut app));
    app.update_with_delta(Duration::ZERO).unwrap();
    let state = app
        .world()
        .resource::<AssetStates>()
        .state(handle.id())
        .unwrap();
    assert!(
        matches!(state.load_state(), LoadState::Failed { message } if message.contains("invalid document"))
    );
    assert_eq!(hits.load(Ordering::Relaxed), 1);
    assert_eq!(
        app.world()
            .resource::<AssetEvents>()
            .iter()
            .next()
            .unwrap()
            .kind(),
        AssetEventKind::LoadFailed
    );
}

#[test]
fn a_new_source_can_recover_a_previously_failed_handle() {
    let (mut app, hits) = app(memory(b"broken"));
    let handle = request(&mut app);
    update_until_settled(&mut app, handle);
    assert!(app.world().resource::<Assets<Text>>().get(handle).is_none());

    app.set_asset_source(memory(b"fixed")).unwrap();
    assert_eq!(request(&mut app), handle);
    update_until_settled(&mut app, handle);
    assert_eq!(hits.load(Ordering::Relaxed), 2);
    assert_eq!(
        app.world().resource::<Assets<Text>>().get(handle),
        Some(&Text(b"fixed world".to_vec()))
    );
}

#[test]
fn a_panicking_loader_fails_its_handle_and_other_requests_still_publish() {
    for (bytes, expected) in [
        (b"panic".as_slice(), "test loader panicked"),
        (b"panic on drop", "non-string panic payload"),
    ] {
        let (mut app, hits) = app(memory(bytes));
        let bad = request(&mut app);
        #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
        app.update_with_delta(Duration::ZERO).unwrap();
        let good = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetServer>()
            .load_id::<Text>(DEPENDENCY.parse().unwrap())
            .unwrap();

        update_until_settled(&mut app, bad);
        update_until_settled(&mut app, good);
        app.update_with_delta(Duration::ZERO).unwrap();
        assert!(matches!(
            app.world().resource::<AssetStates>().state(bad.id()).unwrap().load_state(),
            LoadState::Failed { message } if message.contains(expected)
        ));
        assert_eq!(
            app.world().resource::<Assets<Text>>().get(good),
            Some(&Text(b" world world".to_vec())),
        );
        let events = app.world().resource::<AssetEvents>();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.id() == bad.id())
                .map(|event| event.kind())
                .collect::<Vec<_>>(),
            [AssetEventKind::LoadFailed],
        );
        assert_eq!(hits.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn a_removal_cancels_queued_work_and_a_later_request_can_load_again() {
    let (mut app, hits) = app(memory(b"hello"));
    let handle = request(&mut app);
    remove_text(&mut app, handle);
    app.update_with_delta(Duration::ZERO).unwrap();
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Removed,
    );
    assert_eq!(hits.load(Ordering::Relaxed), 0);

    assert_eq!(request(&mut app), handle);
    remove_text(&mut app, handle);
    assert_eq!(request(&mut app), handle);
    update_until_settled(&mut app, handle);
    assert_eq!(
        app.world().resource::<Assets<Text>>().get(handle),
        Some(&Text(b"hello world".to_vec())),
    );
    assert_eq!(hits.load(Ordering::Relaxed), 1);
}

fn remove_text(app: &mut App, handle: Handle<Text>) {
    use nara_ecs::Mut;

    app.world_mut()
        .unwrap()
        .resource_scope(|world, mut assets: Mut<Assets<Text>>| {
            world.resource_scope(|world, mut states: Mut<AssetStates>| {
                assets
                    .remove_with_state(
                        handle,
                        &mut states,
                        &mut world.resource_mut::<AssetEvents>(),
                    )
                    .unwrap();
            });
        });
}

#[test]
fn a_wrong_kind_request_does_not_claim_the_uuid() {
    struct WrongKind;

    impl AssetLoader for WrongKind {
        type Asset = Vec<u8>;
        type Error = std::convert::Infallible;

        fn cooked_kind(&self) -> Option<NpakEntryKind> {
            Some(NpakEntryKind::Image)
        }

        fn load(
            &self,
            bytes: &[u8],
            _: &mut AssetLoadContext<'_>,
        ) -> Result<Self::Asset, Self::Error> {
            Ok(bytes.to_vec())
        }
    }

    let id = MAIN.parse().unwrap();
    let source = RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Source,
        files(b"hello"),
        [(id, NpakEntryKind::Raw)],
    );
    let (mut app, _) = app(source);
    app.register_asset_loader(WrongKind).unwrap();
    let world = app.world_mut().unwrap();
    assert!(matches!(
        load_asset_sync::<Vec<u8>>(world, id),
        Err(AssetLoadError::Request(AssetError::WrongRuntimeKind {
            expected: NpakEntryKind::Image,
            actual: NpakEntryKind::Raw,
            ..
        }))
    ));
    let handle = load_asset_sync::<Text>(world, id).unwrap();
    assert!(nara_asset::is_runtime_asset_resident(world, handle.id()));
    world.resource_mut::<Assets<Text>>().remove(handle);
    assert!(!nara_asset::is_runtime_asset_resident(world, handle.id()));
}

#[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
#[test]
fn native_loads_decode_off_thread_and_sync_joins_the_running_job() {
    struct WorkerThread;

    impl AssetLoader for WorkerThread {
        type Asset = std::thread::ThreadId;
        type Error = std::convert::Infallible;

        fn cooked_kind(&self) -> Option<NpakEntryKind> {
            None
        }

        fn load(&self, _: &[u8], _: &mut AssetLoadContext<'_>) -> Result<Self::Asset, Self::Error> {
            Ok(std::thread::current().id())
        }
    }

    let (mut app, _) = app(memory(b"hello"));
    app.register_asset_loader(WorkerThread).unwrap();
    let id = MAIN.parse().unwrap();
    let first = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .load_id::<std::thread::ThreadId>(id)
        .unwrap();
    app.update_with_delta(Duration::ZERO).unwrap();
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(first.id())
            .unwrap()
            .load_state(),
        &LoadState::Loading,
    );
    let ready = load_asset_sync::<std::thread::ThreadId>(app.world_mut().unwrap(), id).unwrap();
    assert_eq!(first, ready);
    let worker = app
        .world()
        .resource::<Assets<std::thread::ThreadId>>()
        .get(ready)
        .unwrap();
    assert_ne!(*worker, std::thread::current().id());
}

#[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
#[test]
fn replacing_the_source_restarts_unpublished_loads_without_changing_handles() {
    let (mut app, _) = app(memory(b"previous"));
    let handle = request(&mut app);
    app.update_with_delta(Duration::ZERO).unwrap();
    app.set_asset_source(memory(b"new")).unwrap();
    update_until_settled(&mut app, handle);
    assert_eq!(request(&mut app), handle);
    assert_eq!(
        app.world().resource::<Assets<Text>>().get(handle),
        Some(&Text(b"new world".to_vec())),
    );
}

#[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
#[test]
fn a_removal_cancels_work_requeued_by_a_remount() {
    let (mut app, _) = app(memory(b"previous"));
    let handle = request(&mut app);
    app.update_with_delta(Duration::ZERO).unwrap();
    app.set_asset_source(memory(b"new")).unwrap();
    remove_text(&mut app, handle);
    app.update_with_delta(Duration::ZERO).unwrap();
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Removed,
    );
    assert!(app.world().resource::<Assets<Text>>().get(handle).is_none());
}

#[cfg(not(all(feature = "tasks", not(target_arch = "wasm32"))))]
#[test]
fn no_task_pool_publishes_in_the_next_task_update() {
    let (mut app, _) = app(memory(b"hello"));
    let handle = request(&mut app);
    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(app.world().resource::<Assets<Text>>().get(handle).is_some());
}

#[cfg(all(feature = "authoring", feature = "cooked", any(unix, windows)))]
#[test]
fn project_paths_and_archive_entries_produce_the_same_asset() {
    use std::fs;

    use nara_asset::cooked::{NpakArchive, NpakCompression, NpakInput, encode_npak};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("assets");
    fs::create_dir(&root).unwrap();
    let pairs = [
        (MAIN, "hello.txt", b"hello".as_slice()),
        (DEPENDENCY, "world.txt", b" world".as_slice()),
    ];
    for (id, name, bytes) in pairs {
        fs::write(root.join(name), bytes).unwrap();
        fs::write(
            root.join(format!("{name}.meta")),
            format!("(id:\"{id}\",importer:\"nara.raw\",settings:())"),
        )
        .unwrap();
    }
    let entries = pairs.map(|(id, _, bytes)| {
        NpakInput::new(
            id.parse().unwrap(),
            NpakEntryKind::Raw,
            bytes,
            NpakCompression::None,
        )
    });
    let archive = NpakArchive::from_bytes(encode_npak(&entries).unwrap()).unwrap();
    let (mut source_app, _) =
        app(RuntimeAssetSource::from_project_dir(directory.path(), "assets").unwrap());
    let (mut cooked_app, _) = app(RuntimeAssetSource::from_npak_archive(Arc::new(archive)));
    let by_path = source_app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .load_path::<Text>("hello.txt")
        .unwrap();
    let by_id =
        load_asset_sync::<Text>(source_app.world_mut().unwrap(), MAIN.parse().unwrap()).unwrap();
    let cooked =
        load_asset_sync::<Text>(cooked_app.world_mut().unwrap(), MAIN.parse().unwrap()).unwrap();
    assert_eq!(
        nara_asset::load_dependency_sync(cooked_app.world_mut().unwrap(), MAIN.parse().unwrap())
            .unwrap(),
        cooked.id(),
    );
    assert_eq!(by_path, by_id);
    assert_eq!(
        source_app.world().resource::<Assets<Text>>().get(by_id),
        cooked_app.world().resource::<Assets<Text>>().get(cooked)
    );
}

#[cfg(all(feature = "authoring", any(unix, windows)))]
#[test]
fn project_source_rejects_unmapped_importers_when_a_kind_is_expected() {
    use std::fs;

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("assets");
    fs::create_dir(&root).unwrap();
    for (id, name, importer) in [
        (MAIN, "example.prefab.ron", "nara.prefab"),
        (DEPENDENCY, "example.txt", "custom.example"),
    ] {
        fs::write(root.join(name), b"example").unwrap();
        fs::write(
            root.join(format!("{name}.meta")),
            format!("(id:\"{id}\",importer:\"{importer}\",settings:())"),
        )
        .unwrap();
    }
    let source = RuntimeAssetSource::from_project_dir(directory.path(), "assets").unwrap();
    let (mut app, hits) = app(source.clone());
    for id in [MAIN, DEPENDENCY].map(|id| id.parse::<AssetId>().unwrap()) {
        assert_eq!(source.backend().read_asset(id, None).unwrap(), b"example");
        assert!(
            source
                .backend()
                .read_asset(id, Some(NpakEntryKind::Raw))
                .is_err()
        );
        assert!(matches!(
            app.world_mut().unwrap().resource_mut::<AssetServer>().load_id::<Text>(id),
            Err(AssetError::UnknownRuntimeKind(rejected)) if rejected == id,
        ));
    }
    assert_eq!(hits.load(Ordering::Relaxed), 0);
}
