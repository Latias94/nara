use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_ecs::{Mut, World};
use nara_identity::{SpawnedSceneInstance, WorldEntityToken};
use nara_reflect::{ComponentRegistry, component_registry, validate_component_registry_authority};
use nara_scene::{
    SceneSpawnReport,
    advanced::{
        SceneProductOverlayWriter, SceneProductTransactionLimits,
        replace_expanded_scene_with_product,
    },
};

use super::{
    CurrentSceneOwner, SceneSource, StartupSceneActivationAuthority, StartupSceneActivationPhase,
};
use std::sync::Arc;

/// Restarts the committed Scene from its own retained source at an exclusive simulation boundary.
///
/// `expected` must equal the complete current receipt, including its World identity. The callback
/// only stages bounded product data through the existing lifecycle-free Scene transaction. Rejection
/// leaves the current source and receipt unchanged. Success updates the private owner before this
/// function returns; the returned receipt is an observation of that same committed instance.
///
/// Restart may reuse a project-backed source because its content is already resident. It neither
/// loads another project closure nor reconciles arbitrary native service state.
#[must_use]
pub fn restart_current_scene_with_product<F>(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    replace_current(
        world,
        expected,
        None,
        limits,
        additional_retirements,
        configure,
    )
}

/// Replaces the committed data-only Scene and its retained source in one exclusive operation.
///
/// This operation currently requires both sources to be direct managed-App sources. Project-backed
/// replacement needs a separate residency preflight, publication and retirement contract; passing a
/// project source here is rejected before the callback or any Scene mutation. Use
/// [`restart_current_scene_with_product`] to restart already resident project content.
///
/// The exact expected receipt, registry authority, source coverage and bounded product writes are
/// validated before retiring the prior instance. This does not promise rollback of arbitrary native
/// effects and does not expose hosted project travel.
#[must_use]
pub fn replace_current_scene_with_product<F>(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    source: SceneSource,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    replace_current(
        world,
        expected,
        Some(source),
        limits,
        additional_retirements,
        configure,
    )
}

fn replace_current<F>(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    source: Option<SceneSource>,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    let registry = match current_scene_registry(world, expected) {
        Ok(registry) => registry,
        Err(report) => return report,
    };
    let owner = world.resource::<CurrentSceneOwner>();
    if source
        .as_ref()
        .is_some_and(|source| source.is_project_backed() || owner.source.is_project_backed())
    {
        return rejected(
            "scene.current.project-residency-required",
            "Project Scene replacement requires content residency preparation",
        );
    }
    if nara_ecs::__private::validate_resource_scope::<CurrentSceneOwner>(world).is_err() {
        return rejected(
            "scene.current.owner-lifecycle-unsupported",
            "Current Scene ownership cannot run lifecycle effects during replacement",
        );
    }
    world.resource_scope(|world, mut owner: Mut<CurrentSceneOwner>| {
        let source = source.map_or_else(|| Arc::clone(&owner.source), Arc::new);
        let report = replace_expanded_scene_with_product(
            world,
            &registry,
            &source.candidate,
            source.asset_database(),
            &owner.receipt,
            limits,
            additional_retirements,
            configure,
        );
        if !report.diagnostics.has_errors()
            && let Some(receipt) = &report.instance
        {
            owner.source = source;
            owner.receipt = receipt.clone();
        }
        report
    })
}

pub(crate) fn current_scene_registry(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<ComponentRegistry, SceneSpawnReport> {
    if !world
        .get_resource::<StartupSceneActivationAuthority>()
        .is_some_and(|authority| authority.phase == StartupSceneActivationPhase::Finalized)
    {
        return Err(rejected(
            "scene.current.startup-not-finalized",
            "Current Scene replacement requires completed startup",
        ));
    }
    let Some(owner) = world.get_resource::<CurrentSceneOwner>() else {
        return Err(rejected(
            "scene.current.owner-missing",
            "Current Scene ownership is unavailable",
        ));
    };
    if owner.receipt != *expected {
        return Err(rejected(
            "scene.current.expected-instance-mismatch",
            "Current Scene no longer matches the expected instance",
        ));
    }
    if validate_component_registry_authority(world).is_err() {
        return Err(rejected(
            "scene.current.component-registry-authority-invalid",
            "Current Scene component registry authority is invalid",
        ));
    }
    let Some(registry) = component_registry(world)
        .and_then(|registry| registry.snapshot().ok())
        .map(ComponentRegistry::from_snapshot)
    else {
        return Err(rejected(
            "scene.current.component-registry-unavailable",
            "Current Scene component registry is unavailable",
        ));
    };
    Ok(registry)
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
        SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
    ));
    report
}
