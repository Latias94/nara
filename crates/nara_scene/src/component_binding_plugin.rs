use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, RuntimeFault,
    RuntimeFaultKind, RuntimeFaultReporter, RuntimeGeneration,
};
use nara_ecs::{Resource, World, bevy_ecs::lifecycle::HookContext, bevy_ecs::world::DeferredWorld};
use nara_reflect::{
    COMPONENT_REGISTRY_PLUGIN_REQUIREMENT, ComponentRegistryAuthorityError,
    ComponentSchemaOwnerSnapshot, component_registry, validate_component_registry_authority,
};

use crate::{SceneComponentBindingLimits, SceneComponentBindings};

pub const SCENE_COMPONENT_BINDINGS_PLUGIN_ID: PluginId =
    PluginId::new("nara.scene.component-bindings");
pub const SCENE_COMPONENT_BINDINGS_PLUGIN_REQUIREMENT: &[PluginId] =
    &[SCENE_COMPONENT_BINDINGS_PLUGIN_ID];
pub const SCENE_COMPONENT_BINDINGS_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SCENE_COMPONENT_BINDINGS_PLUGIN_ID, PluginCategory::Core)
        .requires_plugins(COMPONENT_REGISTRY_PLUGIN_REQUIREMENT);

/// Selects data-only Scene definitions alongside the actual managed native registry.
///
/// Install the native registry separately. The selection is assembled after that registry freezes.
/// Admitting metadata does not install a language module or gameplay callback.
#[derive(Debug)]
pub struct SceneComponentBindingsPlugin {
    owners: Vec<ComponentSchemaOwnerSnapshot>,
    limits: SceneComponentBindingLimits,
}

impl SceneComponentBindingsPlugin {
    #[must_use]
    pub fn new(owners: impl IntoIterator<Item = ComponentSchemaOwnerSnapshot>) -> Self {
        Self::new_with_limits(owners, SceneComponentBindingLimits::default())
    }

    #[must_use]
    pub fn new_with_limits(
        owners: impl IntoIterator<Item = ComponentSchemaOwnerSnapshot>,
        limits: SceneComponentBindingLimits,
    ) -> Self {
        Self {
            // One excess declaration is sufficient for the normal admission error at seal.
            // Do not retain or exhaust an arbitrarily long code-authored configuration first.
            owners: owners
                .into_iter()
                .take(limits.owners().get().saturating_add(1))
                .collect(),
            limits,
        }
    }
}

#[derive(Resource)]
#[component(
    on_insert = record_bindings_structure_change,
    on_discard = record_bindings_structure_change
)]
struct SceneComponentBindingsResource {
    owners: Vec<ComponentSchemaOwnerSnapshot>,
    limits: SceneComponentBindingLimits,
    bindings: Option<SceneComponentBindings>,
    instance_token: Arc<()>,
}

#[derive(Debug, Default, Resource)]
struct SceneComponentBindingsMutationRevision(u64);

#[derive(Resource)]
#[component(
    on_insert = record_bindings_structure_change,
    on_discard = record_bindings_structure_change
)]
struct SceneComponentBindingsBuildSeed {
    instance_token: Arc<()>,
    mutation_revision: u64,
}

fn next_revision(revision: u64) -> u64 {
    revision
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort())
}

fn record_bindings_structure_change(mut world: DeferredWorld<'_>, _context: HookContext) {
    if let Some(mut revision) = world.get_resource_mut::<SceneComponentBindingsMutationRevision>() {
        revision.0 = next_revision(revision.0);
    }
}

/// Refusal to use a missing, changed or natively invalid managed Scene selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneComponentBindingsAuthorityError {
    MissingAuthority,
    MissingBindings,
    Replaced,
    NativeRegistry(ComponentRegistryAuthorityError),
}

impl fmt::Display for SceneComponentBindingsAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAuthority => {
                formatter.write_str("Scene component bindings authority guard is missing")
            }
            Self::MissingBindings => {
                formatter.write_str("Scene component bindings resource is missing or unfinished")
            }
            Self::Replaced => formatter
                .write_str("Scene component bindings no longer match their admitted authority"),
            Self::NativeRegistry(error) => {
                write!(formatter, "Scene native registry authority: {error}")
            }
        }
    }
}

impl Error for SceneComponentBindingsAuthorityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NativeRegistry(error) => Some(error),
            _ => None,
        }
    }
}

struct SceneComponentBindingsAuthorityState {
    bindings: SceneComponentBindings,
    instance_token: Arc<()>,
    mutation_revision: u64,
    reporter: RuntimeFaultReporter,
    generation: OnceLock<RuntimeGeneration>,
}

#[derive(Resource)]
#[component(
    on_insert = record_bindings_structure_change,
    on_discard = record_bindings_structure_change
)]
struct SceneComponentBindingsAuthority(Arc<SceneComponentBindingsAuthorityState>);

impl SceneComponentBindingsAuthorityState {
    fn fault(&self) -> RuntimeFault {
        RuntimeFault::engine(
            RuntimeFaultKind::RuntimeAuthority,
            "nara.scene.component-bindings-authority",
        )
    }

    fn validate_bindings(&self, world: &World) -> Result<(), SceneComponentBindingsAuthorityError> {
        validate_component_registry_authority(world)
            .map_err(SceneComponentBindingsAuthorityError::NativeRegistry)?;
        if !component_registry(world)
            .is_some_and(|registry| registry.shares_snapshot(self.bindings.native_snapshot()))
        {
            return Err(SceneComponentBindingsAuthorityError::NativeRegistry(
                ComponentRegistryAuthorityError::Replaced,
            ));
        }
        let resource = world
            .get_resource::<SceneComponentBindingsResource>()
            .ok_or(SceneComponentBindingsAuthorityError::MissingBindings)?;
        let bindings = resource
            .bindings
            .as_ref()
            .ok_or(SceneComponentBindingsAuthorityError::MissingBindings)?;
        let revision = world
            .get_resource::<SceneComponentBindingsMutationRevision>()
            .ok_or(SceneComponentBindingsAuthorityError::Replaced)?;
        if Arc::ptr_eq(&resource.instance_token, &self.instance_token)
            && bindings.same_definitions(&self.bindings)
            && revision.0 == self.mutation_revision
        {
            Ok(())
        } else {
            Err(SceneComponentBindingsAuthorityError::Replaced)
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
            .get_resource::<SceneComponentBindingsAuthority>()
            .is_some_and(|authority| std::ptr::eq(Arc::as_ptr(&authority.0), self));
        let generation_matches = generation.is_none_or(|generation| {
            world.get_resource::<RuntimeGeneration>() == Some(&generation)
                && *self.generation.get_or_init(|| generation) == generation
        });
        if authority_matches && generation_matches && self.validate_bindings(world).is_ok() {
            return Ok(());
        }
        let fault = self.fault();
        self.reporter.report(fault.clone());
        Err(fault)
    }
}

impl Plugin for SceneComponentBindingsPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SCENE_COMPONENT_BINDINGS_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let world = app.world_mut()?;
        if world.contains_resource::<SceneComponentBindingsResource>()
            || world.contains_resource::<SceneComponentBindingsBuildSeed>()
            || world.contains_resource::<SceneComponentBindingsAuthority>()
        {
            return Err(bindings_setup_error(
                "Scene component bindings were already installed",
            ));
        }
        world.init_resource::<SceneComponentBindingsMutationRevision>();
        let instance_token = Arc::new(());
        world.insert_resource(SceneComponentBindingsResource {
            owners: self.owners.clone(),
            limits: self.limits,
            bindings: None,
            instance_token: Arc::clone(&instance_token),
        });
        // Include the seed's own insertion in the captured structural revision.
        let mutation_revision =
            next_revision(world.resource::<SceneComponentBindingsMutationRevision>().0);
        world.insert_resource(SceneComponentBindingsBuildSeed {
            instance_token,
            mutation_revision,
        });
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        let (bindings, instance_token) = {
            let world = app.world_mut()?;
            let seed = world
                .get_resource::<SceneComponentBindingsBuildSeed>()
                .ok_or_else(|| {
                    bindings_setup_error("Scene component bindings build seed is missing")
                })?;
            let resource = world
                .get_resource::<SceneComponentBindingsResource>()
                .ok_or_else(|| {
                    bindings_setup_error("Scene component bindings configuration is missing")
                })?;
            let revision = world
                .get_resource::<SceneComponentBindingsMutationRevision>()
                .ok_or_else(|| {
                    bindings_setup_error("Scene component bindings mutation revision is missing")
                })?;
            if !Arc::ptr_eq(&seed.instance_token, &resource.instance_token)
                || seed.mutation_revision != revision.0
                || resource.bindings.is_some()
                || resource.limits != self.limits
                || resource.owners.len() != self.owners.len()
                || !resource
                    .owners
                    .iter()
                    .zip(&self.owners)
                    .all(|(a, b)| a.ptr_eq(b))
            {
                return Err(bindings_setup_error(
                    "Scene component bindings changed after installation",
                ));
            }
            validate_component_registry_authority(world).map_err(bindings_setup_error)?;
            let native = component_registry(world)
                .ok_or_else(|| {
                    bindings_setup_error("managed native component registry is missing")
                })?
                .snapshot()
                .map_err(bindings_setup_error)?;
            let bindings = SceneComponentBindings::new_with_limits(
                native,
                resource.owners.iter().cloned(),
                resource.limits,
            )
            .map_err(bindings_setup_error)?;
            (bindings, Arc::clone(&resource.instance_token))
        };
        let world = app.world_mut()?;
        let _ = world.remove_resource::<SceneComponentBindingsBuildSeed>();
        world
            .resource_mut::<SceneComponentBindingsResource>()
            .bindings = Some(bindings.clone());
        let mutation_revision =
            next_revision(world.resource::<SceneComponentBindingsMutationRevision>().0);
        let authority = Arc::new(SceneComponentBindingsAuthorityState {
            bindings,
            instance_token,
            mutation_revision,
            reporter: world.resource::<RuntimeFaultReporter>().clone(),
            generation: OnceLock::new(),
        });
        app.insert_resource(SceneComponentBindingsAuthority(Arc::clone(&authority)))?;
        app.__install_runtime_authority_validator(
            SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
            Arc::new(move |world, generation| authority.validate_runtime(world, generation)),
        )?;
        Ok(())
    }
}

fn bindings_setup_error(error: impl fmt::Display) -> PluginError {
    PluginError::component_registration(
        SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
        "scene-component-bindings",
        error,
    )
}

/// Returns the immutable, finished selection owned by this App or managed runtime.
///
/// Consumers that rely on managed authority must also call
/// [`validate_scene_component_bindings_authority`] before using the selection.
#[must_use]
pub fn scene_component_bindings(world: &World) -> Option<&SceneComponentBindings> {
    world
        .get_resource::<SceneComponentBindingsResource>()?
        .bindings
        .as_ref()
}

/// Checks the exact admitted Scene selection and its managed native registry authority.
///
/// Caller-owned definitions never install a missing Scene owner implicitly.
pub fn validate_scene_component_bindings_authority(
    world: &World,
) -> Result<(), SceneComponentBindingsAuthorityError> {
    let authority = world
        .get_resource::<SceneComponentBindingsAuthority>()
        .ok_or(SceneComponentBindingsAuthorityError::MissingAuthority)?;
    authority.0.validate_bindings(world)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nara_app::{CoreStage, RuntimeInstance};
    use nara_ecs::Commands;
    use nara_reflect::{
        ComponentRegistryPlugin, ComponentSchemaCatalog, PersistentComponentSchemaOwnerId,
    };

    use super::*;

    fn owner() -> ComponentSchemaOwnerSnapshot {
        ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("game.data").unwrap(),
            ComponentSchemaCatalog::default(),
            None,
        )
        .unwrap()
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            SceneComponentBindingsPlugin::new([owner()]),
        ))
        .unwrap();
        app
    }

    fn rewrap_resource(world: &mut World) {
        let existing = world.resource::<SceneComponentBindingsResource>();
        let replacement = SceneComponentBindingsResource {
            owners: existing.owners.clone(),
            limits: existing.limits,
            bindings: existing.bindings.clone(),
            instance_token: Arc::new(()),
        };
        world.insert_resource(replacement);
    }

    fn reinsert_resource(world: &mut World) {
        let resource = world
            .remove_resource::<SceneComponentBindingsResource>()
            .unwrap();
        world.insert_resource(resource);
    }

    fn reinsert_seed(world: &mut World) {
        let seed = world
            .remove_resource::<SceneComponentBindingsBuildSeed>()
            .unwrap();
        world.insert_resource(seed);
    }

    fn reinsert_authority(world: &mut World) {
        let authority = world
            .remove_resource::<SceneComponentBindingsAuthority>()
            .unwrap();
        world.insert_resource(authority);
    }

    fn readmit_build_owner(world: &mut World) {
        world
            .resource_mut::<SceneComponentBindingsResource>()
            .owners = vec![owner()];
    }

    fn readmit_finished_owner(world: &mut World) {
        let native = scene_component_bindings(world)
            .unwrap()
            .native_snapshot()
            .clone();
        world
            .resource_mut::<SceneComponentBindingsResource>()
            .bindings = Some(SceneComponentBindings::new(native, [owner()]).unwrap());
    }

    struct FinishMutation(fn(&mut World));

    impl Plugin for FinishMutation {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.scene.bindings-finish-mutation"),
                PluginCategory::Runtime,
            )
            .requires_plugins(SCENE_COMPONENT_BINDINGS_PLUGIN_REQUIREMENT);
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn finish(&self, app: &mut App) -> Result<(), PluginError> {
            (self.0)(app.world_mut()?);
            Ok(())
        }
    }

    #[test]
    fn changed_build_config_or_transient_reinsertion_rejects_seal() {
        for mutation in [
            rewrap_resource as fn(&mut World),
            reinsert_resource,
            reinsert_seed,
            readmit_build_owner,
        ] {
            let mut app = app();
            mutation(app.world_mut().unwrap());
            let error = app.seal().unwrap_err();

            assert!(matches!(
                error.error(),
                PluginError::ComponentRegistrationFailed {
                    plugin: SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
                    ..
                }
            ));
        }
    }

    #[test]
    fn replacement_or_transient_reinsertion_after_finish_rejects_seal() {
        for mutation in [
            rewrap_resource as fn(&mut World),
            reinsert_resource,
            reinsert_authority,
            readmit_finished_owner,
        ] {
            let mut app = app();
            app.add_plugin(FinishMutation(mutation)).unwrap();
            let error = app.seal().unwrap_err();

            assert!(matches!(error.error(), PluginError::SetupFailed { .. }));
        }
    }

    #[test]
    fn reconstructing_a_container_with_the_exact_selected_snapshots_preserves_authority() {
        let mut app = app();
        app.add_plugin(FinishMutation(|world| {
            let bindings = scene_component_bindings(world).unwrap();
            let reconstructed = SceneComponentBindings::new(
                bindings.native_snapshot().clone(),
                bindings.data_owners().iter().cloned(),
            )
            .unwrap();
            world
                .resource_mut::<SceneComponentBindingsResource>()
                .bindings = Some(reconstructed);
            validate_scene_component_bindings_authority(world).unwrap();
        }))
        .unwrap();
        let app = app.seal().unwrap();

        validate_scene_component_bindings_authority(app.world()).unwrap();
    }

    #[derive(Default, Resource)]
    struct LaterStageRuns(u32);

    fn queue_reinsert(mut commands: Commands) {
        commands.queue(reinsert_resource);
    }

    #[test]
    fn deferred_reinsertion_faults_before_the_next_stage_and_remains_sticky() {
        let mut app = app();
        app.init_resource::<LaterStageRuns>().unwrap();
        app.add_systems(CoreStage::Update, queue_reinsert).unwrap();
        app.add_systems(CoreStage::PostUpdate, |world: &mut World| {
            world.resource_mut::<LaterStageRuns>().0 += 1;
        })
        .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();

        let first = runtime.drive(Duration::ZERO).unwrap_err();
        let repeated = runtime.drive(Duration::ZERO).unwrap_err();

        assert_eq!(first, repeated);
        assert_eq!(first.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        assert_eq!(runtime.world().resource::<LaterStageRuns>().0, 0);
        assert_eq!(
            validate_scene_component_bindings_authority(runtime.world()),
            Err(SceneComponentBindingsAuthorityError::Replaced)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn replacing_the_native_basis_inside_the_selection_rejects_authority() {
        let mut app = app();
        app.add_plugin(FinishMutation(|world| {
            let mut native = nara_reflect::ComponentRegistry::new();
            native.freeze().unwrap();
            let bindings = SceneComponentBindings::new(
                native.snapshot().unwrap(),
                scene_component_bindings(world)
                    .unwrap()
                    .data_owners()
                    .iter()
                    .cloned(),
            )
            .unwrap();
            world
                .resource_mut::<SceneComponentBindingsResource>()
                .bindings = Some(bindings);
            assert_eq!(
                validate_scene_component_bindings_authority(world),
                Err(SceneComponentBindingsAuthorityError::Replaced)
            );
        }))
        .unwrap();

        assert!(matches!(
            app.seal().unwrap_err().error(),
            PluginError::SetupFailed { .. }
        ));
    }

    #[test]
    fn scene_authority_still_requires_the_native_authority() {
        let mut app = app();
        app.add_plugin(FinishMutation(|world| {
            let authority = Arc::clone(&world.resource::<SceneComponentBindingsAuthority>().0);
            let mut other = World::new();
            other.insert_resource(SceneComponentBindingsAuthority(authority));

            assert_eq!(
                validate_scene_component_bindings_authority(&other),
                Err(SceneComponentBindingsAuthorityError::NativeRegistry(
                    ComponentRegistryAuthorityError::MissingAuthority,
                ))
            );
        }))
        .unwrap();
        let app = app.seal().unwrap();

        validate_scene_component_bindings_authority(app.world()).unwrap();
    }
}
