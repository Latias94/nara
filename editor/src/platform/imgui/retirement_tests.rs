#![cfg(target_os = "windows")]

use std::panic::{AssertUnwindSafe, catch_unwind};

use dear_imgui_rs::{ConfigFlags, ContextLifecycle};
use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorCloseDecision, EditorDocumentMutation, EditorWorkspaceIntentPhase,
    },
};
use winit::platform::windows::EventLoopBuilderExtWindows;

use super::*;
use crate::{EditorLaunchOptions, frontend::imgui::UiProject};

const RETIREMENT_TIMEOUT: Duration = Duration::from_secs(15);

mod quiet_exit;

type NativeProof = fn(&mut NativeEditorHost, &ActiveEventLoop) -> Result<(), String>;

struct RetirementTestHost {
    host: NativeEditorHost,
    proof: NativeProof,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for RetirementTestHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return;
        }
        // Keep the Host outside the unwind boundary so even a failed production callback leaves
        // its actual owners available to the finite cleanup driver before any assertion panics.
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.host.window = Some(
                super::lifecycle_tests::hidden_window(event_loop, "Nara native retirement proof")
                    .map_err(|error| error.to_string())?,
            );
            (self.proof)(&mut self.host, event_loop)
        }))
        .unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            Err(format!("native retirement trajectory panicked: {message}"))
        });
        let cleanup = cleanup_host(&mut self.host);
        self.result = Some(match (result, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (result, cleanup) => Err([result.err(), cleanup.err()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("; ")),
        });
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn qualify(proof: NativeProof) {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project.0).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let mut test = RetirementTestHost {
        host: NativeEditorHost::new(
            application,
            SupervisedPlayWake::default(),
            ImguiEditorFrontend::default(),
        ),
        proof,
        result: None,
    };
    let event_result = event_loop.run_app(&mut test);
    // A failed event-loop startup can bypass resumed; it must still close the real Session.
    if test.result.is_none() {
        test.result = Some(cleanup_host(&mut test.host).and_then(|()| {
            Err("the native event loop never ran its retirement trajectory".into())
        }));
    }
    // Every behavior failure is reported only after graphics, Context, window and Session cleanup.
    event_result.unwrap();
    test.result.unwrap().unwrap();
}

fn cleanup_host(host: &mut NativeEditorHost) -> Result<(), String> {
    host.retirement_action = NativeRetirementAction::Hold;
    let mut errors = Vec::new();
    while host.native.is_some() || host.retiring.is_some() {
        host.retire_native();
        if let Some(native) = &mut host.retiring {
            let result = native.shutdown_graphics();
            if !native.close.is_complete() {
                // Match the executable's fail-stop policy: never unwind into SDK destructors
                // while a renderer or platform owner still refuses physical retirement.
                eprintln!("native retirement test cannot release unfinished custody: {result:?}");
                std::process::abort();
            }
            if let Err(error) = result {
                errors.push(error.to_string());
            }
            native.publish_render_views(host.application.panel_workspace_mut());
        }
        host.retiring.take();
    }
    host.window.take();
    host.retained.take();
    if let Err(error) = super::canvas_tests::retire_application(&mut host.application) {
        errors.push(error);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn require_dx12(native: &mut NativeImgui) -> Result<(), String> {
    let backend = native
        .gpu
        .with_device(editor_window_packet(&native.window), |scope| {
            scope.adapter().get_info().backend
        })
        .map_err(|error| error.to_string())?;
    if backend != wgpu::Backend::Dx12 {
        return Err(format!(
            "native retirement requires DX12, selected {backend:?}"
        ));
    }
    Ok(())
}

fn initialize(host: &mut NativeEditorHost, event_loop: &ActiveEventLoop) -> Result<(), String> {
    host.resumed(event_loop);
    if let Some(error) = &host.error {
        return Err(error.to_string());
    }
    let native = host
        .native
        .as_mut()
        .ok_or("the native owner did not initialize")?;
    require_dx12(native)?;
    native.layout_loaded = true;
    let flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    native.context.io_mut().set_config_flags(flags);
    check(
        native
            .context
            .platform_io()
            .renderer_render_window_raw()
            .is_some(),
        "the actual native SDK renderer route must be attached",
    )
}

fn held_owner(host: &NativeEditorHost, backend: u64) -> Result<&NativeImgui, String> {
    check(
        host.native.is_none(),
        "a retiring owner coexisted with a replacement",
    )?;
    let native = host
        .retiring
        .as_ref()
        .ok_or("the predecessor custody was lost")?;
    check(
        native.gpu.backend_instance_id() == backend,
        "the held backend identity changed",
    )?;
    check(
        native.context.binding().lifecycle() == ContextLifecycle::Alive,
        "the SDK Context was destroyed while its aggregate was held",
    )?;
    Ok(native)
}

fn drive_retirement(
    host: &mut NativeEditorHost,
    event_loop: &ActiveEventLoop,
) -> Result<(), String> {
    let Some(native) = &host.retiring else {
        return Ok(());
    };
    let backend = native.gpu.backend_instance_id();
    let binding = native.context.binding();
    let deadline = Instant::now() + RETIREMENT_TIMEOUT;
    while host.retiring.is_some() {
        check(
            held_owner(host, backend)?.context.id() == binding.id(),
            "retirement polling exchanged the held Context",
        )?;
        check(
            !event_loop.exiting(),
            "the event loop exited with a held native owner",
        )?;
        if Instant::now() >= deadline {
            return Err("native retirement did not complete before its deadline".into());
        }
        host.drive_native_retirement(event_loop);
        std::thread::sleep(Duration::from_millis(1));
    }
    check(
        binding.lifecycle() == ContextLifecycle::NativeDestroyed,
        "completed retirement left the predecessor Context alive",
    )
}

#[test]
#[ignore = "requires a Windows desktop and an actual DX12 Dear ImGui/WGPU renderer"]
fn native_close_before_first_presentation_retires_before_exit() {
    qualify(|host, event_loop| {
        initialize(host, event_loop)?;
        let native = host
            .native
            .as_ref()
            .ok_or("the initialized owner is missing")?;
        let binding = native.context.binding();
        let backend = native.gpu.backend_instance_id();
        let window = native.window.id();
        check(
            native.frame_index == 0
                && !native.presented
                && native.gpu.frame_transaction_stats().presents() == 0,
            "the pre-close trajectory unexpectedly rendered or presented",
        )?;
        event_loop.set_control_flow(ControlFlow::Wait);
        host.window_event(event_loop, window, WindowEvent::CloseRequested);
        check(
            host.application.has_pending_exit()
                && host.application.current_frame().is_some_and(|frame| {
                    frame.workspace_intent().phase()
                        == Some(nara::tooling::EditorWorkspaceIntentPhase::Prepared)
                }),
            "clean CloseRequested did not retain its real prepared semantic exit",
        )?;
        let deadline = Instant::now() + RETIREMENT_TIMEOUT;
        let mut owner_callbacks = 0;
        loop {
            if !event_loop.exiting()
                && (host.application.has_pending_exit() || host.retiring.is_some())
            {
                check(
                    matches!(event_loop.control_flow(), ControlFlow::WaitUntil(_)),
                    "pending exit lost its maintenance wake without a redraw or unrelated event",
                )?;
            }
            if let Some(native) = &host.native {
                check(
                    native.context.id() == binding.id()
                        && native.gpu.backend_instance_id() == backend
                        && native.frame_index == 0
                        && !native.presented,
                    "semantic exit replaced the live owner or relied on presentation",
                )?;
            }
            if let Some(native) = host.retiring.as_ref() {
                held_owner(host, backend)?;
                check(
                    !event_loop.exiting(),
                    "CloseRequested exited before graphics retired",
                )?;
                check(
                    native.frame_index == 0
                        && !native.presented
                        && native
                            .context
                            .platform_io()
                            .renderer_render_window_raw()
                            .is_some(),
                    "pending initialization drain mutated the renderer or produced a frame",
                )?;
            }
            if event_loop.exiting() {
                check(
                    host.native.is_none() && host.retiring.is_none(),
                    "exit retained native custody",
                )?;
                check(
                    host.application.request_exit().exit_ready(),
                    "exit preceded semantic close",
                )?;
                break;
            }
            if Instant::now() >= deadline {
                return Err("CloseRequested/about_to_wait did not finish native exit".into());
            }
            host.about_to_wait(event_loop);
            owner_callbacks += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
        check(
            owner_callbacks >= 2,
            "prepared exit did not exercise successive owner callbacks",
        )?;
        check(
            binding.lifecycle() == ContextLifecycle::NativeDestroyed,
            "exit left the Context alive",
        )?;
        check(
            host.error.is_none(),
            "a terminal Host error replaced the clean exit trajectory",
        )?;
        check(
            host.retirement_errors.is_empty(),
            "clean pre-presentation close retained an error",
        )
    });
}

#[test]
#[ignore = "requires a Windows desktop and an actual DX12 Dear ImGui/WGPU renderer"]
fn native_quiet_exit_waits_for_healthy_open_host_worker_before_graphics_and_process_exit() {
    quiet_exit::qualify();
}

fn dirty_active_scene(
    application: &mut EditorApplication,
) -> Result<
    (
        nara::tooling::EditorDocumentId,
        nara::scene::SceneAuthoringRevision,
    ),
    String,
> {
    let frame = application
        .current_frame()
        .ok_or("the workspace is unavailable")?;
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .ok_or("the active Scene is missing")?;
    check(
        !scene.dirty,
        "the dirty-exit fixture must begin from a clean Scene",
    )?;
    let document = scene.document;
    let revision = scene.revision;
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("unsaved-close-probe")
                                .map_err(|error| format!("{error:?}"))?,
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let frame = application
        .current_frame()
        .ok_or("the edit lost its workspace")?;
    let tab = frame
        .workspace()
        .document(document)
        .ok_or("the edit lost its Scene")?;
    check(
        tab.dirty && tab.revision != revision,
        "the actual Scene patch did not dirty its source",
    )?;
    Ok((document, tab.revision))
}

#[test]
#[ignore = "requires a Windows desktop and an actual DX12 Dear ImGui/WGPU renderer"]
fn native_dirty_exit_cancel_keeps_live_owner_and_returns_to_hold() {
    qualify(|host, event_loop| {
        initialize(host, event_loop)?;
        let native = host
            .native
            .as_ref()
            .ok_or("the initialized owner is missing")?;
        let binding = native.context.binding();
        let backend = native.gpu.backend_instance_id();
        let (document, _) = dirty_active_scene(&mut host.application)?;

        let failure = "native retirement qualification: terminal failure awaiting close decision";
        host.error = Some(imgui_error(failure));
        host.exit_after_native_retirement(event_loop);
        check(
            host.retirement_action == NativeRetirementAction::AwaitingExitReady
                && host.application.has_pending_exit()
                && host.retiring.is_none()
                && !event_loop.exiting(),
            "terminal failure retired graphics before the dirty Scene decision",
        )?;
        let native = host
            .native
            .as_ref()
            .ok_or("dirty exit lost its live frontend")?;
        check(
            native.context.id() == binding.id()
                && native.gpu.backend_instance_id() == backend
                && binding.lifecycle() == ContextLifecycle::Alive
                && native
                    .context
                    .platform_io()
                    .renderer_render_window_raw()
                    .is_some(),
            "the dirty decision lost its original live SDK renderer",
        )?;
        check(
            matches!(event_loop.control_flow(), ControlFlow::WaitUntil(_)),
            "the pending dirty exit lost its maintenance wake",
        )?;
        let frame = host
            .application
            .current_frame()
            .ok_or("dirty exit lost its workspace")?;
        check(
            frame.workspace_intent().phase() == Some(EditorWorkspaceIntentPhase::AwaitingDecision),
            "dirty exit did not publish the real unsaved-changes decision",
        )?;
        let prompt = frame
            .workspace_intent()
            .decision_prompt()
            .ok_or("dirty exit has no Cancel prompt")?;
        host.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::WorkspaceIntent(
                    prompt.resolve(EditorCloseDecision::Cancel),
                )],
            ),
            Duration::ZERO,
        );

        // Only production maintenance runs after public Cancel: no redraw, second exit request,
        // or manufactured frame can clear or reintroduce semantic exit custody for this proof.
        for _ in 0..3 {
            host.about_to_wait(event_loop);
            check(
                host.retirement_action == NativeRetirementAction::Hold
                    && !host.application.has_pending_exit()
                    && host.retiring.is_none()
                    && !event_loop.exiting(),
                "maintenance re-requested or completed the cancelled exit",
            )?;
            let native = host
                .native
                .as_ref()
                .ok_or("Cancel lost the live native owner")?;
            check(
                native.context.id() == binding.id()
                    && native.gpu.backend_instance_id() == backend
                    && binding.lifecycle() == ContextLifecycle::Alive
                    && native.frame_index == 0
                    && !native.presented
                    && native
                        .context
                        .platform_io()
                        .renderer_render_window_raw()
                        .is_some(),
                "Cancel replaced, destroyed or redrew the retained SDK owner",
            )?;
            check(
                host.application.current_frame().is_some_and(|frame| {
                    frame.workspace_intent().decision_prompt().is_none()
                        && frame
                            .workspace()
                            .document(document)
                            .is_some_and(|tab| tab.dirty)
                }),
                "Cancel reintroduced its close decision or discarded the unsaved Scene",
            )?;
        }
        check(
            host.error
                .as_ref()
                .is_some_and(|error| error.to_string().contains(failure)),
            "cancelling semantic exit erased the retained terminal error",
        )
    });
}

#[test]
#[ignore = "requires a Windows desktop and an actual DX12 native SDK renderer route"]
fn native_dirty_exit_during_replacement_rebuilds_decision_frontend() {
    qualify(|host, event_loop| {
        initialize(host, event_loop)?;
        let (document, revision) = dirty_active_scene(&mut host.application)?;
        let native = host
            .native
            .as_ref()
            .ok_or("the initialized owner is missing")?;
        let binding = native.context.binding();
        let backend = native.gpu.backend_instance_id();
        let window = native.window.id();
        check(
            host.gpu_recovery.begin_automatic_replacement(),
            "the actual Host did not admit its first automatic replacement",
        )?;
        host.retirement_action = NativeRetirementAction::Install;
        host.retire_native();
        held_owner(host, backend)?;

        // Give the first production close poll a real SDK refusal, independent of whether the
        // initialization drain is already ready. This keeps InstallForExit observable even on
        // a fast adapter; the next shutdown retry physically completes the same renderer.
        let native = host
            .retiring
            .as_mut()
            .ok_or("queued replacement lost its predecessor")?;
        // SAFETY: this test owns the held Context between frames. No rendering follows this
        // change; only the production shutdown path observes its exact callback slot drift.
        unsafe {
            native
                .context
                .platform_io_mut()
                .set_renderer_render_window_raw(None);
        }
        host.window_event(event_loop, window, WindowEvent::CloseRequested);
        check(
            host.retirement_action == NativeRetirementAction::InstallForExit
                && host.application.has_pending_exit()
                && !event_loop.exiting(),
            "CloseRequested overwrote the queued replacement instead of preserving InstallForExit",
        )?;
        check(
            held_owner(host, backend)?.context.id() == binding.id(),
            "the close request discarded its actual retiring Context",
        )?;
        check(
            host.install_native(event_loop).is_err(),
            "a direct install bypassed the queued predecessor retirement",
        )?;
        held_owner(host, backend)?;
        check(
            matches!(event_loop.control_flow(), ControlFlow::WaitUntil(_)),
            "the dirty exit lost replacement maintenance while graphics were unavailable",
        )?;
        drive_retirement(host, event_loop)?;
        let replacement = host
            .native
            .as_ref()
            .ok_or("dirty exit lost its rebuilt decision frontend")?;
        let successor_binding = replacement.context.binding();
        let successor_backend = replacement.gpu.backend_instance_id();
        check(
            successor_backend != backend
                && successor_binding.id() != binding.id()
                && binding.lifecycle() == ContextLifecycle::NativeDestroyed
                && replacement
                    .context
                    .platform_io()
                    .renderer_render_window_raw()
                    .is_some()
                && host.retirement_action == NativeRetirementAction::AwaitingExitReady
                && host.application.has_pending_exit()
                && !event_loop.exiting(),
            "replacement did not restore a live frontend for the pending dirty decision",
        )?;
        let frame = host
            .application
            .current_frame()
            .ok_or("replacement lost the dirty workspace")?;
        check(
            frame.workspace_intent().phase() == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
                && frame
                    .workspace()
                    .document(document)
                    .is_some_and(|tab| tab.dirty && tab.revision == revision),
            "native replacement changed the unsaved source or lost its decision",
        )?;
        let prompt = frame
            .workspace_intent()
            .decision_prompt()
            .ok_or("the rebuilt frontend has no Cancel decision")?;
        host.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::WorkspaceIntent(
                    prompt.resolve(EditorCloseDecision::Cancel),
                )],
            ),
            Duration::ZERO,
        );
        for _ in 0..3 {
            host.about_to_wait(event_loop);
            check(
                host.retirement_action == NativeRetirementAction::Hold
                    && !host.application.has_pending_exit()
                    && host.retiring.is_none()
                    && !event_loop.exiting(),
                "maintenance resumed the cancelled exit after rebuilding its frontend",
            )?;
            let native = host
                .native
                .as_ref()
                .ok_or("Cancel discarded the rebuilt frontend")?;
            check(
                native.context.id() == successor_binding.id()
                    && native.gpu.backend_instance_id() == successor_backend
                    && successor_binding.lifecycle() == ContextLifecycle::Alive
                    && native
                        .context
                        .platform_io()
                        .renderer_render_window_raw()
                        .is_some(),
                "Cancel replaced or destroyed the rebuilt native owner",
            )?;
            check(
                host.application.current_frame().is_some_and(|frame| {
                    frame.workspace_intent().decision_prompt().is_none()
                        && frame
                            .workspace()
                            .document(document)
                            .is_some_and(|tab| tab.dirty && tab.revision == revision)
                }),
                "Cancel lost the exact unsaved revision or reintroduced its decision",
            )?;
        }
        Ok(())
    });
}

#[test]
#[ignore = "requires a Windows desktop and an actual DX12 native SDK renderer route"]
fn native_callback_drift_retains_custody_until_retry_then_replaces() {
    qualify(|host, event_loop| {
        initialize(host, event_loop)?;
        let native = host
            .native
            .as_mut()
            .ok_or("the initialized owner is missing")?;
        let binding = native.context.binding();
        let backend = native.gpu.backend_instance_id();
        // SAFETY: this test owns the Context and edits its exact installed callback between
        // frames. No platform rendering runs after this write; only production shutdown does.
        // Clearing a slot introduces SDK-observed drift without publishing a foreign callback.
        unsafe {
            native
                .context
                .platform_io_mut()
                .set_renderer_render_window_raw(None)
        };
        host.replace_native(event_loop);
        held_owner(host, backend)?;
        check(
            host.install_native(event_loop).is_err(),
            "direct install bypassed retiring custody",
        )?;
        let deadline = Instant::now() + RETIREMENT_TIMEOUT;
        loop {
            let native = held_owner(host, backend)?;
            if !native.close.errors.is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                return Err("the native SDK did not report callback drift".into());
            }
            host.drive_native_retirement(event_loop);
            std::thread::sleep(Duration::from_millis(1));
        }
        let native = held_owner(host, backend)?;
        check(
            native.context.id() == binding.id(),
            "callback refusal replaced the held Context",
        )?;
        check(
            !native.close.is_complete()
                && native.renderer.0.is_some()
                && native.context.io().backend_renderer_user_data().is_null()
                && native.context.platform_io().renderer_callbacks_are_empty(),
            "the first SDK error must retain adapter custody after physical renderer release",
        )?;
        check(
            native
                .close
                .errors
                .to_string()
                .contains("Renderer_RenderWindow"),
            "the retained error did not identify the actual callback drift",
        )?;
        check(
            host.install_native(event_loop).is_err(),
            "install bypassed the failed close phase",
        )?;
        held_owner(host, backend)?;
        drive_retirement(host, event_loop)?;
        let replacement = host
            .native
            .as_ref()
            .ok_or("retirement did not install its replacement")?;
        check(
            replacement.gpu.backend_instance_id() != backend,
            "replacement reused its predecessor backend",
        )?;
        check(
            binding.lifecycle() == ContextLifecycle::NativeDestroyed,
            "replacement preceded Context destruction",
        )?;
        check(
            !host.retirement_errors.is_empty()
                && host
                    .retirement_errors
                    .to_string()
                    .contains("Renderer_RenderWindow"),
            "successful retry erased the Host's close error",
        )?;
        check(
            !event_loop.exiting(),
            "renderer replacement unexpectedly exited",
        )
    });
}

fn reject_partial_initialization(
    host: &mut NativeEditorHost,
    event_loop: &ActiveEventLoop,
) -> Result<(), String> {
    super::retirement::reject_next_native_attach();
    check(
        host.install_native(event_loop).is_err(),
        "the SDK accepted the rejected native attachment",
    )?;
    let native = host
        .retiring
        .as_mut()
        .ok_or("attach refusal discarded partial initialization")?;
    require_dx12(native)?;
    let binding = native.context.binding();
    let backend = native.gpu.backend_instance_id();
    check(
        native.renderer.0.is_some()
            && native.close.drain.is_some()
            && !native.context.io().backend_renderer_user_data().is_null()
            && !native.platform.viewports_enabled()
            && native.context.platform_io().renderer_callbacks_are_empty(),
        "attach refusal did not retain its returned docked renderer and initialization drain",
    )?;
    held_owner(host, backend)?;
    check(
        host.install_native(event_loop).is_err(),
        "direct install bypassed partial initialization",
    )?;
    check(
        held_owner(host, backend)?.context.id() == binding.id(),
        "second install changed the held owner",
    )?;
    check(
        host.retirement_action == NativeRetirementAction::Hold,
        "partial initialization requested replacement",
    )?;
    drive_retirement(host, event_loop)?;
    check(
        host.native.is_none(),
        "Hold installed a replacement automatically",
    )?;
    check(
        binding.lifecycle() == ContextLifecycle::NativeDestroyed,
        "partial Context survived retirement",
    )?;
    check(
        host.retirement_errors.is_empty(),
        "partial initialization cleanup failed",
    )
}

#[test]
#[ignore = "requires a Windows desktop, DX12 and actual SDK native attachment refusal"]
fn native_partial_initialization_preserves_fresh_and_recovered_frontends() {
    qualify(|host, event_loop| {
        reject_partial_initialization(host, event_loop)?;
        check(
            matches!(host.retained, Some(PendingImguiFrontend::Fresh(_))),
            "partial initialization lost the fresh frontend",
        )?;
        host.install_native(event_loop)
            .map_err(|error| error.to_string())?;
        let native = host
            .native
            .as_mut()
            .ok_or("explicit installation failed after partial retirement")?;
        native.frame_index = 7;
        native.next_view = 11;
        host.retire_native();
        drive_retirement(host, event_loop)?;
        let layout = match &host.retained {
            Some(PendingImguiFrontend::Recovered(retained)) => retained.layout.clone(),
            _ => return Err("normal retirement did not retain its recovered frontend".into()),
        };
        reject_partial_initialization(host, event_loop)?;
        check(
            matches!(&host.retained, Some(PendingImguiFrontend::Recovered(retained))
                if retained.frame_index == 7 && retained.next_view == 11 && retained.layout == layout),
            "partial initialization overwrote the predecessor's recovered frontend state",
        )?;
        host.install_native(event_loop)
            .map_err(|error| error.to_string())?;
        let native = host
            .native
            .as_ref()
            .ok_or("explicit recovery installation failed")?;
        check(
            native.frame_index == 7 && native.next_view == 11 && native.layout_loaded,
            "successful installation did not restore the retained frontend state",
        )
    });
}
