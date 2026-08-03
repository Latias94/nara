use std::{
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{AppExitRequests, PluginCategory, PluginDeclaration, PluginError, PluginId},
    ecs::error::BevyError,
    fs::DirectoryCapability,
    image::PreparedImageResource,
    prelude::{App, CoreStage, Plugin, Res, ResMut, Resource},
    project_host::{DesktopRun, DesktopRunIntent},
    render::{PreparedRenderResources, RenderFrame},
    render_wgpu::WgpuRenderBackend,
    sprite_render::SpriteBatches,
};
use nara_reference_game::{REFERENCE_DESKTOP_PLUGIN_ID, desktop_wave_recipe};

use crate::desktop_support::submitted_product_frame;

const CANDIDATE_SMOKE_DEADLINE: Duration = Duration::from_secs(30);
const CANDIDATE_SMOKE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-candidate-smoke");
const CANDIDATE_SMOKE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const CANDIDATE_SMOKE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CANDIDATE_SMOKE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(CANDIDATE_SMOKE_REQUIREMENTS);

pub(crate) fn candidate_smoke_run(project_root: DirectoryCapability) -> DesktopRun {
    let recipe = candidate_smoke_recipe();
    DesktopRun::new(
        project_root,
        DesktopRunIntent::new()
            .with_profile("desktop")
            .with_recipe(recipe),
    )
}

fn candidate_smoke_recipe() -> ProductRecipe {
    desktop_wave_recipe()
        .and_then(|recipe| recipe.add_plugin::<CandidateSmokePlugin>())
        .expect("the candidate smoke recipe is statically valid")
}

#[derive(Debug, Default)]
struct CandidateSmokePlugin;

impl Plugin for CandidateSmokePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CANDIDATE_SMOKE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(CandidateSmokeState {
            deadline: Instant::now() + CANDIDATE_SMOKE_DEADLINE,
        })?
        .add_systems(CoreStage::Cleanup, observe_candidate_frame)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct CandidateSmokeState {
    deadline: Instant,
}

fn observe_candidate_frame(
    prepared: Res<PreparedRenderResources<PreparedImageResource>>,
    batches: Res<SpriteBatches>,
    frame: Res<RenderFrame>,
    backend: Res<WgpuRenderBackend>,
    state: Res<CandidateSmokeState>,
    mut exit: ResMut<AppExitRequests>,
) -> Result<(), BevyError> {
    let product_frame_ready = !prepared.is_empty()
        && batches
            .as_slice()
            .iter()
            .any(|batch| batch.material.image.is_some())
        && submitted_product_frame(&frame, &backend);
    if product_frame_ready {
        exit.request_success();
    } else if Instant::now() >= state.deadline {
        return Err(BevyError::error(CandidateSmokeDeadlineExceeded));
    }
    Ok(())
}

#[derive(Debug)]
struct CandidateSmokeDeadlineExceeded;

impl fmt::Display for CandidateSmokeDeadlineExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("desktop candidate did not submit a bounded product frame")
    }
}

impl Error for CandidateSmokeDeadlineExceeded {}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::ecs::{
        World,
        system::{RunSystemError, RunSystemOnce},
    };

    #[test]
    fn candidate_smoke_is_an_ordinary_recipe_contribution() {
        let recipe = candidate_smoke_recipe();
        let plugin_ids = recipe
            .entries()
            .map(|entry| entry.plugin_id())
            .collect::<Vec<_>>();

        assert!(plugin_ids.contains(&REFERENCE_DESKTOP_PLUGIN_ID));
        assert!(plugin_ids.contains(&CANDIDATE_SMOKE_PLUGIN_ID));
    }

    #[test]
    fn expired_candidate_frame_fails_without_requesting_exit() {
        let mut world = World::new();
        world.insert_resource(PreparedRenderResources::<PreparedImageResource>::default());
        world.insert_resource(SpriteBatches::default());
        world.insert_resource(RenderFrame::default());
        world.insert_resource(WgpuRenderBackend::default());
        world.insert_resource(CandidateSmokeState {
            deadline: Instant::now() - Duration::from_millis(1),
        });
        world.insert_resource(AppExitRequests::default());

        let result: Result<(), RunSystemError> = world.run_system_once(observe_candidate_frame);

        assert!(matches!(result, Err(RunSystemError::Failed(_))));
        assert_eq!(world.resource::<AppExitRequests>().requested(), None);
    }
}
