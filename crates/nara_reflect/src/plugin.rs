use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginId, PluginPreflightContext, PluginPreflightResource, PluginPrepareFailure,
    RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter, RuntimeGeneration,
};
use nara_ecs::{Resource, World, lifecycle::HookContext, world::DeferredWorld};

use crate::{ComponentRegistry, ComponentRegistrySnapshot, ComponentSchemaProviderDefinition};

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
                .ok_or_else(|| PluginPrepareFailure::new("component-registry-snapshot-missing"))
        },
    )
}

#[derive(Resource)]
#[component(
    on_insert = record_component_registry_structure_change,
    on_discard = record_component_registry_structure_change
)]
struct ComponentRegistryResource {
    registry: ComponentRegistry,
}

impl Default for ComponentRegistryResource {
    fn default() -> Self {
        Self {
            registry: ComponentRegistry::new(),
        }
    }
}

impl ComponentRegistryResource {
    fn from_snapshot(snapshot: ComponentRegistrySnapshot) -> Self {
        Self {
            registry: ComponentRegistry::from_snapshot(snapshot),
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
        if registry.registry.shares_snapshot(&self.snapshot)
            && registry
                .registry
                .shares_instance_token(&self.instance_token)
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

pub fn registry_for_plugin_preflight<'a>(
    context: &'a PluginPreflightContext<'_>,
    plugin: PluginId,
    component: &str,
) -> Result<&'a ComponentRegistry, PluginError> {
    context
        .get_structural_resource::<ComponentRegistryResource>()
        .map(|resource| &resource.registry)
        .ok_or_else(|| {
            PluginError::component_registration(
                plugin,
                component,
                "component registry resource is unavailable",
            )
        })
}

/// Returns the immutable component registry owned by the current App or runtime World.
///
/// `ComponentRegistry` is intentionally not an ECS resource. Schema-owning plugins register
/// through [`register_schema_provider_for_plugin`], and runtime consumers receive only this
/// shared read view, so native code cannot temporarily replace the executable behavior authority.
#[must_use]
pub fn component_registry(world: &World) -> Option<&ComponentRegistry> {
    world
        .get_resource::<ComponentRegistryResource>()
        .map(|resource| &resource.registry)
}

/// Registers or validates one schema provider against the registry owned by `app`.
///
/// The private owner resource keeps mutation scoped to this operation. A preloaded frozen
/// registry accepts only an exact compatible provider; it cannot be replaced through this API.
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
    provider
        .register_or_validate_into(&mut resource.registry)
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
            .map(|existing| existing.registry.shares_snapshot(snapshot));
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
                app.insert_resource(ComponentRegistryResource::from_snapshot(snapshot.clone()))?;
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
            instance_token: registry.registry.instance_token().clone(),
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
        if !registry
            .registry
            .shares_instance_token(&seed.instance_token)
            || mutation_revision != seed.mutation_revision
        {
            return Err(component_registry_authority_changed(
                "component registry authority changed after installation",
            ));
        }
        let snapshot = registry
            .registry
            .freeze()
            .map_err(|error| {
                PluginError::component_registration(
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    "component-schema-catalog",
                    error,
                )
            })?
            .snapshot()
            .map_err(|error| {
                PluginError::component_registration(
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    "component-schema-catalog",
                    error,
                )
            })?;
        (snapshot, registry.registry.instance_token().clone())
    };
    if let Some(expected) = expected
        && !snapshot.ptr_eq(expected)
    {
        return Err(PluginError::component_registration(
            COMPONENT_REGISTRY_PLUGIN_ID,
            "component-schema-catalog",
            "component registry finished with a different admitted snapshot",
        ));
    }
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

    use nara_app::{
        AppRunError, AppScheduleRunError, CoreStage, FrameExecutionStart, RealTime, VirtualTime,
    };
    use nara_ecs::schedule::ScheduleLabel;

    use super::*;

    const REGISTRY_MUTATION_PLUGIN_ID: PluginId =
        PluginId::new("nara.test.reflect.registry-mutation");
    const REGISTRY_MUTATION_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(REGISTRY_MUTATION_PLUGIN_ID, PluginCategory::Runtime);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct RegistryMutationSchedule;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct RegistryProbeSchedule;

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
                    app.insert_resource(ComponentRegistryResource {
                        registry: ComponentRegistry::new(),
                    })?;
                }
                RegistryBuildMutation::Rewrap(snapshot) => {
                    let existing_token = {
                        let existing = app
                            .world_mut()?
                            .get_resource::<ComponentRegistryResource>()
                            .expect("the registry owner installed its resource first");
                        assert!(existing.registry.shares_snapshot(snapshot));
                        existing.registry.instance_token().clone()
                    };
                    let replacement = ComponentRegistry::from_snapshot(snapshot.clone());
                    assert!(replacement.shares_snapshot(snapshot));
                    assert!(!replacement.shares_instance_token(&existing_token));
                    app.insert_resource(ComponentRegistryResource {
                        registry: replacement,
                    })?;
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
            app.insert_resource(ComponentRegistryResource::from_snapshot(
                self.snapshot.clone(),
            ))?;
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
        let snapshot = world
            .resource::<ComponentRegistryResource>()
            .registry
            .snapshot()
            .expect("the registry is frozen before direct App execution");
        world.insert_resource(ComponentRegistryResource::from_snapshot(snapshot));
    }

    fn rewrap_registry_then_panic(world: &mut World) {
        rewrap_registry_during_frame(world);
        panic!("injected panic after registry replacement");
    }

    fn record_registry_probe_run(mut runs: nara_ecs::ResMut<RegistryProbeRuns>) {
        runs.0 += 1;
    }

    fn empty_snapshot() -> ComponentRegistrySnapshot {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        registry.snapshot().unwrap()
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
            error,
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
                error,
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
            error,
            PluginError::SetupFailed {
                plugin: COMPONENT_REGISTRY_PLUGIN_ID,
                ..
            }
        ));
    }

    #[test]
    fn direct_app_rejects_registry_rewrap_during_frame() {
        let mut app = App::new();
        app.add_plugins((ComponentRegistryPlugin, RegistryFrameRewrapPlugin))
            .unwrap();

        let error = app.run_once(Duration::ZERO).unwrap_err();

        assert!(matches!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            }
        ));
    }

    #[test]
    fn direct_app_rejects_registry_rewrap_in_custom_schedule() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_resource::<RegistryProbeRuns>().unwrap();
        app.init_schedule(RegistryMutationSchedule).unwrap();
        app.init_schedule(RegistryProbeSchedule).unwrap();
        app.add_systems(RegistryMutationSchedule, rewrap_registry_during_frame)
            .unwrap();
        app.add_systems(RegistryProbeSchedule, record_registry_probe_run)
            .unwrap();

        let first_error = app.run_schedule(RegistryMutationSchedule).unwrap_err();
        let sticky_error = app.run_schedule(RegistryProbeSchedule).unwrap_err();

        assert_eq!(sticky_error, first_error);
        assert!(matches!(
            &first_error,
            AppScheduleRunError::Runtime(AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            })
        ));
        assert_eq!(app.world().resource::<RegistryProbeRuns>().0, 0);
    }

    #[test]
    fn caught_schedule_unwind_cannot_bypass_the_next_authority_boundary() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_resource::<RegistryProbeRuns>().unwrap();
        app.init_schedule(RegistryMutationSchedule).unwrap();
        app.init_schedule(RegistryProbeSchedule).unwrap();
        app.add_systems(RegistryMutationSchedule, rewrap_registry_then_panic)
            .unwrap();
        app.add_systems(RegistryProbeSchedule, record_registry_probe_run)
            .unwrap();

        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let _ = app.run_schedule(RegistryMutationSchedule);
        }));
        let recorded_fault = app
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .expect("the unwinding schedule must record its authority fault");
        let error = app.run_schedule(RegistryProbeSchedule).unwrap_err();

        assert!(unwind.is_err());
        assert_eq!(recorded_fault.kind(), RuntimeFaultKind::RuntimeAuthority);
        assert!(matches!(
            error,
            AppScheduleRunError::Runtime(AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            })
        ));
        assert_eq!(app.world().resource::<RegistryProbeRuns>().0, 0);
    }

    #[test]
    fn direct_app_run_rejects_runner_ignored_registry_authority_fault() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_schedule(RegistryMutationSchedule).unwrap();
        app.add_systems(RegistryMutationSchedule, rewrap_registry_during_frame)
            .unwrap();
        app.set_runner(|app| {
            let _ignored = app.run_schedule(RegistryMutationSchedule);
            Ok(nara_app::AppExit::Success)
        })
        .unwrap();

        let error = app.run().unwrap_err();

        assert!(matches!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            }
        ));
    }

    #[test]
    fn direct_app_run_does_not_duplicate_a_propagated_authority_fault() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.init_schedule(RegistryMutationSchedule).unwrap();
        app.add_systems(RegistryMutationSchedule, rewrap_registry_during_frame)
            .unwrap();
        app.set_runner(|app| {
            let AppScheduleRunError::Runtime(error) = app
                .run_schedule(RegistryMutationSchedule)
                .expect_err("registry replacement must fault the custom schedule")
            else {
                panic!("registry replacement returned a non-runtime schedule error");
            };
            Err(error)
        })
        .unwrap();

        let error = app.run().unwrap_err();

        assert!(matches!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            }
        ));
    }

    #[test]
    fn direct_app_run_rejects_runner_replacing_the_app_instance() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.set_runner(|app| {
            *app = App::new();
            Ok(nara_app::AppExit::Success)
        })
        .unwrap();

        let error = app.run().unwrap_err();

        assert!(matches!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                fault_source: "nara.app.instance-authority",
                ..
            }
        ));
    }

    #[test]
    fn direct_frame_rejects_replaced_registry_before_committing_clock_state() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.run_once(Duration::ZERO).unwrap();
        let real_time = *app.world().resource::<RealTime>();
        let virtual_time = *app.world().resource::<VirtualTime>();
        let frame_execution_start = *app.world().resource::<FrameExecutionStart>();
        let snapshot = app
            .world()
            .resource::<ComponentRegistryResource>()
            .registry
            .snapshot()
            .expect("the registry is frozen after the first frame");
        app.world_mut()
            .unwrap()
            .insert_resource(ComponentRegistryResource::from_snapshot(snapshot));

        let error = app.run_once(Duration::from_millis(1)).unwrap_err();

        assert!(matches!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::RuntimeAuthority,
                ..
            }
        ));
        assert_eq!(*app.world().resource::<RealTime>(), real_time);
        assert_eq!(*app.world().resource::<VirtualTime>(), virtual_time);
        let current_frame_execution_start = *app.world().resource::<FrameExecutionStart>();
        assert_eq!(
            current_frame_execution_start.frame(),
            frame_execution_start.frame()
        );
        assert_eq!(
            current_frame_execution_start.started_at(),
            frame_execution_start.started_at()
        );
    }
}
