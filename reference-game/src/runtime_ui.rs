use nara::ecs as bevy_ecs;
use nara::ecs::schedule::IntoScheduleConfigs;
use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginProductCapability, PluginShutdownContext, PluginShutdownObligationId, RuntimeFault,
        RuntimeFaultKind, RuntimeFaultReporter,
    },
    ecs::{Commands, Component, Entity, Mut, Query, Res, ResMut, Resource, system::SystemParam},
    gameplay::{LocalGameplayCommands, add_local_gameplay_command_systems},
    input::{InputPointerId, InputSeatId, InputWindowId},
    scene::SceneEntitySource,
    ui::{
        UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal, UiButton, UiDimension,
        UiLayer, UiNode, UiRoot, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform,
        UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiText, install_ui_surface,
    },
};

use crate::{
    ReferenceWaveCaptureSet, WaveOutcome, WaveRetryPhase, WaveRetryRejection, WaveRetryStatus,
    WaveSnapshot,
    resources::WaveRunOwner,
    retry_draft_for_ui_action,
    ui_observation::{
        ReferenceProductObservation, capture_reference_product_observation,
        reference_ui_source_entity,
    },
};

pub const REFERENCE_RUNTIME_UI_PLUGIN_ID: PluginId = PluginId::new("reference-game.runtime-ui");
pub const REFERENCE_RETRY_ACTION: &str = "reference-game.retry";
pub const REFERENCE_UI_SURFACE_ID: u64 = 1;
const REFERENCE_UI_ACTION_CONSUMER: &str = "reference-game.runtime-ui";
const REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("reference-game.runtime-ui.action-consumer");
const LOGICAL_WIDTH: f32 = 960.0;
const LOGICAL_HEIGHT: f32 = 540.0;

const REFERENCE_RUNTIME_UI_REQUIREMENTS: &[PluginId] = &[
    crate::REFERENCE_WAVE_PLUGIN_ID,
    nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    nara::input::INPUT_PLUGIN_ID,
    nara::ui::UI_PLUGIN_ID,
];
const REFERENCE_RUNTIME_UI_PRODUCT_REQUIREMENTS: &[PluginProductCapability] =
    &[PluginProductCapability::new("runtime-ui")];
const REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATIONS: &[PluginShutdownObligationId] =
    &[REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATION];
const REFERENCE_RUNTIME_UI_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_RUNTIME_UI_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_RUNTIME_UI_REQUIREMENTS)
        .requires_product_capabilities(REFERENCE_RUNTIME_UI_PRODUCT_REQUIREMENTS)
        .shutdown_obligations(REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATIONS);

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceRuntimeUiPlugin;

impl Plugin for ReferenceRuntimeUiPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_RUNTIME_UI_DECLARATION
    }

    fn build(&self, app: &mut nara::prelude::App) -> Result<(), PluginError> {
        let surface_id = UiSurfaceId::new(REFERENCE_UI_SURFACE_ID)
            .expect("the reference UI surface ID is non-zero");
        let logical_size = UiSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT)
            .expect("the reference UI authored extent is valid");
        let transform = UiSurfaceContentTransform::new(
            logical_size,
            logical_size,
            UiSafeAreaInsets::default(),
            1.0,
        )
        .map_err(|error| setup_error(error.to_string()))?;
        let slot = UiSurfaceSlot::new(
            surface_id,
            UiSurfaceEndpoint::new(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            transform,
        )
        .map_err(|error| setup_error(error.to_string()))?;
        install_ui_surface(app.world_mut()?, slot)
            .map_err(|error| setup_error(error.to_string()))?;
        app.insert_resource(ReferenceHudProjection::default())?;
        app.insert_resource(ReferenceProductObservation::default())?;
        add_local_gameplay_command_systems(app, collect_retry_intents)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (project_runtime_ui, capture_reference_product_observation)
                .chain()
                .in_set(ReferenceWaveCaptureSet::Presentation),
        )?;
        let consumer = UiActionConsumerId::new(REFERENCE_UI_ACTION_CONSUMER)
            .map_err(|error| setup_error(error.to_string()))?;
        if app.world().contains_resource::<ReferenceUiActionCursor>() {
            return Err(setup_error(
                "reference UI action consumer is already installed".to_owned(),
            ));
        }
        app.register_plugin_shutdown_obligation(REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATION)?;
        let cursor = app
            .world_mut()?
            .get_resource_mut::<UiActionJournal>()
            .ok_or_else(|| setup_error("UI action journal is not installed".to_owned()))?
            .register_consumer(consumer)
            .map_err(|error| setup_error(error.to_string()))?;
        app.insert_resource(ReferenceUiActionCursor(cursor))?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        if !context
            .world()
            .contains_resource::<ReferenceUiActionCursor>()
        {
            return Ok(());
        }
        if !context.world().contains_resource::<UiActionJournal>() {
            return Err(setup_error("UI action journal is not installed".to_owned()));
        }
        context.world_mut().resource_scope(
            |world, mut journal: Mut<UiActionJournal>| -> Result<(), PluginError> {
                let cursor = world
                    .get_resource::<ReferenceUiActionCursor>()
                    .expect("validated UI action cursor remains installed");
                journal
                    .unregister_consumer(&cursor.0)
                    .map_err(|error| setup_error(error.to_string()))
            },
        )?;
        context
            .world_mut()
            .remove_resource::<ReferenceUiActionCursor>();
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct ReferenceUiActionCursor(UiActionCursor);

#[derive(Debug, Default, Clone, Copy, PartialEq, Resource)]
pub struct ReferenceHudProjection {
    pub health_current: i64,
    pub health_maximum: i64,
    pub health_width: f32,
    pub defeated_enemies: u64,
    pub planned_enemies: u64,
    pub progress_width: f32,
    pub outcome: WaveOutcome,
    pub terminal_visible: bool,
    pub retry_enabled: bool,
}

type RuntimeUiEntity<'a> = (
    Entity,
    &'a SceneEntitySource,
    Option<&'a mut UiRoot>,
    Option<&'a mut UiNode>,
    Option<&'a mut UiText>,
    Option<&'a mut UiButton>,
    Option<&'a ReferenceAuthoredHealthWidth>,
);

#[derive(Debug, Clone, Copy, Component)]
struct ReferenceAuthoredHealthWidth(f32);

fn project_runtime_ui(
    mut commands: Commands,
    snapshot: Res<WaveSnapshot>,
    retry: Res<WaveRetryStatus>,
    owner: Option<Res<WaveRunOwner>>,
    mut projection: ResMut<ReferenceHudProjection>,
    mut entities: Query<RuntimeUiEntity<'_>>,
) {
    let Some(owner) = owner else {
        return;
    };
    let health_maximum = 20;
    let health_current = snapshot.player.hit_points.clamp(0, health_maximum);
    let health_ratio = health_current as f32 / health_maximum as f32;
    let progress_ratio = if snapshot.planned_enemies == 0 {
        0.0
    } else {
        (snapshot.defeated_enemies as f32 / snapshot.planned_enemies as f32).clamp(0.0, 1.0)
    };
    let terminal_visible = snapshot.outcome.is_terminal();
    let retry_enabled = terminal_visible && retry.phase() != WaveRetryPhase::Pending;
    let mut next_projection = ReferenceHudProjection {
        health_current,
        health_maximum,
        health_width: 0.0,
        defeated_enemies: snapshot.defeated_enemies,
        planned_enemies: snapshot.planned_enemies,
        progress_width: 0.0,
        outcome: snapshot.outcome,
        terminal_visible,
        retry_enabled,
    };

    for (entity, source, root, node, text, button, authored_health_width) in &mut entities {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        let Some(source_entity) = reference_ui_source_entity(source) else {
            continue;
        };
        match source_entity {
            "health-fill" => {
                if let Some(mut node) = node {
                    let authored_width = authored_health_width.map_or_else(
                        || match node.style().width() {
                            UiDimension::Px(width) => {
                                commands
                                    .entity(entity)
                                    .insert(ReferenceAuthoredHealthWidth(width));
                                width
                            }
                            UiDimension::Auto | UiDimension::Percent(_) => 0.0,
                        },
                        |width| width.0,
                    );
                    next_projection.health_width = authored_width * health_ratio;
                    next_projection.progress_width = authored_width * progress_ratio;
                    *node = UiNode::new(
                        node.style()
                            .with_width(UiDimension::Px(next_projection.health_width)),
                    );
                }
            }
            "health-label" => replace_text_content(
                text,
                format!("HEALTH {health_current:02} / {health_maximum:02}"),
            ),
            "wave-status" => replace_text_content(
                text,
                format!(
                    "WAVE {:02} / {:02}",
                    snapshot.defeated_enemies, snapshot.planned_enemies
                ),
            ),
            "score-status" => replace_text_content(text, format!("SCORE {:06}", snapshot.score)),
            "movement-status" => replace_text_content(
                text,
                format!(
                    "MOVE {:+05.1} {:+05.1}",
                    snapshot.player.position.x, snapshot.player.position.y
                ),
            ),
            "autofire-status" => replace_text_content(
                text,
                format!("AUTO-FIRE ACTIVE  SHOTS {:02}", snapshot.projectiles.len()),
            ),
            "terminal-overlay" => {
                if let Some(mut root) = root {
                    *root = UiRoot::new(if terminal_visible {
                        UiLayer::Modal
                    } else {
                        UiLayer::Overlay
                    });
                }
                if let Some(mut node) = node {
                    *node = UiNode::new(node.style().with_visible(terminal_visible));
                }
            }
            "outcome-label" => replace_text_content(
                text,
                match snapshot.outcome {
                    WaveOutcome::Completed => "VICTORY",
                    WaveOutcome::Defeated => "DEFEAT",
                    WaveOutcome::Running => "",
                },
            ),
            "run-summary" => replace_text_content(
                text,
                format!(
                    "FINAL SCORE {:06}  ENEMIES {:02}/{:02}",
                    snapshot.score, snapshot.defeated_enemies, snapshot.planned_enemies
                ),
            ),
            "retry-button" => {
                if let Some(mut button) = button {
                    *button = button.clone().with_enabled(retry_enabled);
                }
            }
            "input-hint" => replace_text_content(text, retry_hint(*retry)),
            _ => {}
        }
    }
    *projection = next_projection;
}

fn replace_text_content(text: Option<nara::ecs::Mut<'_, UiText>>, content: impl Into<String>) {
    if let Some(mut text) = text {
        *text = text.clone().with_content(content);
    }
}

fn retry_hint(retry: WaveRetryStatus) -> &'static str {
    if retry.phase() == WaveRetryPhase::Pending {
        return "Retry is pending";
    }
    match retry.last_rejection() {
        Some(WaveRetryRejection::WhileRunning) => "Retry is available after the wave",
        Some(WaveRetryRejection::AlreadyPending) => "Retry is already pending",
        Some(WaveRetryRejection::GenerationExhausted) => "Retry generation was exhausted",
        Some(WaveRetryRejection::InvalidAuthoredConfiguration) => {
            "Retry rejected invalid authored data"
        }
        Some(WaveRetryRejection::ReplacementRejected) => "Retry replacement was rejected",
        None => "Press Enter or activate Retry",
    }
}

#[derive(SystemParam)]
struct ReferenceRetryIntentContext<'w, 's> {
    snapshot: Res<'w, WaveSnapshot>,
    retry: Res<'w, WaveRetryStatus>,
    owner: Option<Res<'w, WaveRunOwner>>,
    buttons: Query<'w, 's, (Entity, &'static SceneEntitySource, &'static UiButton)>,
}

impl ReferenceRetryIntentContext<'_, '_> {
    fn authored_retry_action(&self) -> Option<UiActionId> {
        let owner = self.owner.as_deref()?;
        self.buttons
            .iter()
            .find(|(entity, source, _)| {
                reference_ui_source_entity(source) == Some("retry-button")
                    && owner.owns_scene_entity(*entity, source)
            })
            .map(|(_, _, button)| button.action().clone())
    }
}

fn collect_retry_intents(
    mut ui_actions: ResMut<UiActionJournal>,
    ui_cursor: Res<ReferenceUiActionCursor>,
    context: ReferenceRetryIntentContext<'_, '_>,
    mut commands: ResMut<LocalGameplayCommands>,
    faults: Res<RuntimeFaultReporter>,
) {
    let retry_action = context.authored_retry_action();
    let (semantic_sequence, last_sequence) = match ui_actions.pending(&ui_cursor.0) {
        Ok(pending) => (
            pending
                .iter()
                .find(|event| {
                    retry_action
                        .as_ref()
                        .is_some_and(|action| event.action() == action)
                })
                .map(|event| event.sequence().get()),
            pending.last().map(|event| event.sequence()),
        ),
        Err(_) => {
            faults.report(RuntimeFault::engine(
                RuntimeFaultKind::System,
                "reference-game.ui-action-cursor",
            ));
            return;
        }
    };
    let retry_is_eligible =
        context.snapshot.is_terminal() && context.retry.phase() != WaveRetryPhase::Pending;
    if retry_is_eligible
        && let Some(sequence) = semantic_sequence
        && commands.push(retry_draft_for_ui_action(sequence)).is_err()
    {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::LocalIntentLoss,
            "reference-game.retry-intent",
        ));
        return;
    }
    if let Some(sequence) = last_sequence
        && ui_actions.acknowledge(&ui_cursor.0, sequence).is_err()
    {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::System,
            "reference-game.ui-action-acknowledgement",
        ));
    }
}

fn setup_error(message: String) -> PluginError {
    PluginError::SetupFailed {
        plugin: REFERENCE_RUNTIME_UI_PLUGIN_ID,
        message,
    }
}
