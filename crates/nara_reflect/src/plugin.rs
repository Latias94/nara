use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginDomainError, PluginError, PluginId, PluginPreflightContext, PluginPreflightResource,
    RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter, RuntimeGeneration,
};
use nara_ecs::{Resource, World, bevy_ecs::lifecycle::HookContext, bevy_ecs::world::DeferredWorld};

use crate::{
    ComponentRegistryBuilder, ComponentRegistryError, ComponentRegistrySnapshot,
    ComponentSchemaProviderDefinition,
};

pub const COMPONENT_REGISTRY_PLUGIN_ID: PluginId = PluginId::new("nara.reflect.registry");
pub const COMPONENT_REGISTRY_PLUGIN_REQUIREMENT: &[PluginId] = &[COMPONENT_REGISTRY_PLUGIN_ID];
pub const COMPONENT_REGISTRY_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMPONENT_REGISTRY_PLUGIN_ID, PluginCategory::Core);

const PRELOADED_COMPONENT_REGISTRY_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.reflect.registry.preloaded", 1);

#[derive(Debug, Default, Clone, Copy)]
pub struct ComponentRegistryPlugin;

struct PreloadedComponentRegistryPlugin {
    snapshot: ComponentRegistrySnapshot,
}

/// Creates the file-backed registry definition used by a resolved project plan.
///
/// The cell is plan-local and must be initialized before the returned plugin definition is
/// prepared. Keeping this plumbing here lets the product Host reuse the normal registry owner
/// without adding a reflection-specific initializer to `nara_app`.
#[doc(hidden)]
pub fn preloaded_component_registry_plugin(
    snapshot: Arc<OnceLock<ComponentRegistrySnapshot>>,
) -> PluginDefinition {
    PluginDefinition::fallible::<PreloadedComponentRegistryPlugin, _>(
        PRELOADED_COMPONENT_REGISTRY_DEFINITION_ID,
        b"component-registry-preloaded-v1",
        move || {
            snapshot
                .get()
                .cloned()
                .map(|snapshot| PreloadedComponentRegistryPlugin { snapshot })
                .ok_or_else(|| {
                    PluginDomainError::new(
                        "component-registry-snapshot-missing",
                        "The component registry snapshot is unavailable",
                    )
                    .expect("registry preparation uses safe static failure facts")
                })
        },
    )
}

#[derive(Resource)]
#[component(
    on_insert = record_component_registry_structure_change,
    on_discard = record_component_registry_structure_change
)]
struct ComponentRegistryResource {
    instance_token: Arc<()>,
    state: ComponentRegistryState,
}

enum ComponentRegistryState {
    Building(Option<ComponentRegistryBuilder>),
    Preloaded(ComponentRegistrySnapshot),
    Admitted(ComponentRegistrySnapshot),
}

impl Default for ComponentRegistryResource {
    fn default() -> Self {
        Self {
            instance_token: Arc::new(()),
            state: ComponentRegistryState::Building(Some(ComponentRegistryBuilder::new())),
        }
    }
}

impl ComponentRegistryResource {
    fn preloaded(snapshot: ComponentRegistrySnapshot) -> Self {
        Self {
            instance_token: Arc::new(()),
            state: ComponentRegistryState::Preloaded(snapshot),
        }
    }

    fn admitted_snapshot(&self) -> Option<&ComponentRegistrySnapshot> {
        match &self.state {
            ComponentRegistryState::Admitted(snapshot) => Some(snapshot),
            ComponentRegistryState::Building(_) | ComponentRegistryState::Preloaded(_) => None,
        }
    }

    fn shares_snapshot(&self, expected: &ComponentRegistrySnapshot) -> bool {
        match &self.state {
            ComponentRegistryState::Preloaded(snapshot)
            | ComponentRegistryState::Admitted(snapshot) => snapshot.ptr_eq(expected),
            ComponentRegistryState::Building(_) => false,
        }
    }

    fn validate_schema_provider(
        &self,
        provider: &ComponentSchemaProviderDefinition,
    ) -> Result<(), ComponentRegistryError> {
        match &self.state {
            ComponentRegistryState::Building(builder) => {
                provider.preflight(builder.as_ref().expect("registry admission is synchronous"))
            }
            ComponentRegistryState::Preloaded(snapshot)
            | ComponentRegistryState::Admitted(snapshot) => provider.validate_snapshot(snapshot),
        }
    }

    fn register_schema_provider(
        &mut self,
        provider: &ComponentSchemaProviderDefinition,
    ) -> Result<(), ComponentRegistryError> {
        match &mut self.state {
            ComponentRegistryState::Building(builder) => {
                provider.register_into(builder.as_mut().expect("registry admission is synchronous"))
            }
            ComponentRegistryState::Preloaded(snapshot)
            | ComponentRegistryState::Admitted(snapshot) => provider.validate_snapshot(snapshot),
        }
    }
}

impl PluginPreflightResource for ComponentRegistryResource {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRegistryAuthorityError {
    MissingAuthority,
    MissingRegistry,
    Replaced,
}

impl fmt::Display for ComponentRegistryAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingAuthority => "component registry authority guard is missing",
            Self::MissingRegistry => "component registry resource is missing",
            Self::Replaced => {
                "component registry resource no longer matches its authority snapshot"
            }
        })
    }
}

impl Error for ComponentRegistryAuthorityError {}

#[derive(Debug, Default, Resource)]
struct ComponentRegistryMutationRevision(u64);

#[derive(Resource)]
struct ComponentRegistryBuildSeed {
    instance_token: Arc<()>,
    mutation_revision: u64,
}

pub(crate) fn record_component_registry_structure_change(
    mut world: DeferredWorld<'_>,
    _context: HookContext,
) {
    let Some(mut revision) = world.get_resource_mut::<ComponentRegistryMutationRevision>() else {
        return;
    };
    revision.0 = revision
        .0
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

struct ComponentRegistryAuthorityState {
    snapshot: ComponentRegistrySnapshot,
    instance_token: Arc<()>,
    mutation_revision: u64,
    reporter: RuntimeFaultReporter,
    generation: OnceLock<RuntimeGeneration>,
}

#[derive(Resource)]
struct ComponentRegistryAuthority(Arc<ComponentRegistryAuthorityState>);

impl ComponentRegistryAuthorityState {
    fn fault(&self) -> RuntimeFault {
        RuntimeFault::engine(
            RuntimeFaultKind::RuntimeAuthority,
            "nara.reflect.component-registry-authority",
        )
    }

    fn validate_registry(&self, world: &World) -> Result<(), ComponentRegistryAuthorityError> {
        let registry = world
            .get_resource::<ComponentRegistryResource>()
            .ok_or(ComponentRegistryAuthorityError::MissingRegistry)?;
        let revision = world
            .get_resource::<ComponentRegistryMutationRevision>()
            .ok_or(ComponentRegistryAuthorityError::Replaced)?;
        if registry
            .admitted_snapshot()
            .is_some_and(|snapshot| snapshot.ptr_eq(&self.snapshot))
            && Arc::ptr_eq(&registry.instance_token, &self.instance_token)
            && revision.0 == self.mutation_revision
        {
            Ok(())
        } else {
            Err(ComponentRegistryAuthorityError::Replaced)
        }
    }

    fn validate_runtime(
        &self,
        world: &World,
        generation: Option<RuntimeGeneration>,
    ) -> Result<(), RuntimeFault> {
        if let Some(fault) = self.reporter.fault() {
            return Err(fault);
        }
        let authority_matches = world
            .get_resource::<ComponentRegistryAuthority>()
            .is_some_and(|authority| std::ptr::eq(Arc::as_ptr(&authority.0), self));
        let generation_matches = generation.is_none_or(|generation| {
            world.get_resource::<RuntimeGeneration>() == Some(&generation)
                && *self.generation.get_or_init(|| generation) == generation
        });
        if generation_matches && authority_matches && self.validate_registry(world).is_ok() {
            return Ok(());
        }
        let fault = self.fault();
        self.reporter.report(fault.clone());
        Err(fault)
    }
}

/// Preflights one schema provider against the private App registry owner.
///
/// Building owners check source and contribution collisions without running provider callbacks.
/// Preloaded and admitted owners compare receipts with their exact retained snapshot.
pub fn validate_schema_provider_for_plugin(
    context: &PluginPreflightContext<'_>,
    plugin: PluginId,
    component: &str,
    provider: &ComponentSchemaProviderDefinition,
) -> Result<(), PluginError> {
    let resource = context
        .get_structural_resource::<ComponentRegistryResource>()
        .ok_or_else(|| {
            PluginError::component_registration(
                plugin,
                component,
                "component registry resource is unavailable",
            )
        })?;
    resource
        .validate_schema_provider(provider)
        .map_err(|error| PluginError::component_registration(plugin, component, error))
}

/// Returns the admitted immutable component registry owned by this App or runtime World.
///
/// Building and unfinished preloaded owners expose no snapshot. Schema-owning plugins use
/// [`register_schema_provider_for_plugin`]; runtime consumers receive only this admitted view.
/// Snapshot availability alone does not establish valid runtime authority.
#[must_use]
pub fn component_registry(world: &World) -> Option<&ComponentRegistrySnapshot> {
    world
        .get_resource::<ComponentRegistryResource>()
        .and_then(ComponentRegistryResource::admitted_snapshot)
}

/// Returns whether this World has a component registry owner installation.
///
/// Presence includes Building and unfinished Preloaded owners and conveys no runtime authority.
/// Only a World without an owner may use standalone Scene operations; an installed owner must
/// complete admission and pass [`validate_component_registry_authority`].
#[must_use]
pub fn component_registry_is_installed(world: &World) -> bool {
    world.contains_resource::<ComponentRegistryResource>()
}

/// Registers or validates one schema provider against the registry owned by `app`.
///
/// The private owner resource keeps mutation scoped to this operation. Preloaded and admitted
/// owners accept only exact compatible receipts without replaying provider callbacks.
/// `plugin` and `component` identify failures in diagnostics; durable ownership comes from the
/// provider receipt and registration may be delegated by a recipe.
pub fn register_schema_provider_for_plugin(
    app: &mut App,
    plugin: PluginId,
    component: &str,
    provider: &ComponentSchemaProviderDefinition,
) -> Result<(), PluginError> {
    let mut resource = app
        .world_mut()?
        .get_resource_mut::<ComponentRegistryResource>()
        .ok_or_else(|| {
            PluginError::component_registration(
                plugin,
                component,
                "component registry resource is unavailable",
            )
        })?;
    resource
        .register_schema_provider(provider)
        .map_err(|error| PluginError::component_registration(plugin, component, error))
}

impl Plugin for ComponentRegistryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMPONENT_REGISTRY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        build_component_registry(app, None)
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        finish_component_registry(app, None)
    }
}

impl Plugin for PreloadedComponentRegistryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMPONENT_REGISTRY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        build_component_registry(app, Some(&self.snapshot))
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        finish_component_registry(app, Some(&self.snapshot))
    }
}

fn build_component_registry(
    app: &mut App,
    snapshot: Option<&ComponentRegistrySnapshot>,
) -> Result<(), PluginError> {
    app.init_resource::<ComponentRegistryMutationRevision>()?;
    if let Some(snapshot) = snapshot {
        let existing_matches = app
            .world_mut()?
            .get_resource::<ComponentRegistryResource>()
            .map(|existing| existing.shares_snapshot(snapshot));
        match existing_matches {
            Some(true) => {}
            Some(false) => {
                return Err(PluginError::component_registration(
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    "component-schema-catalog",
                    "preloaded component registry does not match the admitted snapshot",
                ));
            }
            None => {
                app.insert_resource(ComponentRegistryResource::preloaded(snapshot.clone()))?;
            }
        }
    } else {
        app.init_resource::<ComponentRegistryResource>()?;
    }

    let world = app.world_mut()?;
    if world.contains_resource::<ComponentRegistryBuildSeed>() {
        return Err(component_registry_authority_changed(
            "component registry build seed was already installed",
        ));
    }
    let seed = {
        let registry = world
            .get_resource::<ComponentRegistryResource>()
            .ok_or_else(|| component_registry_authority_changed("component registry is missing"))?;
        let revision = world
            .get_resource::<ComponentRegistryMutationRevision>()
            .ok_or_else(|| {
                component_registry_authority_changed(
                    "component registry mutation revision is missing",
                )
            })?;
        ComponentRegistryBuildSeed {
            instance_token: Arc::clone(&registry.instance_token),
            mutation_revision: revision.0,
        }
    };
    world.insert_resource(seed);
    Ok(())
}

fn finish_component_registry(
    app: &mut App,
    expected: Option<&ComponentRegistrySnapshot>,
) -> Result<(), PluginError> {
    let declared_providers = app
        .installed_plugins()
        .flat_map(|declaration| declaration.provides_schema.iter().copied())
        .collect::<BTreeSet<_>>();
    let (snapshot, instance_token) = {
        let world = app.world_mut()?;
        let seed = world
            .remove_resource::<ComponentRegistryBuildSeed>()
            .ok_or_else(|| {
                component_registry_authority_changed("component registry build seed is missing")
            })?;
        let mutation_revision = world
            .get_resource::<ComponentRegistryMutationRevision>()
            .ok_or_else(|| {
                component_registry_authority_changed(
                    "component registry mutation revision is missing",
                )
            })?
            .0;
        let mut registry = world
            .get_resource_mut::<ComponentRegistryResource>()
            .ok_or_else(|| component_registry_authority_changed("component registry is missing"))?;
        if !Arc::ptr_eq(&registry.instance_token, &seed.instance_token)
            || mutation_revision != seed.mutation_revision
        {
            return Err(component_registry_authority_changed(
                "component registry authority changed after installation",
            ));
        }
        if let Some(expected) = expected
            && !registry.shares_snapshot(expected)
        {
            return Err(component_registry_authority_changed(
                "component registry finished with a different admitted snapshot",
            ));
        }
        let registration_error = |error| {
            PluginError::component_registration(
                COMPONENT_REGISTRY_PLUGIN_ID,
                "component-schema-catalog",
                error,
            )
        };
        let snapshot = match &mut registry.state {
            ComponentRegistryState::Building(builder) => {
                let candidate = builder.as_ref().expect("registry admission is synchronous");
                candidate
                    .validate_declared_providers(&declared_providers)
                    .map_err(registration_error)?;
                let candidate = builder.take().expect("registry admission is synchronous");
                match candidate.freeze() {
                    Ok(snapshot) => snapshot,
                    Err(failure) => {
                        let (candidate, error) = failure.into_parts();
                        *builder = Some(candidate);
                        return Err(registration_error(error));
                    }
                }
            }
            ComponentRegistryState::Preloaded(snapshot) => {
                snapshot
                    .validate_declared_providers(&declared_providers)
                    .map_err(registration_error)?;
                snapshot.clone()
            }
            ComponentRegistryState::Admitted(_) => {
                return Err(component_registry_authority_changed(
                    "component registry was already admitted",
                ));
            }
        };
        registry.state = ComponentRegistryState::Admitted(snapshot.clone());
        (snapshot, Arc::clone(&registry.instance_token))
    };
    let world = app.world_mut()?;
    let mutation_revision = world.resource::<ComponentRegistryMutationRevision>().0;
    let reporter = world.resource::<RuntimeFaultReporter>().clone();
    let authority = Arc::new(ComponentRegistryAuthorityState {
        snapshot,
        instance_token,
        mutation_revision,
        reporter,
        generation: OnceLock::new(),
    });
    app.insert_resource(ComponentRegistryAuthority(Arc::clone(&authority)))?;
    app.__install_runtime_authority_validator(
        COMPONENT_REGISTRY_PLUGIN_ID,
        Arc::new(move |world, generation| authority.validate_runtime(world, generation)),
    )?;
    Ok(())
}

fn component_registry_authority_changed(message: &'static str) -> PluginError {
    PluginError::component_registration(
        COMPONENT_REGISTRY_PLUGIN_ID,
        "component-schema-catalog",
        message,
    )
}

pub fn validate_component_registry_authority(
    world: &World,
) -> Result<(), ComponentRegistryAuthorityError> {
    let authority = world
        .get_resource::<ComponentRegistryAuthority>()
        .ok_or(ComponentRegistryAuthorityError::MissingAuthority)?;
    authority.0.validate_registry(world)
}

pub fn report_component_registry_authority_fault(
    world: &World,
) -> Result<(), ComponentRegistryAuthorityError> {
    let result = validate_component_registry_authority(world);
    if result.is_err()
        && let Some(authority) = world.get_resource::<ComponentRegistryAuthority>()
    {
        authority.0.reporter.report(authority.0.fault());
    }
    result
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        time::Duration,
    };

    use nara_app::{CoreStage, FrameExecutionStart, PluginSchemaProviderId, RealTime, VirtualTime};
    use nara_ecs::{Component, schedule::ScheduleLabel};

    use crate::{
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    };

    use super::*;

    const REGISTRY_MUTATION_PLUGIN_ID: PluginId =
        PluginId::new("nara.test.reflect.registry-mutation");
    const REGISTRY_MUTATION_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(REGISTRY_MUTATION_PLUGIN_ID, PluginCategory::Runtime);
    const REGISTRY_PROVIDER_PLUGIN_ID: PluginId =
        PluginId::new("nara.test.reflect.registry-provider");
    const REGISTRY_PROVIDER_ID: PluginSchemaProviderId =
        PluginSchemaProviderId::new("nara.test.reflect.registry-provider.components");
    const REGISTRY_PROVIDER_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("nara.test.reflect.registry-provider.components");
    const REGISTRY_PROVIDER_BINDING: ComponentSchemaProviderBindingId =
        ComponentSchemaProviderBindingId::new("nara.test.reflect.registry-provider.native", 1);
    const REGISTRY_PROVIDER_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(REGISTRY_PROVIDER_PLUGIN_ID, PluginCategory::Core)
            .requires_plugins(COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
            .provides_schema(&[REGISTRY_PROVIDER_ID]);
    const REGISTRY_PROVIDER: ComponentSchemaProviderDefinition =
        ComponentSchemaProviderDefinition::new(
            REGISTRY_PROVIDER_OWNER,
            REGISTRY_PROVIDER_ID,
            REGISTRY_PROVIDER_BINDING,
            empty_provider_catalog,
            register_empty_provider,
        );

    struct RegistryProviderPlugin {
        provider: ComponentSchemaProviderDefinition,
    }

    impl Plugin for RegistryProviderPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &REGISTRY_PROVIDER_DECLARATION
        }

        fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
            validate_schema_provider_for_plugin(
                context,
                REGISTRY_PROVIDER_PLUGIN_ID,
                "test-provider",
                &self.provider,
            )
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            register_schema_provider_for_plugin(
                app,
                REGISTRY_PROVIDER_PLUGIN_ID,
                "test-provider",
                &self.provider,
            )
        }

        fn finish(&self, app: &mut App) -> Result<(), PluginError> {
            register_schema_provider_for_plugin(
                app,
                REGISTRY_PROVIDER_PLUGIN_ID,
                "test-provider",
                &self.provider,
            )
        }
    }

    fn empty_provider_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
    {
        Ok(ComponentSchemaCatalog::default())
    }

    fn register_empty_provider(
        _: &mut ComponentRegistryBuilder,
    ) -> Result<(), ComponentRegistryError> {
        Ok(())
    }

    fn panic_provider_validation(
        _: &ComponentRegistryBuilder,
    ) -> Result<(), ComponentRegistryError> {
        panic!("preloaded provider validation callback must not run");
    }

    fn panic_provider_registration(
        _: &mut ComponentRegistryBuilder,
    ) -> Result<(), ComponentRegistryError> {
        panic!("preloaded provider registration callback must not run");
    }

    fn receipt_only_provider(
        binding: ComponentSchemaProviderBindingId,
    ) -> ComponentSchemaProviderDefinition {
        ComponentSchemaProviderDefinition::with_validation(
            REGISTRY_PROVIDER_OWNER,
            REGISTRY_PROVIDER_ID,
            binding,
            empty_provider_catalog,
            panic_provider_validation,
            panic_provider_registration,
        )
    }

    fn provider_snapshot() -> ComponentRegistrySnapshot {
        let mut builder = ComponentRegistryBuilder::new();
        REGISTRY_PROVIDER.register_into(&mut builder).unwrap();
        builder.freeze().unwrap()
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct RegistryMutationSchedule;

    #[derive(Debug, Default, Resource)]
    struct RegistryProbeRuns(u32);

    enum RegistryBuildMutation {
        ReplaceBuilding,
        Rewrap(ComponentRegistrySnapshot),
        Reinsert,
    }

    struct RegistryBuildMutationPlugin {
        mutation: RegistryBuildMutation,
    }

    impl Plugin for RegistryBuildMutationPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &REGISTRY_MUTATION_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            match &self.mutation {
                RegistryBuildMutation::ReplaceBuilding => {
                    app.insert_resource(ComponentRegistryResource::default())?;
                }
                RegistryBuildMutation::Rewrap(snapshot) => {
                    let existing_token = {
                        let existing = app
                            .world_mut()?
                            .get_resource::<ComponentRegistryResource>()
                            .expect("the registry owner installed its resource first");
                        assert!(existing.shares_snapshot(snapshot));
                        Arc::clone(&existing.instance_token)
                    };
                    let replacement = ComponentRegistryResource::preloaded(snapshot.clone());
                    assert!(replacement.shares_snapshot(snapshot));
                    assert!(!Arc::ptr_eq(&replacement.instance_token, &existing_token));
                    app.insert_resource(replacement)?;
                }
                RegistryBuildMutation::Reinsert => {
                    let registry = app
                        .world_mut()?
                        .remove_resource::<ComponentRegistryResource>()
                        .expect("the registry owner installed its resource first");
                    app.insert_resource(registry)?;
                }
            }
            Ok(())
        }
    }

    struct RegistryFinishRewrapPlugin {
        snapshot: ComponentRegistrySnapshot,
    }

    impl Plugin for RegistryFinishRewrapPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &REGISTRY_MUTATION_DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn finish(&self, app: &mut App) -> Result<(), PluginError> {
            app.insert_resource(ComponentRegistryResource::preloaded(self.snapshot.clone()))?;
            Ok(())
        }
    }

    struct RegistryFrameRewrapPlugin;

    impl Plugin for RegistryFrameRewrapPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &REGISTRY_MUTATION_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.add_systems(CoreStage::Last, rewrap_registry_during_frame)?;
            Ok(())
        }
    }

    fn rewrap_registry_during_frame(world: &mut World) {
        let snapshot = component_registry(world)
            .expect("the registry is admitted before managed execution")
            .clone();
        let mut replacement = ComponentRegistryResource::preloaded(snapshot.clone());
        replacement.state = ComponentRegistryState::Admitted(snapshot);
        world.insert_resource(replacement);
    }

    fn rewrap_registry_then_panic(world: &mut World) {
        rewrap_registry_during_frame(world);
        panic!("injected panic after registry replacement");
    }

    fn record_registry_probe_run(mut runs: nara_ecs::ResMut<RegistryProbeRuns>) {
        runs.0 += 1;
    }

    fn empty_snapshot() -> ComponentRegistrySnapshot {
        ComponentRegistryBuilder::new().freeze().unwrap()
    }

    #[test]
    fn registry_admission_preserves_installation_identity_and_mutation_revision() {
        for preloaded in [None, Some(empty_snapshot())] {
            let mut app = App::new();
            assert!(!component_registry_is_installed(app.world()));
            assert!(component_registry(app.world()).is_none());
            if let Some(snapshot) = &preloaded {
                app.add_plugin(PreloadedComponentRegistryPlugin {
                    snapshot: snapshot.clone(),
                })
                .unwrap();
            } else {
                app.add_plugin(ComponentRegistryPlugin).unwrap();
            }

            assert!(component_registry_is_installed(app.world()));
            assert!(component_registry(app.world()).is_none());
            assert_eq!(
                validate_component_registry_authority(app.world()),
                Err(ComponentRegistryAuthorityError::MissingAuthority)
            );
            let token = Arc::clone(
                &app.world()
                    .resource::<ComponentRegistryResource>()
                    .instance_token,
            );
            let revision = app
                .world()
                .resource::<ComponentRegistryMutationRevision>()
                .0;

            let app = app.seal().unwrap();

            let owner = app.world().resource::<ComponentRegistryResource>();
            let snapshot = component_registry(app.world()).unwrap();
            assert!(component_registry_is_installed(app.world()));
            assert!(Arc::ptr_eq(&owner.instance_token, &token));
            assert_eq!(
                app.world()
                    .resource::<ComponentRegistryMutationRevision>()
                    .0,
                revision
            );
            assert!(
                !app.world()
                    .contains_resource::<ComponentRegistryBuildSeed>()
            );
            if let Some(preloaded) = preloaded {
                assert!(snapshot.ptr_eq(&preloaded));
            }
            validate_component_registry_authority(app.world()).unwrap();
        }
    }

    #[derive(Component)]
    struct MissingBinding;

    #[test]
    fn failed_freeze_restores_the_candidate_and_cannot_retry_app_admission() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        let component_id = ComponentTypeId::new("nara.test.MissingBinding");
        let candidate_before = {
            let mut owner = app
                .world_mut()
                .unwrap()
                .resource_mut::<ComponentRegistryResource>();
            let ComponentRegistryState::Building(Some(builder)) = &mut owner.state else {
                panic!("the registry must still be building");
            };
            builder
                .register_component_schema(ComponentSchema::new(
                    component_id.clone(),
                    "Missing binding",
                    ComponentSchemaVersion::ONE,
                ))
                .unwrap();
            builder.catalog_candidate().clone()
        };
        let token = Arc::clone(
            &app.world()
                .resource::<ComponentRegistryResource>()
                .instance_token,
        );
        let revision = app
            .world()
            .resource::<ComponentRegistryMutationRevision>()
            .0;

        let error = finish_component_registry(&mut app, None).unwrap_err();

        assert_eq!(
            error,
            PluginError::component_registration(
                COMPONENT_REGISTRY_PLUGIN_ID,
                "component-schema-catalog",
                ComponentRegistryError::MissingNativeBinding {
                    component_id: component_id.clone(),
                },
            )
        );
        assert!(component_registry_is_installed(app.world()));
        assert!(component_registry(app.world()).is_none());
        assert!(
            !app.world()
                .contains_resource::<ComponentRegistryAuthority>()
        );
        assert!(
            !app.world()
                .contains_resource::<ComponentRegistryBuildSeed>()
        );
        assert_eq!(
            app.world()
                .resource::<ComponentRegistryMutationRevision>()
                .0,
            revision
        );
        {
            let mut owner = app
                .world_mut()
                .unwrap()
                .resource_mut::<ComponentRegistryResource>();
            assert!(Arc::ptr_eq(&owner.instance_token, &token));
            let ComponentRegistryState::Building(Some(builder)) = &mut owner.state else {
                panic!("failed admission must restore the original builder");
            };
            assert_eq!(builder.catalog_candidate(), &candidate_before);
            builder
                .register_native_component_with_codec::<MissingBinding, _, _>(
                    &component_id,
                    |_| Ok(MissingBinding),
                    |_| Ok(ComponentValue::Null),
                )
                .unwrap();
        }

        let Err(error) = app.seal() else {
            panic!("a failed registry admission unexpectedly retried");
        };
        assert_eq!(
            error.error(),
            &component_registry_authority_changed("component registry build seed is missing")
        );
    }

    #[test]
    fn building_provider_helpers_admit_the_registered_receipt() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.world()
            .resource::<ComponentRegistryResource>()
            .validate_schema_provider(&receipt_only_provider(REGISTRY_PROVIDER_BINDING))
            .unwrap();
        app.add_plugin(RegistryProviderPlugin {
            provider: REGISTRY_PROVIDER,
        })
        .unwrap();
        assert!(component_registry(app.world()).is_none());

        let app = app.seal().unwrap();

        assert_eq!(
            component_registry(app.world())
                .unwrap()
                .provider_receipt(REGISTRY_PROVIDER_ID),
            Some(REGISTRY_PROVIDER.receipt())
        );
        validate_component_registry_authority(app.world()).unwrap();
    }

    #[test]
    fn preloaded_provider_preflight_build_and_finish_never_replay_callbacks() {
        let snapshot = provider_snapshot();
        let mut app = App::new();
        app.add_plugin(PreloadedComponentRegistryPlugin {
            snapshot: snapshot.clone(),
        })
        .unwrap();
        app.add_plugin(RegistryProviderPlugin {
            provider: receipt_only_provider(REGISTRY_PROVIDER_BINDING),
        })
        .unwrap();
        assert!(component_registry(app.world()).is_none());

        let app = app.seal().unwrap();

        assert!(component_registry(app.world()).unwrap().ptr_eq(&snapshot));
        validate_component_registry_authority(app.world()).unwrap();
    }

    #[test]
    fn preloaded_provider_rejection_retains_the_unadmitted_definition() {
        for (snapshot, binding, expected) in [
            (
                empty_snapshot(),
                REGISTRY_PROVIDER_BINDING,
                ComponentRegistryError::MissingSchemaProviderReceipt {
                    provider: REGISTRY_PROVIDER_ID,
                },
            ),
            (
                provider_snapshot(),
                REGISTRY_PROVIDER_BINDING.with_codec_version(2),
                ComponentRegistryError::DivergentSchemaProviderReceipt {
                    provider: REGISTRY_PROVIDER_ID,
                },
            ),
        ] {
            let mut app = App::new();
            app.add_plugin(PreloadedComponentRegistryPlugin {
                snapshot: snapshot.clone(),
            })
            .unwrap();

            let error = app
                .add_plugin(RegistryProviderPlugin {
                    provider: receipt_only_provider(binding),
                })
                .err()
                .expect("missing or divergent preloaded receipts must reject");
            let nara_app::AddPluginsError::Plugin(error) = error else {
                panic!("provider receipt validation must fail in plugin preflight");
            };

            let expected = PluginError::component_registration(
                REGISTRY_PROVIDER_PLUGIN_ID,
                "test-provider",
                expected,
            );
            assert_eq!(error.error(), &expected);
            let error = register_schema_provider_for_plugin(
                &mut app,
                REGISTRY_PROVIDER_PLUGIN_ID,
                "test-provider",
                &receipt_only_provider(binding),
            )
            .unwrap_err();
            assert_eq!(error, expected);
            assert!(component_registry_is_installed(app.world()));
            assert!(component_registry(app.world()).is_none());
            assert!(
                app.world()
                    .resource::<ComponentRegistryResource>()
                    .shares_snapshot(&snapshot)
            );
            assert!(
                !app.world()
                    .contains_resource::<ComponentRegistryAuthority>()
            );
        }
    }

    #[test]
    fn finish_rejects_undeclared_provider_receipts_before_admission() {
        for preloaded in [false, true] {
            let snapshot = provider_snapshot();
            let mut app = App::new();
            if preloaded {
                app.add_plugin(PreloadedComponentRegistryPlugin {
                    snapshot: snapshot.clone(),
                })
                .unwrap();
            } else {
                app.add_plugin(ComponentRegistryPlugin).unwrap();
                register_schema_provider_for_plugin(
                    &mut app,
                    REGISTRY_PROVIDER_PLUGIN_ID,
                    "test-provider",
                    &REGISTRY_PROVIDER,
                )
                .unwrap();
            }

            let error =
                finish_component_registry(&mut app, preloaded.then_some(&snapshot)).unwrap_err();

            assert_eq!(
                error,
                PluginError::component_registration(
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    "component-schema-catalog",
                    ComponentRegistryError::UndeclaredSchemaProviderReceipt {
                        provider: REGISTRY_PROVIDER_ID,
                    },
                )
            );
            assert!(component_registry_is_installed(app.world()));
            assert!(component_registry(app.world()).is_none());
            assert!(
                !app.world()
                    .contains_resource::<ComponentRegistryAuthority>()
            );
        }
    }

    #[test]
    fn preinstalled_registry_finishes_without_late_hook_registration() {
        let mut app = App::new();
        app.insert_resource(ComponentRegistryResource::default())
            .unwrap();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        let app = app.seal().unwrap();

        validate_component_registry_authority(app.world()).unwrap();
    }

    #[test]
    fn intrinsic_registry_hooks_track_same_tick_reinsert() {
        let mut world = World::new();
        world.insert_resource(ComponentRegistryMutationRevision::default());
        world.insert_resource(ComponentRegistryResource::default());
        let after_insert = world.resource::<ComponentRegistryMutationRevision>().0;

        let registry = world
            .remove_resource::<ComponentRegistryResource>()
            .unwrap();
        let after_remove = world.resource::<ComponentRegistryMutationRevision>().0;
        world.insert_resource(registry);
        let after_reinsert = world.resource::<ComponentRegistryMutationRevision>().0;

        assert!(after_remove > after_insert);
        assert!(after_reinsert > after_remove);
    }

    #[test]
    fn code_first_registry_replacement_during_build_rejects_seal() {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            RegistryBuildMutationPlugin {
                mutation: RegistryBuildMutation::ReplaceBuilding,
            },
        ))
        .unwrap();

        let Err(error) = app.seal() else {
            panic!("a replaced code-first registry unexpectedly sealed");
        };

        assert!(matches!(
            error.error(),
            PluginError::ComponentRegistrationFailed {
                plugin: COMPONENT_REGISTRY_PLUGIN_ID,
                ..
            }
        ));
    }

    #[test]
    fn preloaded_registry_rewrap_or_reinsert_during_build_rejects_seal() {
        for reinsert in [false, true] {
            let snapshot = empty_snapshot();
            let mutation = if reinsert {
                RegistryBuildMutation::Reinsert
            } else {
                RegistryBuildMutation::Rewrap(snapshot.clone())
            };
            let mut app = App::new();
            app.add_plugins((
                PreloadedComponentRegistryPlugin { snapshot },
                RegistryBuildMutationPlugin { mutation },
            ))
            .unwrap();

            let Err(error) = app.seal() else {
                panic!("a mutated preloaded registry unexpectedly sealed");
            };

            assert!(matches!(
                error.error(),
                PluginError::ComponentRegistrationFailed {
                    plugin: COMPONENT_REGISTRY_PLUGIN_ID,
                    ..
                }
            ));
        }
    }

    #[test]
    fn direct_app_rejects_registry_rewrap_during_finish() {
        let snapshot = empty_snapshot();
        let mut app = App::new();
        app.add_plugins((
            PreloadedComponentRegistryPlugin {
                snapshot: snapshot.clone(),
            },
            RegistryFinishRewrapPlugin { snapshot },
        ))
        .unwrap();

        let Err(error) = app.seal() else {
            panic!("a registry rewrapped during finish unexpectedly sealed");
        };

        assert!(matches!(
            error.error(),
            PluginError::SetupFailed {
                plugin: COMPONENT_REGISTRY_PLUGIN_ID,
                ..
            }
        ));
    }

    #[test]
    fn managed_frame_rejects_registry_rewrap_and_keeps_the_fault_sticky() {
        let mut app = App::new();
        app.add_plugins((ComponentRegistryPlugin, RegistryFrameRewrapPlugin))
            .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();

        let first = runtime.drive(Duration::ZERO).unwrap_err();
        let repeated = runtime.drive(Duration::ZERO).unwrap_err();

        assert_eq!(first, repeated);
        assert_eq!(first.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        runtime.finish().unwrap();
    }

    #[test]
    fn ignored_nested_schedule_mutation_faults_before_the_next_stage() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_resource::<RegistryProbeRuns>().unwrap();
        let mut nested = nara_ecs::schedule::Schedule::new(RegistryMutationSchedule);
        nested.add_systems(rewrap_registry_during_frame);
        app.world_mut().unwrap().add_schedule(nested);
        app.add_systems(CoreStage::Update, |world: &mut World| {
            world.run_schedule(RegistryMutationSchedule);
        })
        .unwrap();
        app.add_systems(CoreStage::PostUpdate, record_registry_probe_run)
            .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();

        let first = runtime.drive(Duration::ZERO).unwrap_err();
        let repeated = runtime.drive(Duration::ZERO).unwrap_err();

        assert_eq!(first, repeated);
        assert_eq!(first.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        assert_eq!(runtime.world().resource::<RegistryProbeRuns>().0, 0);
        runtime.finish().unwrap();
    }

    #[test]
    fn caught_schedule_unwind_cannot_bypass_the_next_managed_boundary() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_resource::<RegistryProbeRuns>().unwrap();
        let mut nested = nara_ecs::schedule::Schedule::new(RegistryMutationSchedule);
        nested.add_systems(rewrap_registry_then_panic);
        app.world_mut().unwrap().add_schedule(nested);
        app.add_systems(CoreStage::Update, |world: &mut World| {
            world.run_schedule(RegistryMutationSchedule);
        })
        .unwrap();
        app.add_systems(CoreStage::PostUpdate, record_registry_probe_run)
            .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();

        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let _ = runtime.drive(Duration::ZERO);
        }));
        assert!(unwind.is_err());
        let error = runtime.drive(Duration::ZERO).unwrap_err();

        assert_eq!(error.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        assert_eq!(runtime.world().resource::<RegistryProbeRuns>().0, 0);
        runtime.finish().unwrap();
    }

    #[derive(Default, Resource)]
    struct RegistryRewrapPort;

    impl nara_app::__RuntimeDriverPort for RegistryRewrapPort {
        type Input = ();
        type Output = ();

        fn apply_driver_input(&mut self, (): ()) {}
    }

    impl RegistryRewrapPort {
        fn apply_driver_input_in_world(
            world: &mut World,
            (): (),
        ) -> Result<(), (nara_app::RuntimeWorldAccessError, ())> {
            rewrap_registry_during_frame(world);
            Ok(())
        }
    }

    #[test]
    fn managed_boundary_rejects_registry_replacement_before_committing_clock_state() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_resource::<RegistryRewrapPort>().unwrap();
        app.__register_driver_port::<RegistryRewrapPort>(
            RegistryRewrapPort::apply_driver_input_in_world,
        )
        .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let real_time = *runtime.world().resource::<RealTime>();
        let virtual_time = *runtime.world().resource::<VirtualTime>();
        let frame_execution_start = *runtime.world().resource::<FrameExecutionStart>();

        let scope_outcome = runtime
            .with_driver_scope(|scope| {
                scope.__apply_port::<RegistryRewrapPort>(()).unwrap();
            })
            .unwrap();
        assert_eq!(
            scope_outcome.post_operation_fault.unwrap().kind(),
            RuntimeFaultKind::RuntimeAuthority
        );
        let error = runtime.drive(Duration::from_millis(1)).unwrap_err();

        assert_eq!(error.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        assert_eq!(*runtime.world().resource::<RealTime>(), real_time);
        assert_eq!(*runtime.world().resource::<VirtualTime>(), virtual_time);
        let current = *runtime.world().resource::<FrameExecutionStart>();
        assert_eq!(current.frame(), frame_execution_start.frame());
        assert_eq!(current.started_at(), frame_execution_start.started_at());
        runtime.finish().unwrap();
    }
}
