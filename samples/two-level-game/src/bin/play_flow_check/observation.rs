//! Joins original native and UI receipts, keeping callback World facts distinct from UI facts.

use nara::{
    app::{FrameExecutionStart, RuntimeGeneration, RuntimeTimeSettings},
    project_host::DesktopPresentationObservation,
    render::{RenderFrame, RenderFrameState},
    render_wgpu::{WgpuAdapterSummary, WgpuBackendState},
    ui::advanced::UiPaintPrimitive,
    ui::{UiRuntimeKey, UiSemanticRole},
    ui_render::advanced::{PresentedUiObservation, latest_presented_ui_observation},
    window::{WindowId, backend::BackendWindowHandles},
};
use two_level_game::{
    GamePhase, GameSnapshot,
    flow::{FlowPhase, FlowSnapshot, PlayFlowHud},
};

const HUD_DOMAIN: &str = "two-level-game.play-flow.hud";

#[derive(Clone, Debug)]
pub(super) struct Presented {
    pub serial: u64,
    pub app_frame: u64,
    pub render_frame: u64,
    pub world_frame: u64,
    pub runtime: RuntimeGeneration,
    pub ui: PresentedUiObservation,
    pub flow: FlowSnapshot,
    pub game: GameSnapshot,
    pub time_scale: f32,
}

impl Presented {
    pub fn matches(&self, phase: FlowPhase, round: u32) -> bool {
        let frame = self.ui.frame();
        let heading = format!("Two-Level Exploration | {phase:?} | Round {round}");
        let collected = |value| if value { "collected" } else { "needed" };
        let goals = format!(
            "Goal: 1 pickup per main level | Meadow: {} | Cavern: {}",
            collected(self.flow.meadow_collected),
            collected(self.flow.cavern_collected),
        );
        let controls = match phase {
            FlowPhase::Title => "Enter: start | X: exit",
            FlowPhase::Playing => "WASD: move / collect gems | E: travel | Escape: pause | X: exit",
            FlowPhase::Paused => "Enter / Escape: continue | T: title | X: exit",
            _ => return false,
        };
        let visible_text = |expected: &str| {
            frame.semantics().iter().any(|node| {
                node.role() == UiSemanticRole::Text
                    && node.name() == expected
                    && !node.rect().size().is_empty()
                    && node
                        .clip_rect()
                        .is_none_or(|clip| node.rect().intersect(clip).is_some())
                    && frame.paint().iter().any(|primitive| {
                        matches!(primitive, UiPaintPrimitive::Text {
                            key: Some(key), rect, clip_rect, color, layout, ..
                        } if key == node.key()
                            && !rect.size().is_empty()
                            && clip_rect.is_none_or(|clip| rect.intersect(clip).is_some())
                            && color.components()[3] > 0.0
                            && !layout.glyphs().is_empty())
                    })
            })
        };
        self.flow.phase == phase
            && self.flow.round == round
            && self.game.initialized
            && self.game.phase == GamePhase::Running
            && !self.game.restoring
            && visible_text(&heading)
            && visible_text(&goals)
            && visible_text(controls)
    }

    pub fn trace(&self, checkpoint: &str) {
        let frame = self.ui.frame();
        let heading = frame
            .semantics()
            .iter()
            .find(|node| node.name().starts_with("Two-Level Exploration | "))
            .map(|node| node.name());
        println!(
            "two_level_play_flow_checkpoint: {checkpoint} phase={:?} round={} presentations={} app_frame={} render_frame={} ui_epoch={} ui_heading={heading:?} world_frame={} logical_tick={} logical_nanos={} player={:?} held={:?} score={} collected={} travel_count={} occurrence={:?} runtime={:?}",
            self.flow.phase,
            self.flow.round,
            self.serial,
            self.app_frame,
            self.render_frame,
            self.ui.receipt().presentation_epoch(),
            self.world_frame,
            self.game.logical_tick,
            self.game.logical_elapsed_nanos,
            self.game.player,
            self.game.held,
            self.game.score,
            self.game.collected,
            self.game.travel_count,
            self.game.current_instance,
            self.runtime,
        );
    }
}

pub(super) struct Stability {
    pub flow: FlowSnapshot,
    pub game: GameSnapshot,
    pub frozen_clock: bool,
}

#[derive(Default)]
pub(super) struct Observations {
    pub target: Option<BackendWindowHandles>,
    pub latest: Option<Presented>,
    pub failure: Option<String>,
    pub stability: Option<Stability>,
    pub adapter: Option<WgpuAdapterSummary>,
    identity: Option<(RuntimeGeneration, u64, u64, u64)>,
}

impl Observations {
    pub fn observe(
        &mut self,
        observation: DesktopPresentationObservation<'_>,
    ) -> Result<(), String> {
        let Some(world) = observation.world() else {
            return Ok(());
        };
        if let Some(handles) = world.get_resource::<BackendWindowHandles>() {
            if let Some(original) = &self.target {
                require(
                    original.shares_authority_with(handles),
                    "native target authority changed",
                )?;
            } else {
                self.target = Some(handles.clone());
            }
        }
        let Some(flow) = world.get_resource::<FlowSnapshot>() else {
            return Ok(());
        };
        let Some(game) = world.get_resource::<GameSnapshot>() else {
            return Ok(());
        };
        if let Some(error) = flow.error.as_ref().or(game.error.as_ref()) {
            return Err(format!("original play flow failed: {error}"));
        }
        let time = world
            .get_resource::<RuntimeTimeSettings>()
            .ok_or("missing original Runtime time")?;
        require(
            !time.paused(),
            "acceptance must run the ordinary unpaused Runtime",
        )?;
        if let Some(stability) = &self.stability {
            require(
                flow == &stability.flow
                    && game.current_instance == stability.game.current_instance
                    && game.player == stability.game.player
                    && game.held == [false; 4]
                    && game.score == stability.game.score
                    && game.collected == stability.game.collected
                    && game.travel_count == stability.game.travel_count,
                "player, held input, progress or Scene changed during the stable interval",
            )?;
            if stability.frozen_clock {
                require(
                    game.logical_tick == stability.game.logical_tick
                        && game.logical_elapsed_nanos == stability.game.logical_elapsed_nanos
                        && time.time_scale() == 0.0,
                    "logical game clock advanced while the menu was paused",
                )?;
            }
        }
        let Some(receipt) = observation.presented_frame() else {
            return Ok(());
        };
        let Some(ui) = latest_presented_ui_observation(world) else {
            return Ok(());
        };
        let hud = world
            .get_resource::<PlayFlowHud>()
            .ok_or("missing original play-flow HUD")?;
        for (entity, local) in [
            (hud.root, "root"),
            (hud.title, "title"),
            (hud.goals, "goals"),
            (hud.controls, "controls"),
            (hud.error, "error"),
        ] {
            let key = world
                .get::<UiRuntimeKey>(entity)
                .ok_or("original HUD runtime key is absent")?;
            require(
                key.domain() == HUD_DOMAIN && key.local() == local,
                "original HUD runtime key changed",
            )?;
        }
        let gpu = ui.wgpu().ok_or("original UI has no submitted WGPU facts")?;
        if gpu.app_frame_index() != receipt.app_frame_index()
            || gpu.render_transaction_frame_index() != receipt.render_transaction_frame_index()
        {
            return Ok(());
        }
        let backend = observation.wgpu();
        let transaction = backend.frame_transaction_stats();
        require(
            receipt.runtime_generation() == observation.runtime_generation()
                && receipt.window_id() == WindowId::PRIMARY
                && receipt.surface_generation() != 0
                && receipt.backend_instance_id() != 0
                && receipt.device_epoch() != 0
                && receipt.backend_instance_id() == backend.instance_id()
                && receipt.device_epoch() == backend.device_epoch()
                && backend.state() == WgpuBackendState::Ready
                && transaction.app_frame_index() == Some(receipt.app_frame_index())
                && transaction.render_transaction_frame_index()
                    == Some(receipt.render_transaction_frame_index())
                && transaction.queue_submissions() > 0
                && transaction.presents() > 0,
            "original native receipt and submitted stock transaction disagree",
        )?;
        require(
            ui.submission().matches_receipt(ui.receipt())
                && ui.submission().packet() == ui.packet().identity()
                && ui.submission().runtime_generation().get()
                    == observation.runtime_generation().get()
                && ui.submission().target().window_id(None) == Some(WindowId::PRIMARY)
                && ui.backend_generation().backend_instance_id() == receipt.backend_instance_id()
                && ui.backend_generation().device_epoch() == receipt.device_epoch()
                && ui.receipt().surface_id() == hud.surface
                && hud.surface.get() == 1
                && ui.receipt().presentation_epoch() > 0
                && gpu.submitted_quad_count() > 0
                && gpu.submitted_glyph_quad_count() > 0
                && gpu.glyph_page_count() > 0
                && gpu.frame_submitted_sprite_count() > 0,
            "original native/UI receipt, primary target or submitted text facts disagree",
        )?;
        let render = world
            .get_resource::<RenderFrame>()
            .ok_or("missing original RenderFrame")?;
        if render.index != receipt.render_transaction_frame_index() {
            return Ok(());
        }
        require(
            render.state == RenderFrameState::Submitted,
            "original RenderFrame was not Submitted",
        )?;
        require(
            ui.frame().diagnostics().is_empty(),
            "presented HUD contains UI diagnostics",
        )?;
        let adapter = observation
            .adapter_summary()
            .ok_or("missing original native adapter")?;
        require(
            adapter.backend() == "dx12"
                && matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu"),
            format!("play flow requires actual DX12 hardware: {adapter:?}"),
        )?;
        let identity = (
            receipt.runtime_generation(),
            receipt.backend_instance_id(),
            receipt.device_epoch(),
            receipt.surface_generation(),
        );
        if let Some(original) = self.identity {
            require(
                original == identity,
                "the original Runtime, backend, device or surface changed",
            )?;
        } else {
            self.identity = Some(identity);
            self.adapter = Some(adapter.clone());
        }
        if self.latest.as_ref().is_some_and(|last| {
            last.app_frame == receipt.app_frame_index()
                && last.render_frame == receipt.render_transaction_frame_index()
        }) {
            return Ok(());
        }
        let serial = self.latest.as_ref().map_or(1, |last| last.serial + 1);
        self.latest = Some(Presented {
            serial,
            app_frame: receipt.app_frame_index(),
            render_frame: receipt.render_transaction_frame_index(),
            world_frame: world
                .get_resource::<FrameExecutionStart>()
                .ok_or("missing original App frame observation")?
                .frame(),
            runtime: observation.runtime_generation(),
            ui,
            flow: flow.clone(),
            game: game.clone(),
            time_scale: time.time_scale(),
        });
        Ok(())
    }
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
