use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginSchemaProviderId, PluginShutdownContext, RuntimeInstance, RuntimeStartFailure,
    StartupStage,
};
use nara_asset::Handle;
use nara_ecs::{
    Component, Entity, Mut, Resource, World, schedule::IntoScheduleConfigs,
    upstream::error::BevyError, upstream::lifecycle::HookContext, upstream::world::DeferredWorld,
};
use nara_identity::EntityLookup;
use nara_image::{
    ImageAsset, ImageReloadStats, PreparedImageResource, PreparedImageSnapshot, image_resource_key,
    prepare_images,
};
use nara_reflect::{
    ComponentRegistry, ComponentRegistryError, ComponentRegistryPlugin, ComponentSchemaCatalog,
    ComponentSchemaOwnerId, ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSourceError, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    PersistentComponent, PersistentComponentProvider, component_registry,
    register_schema_provider_for_plugin,
};
use nara_render::PreparedRenderResources;
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
};

const PLUGIN_ID: PluginId = PluginId::new("consumer.native-domain");
const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.native-domain.schema");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID])
    .provides_schema(&[PROVIDER_ID]);
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.native-domain"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.native-domain.native", 1),
    catalog,
    register_configuration,
);

/// This persistent value is also the ordinary runtime configuration queried by the domain.
#[derive(Debug, Component, PersistentComponent)]
#[nara(
    id = "consumer.native-domain.Configuration",
    version = 1,
    alias = "Native configuration",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct Configuration {
    #[nara(id = "value", alias = "Value")]
    pub value: u64,
}

#[derive(Component, Default)]
#[component(on_remove = required_removed)]
pub struct NativeRequired;

#[derive(Component)]
#[component(on_remove = hook_owned_removed)]
pub struct HookOwned {
    pub allocation: u64,
}

#[derive(Component)]
#[require(NativeRequired)]
#[component(on_add = native_added, on_remove = native_removed)]
pub struct NativeState {
    pub revision: SceneAuthoringRevision,
    pub value: u64,
    pub image: Option<PreparedImageSnapshot>,
    lease: NativeLease,
}

#[derive(Clone, Copy, Resource)]
pub struct NativeImageSource(pub Handle<ImageAsset>);

impl NativeState {
    pub fn allocation(&self) -> u64 {
        self.lease.id
    }
}

struct NativeLease {
    id: u64,
    effects: Arc<Mutex<Effects>>,
}

impl Drop for NativeLease {
    fn drop(&mut self) {
        let mut effects = self.effects.lock().unwrap();
        assert!(effects.live.remove(&self.id).is_some());
        effects.released.push(self.id);
    }
}

fn native_added(mut world: DeferredWorld<'_>, context: HookContext) {
    let native = world.get::<NativeState>(context.entity).unwrap();
    let allocation = native.allocation();
    native
        .lease
        .effects
        .lock()
        .unwrap()
        .added
        .push((context.entity, allocation));
    world
        .commands()
        .entity(context.entity)
        .insert(HookOwned { allocation });
}

fn native_removed(world: DeferredWorld<'_>, context: HookContext) {
    let native = world.get::<NativeState>(context.entity).unwrap();
    native
        .lease
        .effects
        .lock()
        .unwrap()
        .removed
        .push((context.entity, native.allocation()));
}

fn required_removed(world: DeferredWorld<'_>, context: HookContext) {
    world
        .resource::<Control>()
        .effects
        .lock()
        .unwrap()
        .required_removed
        .push(context.entity);
}

fn hook_owned_removed(world: DeferredWorld<'_>, context: HookContext) {
    let allocation = world.get::<HookOwned>(context.entity).unwrap().allocation;
    world
        .resource::<Control>()
        .effects
        .lock()
        .unwrap()
        .hook_owned_removed
        .push((context.entity, allocation));
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Effects {
    pub accepted: Vec<SceneAuthoringRevision>,
    pub created: Vec<(u64, u64)>,
    pub released: Vec<u64>,
    pub added: Vec<(Entity, u64)>,
    pub removed: Vec<(Entity, u64)>,
    pub required_removed: Vec<Entity>,
    pub hook_owned_removed: Vec<(Entity, u64)>,
    pub creation_failures: Vec<SceneAuthoringRevision>,
    pub live: BTreeMap<u64, u64>,
    next_allocation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    PersistentProjection,
    NativeCreation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    Pending(SceneAuthoringRevision),
    Ready(SceneAuthoringRevision),
    Unavailable(SceneAuthoringRevision, UnavailableReason),
}

#[derive(Resource)]
pub struct Domain {
    pub session: SceneAuthoringSession,
    pub readiness: Readiness,
    pub rejected_document: bool,
    pub legacy_sync_rejected: bool,
    pub legacy_clear_rejected: bool,
}

pub enum Request {
    Accept(SceneDocumentCandidate),
    Retry,
    LegacySync,
    LegacyClear,
}

/// The test host submits ordinary domain input without obtaining mutable Runtime World access.
#[derive(Default, Clone, Resource)]
pub struct Control {
    pending: Arc<Mutex<Option<Request>>>,
    pub effects: Arc<Mutex<Effects>>,
    fail_creation: Arc<AtomicBool>,
}

impl Control {
    pub fn submit(&self, request: Request) {
        assert!(self.pending.lock().unwrap().replace(request).is_none());
    }

    pub fn fail_next_creation(&self) {
        self.fail_creation.store(true, Ordering::SeqCst);
    }

    pub fn effects(&self) -> Effects {
        self.effects.lock().unwrap().clone()
    }
}

struct NativeDomainPlugin {
    control: Control,
}

impl Plugin for NativeDomainPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        register_schema_provider_for_plugin(app, PLUGIN_ID, "Configuration", &PROVIDER)?;
        let session = SceneAuthoringSession::new(SceneDocument::default());
        let revision = session.revision();
        app.insert_resource(self.control.clone())?
            .insert_resource(Domain {
                session,
                readiness: Readiness::Pending(revision),
                rejected_document: false,
                legacy_sync_rejected: false,
                legacy_clear_rejected: false,
            })?
            .add_systems(StartupStage::Scene, start_domain)?
            .add_systems(CoreStage::Update, update_domain)?;
        if app.world().contains_resource::<NativeImageSource>() {
            app.add_systems(
                CoreStage::Prepare,
                reconcile_image_native_state.after(prepare_images),
            )?;
        }
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        retire_native_state(context.world_mut());
        Ok(())
    }
}

pub fn start(control: &Control) -> Result<RuntimeInstance, RuntimeStartFailure> {
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    install(&mut app, control);
    RuntimeInstance::start(app)
}

pub fn install(app: &mut App, control: &Control) {
    app.add_plugin(NativeDomainPlugin {
        control: control.clone(),
    })
    .unwrap();
}

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![Configuration::persistent_component_schema()],
        ..Default::default()
    })
}

fn register_configuration(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component::<Configuration>()?;
    Ok(())
}

pub fn candidate(value: u64) -> SceneDocumentCandidate {
    candidate_with_value(ComponentValue::U64(value))
}

pub fn invalid_candidate() -> SceneDocumentCandidate {
    candidate_with_value(ComponentValue::String(
        "invalid numeric configuration".to_owned(),
    ))
}

fn candidate_with_value(value: ComponentValue) -> SceneDocumentCandidate {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(local_id())
        .with_component(
            ComponentTypeId::new("consumer.native-domain.Configuration"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", value)]),
            ),
        )]);
    decode(&document)
}

pub fn empty_candidate() -> SceneDocumentCandidate {
    decode(&SceneDocument::default())
}

fn decode(document: &SceneDocument) -> SceneDocumentCandidate {
    let encoded = document.to_ron_string().unwrap();
    SceneDocumentCandidate::decode_ron_bytes(encoded.as_bytes()).unwrap()
}

fn local_id() -> SceneEntityId {
    SceneEntityId::new("configured-native-object").unwrap()
}

pub fn configuration_entity(world: &World) -> Option<Entity> {
    let instance = world.resource::<Domain>().session.live_instance()?;
    match instance.resolve(world, &local_id()) {
        EntityLookup::Resolved(entity) => Some(entity),
        _ => None,
    }
}

fn registry(world: &World) -> ComponentRegistry {
    ComponentRegistry::from_snapshot(component_registry(world).unwrap().snapshot().unwrap())
}

fn start_domain(world: &mut World) -> Result<(), BevyError> {
    let control = world.resource::<Control>().clone();
    let registry = registry(world);
    world.resource_scope(|world, mut domain: Mut<Domain>| {
        accept(&mut domain, &registry, &control, candidate(1));
        reconcile(world, &mut domain, &registry, &control);
        if matches!(domain.readiness, Readiness::Unavailable(..)) {
            return Err(BevyError::error(std::io::Error::other(
                "the accepted startup configuration is unavailable",
            )));
        }
        Ok(())
    })
}

fn update_domain(world: &mut World) {
    let control = world.resource::<Control>().clone();
    let request = control.pending.lock().unwrap().take();
    let Some(request) = request else {
        return;
    };
    let registry = registry(world);
    world.resource_scope(|world, mut domain: Mut<Domain>| match request {
        Request::Accept(candidate) => {
            if accept(&mut domain, &registry, &control, candidate) {
                reconcile(world, &mut domain, &registry, &control);
            }
        }
        Request::Retry => reconcile(world, &mut domain, &registry, &control),
        Request::LegacySync => {
            domain.legacy_sync_rejected =
                !domain.session.sync_world(world, &registry).unwrap().synced;
        }
        Request::LegacyClear => {
            domain.legacy_clear_rejected = !domain.session.clear_live_world(world).cleared;
        }
    });
}

fn accept(
    domain: &mut Domain,
    registry: &ComponentRegistry,
    control: &Control,
    candidate: SceneDocumentCandidate,
) -> bool {
    domain.rejected_document = domain
        .session
        .try_replace_file_candidate(candidate, registry)
        .is_err();
    if domain.rejected_document {
        return false;
    }
    let revision = domain.session.revision();
    domain.readiness = Readiness::Pending(revision);
    control.effects.lock().unwrap().accepted.push(revision);
    true
}

fn reconcile(
    world: &mut World,
    domain: &mut Domain,
    registry: &ComponentRegistry,
    control: &Control,
) {
    let revision = domain.session.revision();
    // Native effects retire in their owning domain phase, outside persistent rollback. Remove
    // both the owner and its required/hook-created state before the strict scene transaction.
    retire_native_state(world);
    let report = domain.session.sync_world(world, registry).unwrap();
    if !report.synced {
        domain.readiness =
            Readiness::Unavailable(revision, UnavailableReason::PersistentProjection);
        return;
    }
    let entity = domain.session.live_instance().and_then(|instance| {
        match instance.resolve(world, &local_id()) {
            EntityLookup::Resolved(entity) => Some(entity),
            _ => None,
        }
    });
    let Some(entity) = entity else {
        domain.readiness = Readiness::Ready(revision);
        return;
    };
    create_native_state(world, domain, control, entity);
}

fn create_native_state(world: &mut World, domain: &mut Domain, control: &Control, entity: Entity) {
    let revision = domain.session.revision();
    let image = if world.contains_resource::<NativeImageSource>() {
        let Some(snapshot) = current_image_snapshot(world) else {
            domain.readiness = Readiness::Pending(revision);
            return;
        };
        Some(snapshot)
    } else {
        None
    };
    let value = world.get::<Configuration>(entity).unwrap().value;
    if control.fail_creation.swap(false, Ordering::SeqCst) {
        control
            .effects
            .lock()
            .unwrap()
            .creation_failures
            .push(revision);
        domain.readiness = Readiness::Unavailable(revision, UnavailableReason::NativeCreation);
        return;
    }
    let allocation = {
        let mut effects = control.effects.lock().unwrap();
        effects.next_allocation += 1;
        let allocation = effects.next_allocation;
        effects.live.insert(allocation, value);
        effects.created.push((allocation, value));
        allocation
    };
    world.entity_mut(entity).insert(NativeState {
        revision,
        value,
        image,
        lease: NativeLease {
            id: allocation,
            effects: Arc::clone(&control.effects),
        },
    });
    world.flush();
    domain.readiness = Readiness::Ready(revision);
}

fn current_image_snapshot(world: &World) -> Option<PreparedImageSnapshot> {
    if world.resource::<ImageReloadStats>().pending != 0 {
        return None;
    }
    let handle = world.resource::<NativeImageSource>().0;
    let record = world
        .resource::<PreparedRenderResources<PreparedImageResource>>()
        .get(image_resource_key(handle))?;
    Some(PreparedImageSnapshot::new(
        record.snapshot().clone(),
        record.resource()?.clone(),
    ))
}

fn reconcile_image_native_state(world: &mut World) {
    let control = world.resource::<Control>().clone();
    world.resource_scope(|world, mut domain: Mut<Domain>| {
        if matches!(domain.readiness, Readiness::Unavailable(..)) {
            return;
        }
        let Some(entity) = domain.session.live_instance().and_then(|instance| {
            match instance.resolve(world, &local_id()) {
                EntityLookup::Resolved(entity) => Some(entity),
                _ => None,
            }
        }) else {
            return;
        };
        let snapshot = current_image_snapshot(world);
        let native = world.get::<NativeState>(entity);
        match (native, snapshot) {
            (Some(native), Some(snapshot)) if native.image.as_ref() == Some(&snapshot) => return,
            (Some(_), None) if world.resource::<ImageReloadStats>().pending != 0 => return,
            (Some(_), _) => retire_native_state(world),
            (None, None) => return,
            (None, Some(_)) => {}
        }
        create_native_state(world, &mut domain, &control, entity);
    });
}

fn retire_native_state(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, nara_ecs::With<NativeState>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in entities {
        world
            .entity_mut(entity)
            .remove::<(NativeState, NativeRequired, HookOwned)>();
    }
    world.flush();
}
