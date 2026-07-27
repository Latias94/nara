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

use crate::{ComponentRegistry, ComponentRegistrySnapshot};

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

impl PluginPreflightResource for ComponentRegistry {}

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
            .get_resource::<ComponentRegistry>()
            .ok_or(ComponentRegistryAuthorityError::MissingRegistry)?;
        let revision = world
            .get_resource::<ComponentRegistryMutationRevision>()
            .ok_or(ComponentRegistryAuthorityError::Replaced)?;
        if registry.shares_snapshot(&self.snapshot)
            && registry.shares_instance_token(&self.instance_token)
            && revision.0 == self.mutation_revision
        {
            Ok(())
        } else {
            Err(ComponentRegistryAuthorityError::Replaced)
        }
    }

    fn validate_managed(
        &self,
        world: &World,
        generation: RuntimeGeneration,
    ) -> Result<(), RuntimeFault> {
        let authority_matches = world
            .get_resource::<ComponentRegistryAuthority>()
            .is_some_and(|authority| std::ptr::eq(Arc::as_ptr(&authority.0), self));
        if world.get_resource::<RuntimeGeneration>() != Some(&generation) {
            let fault = self.fault();
            self.reporter.report(fault.clone());
            return Err(fault);
        }
        let generation_matches = *self.generation.get_or_init(|| generation) == generation;
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
        .get_structural_resource::<ComponentRegistry>()
        .ok_or_else(|| {
            PluginError::component_registration(
                plugin,
                component,
                "component registry resource is unavailable",
            )
        })
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
    let Some(snapshot) = snapshot else {
        app.init_resource::<ComponentRegistry>()?;
        return Ok(());
    };
    let existing = app.world_mut()?.get_resource::<ComponentRegistry>();
    if let Some(existing) = existing {
        if !existing.shares_snapshot(snapshot) {
            return Err(PluginError::component_registration(
                COMPONENT_REGISTRY_PLUGIN_ID,
                "component-schema-catalog",
                "preloaded component registry does not match the admitted snapshot",
            ));
        }
        return Ok(());
    }
    app.insert_resource(ComponentRegistry::from_snapshot(snapshot.clone()))?;
    Ok(())
}

fn finish_component_registry(
    app: &mut App,
    expected: Option<&ComponentRegistrySnapshot>,
) -> Result<(), PluginError> {
    let (snapshot, instance_token) = {
        let world = app.world_mut()?;
        let mut registry = world.resource_mut::<ComponentRegistry>();
        let snapshot = registry
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
        (snapshot, registry.instance_token().clone())
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
    app.__install_managed_runtime_authority_validator(
        COMPONENT_REGISTRY_PLUGIN_ID,
        Arc::new(move |world, generation| authority.validate_managed(world, generation)),
    )?;
    Ok(())
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
    use super::*;

    #[test]
    fn preinstalled_registry_finishes_without_late_hook_registration() {
        let mut app = App::new();
        app.insert_resource(ComponentRegistry::default()).unwrap();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        let app = app.seal().unwrap();

        validate_component_registry_authority(app.world()).unwrap();
    }

    #[test]
    fn intrinsic_registry_hooks_track_same_tick_reinsert() {
        let mut world = World::new();
        world.insert_resource(ComponentRegistryMutationRevision::default());
        world.insert_resource(ComponentRegistry::default());
        let after_insert = world.resource::<ComponentRegistryMutationRevision>().0;

        let registry = world.remove_resource::<ComponentRegistry>().unwrap();
        let after_remove = world.resource::<ComponentRegistryMutationRevision>().0;
        world.insert_resource(registry);
        let after_reinsert = world.resource::<ComponentRegistryMutationRevision>().0;

        assert!(after_remove > after_insert);
        assert!(after_reinsert > after_remove);
    }
}
