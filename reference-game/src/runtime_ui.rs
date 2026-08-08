use nara::ecs::schedule::IntoScheduleConfigs;
use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginProductCapability, PluginShutdownContext, PluginShutdownObligationId, RuntimeFault,
        RuntimeFaultKind, RuntimeFaultReporter,
    },
    ecs::{Entity, Mut, Query, Res, ResMut, Resource},
    gameplay::{LocalGameplayCommands, add_local_gameplay_command_systems},
    input::{ActionOutcomes, ActionPhase, InputSeatId, InputWindowId},
    scene::SceneEntitySource,
    ui::{
        UiActionConsumerId, UiActionCursor, UiActionJournal, UiBrush, UiButton, UiColorRole,
        UiNode, UiStyle, UiSurfaceDescriptor, UiSurfaceId, UiText, UiTextAlignment, UiTextOverflow,
        UiVisual, install_ui_surface,
    },
};

use crate::{
    ReferenceWaveCaptureSet, WaveOutcome, WaveRetryPhase, WaveRetryRejection, WaveRetryStatus,
    WaveSnapshot, resources::WaveRunOwner, retry_draft,
};

pub const REFERENCE_RUNTIME_UI_PLUGIN_ID: PluginId = PluginId::new("reference-game.runtime-ui");
pub(crate) const REFERENCE_RETRY_ACTION: &str = "reference-game.retry";
pub const REFERENCE_UI_SURFACE_ID: u64 = 1;
const REFERENCE_UI_ACTION_CONSUMER: &str = "reference-game.runtime-ui";
const REFERENCE_RUNTIME_UI_SHUTDOWN_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("reference-game.runtime-ui.action-consumer");
const LOGICAL_WIDTH: f32 = 960.0;
const LOGICAL_HEIGHT: f32 = 540.0;
const HEALTH_BAR_WIDTH: f32 = 280.0;

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
        let descriptor = UiSurfaceDescriptor::new(
            surface_id,
            InputWindowId::new(1),
            InputSeatId::PRIMARY,
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
            1.0,
        )
        .map_err(|error| setup_error(error.to_string()))?;
        install_ui_surface(app.world_mut()?, descriptor)
            .map_err(|error| setup_error(error.to_string()))?;
        app.insert_resource(ReferenceHudProjection::default())?;
        add_local_gameplay_command_systems(app, collect_retry_intents)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            project_runtime_ui.in_set(ReferenceWaveCaptureSet::Presentation),
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
        if !context.world().contains_resource::<ReferenceUiActionCursor>() {
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
    Option<&'a mut UiNode>,
    Option<&'a mut UiText>,
    Option<&'a mut UiVisual>,
    Option<&'a mut UiButton>,
);

fn project_runtime_ui(
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
    *projection = ReferenceHudProjection {
        health_current,
        health_maximum,
        health_width: HEALTH_BAR_WIDTH * health_ratio,
        defeated_enemies: snapshot.defeated_enemies,
        planned_enemies: snapshot.planned_enemies,
        progress_width: HEALTH_BAR_WIDTH * progress_ratio,
        outcome: snapshot.outcome,
        terminal_visible,
        retry_enabled,
    };

    for (entity, source, node, text, visual, button) in &mut entities {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        match source.source_entity_id.as_str() {
            "health-fill" => {
                if let Some(mut node) = node {
                    *node = UiNode::new(
                        UiStyle::absolute(0.0, 0.0, projection.health_width, 32.0).with_z_index(1),
                    );
                }
                if let Some(mut visual) = visual {
                    *visual = UiVisual::role(if health_current == 0 {
                        UiColorRole::Danger
                    } else {
                        UiColorRole::Positive
                    });
                }
            }
            "health-label" => replace_text(
                text,
                format!("HEALTH {health_current:02} / {health_maximum:02}"),
                16.0,
                UiColorRole::PrimaryText,
                UiTextAlignment::Start,
            ),
            "wave-status" => replace_text(
                text,
                format!(
                    "WAVE {:02} / {:02}",
                    snapshot.defeated_enemies, snapshot.planned_enemies
                ),
                16.0,
                UiColorRole::PrimaryText,
                UiTextAlignment::Start,
            ),
            "score-status" => replace_text(
                text,
                format!("SCORE {:06}", snapshot.score),
                16.0,
                UiColorRole::Warning,
                UiTextAlignment::Start,
            ),
            "movement-status" => replace_text(
                text,
                format!(
                    "MOVE {:+05.1} {:+05.1}",
                    snapshot.player.position.x, snapshot.player.position.y
                ),
                16.0,
                UiColorRole::SecondaryText,
                UiTextAlignment::Start,
            ),
            "autofire-status" => replace_text(
                text,
                format!("AUTO-FIRE ACTIVE  SHOTS {:02}", snapshot.projectiles.len()),
                16.0,
                UiColorRole::Accent,
                UiTextAlignment::Start,
            ),
            "terminal-overlay" => {
                if let Some(mut node) = node {
                    *node = UiNode::new(
                        UiStyle::fill()
                            .with_visible(terminal_visible)
                            .with_stacking_context(true),
                    );
                }
            }
            "outcome-label" => replace_text(
                text,
                match snapshot.outcome {
                    WaveOutcome::Completed => "VICTORY",
                    WaveOutcome::Defeated => "DEFEAT",
                    WaveOutcome::Running => "",
                },
                28.0,
                if snapshot.outcome == WaveOutcome::Completed {
                    UiColorRole::Positive
                } else {
                    UiColorRole::Danger
                },
                UiTextAlignment::Center,
            ),
            "run-summary" => replace_text(
                text,
                format!(
                    "FINAL SCORE {:06}  ENEMIES {:02}/{:02}",
                    snapshot.score, snapshot.defeated_enemies, snapshot.planned_enemies
                ),
                18.0,
                UiColorRole::PrimaryText,
                UiTextAlignment::Center,
            ),
            "retry-button" => {
                if let Some(mut button) = button {
                    *button = UiButton::new(
                        if retry.phase() == WaveRetryPhase::Pending {
                            "Retrying..."
                        } else {
                            "Retry"
                        },
                        nara::ui::UiActionId::new(REFERENCE_RETRY_ACTION)
                            .expect("the reference retry action is valid"),
                        18.0,
                    )
                    .expect("the reference retry button is valid")
                    .with_enabled(retry_enabled);
                }
            }
            "input-hint" => replace_text(
                text,
                retry_hint(*retry),
                14.0,
                if retry.last_rejection().is_some() {
                    UiColorRole::Danger
                } else {
                    UiColorRole::SecondaryText
                },
                UiTextAlignment::Center,
            ),
            _ => {}
        }
    }
}

fn replace_text(
    text: Option<nara::ecs::Mut<'_, UiText>>,
    content: impl Into<String>,
    size: f32,
    role: UiColorRole,
    alignment: UiTextAlignment,
) {
    if let Some(mut text) = text {
        let content = content.into();
        *text = UiText::new(content.clone(), size)
            .expect("reference UI text facts are bounded")
            .with_brush(UiBrush::Role(role))
            .with_alignment(alignment)
            .with_overflow(UiTextOverflow::Clip)
            .with_semantic_text(content);
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

fn collect_retry_intents(
    outcomes: Option<Res<ActionOutcomes>>,
    mut ui_actions: ResMut<UiActionJournal>,
    ui_cursor: Res<ReferenceUiActionCursor>,
    snapshot: Res<WaveSnapshot>,
    retry: Res<WaveRetryStatus>,
    mut commands: ResMut<LocalGameplayCommands>,
    faults: Res<RuntimeFaultReporter>,
) {
    let keyboard = outcomes.is_some_and(|outcomes| {
        outcomes.as_slice().iter().any(|outcome| {
            outcome.action.as_str() == REFERENCE_RETRY_ACTION
                && outcome.phase == ActionPhase::Started
        })
    });
    let (semantic, last_sequence) = match ui_actions.pending(&ui_cursor.0) {
        Ok(pending) => (
            pending
                .iter()
                .any(|event| event.action().as_str() == REFERENCE_RETRY_ACTION),
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
    let retry_is_eligible = snapshot.is_terminal() && retry.phase() != WaveRetryPhase::Pending;
    if retry_is_eligible && (keyboard || semantic) && commands.push(retry_draft()).is_err() {
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
