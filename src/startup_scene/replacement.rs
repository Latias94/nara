use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_ecs::{Mut, World};
use nara_identity::{SpawnedSceneInstance, WorldEntityToken};
use nara_scene::{
    SceneComponentBindings, SceneSpawnReport,
    advanced::{
        SceneAssetBindings, SceneNativeRetirementClaims, SceneProductOverlayWriter,
        SceneProductTransactionLimits,
    },
    scene_component_bindings, validate_scene_component_bindings_authority,
};

use super::{
    CurrentSceneOwner, RetainedSceneCandidate, SceneSource, StartupSceneActivationAuthority,
    StartupSceneActivationPhase,
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
/// Both Scenes may be expanded or processed Runtime inputs. First-party hosted content needs a
/// separate residency preflight, publication and retirement contract; replacing either side with
/// hosted content here is rejected before the callback or any Scene mutation. Independent
/// Runtime-content integrations retain their own resource custody. Use
/// [`restart_current_scene_with_product`] to restart already resident content.
///
/// The exact expected receipt, bindings authority, source coverage and bounded product writes are
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
    let bindings = match current_scene_bindings(world, expected) {
        Ok(bindings) => bindings,
        Err(report) => return report,
    };
    let owner = world.resource::<CurrentSceneOwner>();
    if source
        .as_ref()
        .is_some_and(|source| source.is_content_managed() || owner.source.is_content_managed())
    {
        return rejected(
            "scene.current.project-residency-required",
            "Hosted Scene replacement requires content residency preparation",
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
        if !source.matches_component_bindings(&bindings) {
            return rejected(
                "scene.current.component-bindings-mismatch",
                "Current Scene candidate does not match the installed component definitions",
            );
        }
        let assets = SceneAssetBindings::from_world(world);
        let prepared = match &source.candidate {
            RetainedSceneCandidate::Expanded(candidate) => bindings
                .prepare_scene_replacement_with_claims(
                    world,
                    candidate,
                    source.asset_resolution(),
                    &owner.receipt,
                    assets,
                    &SceneNativeRetirementClaims::default(),
                    limits,
                    additional_retirements,
                    configure,
                ),
            #[cfg(feature = "serde")]
            RetainedSceneCandidate::Runtime(candidate) => bindings
                .prepare_runtime_scene_replacement_with_claims(
                    world,
                    candidate,
                    &owner.receipt,
                    assets,
                    &SceneNativeRetirementClaims::default(),
                    limits,
                    additional_retirements,
                    configure,
                ),
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(report) => return report,
        };
        let report = match prepared.commit(world) {
            Ok(report) => report,
            Err((prepared, mut report)) => {
                if prepared.discard(world).is_err() {
                    report.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::new("scene.candidate-cleanup-incomplete")
                            .expect("engine-owned diagnostic codes are valid"),
                        SafeSummary::new("Rejected Scene candidate could not be safely discarded")
                            .expect("engine-owned diagnostic summaries are safe"),
                    ));
                }
                return report;
            }
        };
        if !report.diagnostics.has_errors()
            && let Some(receipt) = &report.instance
        {
            owner.source = source;
            owner.receipt = receipt.clone();
        }
        report
    })
}

pub(crate) fn current_scene_bindings(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<SceneComponentBindings, SceneSpawnReport> {
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
    if validate_scene_component_bindings_authority(world).is_err() {
        return Err(rejected(
            "scene.current.component-bindings-authority-invalid",
            "Current Scene component bindings authority is invalid",
        ));
    }
    let Some(bindings) = scene_component_bindings(world).cloned() else {
        return Err(rejected(
            "scene.current.component-bindings-unavailable",
            "Current Scene component bindings are unavailable",
        ));
    };
    Ok(bindings)
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
        SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
    ));
    report
}
