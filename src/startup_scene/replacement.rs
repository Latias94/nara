use nara_core::diagnostic::{DiagnosticCode, ReportDiagnostic as Diagnostic, SafeSummary};
use nara_ecs::{World, identity::SpawnedSceneInstance};
use nara_scene::{
    SceneSpawnReport,
    advanced::{
        SceneAssetReservations, SceneProductOverlayWriter, SceneProductRetirement,
        SceneProductTransactionLimits,
    },
    current_scene::{current_scene_bindings, stage_current_scene_replacement},
};

use super::{CurrentScene, SceneSource};

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
    additional_retirements: &[SceneProductRetirement<'_>],
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
    additional_retirements: &[SceneProductRetirement<'_>],
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
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    if let Err(report) = current_scene_bindings(world, expected) {
        return report;
    }
    #[cfg(feature = "serde")]
    if crate::project_host::has_active_project_scene_addition(world) {
        return rejected(
            "project.scene.addition-active",
            "Unload active project Scene additions before restarting the retained primary Scene",
        );
    }
    if source.as_ref().is_some_and(|source| {
        source.is_content_managed()
            || world
                .resource::<CurrentScene>()
                .retained_source()
                .is_content_managed()
    }) {
        return rejected(
            "scene.current.project-residency-required",
            "Hosted Scene replacement requires content residency preparation",
        );
    }
    let assets = SceneAssetReservations::from_world(world);
    let staged = match stage_current_scene_replacement(
        world,
        expected,
        source,
        assets,
        limits,
        additional_retirements,
        configure,
    ) {
        Ok(staged) => staged,
        Err(report) => return report,
    };
    match staged.commit(world) {
        Ok(report) => report,
        Err((staged, mut report)) => {
            if staged.discard(world).is_err() {
                report.diagnostics.extend(
                    rejected(
                        "scene.candidate-cleanup-incomplete",
                        "Rejected Scene candidate could not be safely discarded",
                    )
                    .diagnostics,
                );
            }
            report
        }
    }
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
        SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
    ));
    report
}
