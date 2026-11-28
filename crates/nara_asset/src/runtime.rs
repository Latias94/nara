use std::{
    any::{TypeId, type_name},
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt, io,
    sync::Arc,
};

use nara_app::{App, CoreStage, PluginError};
use nara_core::AssetId;
use nara_ecs::World;

use crate::{
    ASSET_PLUGIN_ID, Asset, AssetError, AssetEvents, AssetId as RuntimeId, AssetPath,
    AssetPathError, AssetProductRef, AssetServer, AssetStateError, AssetStates, Assets, Handle,
    ImportedProductId, LoadState, NpakEntryKind, RuntimeAssetBackend, RuntimeAssetMode,
    RuntimeAssetSource, StableAssetId, plugin::publish_asset,
};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetLoadError {
    #[error("install AssetPlugin before loading runtime assets")]
    MissingPlugin,
    #[error(transparent)]
    Request(#[from] AssetError),
    #[error(transparent)]
    Path(#[from] AssetPathError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[cfg(all(feature = "authoring", any(unix, windows)))]
    #[error(transparent)]
    Scan(#[from] crate::AssetScanError),
    #[cfg(feature = "cooked")]
    #[error(transparent)]
    Archive(#[from] crate::cooked::NpakError),
    #[error("could not read asset '{id}': {source}")]
    Read { id: AssetId, source: io::Error },
    #[error("could not decode asset '{id}': {source}")]
    Decode {
        id: AssetId,
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("asset '{id}' failed to load: {message}")]
    PreviouslyFailed { id: AssetId, message: String },
    #[error(transparent)]
    Publish(#[from] AssetStateError),
}

/// Reads all bytes for one load from the same source and records dependency edges.
pub struct AssetLoadContext<'a> {
    id: AssetId,
    backend: &'a dyn RuntimeAssetBackend,
    dependencies: Vec<AssetId>,
}

impl AssetLoadContext<'_> {
    #[must_use]
    pub const fn id(&self) -> AssetId {
        self.id
    }

    #[must_use]
    pub fn mode(&self) -> RuntimeAssetMode {
        self.backend.mode()
    }

    #[cfg(feature = "authoring")]
    #[must_use]
    pub fn meta(&self) -> Option<crate::AssetMeta> {
        self.backend.meta(self.id)
    }

    pub fn read_dependency(
        &mut self,
        id: AssetId,
        expected_cooked_kind: Option<NpakEntryKind>,
    ) -> Result<Vec<u8>, AssetLoadError> {
        let bytes = self.backend.read_asset(id, expected_cooked_kind)?;
        self.dependencies.push(id);
        Ok(bytes)
    }
}

/// Decodes one type of runtime asset on the asset server's worker or synchronous path.
pub trait AssetLoader: Send + Sync + 'static {
    type Asset: Asset;
    type Error: Error + Send + Sync + 'static;

    fn cooked_kind(&self) -> Option<NpakEntryKind>;

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AssetLoaderRegistry {
    loaders: HashMap<TypeId, RegisteredLoader>,
}

impl AssetLoaderRegistry {
    #[must_use]
    pub fn contains<T: Asset>(&self) -> bool {
        self.loaders.contains_key(&TypeId::of::<T>())
    }
}

#[derive(Clone)]
struct RegisteredLoader(Arc<dyn ErasedLoader>);

impl fmt::Debug for RegisteredLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredLoader").finish_non_exhaustive()
    }
}

impl PartialEq for RegisteredLoader {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RegisteredLoader {}

type Publish = Box<dyn FnOnce(&mut World) -> Result<(), AssetStateError> + Send + Sync>;

struct DecodedAsset {
    publish: Publish,
    dependencies: Vec<AssetId>,
}

trait ErasedLoader: Send + Sync {
    fn cooked_kind(&self) -> Option<NpakEntryKind>;
    fn request(&self, server: &mut AssetServer, id: AssetId) -> Result<RuntimeId, AssetError>;
    fn read(
        &self,
        request: Request,
        source: &RuntimeAssetSource,
    ) -> Result<DecodedAsset, AssetLoadError>;
    fn is_resident(&self, world: &World, handle: RuntimeId) -> bool;
}

impl<L: AssetLoader> ErasedLoader for L {
    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        AssetLoader::cooked_kind(self)
    }

    fn request(&self, server: &mut AssetServer, id: AssetId) -> Result<RuntimeId, AssetError> {
        server.load_id::<L::Asset>(id).map(Handle::id)
    }

    fn read(
        &self,
        request: Request,
        source: &RuntimeAssetSource,
    ) -> Result<DecodedAsset, AssetLoadError> {
        let backend = source.backend();
        let bytes = backend.read_asset(request.id, AssetLoader::cooked_kind(self))?;
        let mut context = AssetLoadContext {
            id: request.id,
            backend,
            dependencies: Vec::new(),
        };
        let value = self
            .load(&bytes, &mut context)
            .map_err(|source| AssetLoadError::Decode {
                id: request.id,
                source: Box::new(source),
            })?;
        context.dependencies.sort_unstable();
        context.dependencies.dedup();
        Ok(DecodedAsset {
            publish: Box::new(move |world| {
                publish_asset(world, Handle::new(request.handle), value)
            }),
            dependencies: context.dependencies,
        })
    }

    fn is_resident(&self, world: &World, handle: RuntimeId) -> bool {
        world
            .get_resource::<Assets<L::Asset>>()
            .is_some_and(|assets| assets.contains_id(handle))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Request {
    id: AssetId,
    handle: RuntimeId,
    asset_type: TypeId,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeLoading {
    source: Option<RuntimeAssetSource>,
    registry: AssetLoaderRegistry,
    requests: BTreeMap<RuntimeId, Request>,
}

/// Installs a byte source and registers typed loaders for an App with `AssetPlugin`.
pub trait AssetAppExt {
    fn set_asset_source(&mut self, source: RuntimeAssetSource) -> Result<&mut Self, PluginError>;

    fn register_asset_loader<L: AssetLoader>(
        &mut self,
        loader: L,
    ) -> Result<&mut Self, PluginError>;
}

impl AssetAppExt for App {
    fn set_asset_source(&mut self, source: RuntimeAssetSource) -> Result<&mut Self, PluginError> {
        let mut server = self
            .world_mut()?
            .get_resource_mut::<AssetServer>()
            .ok_or_else(missing_plugin)?;
        server.runtime.source = Some(source.clone());
        self.insert_resource(source)?;
        Ok(self)
    }

    fn register_asset_loader<L: AssetLoader>(
        &mut self,
        loader: L,
    ) -> Result<&mut Self, PluginError> {
        let world = self.world_mut()?;
        let mut server = world
            .get_resource_mut::<AssetServer>()
            .ok_or_else(missing_plugin)?;
        let loaders = &mut server.runtime.registry.loaders;
        if loaders.contains_key(&TypeId::of::<L::Asset>()) {
            return Err(PluginError::SetupFailed {
                plugin: ASSET_PLUGIN_ID,
                message: format!(
                    "an asset loader is already registered for {}",
                    type_name::<L::Asset>()
                ),
            });
        }
        loaders.insert(TypeId::of::<L::Asset>(), RegisteredLoader(Arc::new(loader)));
        world.init_resource::<Assets<L::Asset>>();
        Ok(self)
    }
}

fn missing_plugin() -> PluginError {
    PluginError::SetupFailed {
        plugin: ASSET_PLUGIN_ID,
        message: AssetLoadError::MissingPlugin.to_string(),
    }
}

impl AssetServer {
    #[must_use]
    pub fn loader_registry(&self) -> &AssetLoaderRegistry {
        &self.runtime.registry
    }

    /// Requests a persistent UUID and returns its typed handle immediately. Decode errors are
    /// available through [`AssetStates`] after `TaskUpdate` publishes the result.
    pub fn load_id<T: Asset>(&mut self, id: AssetId) -> Result<Handle<T>, AssetError> {
        self.runtime
            .source
            .as_ref()
            .ok_or(AssetError::MissingRuntimeSource)?;
        if !self.runtime.registry.contains::<T>() {
            return Err(AssetError::MissingLoader(type_name::<T>()));
        }
        let product = AssetProductRef::new(
            StableAssetId::from_uuid(id.as_uuid()),
            ImportedProductId::PRIMARY,
        )
        .map_err(AssetError::InvalidProductIdentity)?;
        let handle = self.reserve_product(product)?;
        self.runtime.requests.entry(handle.id()).or_insert(Request {
            id,
            handle: handle.id(),
            asset_type: TypeId::of::<T>(),
        });
        Ok(handle)
    }

    /// Resolves a logical path through the configured source before requesting its UUID.
    pub fn load_path<T: Asset>(&mut self, path: &str) -> Result<Handle<T>, AssetError> {
        let path = AssetPath::new(path).map_err(AssetError::InvalidPath)?;
        let id = self
            .runtime
            .source
            .as_ref()
            .ok_or(AssetError::MissingRuntimeSource)?
            .backend()
            .resolve_path(&path)
            .ok_or(AssetError::UnresolvedPath(path))?;
        self.load_id(id)
    }

    /// Requests a persistent dependency using the unique registered loader for its kind.
    /// The returned runtime ID can be queried in [`AssetStates`].
    pub fn load_dependency(&mut self, id: AssetId) -> Result<RuntimeId, AssetError> {
        let kind = self
            .runtime
            .source
            .as_ref()
            .ok_or(AssetError::MissingRuntimeSource)?
            .backend()
            .kind(id)
            .ok_or(AssetError::UnknownRuntimeKind(id))?;
        let mut loaders = self
            .runtime
            .registry
            .loaders
            .values()
            .filter(|loader| loader.0.cooked_kind() == Some(kind));
        let loader = loaders
            .next()
            .ok_or(AssetError::MissingKindLoader(kind))?
            .clone();
        if loaders.next().is_some() {
            return Err(AssetError::AmbiguousKindLoader(kind));
        }
        loader.0.request(self, id)
    }
}

/// Returns a typed asset with its value already published. A pending request for the same UUID
/// shares its decode with the caller.
pub fn load_asset_sync<T: Asset>(
    world: &mut World,
    id: AssetId,
) -> Result<Handle<T>, AssetLoadError> {
    load_sync(world, |server| server.load_id::<T>(id).map(Handle::id)).map(Handle::new)
}

/// Publishes a dependency immediately using the unique loader for its source kind.
pub fn load_dependency_sync(world: &mut World, id: AssetId) -> Result<RuntimeId, AssetLoadError> {
    load_sync(world, |server| server.load_dependency(id))
}

fn load_sync(
    world: &mut World,
    request_load: impl FnOnce(&mut AssetServer) -> Result<RuntimeId, AssetError>,
) -> Result<RuntimeId, AssetLoadError> {
    if !world.contains_resource::<AssetStates>() || !world.contains_resource::<AssetEvents>() {
        return Err(AssetLoadError::MissingPlugin);
    }
    let (handle, request, source, loader) = {
        let mut server = world
            .get_resource_mut::<AssetServer>()
            .ok_or(AssetLoadError::MissingPlugin)?;
        let handle = request_load(&mut server)?;
        let request = server
            .runtime
            .requests
            .remove(&handle)
            .ok_or(AssetLoadError::MissingPlugin)?;
        let source = server
            .runtime
            .source
            .clone()
            .ok_or(AssetError::MissingRuntimeSource)?;
        let loader = server
            .runtime
            .registry
            .loaders
            .get(&request.asset_type)
            .ok_or(AssetLoadError::MissingPlugin)?
            .clone();
        (handle, request, source, loader)
    };
    if already_settled(world, request, &loader)? {
        return Ok(handle);
    }
    world.resource_mut::<AssetStates>().set_loading(handle);
    #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
    let task = world
        .get_resource_mut::<pending::PendingLoads>()
        .and_then(|mut pending| pending.0.remove(&handle));
    #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
    let result = task.map_or_else(
        || loader.0.read(request, &source),
        nara_app::tasks::block_on,
    );
    #[cfg(not(all(feature = "tasks", not(target_arch = "wasm32"))))]
    let result = loader.0.read(request, &source);
    finish(world, handle, result)?;
    Ok(handle)
}

pub(crate) fn install(app: &mut App) -> Result<(), PluginError> {
    #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
    app.init_resource::<pending::PendingLoads>()?;
    app.add_systems(CoreStage::TaskUpdate, update_loads)?;
    Ok(())
}

fn update_loads(world: &mut World) {
    let (source, requests) = {
        let mut server = world.resource_mut::<AssetServer>();
        (
            server.runtime.source.clone(),
            std::mem::take(&mut server.runtime.requests),
        )
    };
    #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
    pending::publish_ready(world);
    let Some(source) = source else { return };
    for request in requests.into_values() {
        let Some(loader) = world
            .resource::<AssetServer>()
            .runtime
            .registry
            .loaders
            .get(&request.asset_type)
            .cloned()
        else {
            continue;
        };
        if !matches!(already_settled(world, request, &loader), Ok(false)) {
            continue;
        }
        #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
        if world
            .resource::<pending::PendingLoads>()
            .0
            .contains_key(&request.handle)
        {
            continue;
        }
        world
            .resource_mut::<AssetStates>()
            .set_loading(request.handle);
        #[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
        pending::spawn(world, request, source.clone(), loader);
        #[cfg(not(all(feature = "tasks", not(target_arch = "wasm32"))))]
        let _ = finish(world, request.handle, loader.0.read(request, &source));
    }
}

fn already_settled(
    world: &World,
    request: Request,
    loader: &RegisteredLoader,
) -> Result<bool, AssetLoadError> {
    let Some(state) = world.resource::<AssetStates>().state(request.handle) else {
        return Ok(false);
    };
    if let LoadState::Failed { message } = state.load_state() {
        return Err(AssetLoadError::PreviouslyFailed {
            id: request.id,
            message: message.clone(),
        });
    }
    Ok(state.usable_version().is_some() && loader.0.is_resident(world, request.handle))
}

fn finish(
    world: &mut World,
    handle: RuntimeId,
    result: Result<DecodedAsset, AssetLoadError>,
) -> Result<(), AssetLoadError> {
    let result = result.and_then(|decoded| {
        (decoded.publish)(world)?;
        world
            .resource_mut::<AssetStates>()
            .set_dependencies(handle, decoded.dependencies);
        Ok(())
    });
    if let Err(error) = &result {
        world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
            states.fail_loading(
                handle,
                error.to_string(),
                &mut world.resource_mut::<AssetEvents>(),
            );
        });
    }
    result
}

#[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
mod pending {
    use nara_app::tasks::{Task, block_on, io_pool, poll_once};
    use nara_ecs::{Mut, Resource};

    use super::*;

    #[derive(Default, Resource)]
    pub(super) struct PendingLoads(
        pub BTreeMap<RuntimeId, Task<Result<DecodedAsset, AssetLoadError>>>,
    );

    pub(super) fn spawn(
        world: &mut World,
        request: Request,
        source: RuntimeAssetSource,
        loader: RegisteredLoader,
    ) {
        let task = io_pool().spawn(async move { loader.0.read(request, &source) });
        world
            .resource_mut::<PendingLoads>()
            .0
            .insert(request.handle, task);
    }

    pub(super) fn publish_ready(world: &mut World) {
        world.resource_scope(|world, mut pending: Mut<PendingLoads>| {
            pending.0.retain(|handle, task| {
                if let Some(result) = block_on(poll_once(task)) {
                    let _ = finish(world, *handle, result);
                    false
                } else {
                    true
                }
            });
        });
    }
}
