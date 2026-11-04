//! Opt-in whole-window costs with a retained Inspector and a genuinely advancing Game.
//!
//! Setup and validation are outside the three CPU timers. Completion spans the start of the
//! owner drive through observation of the original queue's completion callback; it includes
//! redraw and callback/polling delay, not isolated GPU execution or display latency.

pub(super) mod gpu;
mod memory;

use super::*;

use std::{sync::mpsc, time::Instant};

use nara::{
    render_wgpu::{
        __private::WgpuRuntimeHostFacts, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
        WgpuFrameCompletionStats, WgpuFrameTransactionStats, WgpuGpuResourceStats, WgpuRenderError,
        WgpuRenderHostError, WgpuWindowPresentedReceipt,
    },
    tooling::{EditorDocumentKind, EditorFrameModel},
};
use nara_ui_kernel::{UiCompileWorkStats, UiFrame, UiSemanticRole};
use nara_ui_render::UiPresentationWorkStats;

use crate::workspace::EditorPanelInstanceId;

const WARMUP_FRAMES: usize = 16;
const SAMPLE_FRAMES: usize = 128;
const MAX_HOST_COMPLETIONS: usize = 512;

#[test]
#[ignore = "whole-window Windows/WGPU work baseline; run serially in release with captured output"]
fn measure_native_static_inspector_beside_advancing_game() -> Result<(), String> {
    measure_native_work_costs(false)
}

#[test]
#[ignore = "instrumented primary paint GPU markers; run serially in release with captured output"]
fn measure_native_primary_gpu_timestamps_beside_advancing_game() -> Result<(), String> {
    measure_native_work_costs(true)
}

fn measure_native_work_costs(primary_gpu_timestamps: bool) -> Result<(), String> {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|error| error.to_string())?;
    let mut host = WorkCostHost {
        project: &project.0,
        primary_gpu_timestamps,
        result: None,
    };
    let event_loop_result = event_loop.run_app(&mut host);
    // run_game_workflow has already finished graphics, dropped native/window ownership and
    // retired the application, including when the measurement returned an error.
    event_loop_result.map_err(|error| error.to_string())?;
    let observed = host
        .result
        .ok_or("the native work baseline did not run")??;
    observed.report();
    Ok(())
}

struct WorkCostHost<'a> {
    project: &'a Path,
    primary_gpu_timestamps: bool,
    result: Option<Result<WorkCostObservation, String>>,
}

impl ApplicationHandler for WorkCostHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            let requirements = if self.primary_gpu_timestamps {
                gpu::device_requirements()
            } else {
                WgpuDeviceRequirements::default()
            };
            self.result = Some(run_game_workflow_with_device_requirements(
                event_loop,
                self.project,
                false,
                &requirements,
                |mut workflow| workflow.static_inspector_work_costs(self.primary_gpu_timestamps),
            ));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

struct RetainedInspector {
    panel: EditorPanelInstanceId,
    frame: Arc<UiFrame>,
    image: WgpuSubmittedImageReceipt,
    occurrence: WgpuDisplayOccurrence,
}

#[derive(Debug)]
struct InspectorWork {
    // compile_calls, built_snapshots, authored_nodes, reused_frames.
    counts: [u64; 4],
    compiler: UiCompileWorkStats,
    planner: UiPresentationWorkStats,
}

struct CostSample {
    owner_drive_ns: u64,
    cursor_flush_ns: u64,
    native_redraw_ns: u64,
    window: WgpuWindowPresentedReceipt,
    runtime_frame: u64,
    center_x: f32,
    transaction: WgpuFrameTransactionStats,
    retained_resource_bytes: u64,
    retained_candidate_metadata_bytes: u64,
    completion: WgpuFrameCompletionStats,
    resources: WgpuGpuResourceStats,
}

struct QueueCompletion {
    sample: usize,
    window: WgpuWindowPresentedReceipt,
    runtime_frame: u64,
    duration_ns: u64,
}

#[derive(Debug, PartialEq)]
struct WorkGeometry {
    window_physical: [u32; 2],
    window_scale: f64,
    viewport_logical: [f32; 2],
    framebuffer_scale: [f32; 2],
    inspector_logical: [f32; 2],
    inspector_scale: f32,
    inspector_rect: DisplayRect,
    inspector_physical: [u32; 2],
    game_rect: DisplayRect,
    game_physical: [u32; 2],
}

struct WorkCostObservation {
    geometry: WorkGeometry,
    samples: Vec<CostSample>,
    queue_completions: Vec<QueueCompletion>,
    host_completions: Vec<WgpuFrameCompletionSample>,
    work_before: InspectorWork,
    work_after: InspectorWork,
    backend_before: WgpuRuntimeHostFacts,
    backend_after: WgpuRuntimeHostFacts,
    maintenance_ns: u64,
    completion_drain_ns: u64,
    primary_gpu: Option<gpu::PrimaryGpuObservation>,
    retirement: WorkRetirement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkRetirement {
    Healthy,
    DestroyOriginalDevice,
}

impl GameInputWorkflow<'_> {
    fn work_geometry(&mut self, inspector: &RetainedInspector) -> Result<WorkGeometry, String> {
        let (_, _, mapping) = self
            .native
            .owned_frames_for_test()
            .into_iter()
            .find(|(id, _, _)| *id == inspector.panel)
            .ok_or("the measured Inspector lost its native placement")?;
        let game = self
            .native
            .game
            .display
            .as_ref()
            .ok_or("the measured Game lost its native image")?;
        let window = self.native.window.inner_size();
        let environment = inspector.frame.environment();
        let inspector_extent = inspector.image.target().extent();
        let game_extent = game.receipt().target().extent();
        Ok(WorkGeometry {
            window_physical: [window.width, window.height],
            window_scale: self.native.window.scale_factor(),
            viewport_logical: self.native.context.main_viewport().size(),
            framebuffer_scale: self.native.context.io().display_framebuffer_scale(),
            inspector_logical: [
                environment.logical_size().width(),
                environment.logical_size().height(),
            ],
            inspector_scale: environment.scale_factor(),
            inspector_rect: mapping.rect,
            inspector_physical: [inspector_extent.width, inspector_extent.height],
            game_rect: control_rect("embedded:Game image")
                .ok_or("the measured Game lost its native rectangle")?,
            game_physical: [game_extent.width, game_extent.height],
        })
    }

    fn work_original_device_is_healthy(
        &mut self,
        original: &WgpuRuntimeHostFacts,
    ) -> Result<bool, WgpuRenderHostError> {
        let checked = self.native.gpu.with_device(
            super::super::editor_window_packet(&self.native.window),
            |scope| {
                scope.backend_instance_id() == original.backend_instance_id
                    && scope.device_epoch() == original.device_epoch
            },
        );
        if matches!(
            &checked,
            Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
        ) {
            self.native.close.device_lost = true;
        }
        checked
    }

    fn work_inspector(
        &mut self,
        panel: EditorPanelInstanceId,
    ) -> Result<RetainedInspector, String> {
        if self.application.current_ui_authoring_frame().is_some() {
            return Err("the work baseline requires the ordinary Nara-owned Inspector".into());
        }
        let (_, frame, mapping) = self
            .native
            .owned_frames_for_test()
            .into_iter()
            .find(|(id, _, _)| *id == panel)
            .ok_or("the selected source Inspector has no confirmed native frame")?;
        let display = self
            .native
            .owned
            .get(&panel)
            .and_then(|surface| surface.target.display.as_ref())
            .ok_or("the Inspector has no retained native image")?;
        let primary_viewport = self.native.context.main_viewport().id();
        if mapping.occurrence != Some(display.occurrence())
            || mapping.viewport != primary_viewport
            || !frame.diagnostics().is_empty()
            || !frame
                .semantics()
                .iter()
                .any(|node| node.role() == UiSemanticRole::Group && node.name() == "anchor.x")
        {
            return Err(format!(
                "the source Inspector has no exact, diagnostic-free property image: occurrence_matches={}, viewport_matches={}, diagnostics={:?}, semantics={:?}",
                mapping.occurrence == Some(display.occurrence()),
                mapping.viewport == primary_viewport,
                frame.diagnostics(),
                frame
                    .semantics()
                    .iter()
                    .map(|node| (node.role(), node.name()))
                    .collect::<Vec<_>>(),
            ));
        }
        Ok(RetainedInspector {
            panel,
            frame,
            image: display.receipt(),
            occurrence: display.occurrence(),
        })
    }

    fn work_inspector_stats(&self, panel: EditorPanelInstanceId) -> Result<InspectorWork, String> {
        let (counts, compiler) = self
            .native
            .frontend
            .owned_inspector_work_for_test(panel)
            .ok_or("the source Inspector has no CPU work observations")?;
        let planner = self
            .native
            .owned_planner_work_for_test(panel)
            .ok_or("the source Inspector has no native planner work observations")?;
        Ok(InspectorWork {
            counts,
            compiler,
            planner,
        })
    }

    fn work_source_unchanged(&self, before: &EditorFrameModel) -> Result<(), String> {
        let after = self
            .application
            .current_frame()
            .ok_or("workspace disappeared")?;
        let before_scene = before
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("no source Scene")?;
        let after_scene = after
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("source Scene disappeared")?;
        let before_tab = before
            .workspace()
            .document(before_scene.document)
            .ok_or("no source tab")?;
        let after_tab = after
            .workspace()
            .document(before_scene.document)
            .ok_or("source tab disappeared")?;
        let before_inspector = &before_scene.editor.inspector;
        let after_inspector = &after_scene.editor.inspector;
        // The enclosing Editor frame can legitimately change with Game facts. Compare the
        // source owner's revision, selection, values and history, not that enclosing revision.
        if before_inspector
            .selected_entity
            .as_ref()
            .map(|id| id.as_str())
            != Some("image-source")
            || before_inspector.selected_entity_view.is_none()
            || before_scene.kind != EditorDocumentKind::Scene
            || after_scene.kind != EditorDocumentKind::Scene
            || before_scene.document != after_scene.document
            || before_scene.revision != after_scene.revision
            || before_scene.saved_revision != after_scene.saved_revision
            || before_scene.dirty != after_scene.dirty
            || before_scene.selection != after_scene.selection
            || before_tab.revision != after_tab.revision
            || before_tab.saved_revision != after_tab.saved_revision
            || before_tab.history != after_tab.history
            || before_tab.dirty != after_tab.dirty
            || before_inspector.selected_entity != after_inspector.selected_entity
            || before_inspector.selected_entity_view != after_inspector.selected_entity_view
            || before_inspector.history != after_inspector.history
            || before_inspector.live_dirty != after_inspector.live_dirty
        {
            return Err(
                "advancing Game changed the selected authoring source, revision or history".into(),
            );
        }
        Ok(())
    }

    fn work_frame(
        &mut self,
        source: &EditorFrameModel,
        inspector: &RetainedInspector,
        previous: GameObservation,
    ) -> Result<GameObservation, String> {
        self.work_source_unchanged(source)?;
        let current = self.work_inspector(inspector.panel)?;
        if !Arc::ptr_eq(&inspector.frame, &current.frame)
            || inspector.image != current.image
            || inspector.occurrence != current.occurrence
        {
            return Err(
                "the static Inspector replaced its UiFrame, retained image or occurrence".into(),
            );
        }
        let game = self.observe()?;
        let designated = self
            .native
            .game
            .display
            .as_ref()
            .ok_or("Game lost its original display")?;
        let (primary, _) = self
            .native
            .lifecycle_frame
            .as_ref()
            .ok_or("no native window frame")?;
        let window = primary
            .window()
            .ok_or("the primary window did not present")?;
        let inspector_composite = primary
            .composites()
            .iter()
            .find(|receipt| {
                receipt.image() == inspector.image && receipt.occurrence() == inspector.occurrence
            })
            .ok_or("the retained Inspector occurrence was not composited in this primary frame")?;
        if window.window_id() != super::super::editor_window_packet(&self.native.window).id
            || window.display_frame_index() != self.native.frame_index
            || game.receipt.window() != window
            || inspector_composite.window() != window
            || !primary.composites().contains(&game.receipt)
            || designated.receipt() != game.submitted.image
            || designated.occurrence() != game.submitted.occurrence
            || game.receipt.image() != game.submitted.image
            || game.receipt.occurrence() != game.submitted.occurrence
            || game.runtime_frame() <= previous.runtime_frame()
            || game.center_x <= previous.center_x + f32::EPSILON
            || game.submitted.image.output_version() <= previous.submitted.image.output_version()
            || game.submitted.image.producer_id() != previous.submitted.image.producer_id()
            || game.submitted.identity.start() != previous.submitted.identity.start()
            || game.submitted.content.sprite_instances == 0
            || !game.submitted.content.runtime_ui_present
            || !self.native.input.is_captured()
            || !self
                .application
                .current_frame()
                .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
        {
            return Err(self.frame_error(
                "work sample",
                "Game did not advance through its exact current primary composite",
            ));
        }
        // UiProject fixes a 16x16 sprite at y=0; InputResponsePlugin installs an identity
        // Camera2d::default() and only changes x. SpriteBatches already contain clip-space
        // centers. Derive the clip-space radius from this fixed camera and the actual target;
        // extraction alone does not prove that a sprite remains fully inside the image.
        let extent = game.submitted.image.target().extent();
        let half_height = Camera2d::default().viewport_height * 0.5;
        let half_width = half_height * extent.width as f32 / extent.height as f32;
        let radius_x_clip = 8.0 / half_width;
        let radius_y_clip = 8.0 / half_height;
        let image =
            control_rect("embedded:Game image").ok_or("Game has no native image rectangle")?;
        let origin = self.native.context.main_viewport().pos();
        let size = self.native.context.main_viewport().size();
        if !game.center_x.is_finite()
            || game.center_x.abs() + radius_x_clip >= 1.0
            || radius_y_clip >= 1.0
            || image.max[0] - image.min[0] < 32.0
            || image.max[1] - image.min[1] < 32.0
            || image.min[0] < origin[0]
            || image.min[1] < origin[1]
            || image.max[0] > origin[0] + size[0]
            || image.max[1] > origin[1] + size[1]
        {
            return Err(format!(
                "the moving sprite left the visible Game image: center_x_clip={}, radius_x_clip={radius_x_clip}, image={image:?}",
                game.center_x,
            ));
        }
        Ok(game)
    }

    fn work_paint(&mut self) -> Result<(Instant, [u64; 3]), String> {
        // Keep these production steps identical to paint(). Only the timing boundaries differ.
        if self
            .application
            .current_frame()
            .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
        {
            self.native.input.mark_presented_stale();
        }
        let frame_started = Instant::now();
        self.application
            .drive_editor_frame(Duration::from_millis(20), false, true);
        let owner_drive_ns = elapsed_ns(frame_started);
        let input_started = Instant::now();
        let origin = self.native.context.main_viewport().pos();
        let scale = f64::from(self.native.context.io().display_framebuffer_scale()[0]);
        self.native.input.handle_event(
            self.native.window.id(),
            &WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(
                    f64::from(self.pointer[0] - origin[0]) * scale,
                    f64::from(self.pointer[1] - origin[1]) * scale,
                ),
            },
            &self.native.context,
        );
        self.native
            .input
            .flush(self.application)
            .map_err(|error| self.frame_error("sample cursor input flush", error))?;
        let cursor_flush_ns = elapsed_ns(input_started);
        clear_controls();
        let pointer = self.pointer;
        let redraw_started = Instant::now();
        let result = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
            });
        let native_redraw_ns = elapsed_ns(redraw_started);
        if result.map_err(|error| self.frame_error("sample native redraw", error))? {
            return Err("the native work baseline unexpectedly requested exit".into());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok((
            frame_started,
            [owner_drive_ns, cursor_flush_ns, native_redraw_ns],
        ))
    }

    fn work_maintenance(
        &mut self,
        samples: &mut Vec<WgpuFrameCompletionSample>,
    ) -> Result<(), String> {
        let maintenance = self.native.gpu.poll_runtime_maintenance();
        if let Some(error) = maintenance.error {
            return Err(self.frame_error("completion maintenance", error));
        }
        if samples.len() + maintenance.samples.len() > MAX_HOST_COMPLETIONS {
            return Err("the bounded Host completion observation filled".into());
        }
        samples.extend(maintenance.samples);
        Ok(())
    }

    fn static_inspector_work_costs(
        &mut self,
        primary_gpu_timestamps: bool,
    ) -> Result<WorkCostObservation, String> {
        self.inspector_work_costs(
            primary_gpu_timestamps,
            WorkRetirement::DestroyOriginalDevice,
        )
    }

    fn inspector_work_costs(
        &mut self,
        primary_gpu_timestamps: bool,
        retirement: WorkRetirement,
    ) -> Result<WorkCostObservation, String> {
        self.stage = "static Inspector work baseline setup";
        // Configure before this workflow's first submission, and do not reset at the sample
        // boundary. Backend lifetime peaks therefore include setup; sampled maxima are separate.
        self.native.gpu.configure_telemetry(
            WgpuBackendTelemetryConfig::new(256, MAX_HOST_COMPLETIONS as u32)
                .map_err(|error| error.to_string())?,
        );
        if primary_gpu_timestamps {
            gpu::enable_primary_gpu_timestamps(self.native)?;
        }
        self.settle()?;
        self.presents += super::super::authoring_workflow_tests::hierarchy_click(
            self.native,
            self.application,
            self.event_loop,
            &mut self.pointer,
            "Select image-source",
        )?;
        let source = self
            .application
            .current_frame()
            .ok_or("selection lost its source frame")?;
        self.work_source_unchanged(&source)?;
        self.click("embedded:Play")?;
        self.click("embedded:Game image")?;
        if !self.native.input.is_captured() {
            return Err("the actual Game image did not capture native input".into());
        }
        let panel = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.inspector")
            .ok_or("the source Inspector is missing")?
            .id();
        let mut previous: Option<RetainedInspector> = None;
        let mut stable = None;
        for _ in 0..32 {
            self.paint(|_| {})?;
            self.work_source_unchanged(&source)?;
            let current = self.work_inspector(panel)?;
            if previous.as_ref().is_some_and(|before| {
                Arc::ptr_eq(&before.frame, &current.frame)
                    && before.image == current.image
                    && before.occurrence == current.occurrence
            }) {
                stable = Some(current);
                break;
            }
            previous = Some(current);
        }
        let inspector =
            stable.ok_or("the selected Inspector did not stabilize within 32 frames")?;
        let mut game = self.observe()?;
        let mut host_completions = Vec::with_capacity(MAX_HOST_COMPLETIONS);
        // D is held continuously for exactly 16 warmup + 128 sampled 20ms owner drives.
        // The fixture supports D only. Every frame must move and remain wholly in projection;
        // no sample is dropped, clipped, replaced by idle work, or rescued by teleportation.
        self.stage = "static Inspector beside moving Game warmup";
        self.key(ElementState::Pressed)?;
        for _ in 0..WARMUP_FRAMES {
            self.paint(|_| {})?;
            game = self.work_frame(&source, &inspector, game)?;
            self.work_maintenance(&mut host_completions)?;
        }
        host_completions.clear();
        let geometry = self.work_geometry(&inspector)?;
        let work_before = self.work_inspector_stats(panel)?;
        let backend_before = self.native.gpu.runtime_facts();
        if !backend_before
            .adapter
            .as_ref()
            .is_some_and(|adapter| adapter.backend() == "dx12" && adapter.device_type() != "cpu")
        {
            return Err(format!(
                "the native work baseline requires an actual DX12 adapter: {:?}",
                backend_before.adapter
            ));
        }
        let (sender, receiver) = mpsc::sync_channel(SAMPLE_FRAMES);
        let mut samples = Vec::with_capacity(SAMPLE_FRAMES);
        let mut queue_completions = Vec::with_capacity(SAMPLE_FRAMES);
        let mut maintenance_ns = 0_u64;
        self.stage = "static Inspector beside moving Game sampled frames";
        for index in 0..SAMPLE_FRAMES {
            if let Some(timestamps) = self.native.primary_gpu_timestamps.as_mut() {
                timestamps.arm_sample(index)?;
            }
            let (frame_started, cpu) = self.work_paint()?;
            // Read demand/retention before observer-side maintenance can retire old payloads.
            let transaction = self.native.gpu.frame_transaction_stats();
            let facts = self.native.gpu.runtime_facts();
            let retained_resource_bytes = self.native.gpu.retained_resource_bytes();
            let retained_candidate_metadata_bytes =
                self.native.gpu.retained_candidate_metadata_bytes();
            let submitted = self
                .native
                .submitted_game_observation
                .ok_or("no sampled Game submission")?;
            let window = self
                .native
                .lifecycle_frame
                .as_ref()
                .and_then(|(primary, _)| primary.window())
                .ok_or("no sampled primary presentation")?;
            let runtime_frame = submitted.identity.frame_index();
            let completion_sender = sender.clone();
            // Native Editor image/window submissions currently need not produce the managed
            // Runtime surface telemetry samples. Observe this exact original queue locally;
            // do not create an owner, submit extra commands, or construct acceptance receipts.
            let original_queue = self
                .native
                .gpu
                .with_device(
                    super::super::editor_window_packet(&self.native.window),
                    |scope| {
                        if scope.backend_instance_id() != window.backend_instance_id()
                            || scope.device_epoch() != window.device_epoch()
                        {
                            return false;
                        }
                        scope.queue().on_submitted_work_done(move || {
                            let _ = completion_sender.try_send(QueueCompletion {
                                sample: index,
                                window,
                                runtime_frame,
                                duration_ns: elapsed_ns(frame_started),
                            });
                        });
                        true
                    },
                )
                .map_err(|error| self.frame_error("original queue completion callback", error))?;
            game = self.work_frame(&source, &inspector, game)?;
            if !original_queue
                || self.work_geometry(&inspector)? != geometry
                || facts.backend_instance_id != backend_before.backend_instance_id
                || facts.device_epoch != backend_before.device_epoch
                || window.backend_instance_id() != facts.backend_instance_id
                || window.device_epoch() != facts.device_epoch
                || transaction.render_transaction_frame_index() != Some(self.native.frame_index)
                || transaction.packet_rejections() != 0
                || transaction.queue_submissions() == 0
                || transaction.presents() != 1
            {
                return Err(
                    "the sample lost its backend epoch or exact native frame transaction".into(),
                );
            }
            if let Some(timestamps) = self.native.primary_gpu_timestamps.as_mut() {
                timestamps.confirm_sample(index, window, runtime_frame)?;
            }
            samples.push(CostSample {
                owner_drive_ns: cpu[0],
                cursor_flush_ns: cpu[1],
                native_redraw_ns: cpu[2],
                window,
                runtime_frame,
                center_x: game.center_x,
                transaction,
                retained_resource_bytes,
                retained_candidate_metadata_bytes,
                completion: facts.completion,
                resources: facts.resources,
            });
            let maintenance_started = Instant::now();
            self.work_maintenance(&mut host_completions)?;
            queue_completions.extend(receiver.try_iter());
            maintenance_ns = maintenance_ns.saturating_add(elapsed_ns(maintenance_started));
        }
        self.key(ElementState::Released)?;
        drop(sender);
        let drain_started = Instant::now();
        let deadline = drain_started + Duration::from_secs(10);
        loop {
            self.work_maintenance(&mut host_completions)?;
            queue_completions.extend(receiver.try_iter());
            let facts = self.native.gpu.runtime_facts();
            if queue_completions.len() == SAMPLE_FRAMES
                && facts.completion.pending_samples() == 0
                && facts.resources.pending_retirements() == 0
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "native completion drain timed out: exact_queue_samples={}/{SAMPLE_FRAMES}, facts={facts:?}",
                    queue_completions.len(),
                ));
            }
            // End-boundary drain only: never wait for GPU completion inside a CPU sample.
            std::thread::sleep(Duration::from_millis(1));
        }
        let completion_drain_ns = elapsed_ns(drain_started);
        queue_completions.sort_unstable_by_key(|completion| completion.sample);
        for (index, (sample, completed)) in samples.iter().zip(&queue_completions).enumerate() {
            if completed.sample != index
                || completed.window != sample.window
                || completed.runtime_frame != sample.runtime_frame
            {
                return Err(
                    "completion did not belong to its exact sampled native/Runtime frame".into(),
                );
            }
        }
        self.work_source_unchanged(&source)?;
        // The optional marker readback uses the original Host after all timed frames and exact
        // completion associations. It cannot substitute for receipts or final device health.
        let primary_gpu =
            self.read_primary_gpu_timestamps(&samples, &backend_before, &mut host_completions)?;
        // A completed queue callback can also arrive on terminal loss. Re-enter the original
        // Host's health boundary after draining, outside all sampled CPU/completion timers.
        if !self
            .work_original_device_is_healthy(&backend_before)
            .map_err(|error| self.frame_error("final original-device health", error))?
        {
            return Err("the native baseline ended on a replacement device".into());
        }
        let work_after = self.work_inspector_stats(panel)?;
        let backend_after = self.native.gpu.runtime_facts();
        let calls = work_after.counts[0].saturating_sub(work_before.counts[0]);
        let reused = work_after.counts[3].saturating_sub(work_before.counts[3]);
        if work_after.compiler != work_before.compiler
            || work_after.planner != work_before.planner
            || calls < SAMPLE_FRAMES as u64
            || calls != reused
            || backend_after.backend_instance_id != backend_before.backend_instance_id
            || backend_after.device_epoch != backend_before.device_epoch
            || !backend_after.completion.enabled()
            || !backend_after.resources.tracking_enabled()
            || backend_after.completion.lost_samples() != 0
            || backend_after.completion.buffered_samples() != 0
            || backend_after.resources.lost_retirements() != 0
        {
            return Err(format!(
                "static Inspector work or bounded telemetry changed unexpectedly: before={work_before:?}, after={work_after:?}, backend={backend_after:?}"
            ));
        }
        let observed = WorkCostObservation {
            geometry,
            samples,
            queue_completions,
            host_completions,
            work_before,
            work_after,
            backend_before,
            backend_after,
            maintenance_ns,
            completion_drain_ns,
            primary_gpu,
            retirement,
        };
        if retirement == WorkRetirement::DestroyOriginalDevice {
            self.destroy_work_original_device(&observed.backend_before)?;
        }
        Ok(observed)
    }

    fn destroy_work_original_device(
        &mut self,
        original: &WgpuRuntimeHostFacts,
    ) -> Result<(), String> {
        // Refusal control after all samples: destroy that same drained device, keep its
        // namespace unchanged and require the exact terminal-loss error. Cleanup still runs
        // through run_game_workflow's original graphics/application owners.
        self.native
            .gpu
            .wait_for_idle()
            .map_err(|error| self.frame_error("loss-control idle boundary", error))?;
        let device = self
            .native
            .gpu
            .with_device(
                super::super::editor_window_packet(&self.native.window),
                |scope| scope.device().clone(),
            )
            .map_err(|error| self.frame_error("loss-control original device", error))?;
        device.destroy();
        self.native.close.device_lost = true;
        // with_device polls and checks the actual backend loss signal before granting access.
        let lost = self.work_original_device_is_healthy(original);
        let lost_facts = self.native.gpu.runtime_facts();
        if !matches!(
            lost,
            Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
        ) || lost_facts.backend_instance_id != original.backend_instance_id
            || lost_facts.device_epoch != original.device_epoch
        {
            return Err(format!(
                "the final health boundary accepted or misclassified the destroyed original device: result={lost:?}, facts={lost_facts:?}"
            ));
        }
        Ok(())
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn cost_percentiles(mut values: Vec<u64>) -> [u64; 4] {
    values.sort_unstable();
    [50, 95, 99, 100].map(|percent| {
        let index = (values.len() * percent).div_ceil(100).saturating_sub(1);
        values.get(index).copied().unwrap_or(0)
    })
}

impl WorkCostObservation {
    fn report(&self) {
        let timing_lane = if self.primary_gpu.is_some() {
            "primary_gpu_timestamps_instrumented"
        } else {
            "ordinary_cpu_completion_baseline"
        };
        println!(
            "NATIVE_WORK static_inspector_advancing_game warmup={WARMUP_FRAMES} samples={} owner_delta_ms=20 input=D_held_for_16_plus_128_frames percentile_order=p50,p95,p99,max scope=whole_native_window timing_lane={timing_lane} cpu_and_completion_timings_instrumented={}",
            self.samples.len(),
            self.primary_gpu.is_some()
        );
        println!(
            "NATIVE_WORK geometry={:?} stable_every_sample=true final_original_device_healthy=true retirement={:?} destroyed_original_device_refused={}",
            self.geometry,
            self.retirement,
            self.retirement == WorkRetirement::DestroyOriginalDevice
        );
        println!(
            "NATIVE_WORK owner_drive_ns={:?} cursor_flush_ns={:?} native_redraw_ns={:?}",
            cost_percentiles(
                self.samples
                    .iter()
                    .map(|sample| sample.owner_drive_ns)
                    .collect()
            ),
            cost_percentiles(
                self.samples
                    .iter()
                    .map(|sample| sample.cursor_flush_ns)
                    .collect()
            ),
            cost_percentiles(
                self.samples
                    .iter()
                    .map(|sample| sample.native_redraw_ns)
                    .collect()
            )
        );
        println!(
            "NATIVE_WORK frame_to_queue_complete_callback_ns={:?} completions={} maintenance_outside_cpu_samples_ns={} end_drain_ns={} completion_scope=owner_drive_start_to_original_queue_callback_including_poll_delay_not_gpu_timestamps_or_display_latency",
            cost_percentiles(
                self.queue_completions
                    .iter()
                    .map(|sample| sample.duration_ns)
                    .collect()
            ),
            self.queue_completions.len(),
            self.maintenance_ns,
            self.completion_drain_ns
        );
        println!(
            "NATIVE_WORK inspector_counts_order=compile_calls,built_snapshots,authored_nodes,reused_frames delta={:?} before={:?} after={:?}",
            std::array::from_fn::<_, 4, _>(|index| self.work_after.counts[index]
                .saturating_sub(self.work_before.counts[index])),
            self.work_before,
            self.work_after
        );
        let mut totals = [0_u64; 9];
        let mut maxima = [0_u64; 8];
        let mut native_totals = [0_u64; 11];
        for sample in &self.samples {
            let work = sample.transaction;
            let native = work.native_work();
            let values = [
                native.issued_buffer_write_calls,
                native.issued_buffer_write_bytes,
                native.issued_texture_write_calls,
                native.issued_texture_write_bytes,
                native.initialized_buffer_calls,
                native.initialized_buffer_payload_bytes,
                native.initialized_buffer_storage_bytes,
                native.encoded_buffer_copy_commands,
                native.encoded_buffer_copy_bytes,
                native.encoded_texture_copy_commands,
                native.encoded_texture_copy_bytes,
            ];
            for (total, value) in native_totals.iter_mut().zip(values) {
                *total = total.saturating_add(value);
            }
            let values = [
                work.packet_admissions(),
                work.packet_rejections(),
                work.surface_acquire_attempts(),
                work.surface_acquires(),
                work.queue_submissions(),
                work.presents(),
                work.ui_packet_count(),
                work.ui_primitives(),
                work.ui_glyph_primitives(),
            ];
            for (total, value) in totals.iter_mut().zip(values) {
                *total = total.saturating_add(value);
            }
            let values = [
                sample.retained_resource_bytes,
                sample.retained_candidate_metadata_bytes,
                sample.resources.current_texture_bytes(),
                sample.resources.current_instance_buffer_bytes(),
                sample.resources.current_total_bytes(),
                work.submitted_instance_buffer_demand_bytes(),
                u64::from(sample.completion.pending_samples()),
                u64::from(sample.resources.pending_retirements()),
            ];
            for (maximum, value) in maxima.iter_mut().zip(values) {
                *maximum = (*maximum).max(value);
            }
        }
        println!(
            "NATIVE_WORK transaction_totals_order=admissions,rejections,acquire_attempts,acquires,submissions,presents,ui_packets,ui_primitives,ui_glyph_primitives values={totals:?}"
        );
        println!(
            "NATIVE_WORK native_issued_totals_order=buffer_write_calls,buffer_write_bytes,texture_write_calls,texture_write_bytes,initialized_buffer_calls,initialized_payload_bytes,initialized_storage_bytes,encoded_buffer_copy_commands,encoded_buffer_copy_bytes,encoded_texture_copy_commands,encoded_texture_copy_bytes values={native_totals:?} scope=Nara_preparation_and_program_Preserve_only excluded=Dear_and_native_callbacks_and_Feature_internal_commands_not_measured_hardware_transfer_or_residency"
        );
        println!(
            "NATIVE_WORK sampled_maxima_order=retained_resource_bytes,candidate_metadata_bytes,backend_current_texture_bytes,backend_current_instance_bytes,backend_current_total_bytes,submitted_instance_demand_bytes,pending_host_completions,pending_resource_retirements values={maxima:?}"
        );
        println!(
            "NATIVE_WORK memory_scope=logical_Host_retention_and_backend_tracked_payloads_not_process_RSS_or_complete_driver_residency backend_lifetime_peaks_include_setup=true before={:?} after={:?}",
            self.backend_before, self.backend_after
        );
        println!(
            "NATIVE_WORK host_telemetry_completion_samples={} host_completion_ns={:?} host_scope=managed_Runtime_surface_telemetry_not_substituted_for_exact_native_queue_samples",
            self.host_completions.len(),
            (!self.host_completions.is_empty()).then(|| {
                cost_percentiles(
                    self.host_completions
                        .iter()
                        .map(|sample| sample.duration_ns())
                        .collect(),
                )
            })
        );
        if let Some((first, last)) = self.samples.first().zip(self.samples.last()) {
            println!(
                "NATIVE_WORK validated_every_frame=true inspector_same_Arc_and_image=true source_values_revision_history_dirty_unchanged=true first_last_host_frames={:?} first_last_runtime_frames={:?} first_last_sprite_x_clip={:?}",
                [
                    first.window.display_frame_index(),
                    last.window.display_frame_index()
                ],
                [first.runtime_frame, last.runtime_frame],
                [first.center_x, last.center_x]
            );
        }
        if let Some(primary_gpu) = &self.primary_gpu {
            primary_gpu.report();
        }
    }
}
