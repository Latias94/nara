use nara::{
    ecs::World,
    input::{InputSeatId, InputTimeline, InputWindowId},
    ui::{UiSemanticRole, advanced::UiPaintPrimitive},
    ui_render::advanced::latest_presented_ui_observation,
    window::WindowId,
};
use serde_json::{Value, json};
use two_level_game::{
    delivery::GameFlow,
    flow::{FlowPhase, FlowSnapshot, PlayFlowHud},
};

use super::{Frame, output};

pub(super) struct Observation {
    game_flow: GameFlow,
    snapshot: FlowSnapshot,
    input_focused: bool,
    presented_ui: Option<PresentedHud>,
}

struct PresentedHud {
    heading: Option<String>,
    text: Vec<String>,
    matches_snapshot: bool,
    submitted_glyphs: u64,
    receipt: Value,
}

impl Observation {
    pub(super) fn facts(&self) -> Value {
        let mut facts = self.semantic();
        if let Some(ui) = &self.presented_ui {
            facts["presented_ui"]["receipt"] = ui.receipt.clone();
        }
        facts
    }

    pub(super) fn semantic(&self) -> Value {
        json!({
            "game_flow": self.game_flow.name(),
            "phase": format!("{:?}", self.snapshot.phase),
            "round": self.snapshot.round,
            "input_focused": self.input_focused,
            "goals": {
                "meadow_collected": self.snapshot.meadow_collected,
                "cavern_collected": self.snapshot.cavern_collected,
            },
            "error": self.snapshot.error.as_deref().map(output::bounded),
            "presented_ui": self.presented_ui.as_ref().map(|ui| json!({
                "heading": ui.heading,
                "text": ui.text,
                "matches_snapshot": ui.matches_snapshot,
            })),
        })
    }

    pub(super) fn initial_title_presented(&self) -> bool {
        self.snapshot.phase == FlowPhase::Title
            && self.snapshot.round == 0
            && self.snapshot.error.is_none()
            && self.presented_ui.as_ref().is_some_and(|ui| {
                ui.matches_snapshot
                    && ui.submitted_glyphs > 0
                    && ui.text.iter().any(|text| text == "Enter: start | X: exit")
            })
    }

    pub(super) fn current_presented(&self) -> bool {
        self.snapshot.error.is_none()
            && self
                .presented_ui
                .as_ref()
                .is_some_and(|ui| ui.matches_snapshot && ui.submitted_glyphs > 0)
    }
}

pub(super) fn observe(world: &World, frame: &Frame, game_flow: GameFlow) -> Option<Observation> {
    let snapshot = world.get_resource::<FlowSnapshot>()?.clone();
    let input_focused = world
        .get_resource::<InputTimeline>()?
        .endpoint_observation(
            InputWindowId::new(WindowId::PRIMARY.get()),
            InputSeatId::PRIMARY,
        )
        .is_some_and(|endpoint| endpoint.focused());
    let presented_ui = join_presented_ui(world, frame, &snapshot);
    Some(Observation {
        game_flow,
        snapshot,
        input_focused,
        presented_ui,
    })
}

fn join_presented_ui(
    world: &World,
    frame: &Frame,
    snapshot: &FlowSnapshot,
) -> Option<PresentedHud> {
    let ui = latest_presented_ui_observation(world)?;
    let gpu = ui.wgpu()?;
    let hud = world.get_resource::<PlayFlowHud>()?;
    let receipt = ui.receipt();
    let submission = ui.submission();
    if gpu.app_frame_index() != frame.app_frame
        || gpu.render_transaction_frame_index() != frame.render_frame
        || submission.runtime_generation().get() != frame.runtime_generation
        || submission.target().window_id(None) != Some(WindowId::PRIMARY)
        || ui.backend_generation().backend_instance_id() != frame.backend_instance
        || ui.backend_generation().device_epoch() != frame.device_epoch
        || !submission.matches_receipt(receipt)
        || submission.packet() != ui.packet().identity()
        || receipt.surface_id() != hud.surface
        || receipt.presentation_epoch() == 0
    {
        return None;
    }
    let presented = ui.frame();
    let root = presented
        .semantics()
        .iter()
        .find(|node| {
            node.role() == UiSemanticRole::Group && node.name() == "Two-Level Exploration HUD"
        })?
        .key()
        .root_key();
    let text: Vec<String> = presented
        .semantics()
        .iter()
        .filter(|node| {
            node.key().root_key() == root
                && node.role() == UiSemanticRole::Text
                && !node.rect().size().is_empty()
                && node
                    .clip_rect()
                    .is_none_or(|clip| node.rect().intersect(clip).is_some())
                && presented.paint().iter().any(|primitive| {
                    matches!(primitive, UiPaintPrimitive::Text {
                        key: Some(key), rect, clip_rect, color, layout, ..
                    } if key == node.key()
                        && !rect.size().is_empty()
                        && clip_rect.is_none_or(|clip| rect.intersect(clip).is_some())
                        && color.components()[3] > 0.0
                        && !layout.glyphs().is_empty())
                })
        })
        .map(|node| node.name().to_owned())
        .collect();
    let heading = text
        .iter()
        .find(|text| text.starts_with("Two-Level Exploration | "))
        .cloned();
    let expected_heading = format!(
        "Two-Level Exploration | {:?} | Round {}",
        snapshot.phase, snapshot.round,
    );
    let collected = |value| if value { "collected" } else { "needed" };
    let expected_goals = format!(
        "Goal: 1 pickup per main level | Meadow: {} | Cavern: {}",
        collected(snapshot.meadow_collected),
        collected(snapshot.cavern_collected),
    );
    let matches_snapshot = heading.as_deref() == Some(expected_heading.as_str())
        && text.iter().any(|text| text == &expected_goals);
    Some(PresentedHud {
        heading,
        text,
        matches_snapshot,
        submitted_glyphs: gpu.submitted_glyph_quad_count(),
        receipt: json!({
            "runtime_generation": frame.runtime_generation,
            "app_frame": gpu.app_frame_index(),
            "render_frame": gpu.render_transaction_frame_index(),
            "window_id": frame.window_id,
            "surface_generation": frame.surface_generation,
            "surface_id": receipt.surface_id().get(),
            "presentation_epoch": receipt.presentation_epoch(),
            "backend_instance": ui.backend_generation().backend_instance_id(),
            "device_epoch": ui.backend_generation().device_epoch(),
            "submitted_quads": gpu.submitted_quad_count(),
            "submitted_glyph_quads": gpu.submitted_glyph_quad_count(),
            "glyph_pages": gpu.glyph_page_count(),
        }),
    })
}
