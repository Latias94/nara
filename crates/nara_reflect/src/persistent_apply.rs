//! Guarded persistent-component application contracts.

use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetRef, AssetRefError, AssetServer, Handle, ProjectAssetDatabase};
use nara_ecs::{
    __private::{
        PersistentComponentMetadataError, PersistentLifecycleEvent as EcsPersistentLifecycleEvent,
        PersistentObserverScope as EcsPersistentObserverScope,
        validate_registered_persistent_component_apply, validate_resource_insertion,
    },
    Component, Entity, Resource, World,
    component::ComponentId,
    world::WorldId,
};

use crate::{
    ComponentTypeId,
    codec::{ComponentCodecError, ComponentDecodeContext},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentLifecycleEvent {
    Add,
    Insert,
    Discard,
    Remove,
    Despawn,
}

impl PersistentLifecycleEvent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Insert => "insert",
            Self::Discard => "discard",
            Self::Remove => "remove",
            Self::Despawn => "despawn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentObserverScope {
    EventGlobal,
    ComponentGlobal,
    Entity,
    EntityComponent,
}

impl PersistentObserverScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventGlobal => "event-global",
            Self::ComponentGlobal => "component-global",
            Self::Entity => "entity",
            Self::EntityComponent => "entity-component",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentApplyRejection {
    ComponentMetadataMissing,
    RequiredComponents,
    LifecycleHook {
        event: PersistentLifecycleEvent,
    },
    Observer {
        event: PersistentLifecycleEvent,
        scope: PersistentObserverScope,
    },
}

impl Display for PersistentApplyRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentMetadataMissing => {
                formatter.write_str("component metadata is unavailable")
            }
            Self::RequiredComponents => {
                formatter.write_str("implicit required components are registered")
            }
            Self::LifecycleHook { event } => {
                write!(
                    formatter,
                    "a {} lifecycle hook is registered",
                    event.as_str()
                )
            }
            Self::Observer { event, scope } => {
                write!(
                    formatter,
                    "a {} {} lifecycle observer is registered",
                    scope.as_str(),
                    event.as_str()
                )
            }
        }
    }
}

pub struct ComponentApplyContext {
    asset_server: AssetServer,
    asset_server_touched: bool,
}

impl ComponentApplyContext {
    fn from_asset_server(asset_server: AssetServer) -> Self {
        Self {
            asset_server,
            asset_server_touched: false,
        }
    }

    pub fn resolve_asset_ref<T>(
        &mut self,
        asset_ref: &AssetRef,
    ) -> Result<Handle<T>, AssetRefError> {
        self.asset_server_touched = true;
        asset_ref.resolve(&mut self.asset_server)
    }
}

trait PreparedComponentValue: Send {
    fn insert(self: Box<Self>, world: &mut World, entity: Entity);
}

impl<T> PreparedComponentValue for T
where
    T: Component,
{
    fn insert(self: Box<Self>, world: &mut World, entity: Entity) {
        world.entity_mut(entity).insert(*self);
    }
}

#[derive(Clone)]
struct PreparedComponentBinding {
    component_id: ComponentTypeId,
    register_component: fn(&mut World) -> ComponentId,
    validate_component: fn(&World, Option<Entity>) -> Result<(), PersistentComponentMetadataError>,
}

struct PersistentComponentApplyReceipt {
    world_id: WorldId,
    component_id: ComponentTypeId,
    runtime_component_id: ComponentId,
    validate_component: fn(&World, Option<Entity>) -> Result<(), PersistentComponentMetadataError>,
}

#[derive(Component, Default)]
struct PersistentApplyReceipts {
    bindings: BTreeMap<ComponentTypeId, PersistentComponentApplyReceipt>,
}

#[derive(Resource, Default)]
struct PersistentWorldBindings {
    by_stable: BTreeMap<ComponentTypeId, ComponentId>,
    by_runtime: BTreeMap<ComponentId, ComponentTypeId>,
}

impl PreparedComponentBinding {
    fn register(&self, world: &mut World) -> ComponentId {
        (self.register_component)(world)
    }

    fn validate(&self, world: &World, target: Option<Entity>) -> Result<(), ComponentCodecError> {
        (self.validate_component)(world, target).map_err(|error| {
            ComponentCodecError::PersistentApplyRejected {
                component_id: self.component_id.clone(),
                reason: map_apply_rejection(error),
            }
        })
    }

    fn receipt(
        &self,
        world_id: WorldId,
        runtime_component_id: ComponentId,
    ) -> PersistentComponentApplyReceipt {
        PersistentComponentApplyReceipt {
            world_id,
            component_id: self.component_id.clone(),
            runtime_component_id,
            validate_component: self.validate_component,
        }
    }
}

impl PersistentComponentApplyReceipt {
    fn validate_existing(&self, world: &World, target: Entity) -> Result<(), ComponentCodecError> {
        if world.id() != self.world_id {
            return Err(ComponentCodecError::WrongWorld);
        }
        let entity = world
            .get_entity(target)
            .map_err(|_| ComponentCodecError::EntityMissing)?;
        if !entity.contains_id(self.runtime_component_id) {
            return Ok(());
        }
        (self.validate_component)(world, Some(target)).map_err(|error| {
            ComponentCodecError::PersistentApplyRejected {
                component_id: self.component_id.clone(),
                reason: map_apply_rejection(error),
            }
        })
    }
}

impl PersistentWorldBindings {
    fn validate(
        &self,
        receipt: &PersistentComponentApplyReceipt,
    ) -> Result<(), ComponentCodecError> {
        if self
            .by_stable
            .get(&receipt.component_id)
            .is_some_and(|component_id| *component_id != receipt.runtime_component_id)
            || self
                .by_runtime
                .get(&receipt.runtime_component_id)
                .is_some_and(|stable_id| stable_id != &receipt.component_id)
        {
            return Err(ComponentCodecError::PersistentApplyBindingConflict {
                component_id: receipt.component_id.clone(),
            });
        }
        Ok(())
    }

    fn record(
        &mut self,
        receipt: &PersistentComponentApplyReceipt,
    ) -> Result<(), ComponentCodecError> {
        self.validate(receipt)?;
        self.by_stable
            .insert(receipt.component_id.clone(), receipt.runtime_component_id);
        self.by_runtime
            .insert(receipt.runtime_component_id, receipt.component_id.clone());
        Ok(())
    }
}

fn validate_persistent_apply_receipts<'a>(
    world: &World,
    receipts: impl IntoIterator<Item = &'a PersistentComponentApplyReceipt>,
) -> Result<(), ComponentCodecError> {
    let existing = world.get_resource::<PersistentWorldBindings>();
    if existing.is_none() && world_has_nonempty_persistent_apply_receipts(world) {
        return Err(ComponentCodecError::PersistentApplyReceiptMissing);
    }
    let mut pending = PersistentWorldBindings::default();
    for receipt in receipts {
        if let Some(existing) = existing {
            existing.validate(receipt)?;
        }
        pending.record(receipt)?;
    }
    Ok(())
}

fn world_has_nonempty_persistent_apply_receipts(world: &World) -> bool {
    let Some(receipt_component) = world.component_id::<PersistentApplyReceipts>() else {
        return false;
    };
    world
        .archetypes()
        .iter()
        .filter(|archetype| archetype.contains(receipt_component))
        .flat_map(|archetype| archetype.entities())
        .any(|entity| {
            world
                .get::<PersistentApplyReceipts>(entity.id())
                .is_some_and(|receipts| !receipts.bindings.is_empty())
        })
}

fn record_persistent_apply_receipts(
    world: &mut World,
    receipts: impl IntoIterator<Item = (Entity, PersistentComponentApplyReceipt)>,
) -> Result<(), ComponentCodecError> {
    let receipts = receipts.into_iter().collect::<Vec<_>>();
    if receipts.is_empty() {
        return Ok(());
    }
    if !world.contains_resource::<PersistentWorldBindings>() {
        world.insert_resource(PersistentWorldBindings::default());
    }
    {
        let mut bindings = world.resource_mut::<PersistentWorldBindings>();
        for (_, receipt) in &receipts {
            bindings.record(receipt)?;
        }
    }
    for (entity, receipt) in receipts {
        let mut entity = world
            .get_entity_mut(entity)
            .map_err(|_| ComponentCodecError::EntityMissing)?;
        if let Some(mut applied) = entity.get_mut::<PersistentApplyReceipts>() {
            applied
                .bindings
                .insert(receipt.component_id.clone(), receipt);
        } else {
            entity.insert(PersistentApplyReceipts {
                bindings: BTreeMap::from([(receipt.component_id.clone(), receipt)]),
            });
        }
    }
    Ok(())
}

pub(crate) fn map_apply_rejection(
    error: PersistentComponentMetadataError,
) -> PersistentApplyRejection {
    match error {
        PersistentComponentMetadataError::ComponentMissing => {
            PersistentApplyRejection::ComponentMetadataMissing
        }
        PersistentComponentMetadataError::RequiredComponents => {
            PersistentApplyRejection::RequiredComponents
        }
        PersistentComponentMetadataError::LifecycleHook(event) => {
            PersistentApplyRejection::LifecycleHook {
                event: lifecycle_event_name(event),
            }
        }
        PersistentComponentMetadataError::Observer { event, scope } => {
            PersistentApplyRejection::Observer {
                event: lifecycle_event_name(event),
                scope: observer_scope_name(scope),
            }
        }
    }
}

fn validate_support_component<T: Component>(
    world: &World,
    target: Option<Entity>,
) -> Result<(), ComponentCodecError> {
    validate_registered_persistent_component_apply::<T>(world, target).map_err(|error| {
        ComponentCodecError::PersistentApplySupportRejected {
            reason: map_apply_rejection(error),
        }
    })
}

fn validate_support_resource_insertion<R: Resource>(
    world: &World,
) -> Result<(), ComponentCodecError> {
    validate_resource_insertion::<R>(world).map_err(|error| {
        ComponentCodecError::PersistentApplySupportRejected {
            reason: map_apply_rejection(error),
        }
    })
}

const fn lifecycle_event_name(event: EcsPersistentLifecycleEvent) -> PersistentLifecycleEvent {
    match event {
        EcsPersistentLifecycleEvent::Add => PersistentLifecycleEvent::Add,
        EcsPersistentLifecycleEvent::Insert => PersistentLifecycleEvent::Insert,
        EcsPersistentLifecycleEvent::Discard => PersistentLifecycleEvent::Discard,
        EcsPersistentLifecycleEvent::Remove => PersistentLifecycleEvent::Remove,
        EcsPersistentLifecycleEvent::Despawn => PersistentLifecycleEvent::Despawn,
    }
}

const fn observer_scope_name(scope: EcsPersistentObserverScope) -> PersistentObserverScope {
    match scope {
        EcsPersistentObserverScope::EventGlobal => PersistentObserverScope::EventGlobal,
        EcsPersistentObserverScope::ComponentGlobal => PersistentObserverScope::ComponentGlobal,
        EcsPersistentObserverScope::Entity => PersistentObserverScope::Entity,
        EcsPersistentObserverScope::EntityComponent => PersistentObserverScope::EntityComponent,
    }
}

type PrepareComponentFn = dyn FnOnce(
        &mut ComponentApplyContext,
    ) -> Result<Box<dyn PreparedComponentValue>, ComponentCodecError>
    + Send
    + 'static;

struct StagedComponent {
    entity: Entity,
    value: Box<dyn PreparedComponentValue>,
    binding: PreparedComponentBinding,
}

/// Stages prepared components and their scratch asset-server state for one world transaction.
///
/// Staging consumes and returns the batch so a failed codec cannot leave a committable partial
/// batch. Commit validates the originating world, every target, and the source asset-server state
/// before publishing components and the scratch asset server together.
#[must_use]
pub struct ComponentApplyBatch {
    world_id: WorldId,
    origin_asset_server: Option<AssetServer>,
    context: ComponentApplyContext,
    components: Vec<StagedComponent>,
}

impl ComponentApplyBatch {
    pub fn from_world(world: &World) -> Self {
        let origin_asset_server = world.get_resource::<AssetServer>().cloned();
        Self {
            world_id: world.id(),
            context: ComponentApplyContext::from_asset_server(
                origin_asset_server.clone().unwrap_or_default(),
            ),
            origin_asset_server,
            components: Vec::new(),
        }
    }

    pub fn with_decode_context<T>(
        &mut self,
        database: Option<&ProjectAssetDatabase>,
        decode: impl FnOnce(&mut ComponentDecodeContext<'_>) -> T,
    ) -> T {
        let mut context = ComponentDecodeContext::with_asset_server(&mut self.context.asset_server);
        if let Some(database) = database {
            context = context.with_project_asset_database(database);
        }
        let result = decode(&mut context);
        self.context.asset_server_touched |= context.asset_server_touched();
        result
    }

    pub fn stage(
        mut self,
        entity: Entity,
        component: PreparedComponent,
    ) -> Result<Self, ComponentCodecError> {
        let PreparedComponent {
            prepare,
            binding,
            may_touch_asset_server: _,
        } = component;
        let value = prepare(&mut self.context)?;
        self.components.push(StagedComponent {
            entity,
            value,
            binding,
        });
        Ok(self)
    }

    pub fn commit(self, world: &mut World) -> Result<(), ComponentCodecError> {
        self.validate_commit(world)?;
        let registered = self
            .components
            .iter()
            .map(|component| component.binding.register(world))
            .collect::<Vec<_>>();
        world.register_component::<PersistentApplyReceipts>();
        let needs_binding_resource =
            !self.components.is_empty() && !world.contains_resource::<PersistentWorldBindings>();
        if needs_binding_resource {
            world.register_component::<PersistentWorldBindings>();
        }
        let needs_asset_server =
            self.context.asset_server_touched && !world.contains_resource::<AssetServer>();
        if needs_asset_server {
            world.register_component::<AssetServer>();
        }
        world.flush();
        self.validate_commit(world)?;
        for component in &self.components {
            component.binding.validate(world, Some(component.entity))?;
        }
        let receipt_targets = self
            .components
            .iter()
            .map(|component| component.entity)
            .collect::<BTreeSet<_>>();
        for target in receipt_targets {
            let entity = world
                .get_entity(target)
                .map_err(|_| ComponentCodecError::EntityMissing)?;
            if !entity.contains::<PersistentApplyReceipts>() {
                validate_support_component::<PersistentApplyReceipts>(world, Some(target))?;
            }
        }
        if needs_binding_resource {
            validate_support_resource_insertion::<PersistentWorldBindings>(world)?;
        }
        if needs_asset_server {
            validate_support_resource_insertion::<AssetServer>(world)?;
        }
        let receipts = self
            .components
            .iter()
            .zip(registered)
            .map(|(component, component_id)| {
                (
                    component.entity,
                    component.binding.receipt(world.id(), component_id),
                )
            })
            .collect::<Vec<_>>();
        validate_persistent_apply_receipts(world, receipts.iter().map(|(_, receipt)| receipt))?;
        self.commit_validated(world);
        record_persistent_apply_receipts(world, receipts)
    }

    pub fn validate_commit(&self, world: &World) -> Result<(), ComponentCodecError> {
        if world.id() != self.world_id {
            return Err(ComponentCodecError::WrongWorld);
        }
        if self
            .components
            .iter()
            .any(|component| world.get_entity(component.entity).is_err())
        {
            return Err(ComponentCodecError::EntityMissing);
        }
        if self.context.asset_server_touched
            && world.get_resource::<AssetServer>() != self.origin_asset_server.as_ref()
        {
            return Err(ComponentCodecError::AssetServerChanged);
        }
        Ok(())
    }

    fn commit_validated(self, world: &mut World) {
        for component in self.components {
            component.value.insert(world, component.entity);
        }
        if self.context.asset_server_touched {
            if let Some(mut current) = world.get_resource_mut::<AssetServer>() {
                *current = self.context.asset_server;
            } else {
                world.insert_resource(self.context.asset_server);
            }
        }
    }
}

/// A codec-produced component value that has not yet been bound to a persistent schema identity.
///
/// Candidates can only become applicable through a frozen [`crate::ComponentRegistry`].
pub struct PreparedComponentCandidate {
    prepare: Box<PrepareComponentFn>,
    rust_type_id: TypeId,
    rust_type_path: &'static str,
    may_touch_asset_server: bool,
}

impl PreparedComponentCandidate {
    /// Defers fallible component preparation that cannot access target-World resources.
    pub fn deferred<T>(
        prepare: impl FnOnce() -> Result<T, ComponentCodecError> + Send + 'static,
    ) -> Self
    where
        T: Component,
    {
        Self {
            prepare: Box::new(move |_context| {
                prepare().map(|component| Box::new(component) as Box<_>)
            }),
            rust_type_id: TypeId::of::<T>(),
            rust_type_path: std::any::type_name::<T>(),
            may_touch_asset_server: false,
        }
    }

    /// Defers component preparation that may resolve an asset through the apply context.
    pub fn with_asset_server<T>(
        prepare: impl FnOnce(&mut ComponentApplyContext) -> Result<T, ComponentCodecError>
        + Send
        + 'static,
    ) -> Self
    where
        T: Component,
    {
        Self {
            prepare: Box::new(move |context| {
                prepare(context).map(|component| Box::new(component) as Box<_>)
            }),
            rust_type_id: TypeId::of::<T>(),
            rust_type_path: std::any::type_name::<T>(),
            may_touch_asset_server: true,
        }
    }

    pub fn insert<T>(component: T) -> Self
    where
        T: Component,
    {
        Self {
            prepare: Box::new(move |_context| {
                Ok(Box::new(component) as Box<dyn PreparedComponentValue>)
            }),
            rust_type_id: TypeId::of::<T>(),
            rust_type_path: std::any::type_name::<T>(),
            may_touch_asset_server: false,
        }
    }

    pub(crate) fn bind(
        self,
        component_id: ComponentTypeId,
        rust_type_id: TypeId,
        rust_type_path: &'static str,
        register_component: fn(&mut World) -> ComponentId,
        validate_component: fn(
            &World,
            Option<Entity>,
        ) -> Result<(), PersistentComponentMetadataError>,
    ) -> Result<PreparedComponent, ComponentCodecError> {
        if self.rust_type_id != rust_type_id {
            return Err(ComponentCodecError::PreparedComponentTypeMismatch {
                expected: rust_type_path,
                actual: self.rust_type_path,
            });
        }
        Ok(PreparedComponent {
            prepare: self.prepare,
            binding: PreparedComponentBinding {
                component_id,
                register_component,
                validate_component,
            },
            may_touch_asset_server: self.may_touch_asset_server,
        })
    }
}

/// A registry-bound persistent component ready for guarded target-World application.
pub struct PreparedComponent {
    prepare: Box<PrepareComponentFn>,
    binding: PreparedComponentBinding,
    may_touch_asset_server: bool,
}

impl PreparedComponent {
    pub fn apply(self, world: &mut World, entity: Entity) -> Result<(), ComponentCodecError> {
        ComponentApplyBatch::from_world(world)
            .stage(entity, self)?
            .commit(world)
    }
}

#[doc(hidden)]
pub fn validate_persistent_apply_support_topology(
    world: &mut World,
    batch: &ComponentApplyBatch,
    has_targets: bool,
    has_persistent_components: bool,
) -> Result<(), ComponentCodecError> {
    let receipt_component =
        has_targets.then(|| world.register_component::<PersistentApplyReceipts>());
    let needs_binding_resource =
        has_persistent_components && !world.contains_resource::<PersistentWorldBindings>();
    if needs_binding_resource {
        world.register_component::<PersistentWorldBindings>();
    }
    let needs_asset_server =
        batch.context.asset_server_touched && !world.contains_resource::<AssetServer>();
    if needs_asset_server {
        world.register_component::<AssetServer>();
    }
    world.flush();
    if receipt_component.is_some() {
        validate_support_component::<PersistentApplyReceipts>(world, None)?;
    }
    if needs_binding_resource {
        validate_support_resource_insertion::<PersistentWorldBindings>(world)?;
    }
    if needs_asset_server {
        validate_support_resource_insertion::<AssetServer>(world)?;
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_fresh_persistent_component_apply<'a>(
    world: &mut World,
    components: impl IntoIterator<Item = &'a PreparedComponent>,
) -> Result<(), ComponentCodecError> {
    let mut bindings = BTreeMap::<ComponentTypeId, &PreparedComponentBinding>::new();
    let mut may_touch_asset_server = false;
    for component in components {
        may_touch_asset_server |= component.may_touch_asset_server;
        let binding = &component.binding;
        bindings
            .entry(binding.component_id.clone())
            .or_insert(binding);
    }

    let registered = bindings
        .iter()
        .map(|(id, binding)| (id.clone(), binding.register(world)))
        .collect::<BTreeMap<_, _>>();
    let needs_asset_server = may_touch_asset_server && !world.contains_resource::<AssetServer>();
    if needs_asset_server {
        world.register_component::<AssetServer>();
    }
    world.flush();
    if needs_asset_server {
        validate_support_resource_insertion::<AssetServer>(world)?;
    }
    let receipts = bindings
        .iter()
        .map(|(id, binding)| binding.receipt(world.id(), registered[id]))
        .collect::<Vec<_>>();
    validate_persistent_apply_receipts(world, &receipts)?;
    for binding in bindings.values() {
        binding.validate(world, None)?;
    }
    Ok(())
}

#[doc(hidden)]
pub fn declare_persistent_apply_targets(
    world: &mut World,
    targets: impl IntoIterator<Item = Entity>,
) -> Result<(), ComponentCodecError> {
    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    world.register_component::<PersistentApplyReceipts>();
    world.flush();
    for target in &targets {
        let entity = world
            .get_entity(*target)
            .map_err(|_| ComponentCodecError::EntityMissing)?;
        if !entity.contains::<PersistentApplyReceipts>()
            && !entity.archetype().components().is_empty()
        {
            return Err(ComponentCodecError::PersistentApplyTargetNotEmpty);
        }
        if !entity.contains::<PersistentApplyReceipts>() {
            validate_support_component::<PersistentApplyReceipts>(world, Some(*target))?;
        }
    }
    for target in targets {
        let mut entity = world
            .get_entity_mut(target)
            .map_err(|_| ComponentCodecError::EntityMissing)?;
        if !entity.contains::<PersistentApplyReceipts>() {
            entity.insert(PersistentApplyReceipts::default());
        }
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_declared_persistent_apply_targets(
    world: &mut World,
    targets: impl IntoIterator<Item = Entity>,
) -> Result<(), ComponentCodecError> {
    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    world.register_component::<PersistentApplyReceipts>();
    world.flush();
    let bindings = world.get_resource::<PersistentWorldBindings>();
    for target in targets {
        let entity = world
            .get_entity(target)
            .map_err(|_| ComponentCodecError::EntityMissing)?;
        validate_support_component::<PersistentApplyReceipts>(world, Some(target))?;
        let receipts = entity
            .get::<PersistentApplyReceipts>()
            .ok_or(ComponentCodecError::PersistentApplyReceiptMissing)?;
        if !receipts.bindings.is_empty() {
            let bindings = bindings.ok_or(ComponentCodecError::PersistentApplyReceiptMissing)?;
            for receipt in receipts.bindings.values() {
                bindings.validate(receipt)?;
                receipt.validate_existing(world, target)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod persistent_apply_contract_tests {
    use std::{
        any::TypeId,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use nara_ecs::{
        Component, Resource, World,
        lifecycle::{Add, Discard},
        observer::{Observer, On},
        system::ResMut,
    };

    use super::*;

    static SUPPORT_OBSERVER_RUNS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Component)]
    struct DirectProbe;

    #[derive(Component)]
    struct OtherProbe;

    #[derive(Component, Debug, PartialEq, Eq)]
    struct ReplaceProbe(u32);

    struct SupportAsset;

    #[derive(Resource, Default)]
    struct SupportCanary(usize);

    fn register_test_component<T: Component>(world: &mut World) -> ComponentId {
        world.register_component::<T>()
    }

    fn bound_component<T: Component>(component: T, stable_id: &str) -> PreparedComponent {
        PreparedComponentCandidate::insert(component)
            .bind(
                ComponentTypeId::new(stable_id),
                TypeId::of::<T>(),
                std::any::type_name::<T>(),
                register_test_component::<T>,
                validate_registered_persistent_component_apply::<T>,
            )
            .unwrap()
    }

    fn direct_probe() -> PreparedComponent {
        bound_component(DirectProbe, "nara.test.DirectProbe")
    }

    #[test]
    fn direct_apply_rejects_target_receipt_observer_before_component_mutation() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let target = world.spawn_empty().id();
        let receipt_component = world.register_component::<PersistentApplyReceipts>();
        world.spawn(
            Observer::new(|_: On<Add>, mut canary: ResMut<SupportCanary>| canary.0 += 1)
                .with_entity(target)
                .with_component(receipt_component),
        );
        world.flush();

        let error = direct_probe().apply(&mut world, target).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplySupportRejected {
                reason: PersistentApplyRejection::Observer {
                    event: PersistentLifecycleEvent::Add,
                    scope: PersistentObserverScope::EntityComponent,
                },
            }
        ));
        assert!(world.get::<DirectProbe>(target).is_none());
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn direct_apply_rejects_first_binding_resource_observer_before_publication() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let target = world.spawn_empty().id();
        let binding_component = world.register_component::<PersistentWorldBindings>();
        world.spawn(
            Observer::new(|_: On<Add>| {
                SUPPORT_OBSERVER_RUNS.fetch_add(1, Ordering::Relaxed);
            })
            .with_component(binding_component),
        );
        world.flush();
        SUPPORT_OBSERVER_RUNS.store(0, Ordering::Relaxed);

        let error = direct_probe().apply(&mut world, target).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplySupportRejected {
                reason: PersistentApplyRejection::Observer {
                    event: PersistentLifecycleEvent::Add,
                    scope: PersistentObserverScope::ComponentGlobal,
                },
            }
        ));
        assert!(world.get::<DirectProbe>(target).is_none());
        assert!(!world.contains_resource::<PersistentWorldBindings>());
        assert_eq!(SUPPORT_OBSERVER_RUNS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn batch_rejects_both_directions_of_persistent_binding_conflict() {
        let mut stable_conflict = World::new();
        let target = stable_conflict.spawn_empty().id();
        let error = ComponentApplyBatch::from_world(&stable_conflict)
            .stage(
                target,
                bound_component(DirectProbe, "nara.test.SharedStable"),
            )
            .unwrap()
            .stage(
                target,
                bound_component(OtherProbe, "nara.test.SharedStable"),
            )
            .unwrap()
            .commit(&mut stable_conflict)
            .unwrap_err();
        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyBindingConflict { .. }
        ));
        assert!(stable_conflict.get::<DirectProbe>(target).is_none());
        assert!(stable_conflict.get::<OtherProbe>(target).is_none());
        assert!(!stable_conflict.contains_resource::<PersistentWorldBindings>());

        let mut runtime_conflict = World::new();
        let target = runtime_conflict.spawn_empty().id();
        let error = ComponentApplyBatch::from_world(&runtime_conflict)
            .stage(
                target,
                bound_component(DirectProbe, "nara.test.FirstStable"),
            )
            .unwrap()
            .stage(
                target,
                bound_component(DirectProbe, "nara.test.SecondStable"),
            )
            .unwrap()
            .commit(&mut runtime_conflict)
            .unwrap_err();
        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyBindingConflict { .. }
        ));
        assert!(runtime_conflict.get::<DirectProbe>(target).is_none());
        assert!(!runtime_conflict.contains_resource::<PersistentWorldBindings>());
    }

    #[test]
    fn world_binding_survives_target_despawn_and_blocks_temporal_rebinding() {
        let mut stable_conflict = World::new();
        let first = stable_conflict.spawn_empty().id();
        bound_component(DirectProbe, "nara.test.TemporalStable")
            .apply(&mut stable_conflict, first)
            .unwrap();
        stable_conflict.despawn(first);
        assert!(
            stable_conflict
                .iter_entities()
                .all(|entity| { entity.get::<PersistentApplyReceipts>().is_none() })
        );
        let second = stable_conflict.spawn_empty().id();
        let error = bound_component(OtherProbe, "nara.test.TemporalStable")
            .apply(&mut stable_conflict, second)
            .unwrap_err();
        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyBindingConflict { .. }
        ));
        assert!(stable_conflict.get::<OtherProbe>(second).is_none());

        let mut runtime_conflict = World::new();
        let first = runtime_conflict.spawn_empty().id();
        bound_component(DirectProbe, "nara.test.TemporalFirst")
            .apply(&mut runtime_conflict, first)
            .unwrap();
        let second = runtime_conflict.spawn_empty().id();
        let error = bound_component(DirectProbe, "nara.test.TemporalSecond")
            .apply(&mut runtime_conflict, second)
            .unwrap_err();
        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyBindingConflict { .. }
        ));
        assert!(runtime_conflict.get::<DirectProbe>(second).is_none());
    }

    #[test]
    fn replacement_rejects_discard_observer_before_overwriting_the_component() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let target = world.spawn_empty().id();
        bound_component(ReplaceProbe(1), "nara.test.ReplaceProbe")
            .apply(&mut world, target)
            .unwrap();
        world.add_observer(
            |_: On<Discard, ReplaceProbe>, mut canary: ResMut<SupportCanary>| canary.0 += 1,
        );
        world.flush();

        let error = bound_component(ReplaceProbe(2), "nara.test.ReplaceProbe")
            .apply(&mut world, target)
            .unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyRejected {
                reason: PersistentApplyRejection::Observer {
                    event: PersistentLifecycleEvent::Discard,
                    scope: PersistentObserverScope::ComponentGlobal,
                },
                ..
            }
        ));
        assert_eq!(world.get::<ReplaceProbe>(target), Some(&ReplaceProbe(1)));
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn declared_target_validation_covers_receipt_marker_retirement() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        let target = world.spawn_empty().id();
        direct_probe().apply(&mut world, target).unwrap();
        let receipt_component = world.component_id::<PersistentApplyReceipts>().unwrap();
        world.spawn(
            Observer::new(
                |_: On<nara_ecs::lifecycle::Despawn>, mut canary: ResMut<SupportCanary>| {
                    canary.0 += 1;
                },
            )
            .with_entity(target)
            .with_component(receipt_component),
        );
        world.flush();

        let error = validate_declared_persistent_apply_targets(&mut world, [target]).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplySupportRejected {
                reason: PersistentApplyRejection::Observer {
                    event: PersistentLifecycleEvent::Despawn,
                    scope: PersistentObserverScope::EntityComponent,
                },
            }
        ));
        assert!(world.get_entity(target).is_ok());
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn direct_apply_rejects_first_asset_server_observer_before_component_mutation() {
        let mut world = World::new();
        world.init_resource::<SupportCanary>();
        world.add_observer(
            |_: On<Add, AssetServer>, mut canary: ResMut<SupportCanary>| canary.0 += 1,
        );
        world.flush();
        let target = world.spawn_empty().id();
        let candidate = PreparedComponentCandidate::with_asset_server(|context| {
            let asset_ref = AssetRef::path("textures/probe.png").unwrap();
            let _ = context
                .resolve_asset_ref::<SupportAsset>(&asset_ref)
                .map_err(|_| ComponentCodecError::invalid_field("asset", "resolvable asset ref"))?;
            Ok(DirectProbe)
        });
        let prepared = candidate
            .bind(
                ComponentTypeId::new("nara.test.AssetTouchProbe"),
                TypeId::of::<DirectProbe>(),
                std::any::type_name::<DirectProbe>(),
                register_test_component::<DirectProbe>,
                validate_registered_persistent_component_apply::<DirectProbe>,
            )
            .unwrap();

        let error = prepared.apply(&mut world, target).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplySupportRejected {
                reason: PersistentApplyRejection::Observer {
                    event: PersistentLifecycleEvent::Add,
                    scope: PersistentObserverScope::ComponentGlobal,
                },
            }
        ));
        assert!(world.get::<DirectProbe>(target).is_none());
        assert!(!world.contains_resource::<AssetServer>());
        assert_eq!(world.resource::<SupportCanary>().0, 0);
    }

    #[test]
    fn missing_world_binding_authority_rejects_later_persistent_apply() {
        let mut world = World::new();
        let first = world.spawn_empty().id();
        direct_probe().apply(&mut world, first).unwrap();
        assert!(world.remove_resource::<PersistentWorldBindings>().is_some());
        let second = world.spawn_empty().id();

        let error = direct_probe().apply(&mut world, second).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyReceiptMissing
        ));
        assert!(world.get::<DirectProbe>(second).is_none());
        assert!(!world.contains_resource::<PersistentWorldBindings>());
    }

    #[test]
    fn declaring_a_nonempty_unowned_target_rejects_without_rewriting_it() {
        let mut world = World::new();
        let target = world.spawn(OtherProbe).id();

        let error = declare_persistent_apply_targets(&mut world, [target]).unwrap_err();

        assert!(matches!(
            error,
            ComponentCodecError::PersistentApplyTargetNotEmpty
        ));
        assert!(world.get::<OtherProbe>(target).is_some());
        assert!(world.get::<PersistentApplyReceipts>(target).is_none());
    }
}
