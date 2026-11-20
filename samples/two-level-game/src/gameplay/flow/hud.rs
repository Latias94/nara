//! Presentation of the opt-in round flow through the existing ECS UI adapter.

use nara::{
    ProductRecipe, ProductRecipeError,
    app::{PluginCategory, PluginDeclaration, PluginId, StartupStage},
    ecs::{Entity, World, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    hierarchy::{
        HIERARCHY_PLUGIN_ID, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
        HierarchyWriter,
    },
    prelude::{App, CoreStage, Plugin, PluginError, Resource},
    ui::{
        UI_PLUGIN_ID, UiBrush, UiColor, UiDimension, UiLayer, UiNode, UiOverflow, UiRoot,
        UiRuntimeKey, UiSemanticRole, UiSemantics, UiStyle, UiSurfaceId, UiText, UiVisual,
    },
    ui_render::UI_RENDER_PLUGIN_ID,
};

use super::{FlowPhase, FlowSnapshot};

const ID: PluginId = PluginId::new("two-level-game.play-flow.hud");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[
        super::ID,
        UI_PLUGIN_ID,
        HIERARCHY_PLUGIN_ID,
        UI_RENDER_PLUGIN_ID,
    ]);
const DOMAIN: &str = "two-level-game.play-flow.hud";
const MAX_ERROR_CHARS: usize = 512;
const FOREGROUND: UiColor = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
const ERROR_COLOR: UiColor = UiColor::rgba(1.0, 0.65, 0.65, 1.0);
const PANEL_STYLE: UiStyle =
    UiStyle::absolute(16.0, 16.0, 928.0, 112.0).with_overflow(UiOverflow::Clip);
const ERROR_STYLE: UiStyle =
    UiStyle::absolute(16.0, 108.0, 896.0, 60.0).with_overflow(UiOverflow::Clip);

/// Stable entities for inspecting the original HUD and its presented semantic keys.
///
/// Runtime keys use the domain `two-level-game.play-flow.hud` and the local names `root`,
/// `panel`, `title`, `goals`, `controls`, and `error`. The root supplies Group semantics;
/// each text node supplies Text semantics from its current display content. The selected
/// presentation profile owns primary surface 1 and its 960 by 540 logical coordinate space.
#[derive(Debug, Resource)]
pub struct PlayFlowHud {
    pub surface: UiSurfaceId,
    pub root: Entity,
    pub panel: Entity,
    pub title: Entity,
    pub goals: Entity,
    pub controls: Entity,
    pub error: Entity,
    snapshot: Option<FlowSnapshot>,
    control_instructions: &'static str,
}

impl PlayFlowHud {
    /// Control instructions authored by the latest game HUD update.
    #[must_use]
    pub const fn control_instructions(&self) -> &'static str {
        self.control_instructions
    }
}

/// Adds only the HUD plugin; the caller installs the round flow and UI presentation profile.
pub fn with_play_flow_hud(recipe: ProductRecipe) -> Result<ProductRecipe, ProductRecipeError> {
    recipe.add_plugin::<HudPlugin>()
}

#[derive(Default)]
struct HudPlugin;

impl Plugin for HudPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, initialize.after(super::initialize))?;
        app.add_systems(CoreStage::Update, update.after(super::advance))?;
        Ok(())
    }
}

fn initialize(world: &mut World) -> Result<(), BevyError> {
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Overlay),
            UiNode::new(UiStyle::fill()),
            runtime_key("root"),
            UiSemantics::new(UiSemanticRole::Group, "Two-Level Exploration HUD")
                .expect("the HUD group has a fixed nonempty semantic name"),
        ))
        .id();
    let panel = world
        .spawn((
            UiNode::new(PANEL_STYLE),
            UiVisual::solid(UiColor::rgba(0.025, 0.035, 0.06, 0.98)),
            runtime_key("panel"),
        ))
        .id();
    let [title, goals, controls, error] = [
        (
            "title",
            UiStyle::absolute(16.0, 12.0, 896.0, 32.0),
            24.0,
            FOREGROUND,
        ),
        (
            "goals",
            UiStyle::absolute(16.0, 48.0, 896.0, 24.0),
            18.0,
            FOREGROUND,
        ),
        (
            "controls",
            UiStyle::absolute(16.0, 76.0, 896.0, 24.0),
            16.0,
            FOREGROUND,
        ),
        ("error", ERROR_STYLE.with_visible(false), 16.0, ERROR_COLOR),
    ]
    .map(|(local, style, size, color)| {
        world
            .spawn((
                UiNode::new(style.with_overflow(UiOverflow::Clip)),
                text("", size, color),
                runtime_key(local),
            ))
            .id()
    });

    let mut placements = vec![
        HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
        HierarchyOperation::Place {
            entity: panel,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
        },
    ];
    placements.extend(
        [title, goals, controls, error].map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: panel }),
        }),
    );
    HierarchyWriter::new(world).apply_batch(&placements)?;
    world.insert_resource(PlayFlowHud {
        surface: UiSurfaceId::new(1).expect("the primary HUD surface ID is nonzero"),
        root,
        panel,
        title,
        goals,
        controls,
        error,
        snapshot: None,
        control_instructions: "",
    });
    update(world);
    Ok(())
}

fn update(world: &mut World) {
    let snapshot = world.resource::<FlowSnapshot>();
    let hud = world.resource::<PlayFlowHud>();
    let previous = hud.snapshot.as_ref();
    let phase_changed = previous.is_none_or(|last| last.phase != snapshot.phase);
    let title_changed = phase_changed || previous.is_none_or(|last| last.round != snapshot.round);
    let goals_changed = previous.is_none_or(|last| {
        last.meadow_collected != snapshot.meadow_collected
            || last.cavern_collected != snapshot.cavern_collected
    });
    let error_changed = previous.is_none_or(|last| last.error != snapshot.error);
    if !title_changed && !goals_changed && !error_changed {
        return;
    }
    let snapshot = snapshot.clone();
    let [panel, title, goals, controls, error] =
        [hud.panel, hud.title, hud.goals, hud.controls, hud.error];

    if title_changed {
        let phase = match snapshot.phase {
            FlowPhase::Boot => "Boot",
            FlowPhase::Title => "Title",
            FlowPhase::Starting => "Starting",
            FlowPhase::Playing => "Playing",
            FlowPhase::Paused => "Paused",
            FlowPhase::Resuming => "Resuming",
            FlowPhase::Complete => "Complete",
            FlowPhase::Degraded => "Degraded",
            FlowPhase::Exiting => "Exiting",
        };
        world.entity_mut(title).insert(text(
            format!("Two-Level Exploration | {phase} | Round {}", snapshot.round),
            24.0,
            FOREGROUND,
        ));
    }
    if goals_changed {
        let collected = |value| if value { "collected" } else { "needed" };
        world.entity_mut(goals).insert(text(
            format!(
                "Goal: 1 pickup per main level | Meadow: {} | Cavern: {}",
                collected(snapshot.meadow_collected),
                collected(snapshot.cavern_collected),
            ),
            18.0,
            FOREGROUND,
        ));
    }
    if phase_changed {
        let gamepad = world.resource::<super::Flow>().gamepad_menu.is_some();
        let instructions = match (snapshot.phase, gamepad) {
            (FlowPhase::Boot, _) => "Loading the game... | X: exit",
            (FlowPhase::Title, false) => "Enter: start | X: exit",
            (FlowPhase::Title, true) => "Enter / Start: start | X / Select: exit",
            (FlowPhase::Starting, _) => "Starting a new round... | X: exit",
            (FlowPhase::Playing, false) => {
                "WASD: move / collect gems | E: travel | Escape: pause | X: exit"
            }
            (FlowPhase::Playing, true) => {
                "WASD / D-pad: move | E / South: travel | Escape / Start: pause | X: exit"
            }
            (FlowPhase::Paused, false) => "Enter / Escape: continue | T: title | X: exit",
            (FlowPhase::Paused, true) => {
                "Enter / Escape / Start: continue | T / East: title | X / Select: exit"
            }
            (FlowPhase::Resuming, _) => "Resuming the round... | X: exit",
            (FlowPhase::Complete, false) => "Enter: restart | T: title | X: exit",
            (FlowPhase::Complete, true) => {
                "Enter / Start: restart | T / East: title | X / Select: exit"
            }
            (FlowPhase::Degraded, false) => "Enter: new round | T: title | X: exit",
            (FlowPhase::Degraded, true) => {
                "Enter / Start: new round | T / East: title | X / Select: exit"
            }
            (FlowPhase::Exiting, _) => "Exiting the game...",
        };
        world
            .entity_mut(controls)
            .insert(text(instructions, 16.0, FOREGROUND));
        world.resource_mut::<PlayFlowHud>().control_instructions = instructions;
    }
    if error_changed {
        let visible = snapshot.error.is_some();
        let content = snapshot
            .error
            .as_deref()
            .map_or_else(String::new, display_error);
        world.entity_mut(error).insert((
            text(content, 16.0, ERROR_COLOR),
            UiNode::new(ERROR_STYLE.with_visible(visible)),
        ));
        world.entity_mut(panel).insert(UiNode::new(
            PANEL_STYLE.with_height(UiDimension::Px(if visible { 180.0 } else { 112.0 })),
        ));
    }
    world.resource_mut::<PlayFlowHud>().snapshot = Some(snapshot);
}

fn display_error(error: &str) -> String {
    let mut chars = "Error: ".chars().chain(error.chars());
    let mut content: String = chars.by_ref().take(MAX_ERROR_CHARS - 1).collect();
    if let Some(last) = chars.next() {
        content.push(if chars.next().is_some() { '…' } else { last });
    }
    content
}

fn runtime_key(local: &str) -> UiRuntimeKey {
    UiRuntimeKey::new(DOMAIN, local).expect("the HUD runtime keys are fixed and valid")
}

fn text(content: impl Into<String>, size: f32, color: UiColor) -> UiText {
    UiText::new(content, size)
        .expect("the HUD uses positive logical font sizes")
        .with_brush(UiBrush::Solid(color))
}
