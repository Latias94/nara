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
    AssetAppExt, AssetEventKind, AssetEvents, AssetLoadContext, AssetLoadError, AssetLoader,
    AssetPlugin, AssetServer, AssetStates, Assets, Handle, LoadState, NpakEntryKind,
    RuntimeAssetMode, RuntimeAssetSource, load_asset_sync, source::asset_id_path,
};
use nara_core::{AssetId, vfs::MemoryAssetSource};

const MAIN: &str = "0193a100-1111-7000-8000-000000000001";
const DEPENDENCY: &str = "0193a100-1111-7000-8000-000000000002";

#[derive(Debug, PartialEq, Eq)]
struct Text(Vec<u8>);

struct TextLoader(Arc<AtomicUsize>);

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
    let mut memory = MemoryAssetSource::new();
    memory
        .insert(&asset_id_path(MAIN.parse().unwrap()), main)
        .unwrap();
    memory
        .insert(&asset_id_path(DEPENDENCY.parse().unwrap()), b" world")
        .unwrap();
    RuntimeAssetSource::from_memory(RuntimeAssetMode::Source, memory)
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
    assert_eq!(by_path, by_id);
    assert_eq!(
        source_app.world().resource::<Assets<Text>>().get(by_id),
        cooked_app.world().resource::<Assets<Text>>().get(cooked)
    );
}
