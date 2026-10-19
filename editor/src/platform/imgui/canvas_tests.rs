#![cfg(target_os = "windows")]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    render::Extent2d,
    render_wgpu::__private::WgpuSubmittedImageReceipt,
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorNavigationAction,
        EditorWorkspaceIntentPhase,
    },
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{NativeImgui, editor_window_packet};
use crate::{EditorApplication, EditorLaunchOptions, frontend::imgui::UiProject};

struct CanvasObservation {
    submitted: Option<WgpuSubmittedImageReceipt>,
    current: bool,
    pick_granted: bool,
    surface_acquires: u64,
    presents: u64,
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn unpresented_canvas_keeps_its_submitted_image_without_granting_picking() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = CanvasTestApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // The complete native and Session shutdown precedes every behavior assertion.
    let observations = application
        .result
        .expect("the native Canvas trajectory must run")
        .unwrap();
    assert_eq!(observations.len(), 4);
    let first = observations[0]
        .submitted
        .expect("the first candidate must reach the actual GPU");
    for (index, observed) in observations.into_iter().enumerate() {
        assert_eq!(
            observed.submitted,
            Some(first),
            "owner drive {index} must retain the exact unsampled image instead of submitting it again"
        );
        assert!(
            !observed.current,
            "an unsampled image cannot become the displayed Canvas"
        );
        assert!(
            !observed.pick_granted,
            "an unsampled Canvas cannot grant source picking"
        );
        assert_eq!(
            observed.surface_acquires, 0,
            "the trajectory must not acquire a native surface"
        );
        assert_eq!(
            observed.presents, 0,
            "the trajectory must not present a native surface"
        );
    }
}

struct CanvasTestApplication {
    project: PathBuf,
    result: Option<Result<Vec<CanvasObservation>, String>>,
}

impl ApplicationHandler for CanvasTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return;
        }
        self.result = Some(run_native_canvas(event_loop, &self.project));
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_native_canvas(
    event_loop: &ActiveEventLoop,
    project: &Path,
) -> Result<Vec<CanvasObservation>, String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara pending UI Canvas regression")
                    .with_visible(false)
                    .with_inner_size(LogicalSize::new(320.0, 240.0)),
            )
            .map_err(|error| error.to_string())?,
    );
    let mut native =
        NativeImgui::new(event_loop, Arc::clone(&window)).map_err(|error| error.to_string())?;
    let opened = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    );
    let mut application = match opened {
        Ok(application) => application,
        Err(error) => {
            let shutdown = native.shutdown_graphics();
            return Err(match shutdown {
                Ok(()) => error.to_string(),
                Err(cleanup) => format!("{error}; native cleanup also failed: {cleanup}"),
            });
        }
    };
    application.native_shell_ready();
    let result = record_pending_canvas(&mut native, &mut application);
    let shutdown = native
        .shutdown_graphics()
        .map_err(|error| error.to_string());
    drop(native);
    drop(window);
    let retirement = retire_application(&mut application);
    drop(application);
    match (result, shutdown, retirement) {
        (Ok(observations), Ok(()), Ok(())) => Ok(observations),
        (result, shutdown, retirement) => {
            let mut failures = Vec::new();
            if let Err(error) = result {
                failures.push(error);
            }
            if let Err(error) = shutdown {
                failures.push(format!("native cleanup failed: {error}"));
            }
            if let Err(error) = retirement {
                failures.push(format!("Session cleanup failed: {error}"));
            }
            Err(failures.join("; "))
        }
    }
}

fn record_pending_canvas(
    native: &mut NativeImgui,
    application: &mut EditorApplication,
) -> Result<Vec<CanvasObservation>, String> {
    let frame = application
        .current_frame()
        .ok_or("the trusted project has no Editor frame")?;
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .ok_or("the startup Scene is unavailable")?;
    let anchor = SceneEntityId::new("ui-anchor").map_err(|error| error.to_string())?;
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: anchor,
                }),
            ],
        ),
        Duration::ZERO,
    );
    application.set_scene_preview_extent(Extent2d::new(320, 240));
    let window = editor_window_packet(&native.window);
    let mut observations = Vec::with_capacity(4);
    for _ in 0..4 {
        application.drive_editor_frame(Duration::ZERO, false, true);
        let authoring = application
            .current_ui_authoring_frame()
            .ok_or("the UI Prefab has no published authoring frame")?;
        native.frame_index += 1;
        native
            .gpu
            .begin_editor_frame(window, native.frame_index)
            .map_err(|error| error.to_string())?;
        native
            .submit_ui_canvas(application, &authoring, window)
            .map_err(|error| error.to_string())?;
        let stats = native.gpu.frame_transaction_stats();
        observations.push(CanvasObservation {
            submitted: native.scene.submitted,
            current: native.canvas.is_current(&native.scene, &authoring),
            pick_granted: native
                .canvas
                .resolve_pick(&native.scene, &authoring, [0.5, 0.5])
                .is_some(),
            surface_acquires: stats.surface_acquire_attempts(),
            presents: stats.presents(),
        });
    }
    Ok(observations)
}

pub(super) fn retire_application(application: &mut EditorApplication) -> Result<(), String> {
    if application.request_exit().exit_ready() {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let decision = application.current_frame().filter(|frame| {
            frame.workspace_intent().phase() == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        });
        let update = if let Some(frame) = decision {
            application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        frame
                            .workspace_intent()
                            .decision_prompt()
                            .unwrap()
                            .resolve(EditorCloseDecision::Discard),
                    )],
                ),
                Duration::ZERO,
            )
        } else {
            application.drive_editor_frame(Duration::ZERO, false, true)
        };
        if update.exit_ready() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err("the real trusted project did not complete its normal exit".to_owned())
}
