use super::*;

use nara::{
    scene::SceneEntityId,
    tooling::{
        EditorActionAcknowledgement, EditorDocumentKind, EditorNavigationAction,
        EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
    },
};

#[test]
#[ignore = "requires a Windows desktop and actual DX12 Game submission and native composite"]
fn native_unrelated_prefab_close_preserves_presented_play_source_until_source_close() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut host = SourceContinuityHost {
        project: &project.0,
        result: None,
    };
    let event_loop_result = event_loop.run_app(&mut host);
    // All native owners and the Session are retired before asserting either result.
    assert!(event_loop_result.is_ok(), "{event_loop_result:?}");
    host.result
        .expect("the native source workflow must run")
        .unwrap();
}

struct SourceContinuityHost<'a> {
    project: &'a Path,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for SourceContinuityHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_source_continuity(event_loop, self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_source_continuity(event_loop: &ActiveEventLoop, project: &Path) -> Result<(), String> {
    let intent = game_view_intent()?;
    let window = hidden_window(event_loop, "Nara native Play source continuity")
        .map_err(|error| error.to_string())?;
    let mut native = initialize_native_for_test(event_loop, Arc::clone(&window))?;
    native.layout_loaded = true;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    native.frontend.place_test_game(false);
    let mut application = match EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        intent,
    ) {
        Ok(application) => application,
        Err(error) => {
            let cleanup = finish_graphics_for_test(&mut native);
            return Err(format!(
                "trusted Open failed: {error:?}; diagnostics={:?}; graphics cleanup: {cleanup:?}",
                error.diagnostics()
            ));
        }
    };
    application.native_shell_ready();
    let result = (|| {
        let game_panel = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.game-view")
            .ok_or("the built-in Game panel is missing")?
            .id();
        application
            .panel_workspace_mut()
            .layout_mut()
            .place(game_panel, crate::workspace::EditorPanelPlacement::Floating);
        GameInputWorkflow {
            native: &mut native,
            application: &mut application,
            event_loop,
            pointer: [-1000.0, -1000.0],
            presents: 0,
            stage: "initial source workspace",
            canvas_buttons: false,
            interaction: String::new(),
        }
        .source_continuity()
    })();
    let graphics =
        finish_graphics_for_test(&mut native).map_err(|error| format!("graphics cleanup: {error}"));
    drop(native);
    drop(window);
    let retirement =
        retire_application(&mut application).map_err(|error| format!("Session cleanup: {error}"));
    let failures = [result.err(), graphics.err(), retirement.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

impl GameInputWorkflow<'_> {
    fn continuity_actions(
        &mut self,
        actions: impl IntoIterator<Item = EditorAction>,
    ) -> Result<Vec<EditorActionOutcome>, String> {
        let frame = self
            .application
            .current_frame()
            .ok_or("workspace missing")?;
        let dispatch = self
            .application
            .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), actions));
        let outcomes = dispatch
            .feedback()
            .ok_or("no action feedback")?
            .map_err(|error| format!("action batch rejected: {error:?}"))?
            .outcomes()
            .to_vec();
        if dispatch.invalidates_game_input() {
            self.native
                .input
                .invalidate_after_control(self.application.active_game_start_request());
            self.native.game_candidate = None;
        }
        // Observe exact action results without consuming the next Runtime/native frame.
        self.application
            .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), false);
        Ok(outcomes)
    }

    fn source_continuity(mut self) -> Result<(), String> {
        self.settle()?;
        self.click("embedded:Play")?;
        let initial = self.observe()?;
        let window = super::super::editor_window_packet(&self.native.window);
        let backend = self
            .native
            .gpu
            .with_device(window, |scope| scope.adapter().get_info().backend)
            .map_err(|error| error.to_string())?;
        require_continuity(
            backend == dear_imgui_wgpu::wgpu::Backend::Dx12,
            "this native qualification requires the actual DX12 backend",
        )?;
        let start = initial.submitted.identity.start();
        let source = start.document();
        let revision = start.source_revision();
        let generation = start.runtime_generation().get();
        let frame = self
            .application
            .current_frame()
            .ok_or("Play lost workspace")?;
        let retained_play = frame.play().clone();
        require_continuity(
            frame.play().state() == EditorPlayState::Running
                && frame.play().generation() == Some(generation)
                && frame.play().source_revision() == Some(revision)
                && frame.play().current_revision() == Some(revision)
                && frame.play().result()
                    == Some(EditorPlayOperationResult::Applied {
                        operation: EditorPlayOperation::Play,
                        generation: Some(generation),
                    })
                && self.application.active_game_start_request() == Some(start)
                && initial.receipt.image() == initial.submitted.image
                && initial.submitted.content.runtime_ui_present,
            "Play did not retain the exact actually submitted and composited source",
        )?;
        self.stage = "open unrelated anchored Prefab B";
        let anchor = SceneEntityId::new("ui-anchor").map_err(|error| error.to_string())?;
        let outcomes = self.continuity_actions([
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: source }),
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: source,
                entity: Some(anchor.clone()),
            }),
            EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                document: source,
                base_revision: revision,
                entity: anchor,
            }),
        ])?;
        require_continuity(
            outcomes.len() == 3
                && outcomes.iter().all(|outcome| {
                    matches!(
                        outcome, EditorActionOutcome::Workspace(report) if report.applied
                    )
                }),
            "anchored Prefab navigation was not applied exactly",
        )?;
        let prefab = match outcomes.last() {
            Some(EditorActionOutcome::Workspace(report)) => report.opened_document,
            _ => None,
        }
        .ok_or("Prefab navigation did not return its opened document")?;
        let frame = self
            .application
            .current_frame()
            .ok_or("Open lost workspace")?;
        require_continuity(
            prefab != source
                && frame.workspace().active_document == Some(prefab)
                && frame
                    .workspace()
                    .document(prefab)
                    .is_some_and(|document| document.kind == EditorDocumentKind::Prefab)
                && frame.play() == &retained_play
                && self.application.active_game_start_request() == Some(start),
            "Prefab B did not open independently of the retained Play source",
        )?;
        self.settle()?;
        let opened = self.observe()?;
        require_continuity(
            opened.submitted.identity.start() == start
                && opened.submitted.image.view_id() == initial.submitted.image.view_id()
                && opened.submitted.image.target() == initial.submitted.image.target(),
            "Prefab Open replaced the exact Game start or native target",
        )?;

        self.stage = "Close unrelated Prefab B while Game is presented";
        let close = EditorWorkspaceIntent::CloseDocument { document: prefab };
        let outcomes = self.continuity_actions([EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(close),
        )])?;
        require_continuity(
            matches!(
                outcomes.as_slice(),
                [EditorActionOutcome::WorkspaceIntent(
                    EditorWorkspaceIntentRequestResult::Accepted
                )]
            ),
            "Prefab B Close was not accepted",
        )?;
        let frame = self
            .application
            .current_frame()
            .ok_or("Close lost workspace")?;
        require_continuity(
            frame.workspace().document(prefab).is_none()
                && frame
                    .workspace()
                    .scene(source)
                    .is_some_and(|scene| scene.revision == revision)
                && frame.play() == &retained_play
                && self.application.active_game_start_request() == Some(start)
                && frame.workspace_intent().intent().is_none()
                && frame.workspace_intent().result()
                    == Some(EditorWorkspaceIntentResult::Applied { intent: close }),
            "unrelated Close changed the source, generation, Play result or exact Close outcome",
        )?;
        let outcomes = self.continuity_actions([EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )])?;
        require_continuity(
            matches!(
                outcomes.as_slice(),
                [EditorActionOutcome::Acknowledged(
                    EditorActionAcknowledgement::WorkspaceIntent(true)
                )]
            ),
            "Prefab B Close result was not acknowledged",
        )?;
        self.settle()?;
        let continued = self.observe()?;
        let frame = self
            .application
            .current_frame()
            .ok_or("Game lost workspace")?;
        let image = continued.submitted.image;
        let before_close = opened.submitted.image;
        require_continuity(
            continued.submitted.identity.start() == start
                && image.view_id() == before_close.view_id()
                && image.target() == before_close.target()
                && image.target_generation() == before_close.target_generation()
                && image.source_generation() == before_close.source_generation()
                && continued.runtime_frame() > opened.runtime_frame()
                && opened.runtime_frame() > initial.runtime_frame()
                && continued.receipt.window().editor_frame_index()
                    > opened.receipt.window().editor_frame_index()
                && image.render_transaction_frame_index()
                    > before_close.render_transaction_frame_index()
                && self.application.active_game_start_request() == Some(start)
                && frame.play() == &retained_play
                && frame
                    .workspace()
                    .scene(source)
                    .is_some_and(|scene| scene.revision == revision)
                && frame.workspace_intent().result().is_none(),
            "unrelated Close did not preserve the exact live Game target and subsequent native frames",
        )?;

        self.stage = "associated source Close and exact native target retirement";
        let close = EditorWorkspaceIntent::CloseDocument { document: source };
        let outcomes = self.continuity_actions([EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(close),
        )])?;
        let frame = self
            .application
            .current_frame()
            .ok_or("source Close lost workspace")?;
        require_continuity(
            matches!(
                outcomes.as_slice(),
                [EditorActionOutcome::WorkspaceIntent(
                    EditorWorkspaceIntentRequestResult::Accepted
                )]
            ) && frame.workspace().scene(source).is_some()
                && frame.workspace_intent().intent() == Some(close)
                && frame.workspace_intent().phase()
                    == Some(EditorWorkspaceIntentPhase::RetiringRuntime)
                && frame.workspace_intent().result().is_none()
                && frame.play().generation() == Some(generation)
                && frame.play().source_revision() == Some(revision)
                && frame.play().result()
                    == Some(EditorPlayOperationResult::Pending {
                        operation: EditorPlayOperation::Stop,
                    })
                && self.application.active_game_start_request() == Some(start)
                && self.native.gpu.viewport_target_matches(
                    image.view_id(),
                    image.source_generation(),
                    image.target(),
                ),
            "associated Close removed its source before requesting exact Runtime Stop",
        )?;
        // At every completed native frame, source removal requires the exact target to be gone.
        for _ in 0..64 {
            self.paint(|_| {})?;
            let frame = self
                .application
                .current_frame()
                .ok_or("retirement lost workspace")?;
            if frame.workspace().document(source).is_none() {
                return require_continuity(
                    frame.play().state() == EditorPlayState::Empty
                        && frame.play().generation().is_none()
                        && frame.play().source_revision().is_none()
                        && frame.play().result()
                            == Some(EditorPlayOperationResult::Applied {
                                operation: EditorPlayOperation::Stop,
                                generation: Some(generation),
                            })
                        && frame.workspace_intent().intent().is_none()
                        && frame.workspace_intent().result()
                            == Some(EditorWorkspaceIntentResult::Applied { intent: close })
                        && self.application.active_game_start_request().is_none()
                        && self.application.pending_game_start_request().is_none()
                        && self.native.game.view.is_none()
                        && self.native.game.texture.is_none()
                        && self.native.game.submitted.is_none()
                        && self.native.game_candidate.is_none()
                        && !self.native.input.is_captured()
                        && !self.native.gpu.viewport_target_matches(
                            image.view_id(),
                            image.source_generation(),
                            image.target(),
                        ),
                    "source removal became visible without exact Stop and native target retirement",
                );
            }
            require_continuity(
                frame.play().generation() == Some(generation)
                    && frame.play().source_revision() == Some(revision)
                    && frame.workspace_intent().intent() == Some(close)
                    && frame.workspace_intent().result().is_none(),
                "pending source Close lost its retained source or generation",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("associated source Close did not finish within 64 native frames".into())
    }
}

fn require_continuity(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
