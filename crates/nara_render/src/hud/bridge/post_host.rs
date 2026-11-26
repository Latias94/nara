use nara_app::{AppRunError, RuntimeInstance, RuntimeState};
use nara_ecs::World;

use super::commit_runtime_ui_presentations;
use crate::hud::presentation::{UiPresentationPlannerClaim, presentation_planner_claim};

/// Applies presented HUD geometry after the native renderer returns its result.
pub struct UiPostHostConnection(UiPresentationPlannerClaim);

impl UiPostHostConnection {
    pub fn capture(runtime: &RuntimeInstance) -> Result<Option<Self>, AppRunError> {
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Err(AppRunError::runner(
                "HUD attachment requires a running runtime",
            ));
        }
        let world = runtime.world();
        if !world.contains_resource::<super::PresentationSequence>() {
            return Ok(None);
        }
        presentation_planner_claim(world)
            .map(Self)
            .map(Some)
            .ok_or_else(|| AppRunError::runner("the HUD presentation planner is unavailable"))
    }

    pub fn complete(&self, runtime: &mut RuntimeInstance) -> Result<(), AppRunError> {
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Ok(());
        }
        let claim = self.0.clone();
        let outcome = runtime
            .apply_command(move |world: &mut World| {
                if !claim.is_current(world) {
                    return Err(AppRunError::runner("the HUD presentation planner changed"));
                }
                commit_runtime_ui_presentations(world);
                Ok(())
            })
            .map_err(|error| AppRunError::runner(error.to_string()))?;
        match (outcome.output, outcome.post_operation_fault) {
            (result, None) => result,
            (Ok(()), Some(fault)) => Err(AppRunError::managed_runtime_fault(&fault)),
            (Err(error), Some(fault)) => Err(AppRunError::runner_teardown(
                error,
                AppRunError::managed_runtime_fault(&fault),
            )),
        }
    }
}
