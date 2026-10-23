use dear_imgui_wgpu::ExternalTextureId;
use nara::{
    project_host::{
        EditorAcceptedPreview, EditorPresentationImage, EditorPresentedToken,
        EditorPreviewCandidate, EditorPreviewContentStamp, EditorPreviewPresentation,
    },
    render::{DisplayMapping2d, Extent2d, RenderFrameTarget, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuSubmittedImageReceipt,
    },
    tooling::{EditorAction, EditorFrameModel, EditorNavigationAction},
};

use super::{EditorRunError, ImguiRenderer, ImguiTarget, imgui_error, platform_gpu_error};

struct PreparedScene {
    content: EditorPreviewContentStamp,
    accepted: EditorAcceptedPreview,
    submission: WgpuSubmittedImageReceipt,
    texture: ExternalTextureId,
}

struct PresentedScene {
    content: EditorPreviewContentStamp,
    token: EditorPresentedToken,
    receipt: WgpuEditorCompositeReceipt,
    mapping: DisplayMapping2d,
    texture: ExternalTextureId,
}

#[derive(Default)]
pub(super) struct ImguiScenePresenter {
    authority: EditorPreviewPresentation,
    prepared: Option<PreparedScene>,
    presented: Option<PresentedScene>,
}

impl ImguiScenePresenter {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn prepare(
        &mut self,
        target: &mut ImguiTarget,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorPreviewCandidate,
        next_view: &mut u64,
    ) -> Result<(), EditorRunError> {
        let content = candidate.content_stamp();
        if self.prepared.as_ref().is_some_and(|prepared| {
            prepared.content == content
                && matches_target(target, prepared.submission, prepared.texture)
        }) || self.presented.as_ref().is_some_and(|presented| {
            presented.content == content
                && matches_target(target, presented.receipt.image(), presented.texture)
        }) {
            return Ok(());
        }
        self.retire();
        let (accepted, packet, sprites) = self.authority.accept(candidate).map_err(imgui_error)?;
        let RenderFrameTarget::Image(image) = packet.target() else {
            return Err(imgui_error("Scene preview did not retain an image target"));
        };
        target.ensure(
            gpu,
            renderer,
            window,
            packet.source_generation(),
            image,
            next_view,
        )?;
        let view = target
            .view
            .ok_or_else(|| imgui_error("Scene target has no render view"))?;
        let texture = target
            .texture
            .ok_or_else(|| imgui_error("Scene target has no toolkit texture"))?;
        let submission = gpu
            .submit_sprite_image(view, window, packet, sprites)
            .map_err(platform_gpu_error)?;
        if !target
            .identity
            .is_some_and(|identity| identity.matches_submission(submission))
        {
            return Err(imgui_error(
                "Scene submission did not match its committed target",
            ));
        }
        target.submitted = Some(submission);
        self.prepared = Some(PreparedScene {
            content,
            accepted,
            submission,
            texture,
        });
        Ok(())
    }

    pub(super) fn is_current(
        &mut self,
        target: &ImguiTarget,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> bool {
        let Some(scene) = frame
            .scene_preview_document()
            .and_then(|document| frame.workspace().scene(document))
        else {
            return false;
        };
        if let Some(prepared) = self.prepared.as_ref() {
            return prepared.accepted.document() == scene.document
                && prepared.accepted.revision() == scene.revision
                && requested_extent == Some(prepared.submission.target().extent())
                && matches_target(target, prepared.submission, prepared.texture);
        }
        let Some(presented) = self.presented.as_ref() else {
            return false;
        };
        requested_extent == Some(presented.receipt.image().target().extent())
            && matches_target(target, presented.receipt.image(), presented.texture)
            && self.authority.token(
                scene.document,
                scene.revision,
                presentation_image(presented.receipt.image()),
                presented.mapping,
            ) == Some(presented.token)
    }

    pub(super) fn is_presented(
        &mut self,
        target: &ImguiTarget,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> bool {
        self.prepared.is_none() && self.is_current(target, frame, requested_extent)
    }

    /// The caller supplies receipts only for the window that displayed this mapping.
    pub(super) fn promote(
        &mut self,
        target: &ImguiTarget,
        receipts: impl IntoIterator<Item = WgpuEditorCompositeReceipt>,
        mapping: DisplayMapping2d,
    ) -> Result<bool, EditorRunError> {
        let Some(submission) = target.submitted else {
            self.retire();
            return Ok(false);
        };
        let Some(texture) = target.texture else {
            self.retire();
            return Ok(false);
        };
        if !matches_target(target, submission, texture)
            || mapping.target() != submission.target().extent()
        {
            self.retire();
            return Ok(false);
        }
        let Some(receipt) = receipts
            .into_iter()
            .find(|receipt| receipt.image() == submission)
        else {
            self.authority.retire();
            self.presented = None;
            return Ok(false);
        };
        if let Some(prepared) = self.prepared.take() {
            if prepared.submission != receipt.image() || prepared.texture != texture {
                self.retire();
                return Err(imgui_error(
                    "Scene display did not match its accepted preview",
                ));
            }
            let token = self
                .authority
                .finish(
                    prepared.accepted,
                    presentation_image(receipt.image()),
                    mapping,
                )
                .map_err(imgui_error)?;
            self.presented = Some(PresentedScene {
                content: prepared.content,
                token,
                receipt,
                mapping,
                texture,
            });
            return Ok(true);
        }
        let Some(presented) = self.presented.as_mut() else {
            return Ok(false);
        };
        if presented.receipt.image() != receipt.image() || presented.texture != texture {
            self.retire();
            return Ok(false);
        }
        let mapping_changed = presented.mapping != mapping;
        if mapping_changed {
            presented.token = self
                .authority
                .redisplay(
                    presented.token,
                    presentation_image(receipt.image()),
                    mapping,
                )
                .map_err(imgui_error)?;
            presented.mapping = mapping;
        }
        presented.receipt = receipt;
        Ok(mapping_changed)
    }

    pub(super) fn resolve_pick(
        &mut self,
        frame: &EditorFrameModel,
        point: [f32; 2],
        mapping: DisplayMapping2d,
    ) -> Option<EditorAction> {
        let presented = self.presented.as_ref()?;
        if presented.mapping != mapping {
            return None;
        }
        let scene = frame.workspace().scene(frame.scene_preview_document()?)?;
        if frame.workspace().active_document != Some(scene.document) {
            return None;
        }
        let token = self.authority.token(
            scene.document,
            scene.revision,
            presentation_image(presented.receipt.image()),
            mapping,
        )?;
        if token != presented.token {
            return None;
        }
        let document = self.authority.document(token)?;
        let entity = self.authority.pick(token, point).ok()?;
        Some(EditorAction::Navigate(
            EditorNavigationAction::SelectEntity { document, entity },
        ))
    }

    pub(super) fn retire(&mut self) {
        self.authority.retire();
        self.prepared = None;
        self.presented = None;
    }
}

fn matches_target(
    target: &ImguiTarget,
    submission: WgpuSubmittedImageReceipt,
    texture: ExternalTextureId,
) -> bool {
    target.submitted == Some(submission)
        && target.texture == Some(texture)
        && target.target == Some(submission.target())
        && target.source == Some(submission.source_generation())
        && target
            .identity
            .is_some_and(|identity| identity.matches_submission(submission))
}

fn presentation_image(image: WgpuSubmittedImageReceipt) -> EditorPresentationImage {
    EditorPresentationImage {
        target: image.target(),
        target_generation: image.target_generation(),
        source_generation: image.source_generation(),
        source_handoff_generation: image.source_handoff_generation(),
        backend_instance_id: image.backend_instance_id(),
        device_epoch: image.device_epoch(),
        frame_index: image.render_transaction_frame_index(),
    }
}

#[cfg(all(test, target_os = "windows"))]
mod binding_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    };

    use dear_imgui_rs::ConfigFlags;
    use nara::{
        asset::AssetRef,
        scene::{PrefabDocument, PrefabInstance, SceneEntityId, SceneEntityRecord},
        tooling::{
            EditorActionBatch, EditorDocumentId, EditorDocumentKind, EditorWorkspaceIntent,
            EditorWorkspaceIntentAction,
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

    use super::super::{NativeImgui, canvas_tests::retire_application};
    use super::*;
    use crate::{EditorApplication, EditorLaunchOptions, frontend::imgui::UiProject};

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn native_retained_scene_receipt_survives_prefab_navigation_and_bound_close() {
        run_binding_test(false);
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
    fn native_ui_canvas_priority_survives_closing_retained_scene() {
        run_binding_test(true);
    }

    fn run_binding_test(canvas_priority: bool) {
        let project = UiProject::new();
        fs::write(
            project.0.join("prefabs/ordinary.prefab.json"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("ordinary").unwrap(),
            )])
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let source = project.0.join("scenes/startup.scene.json");
        let mut scene: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("ordinary-anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("ordinary.prefab.json").unwrap(),
            overrides: Default::default(),
        });
        scene["payload"]["roots"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&anchor.id).unwrap());
        scene["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(anchor).unwrap());
        fs::write(source, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();
        fs::write(
            project.0.join("scenes/other.scene.json"),
            serde_json::to_vec_pretty(&scene).unwrap(),
        )
        .unwrap();

        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().unwrap();
        let mut application = BindingApplication {
            project: project.0.clone(),
            canvas_priority,
            result: None,
        };
        event_loop.run_app(&mut application).unwrap();
        application
            .result
            .expect("the retained Scene binding trajectory must run")
            .unwrap();
    }

    struct BindingApplication {
        project: PathBuf,
        canvas_priority: bool,
        result: Option<Result<(), String>>,
    }

    impl ApplicationHandler for BindingApplication {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.result.is_none() {
                self.result = Some(run_binding_workflow(
                    event_loop,
                    &self.project,
                    self.canvas_priority,
                ));
            }
            event_loop.exit();
        }

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }

    fn run_binding_workflow(
        event_loop: &ActiveEventLoop,
        project: &Path,
        canvas_priority: bool,
    ) -> Result<(), String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Nara retained Scene binding regression")
                        .with_active(false)
                        .with_visible(false)
                        .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
                )
                .map_err(|error| error.to_string())?,
        );
        let mut native =
            NativeImgui::new(event_loop, Arc::clone(&window)).map_err(|error| error.to_string())?;
        native.layout_loaded = true;
        let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
        flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
        native.context.io_mut().set_config_flags(flags);
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(project).with_profile("desktop"));
        application.native_shell_ready();
        let result = BindingWorkflow {
            native: &mut native,
            application: &mut application,
            event_loop,
        }
        .run(canvas_priority);
        let graphics = native
            .shutdown_graphics()
            .map_err(|error| error.to_string());
        drop(native);
        drop(window);
        let session = retire_application(&mut application);
        drop(application);
        result.and(graphics).and(session)
    }

    struct BindingWorkflow<'a> {
        native: &'a mut NativeImgui,
        application: &'a mut EditorApplication,
        event_loop: &'a ActiveEventLoop,
    }

    impl BindingWorkflow<'_> {
        fn paint(&mut self) -> Result<(), String> {
            self.application.advance_project_open();
            self.application
                .drive_editor_frame(Duration::ZERO, false, false);
            let exited = self
                .native
                .redraw_with_input(self.event_loop, self.application, |context| {
                    let viewport = context.main_viewport().id();
                    let io = context.io_mut();
                    io.clear_events_queue();
                    io.add_focus_event(true);
                    io.add_mouse_pos_event([-1000.0, -1000.0]);
                    io.add_mouse_viewport_event(viewport);
                })
                .map_err(|error| error.to_string())?;
            if exited {
                return Err("the retained Scene workflow unexpectedly exited".to_owned());
            }
            Ok(())
        }

        fn navigate(&mut self, action: EditorNavigationAction) {
            let frame = self.application.current_frame().unwrap();
            self.application.submit_and_drive(
                EditorActionBatch::new(frame.revision(), [EditorAction::Navigate(action)]),
                Duration::ZERO,
            );
        }

        fn open_prefab(&mut self, document: EditorDocumentId, entity: &str) -> Result<(), String> {
            let frame = self.application.current_frame().unwrap();
            let entity = SceneEntityId::new(entity).unwrap();
            let submission = self.application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [
                        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                            document,
                        }),
                        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                            document,
                            entity: Some(entity.clone()),
                        }),
                        EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                            document,
                            base_revision: frame.workspace().document(document).unwrap().revision,
                            entity,
                        }),
                    ],
                ),
                Duration::ZERO,
            );
            if submission.actions().iter().all(|action| matches!(action,
                crate::EditorApplicationActionDispatch::Dispatched(nara::tooling::EditorActionOutcome::Workspace(report)) if report.applied
            )) { Ok(()) } else { Err(format!("Prefab navigation did not pass its owning fences: {:?}", submission.actions())) }
        }

        fn open_scene(&mut self, path: &str) -> Result<EditorDocumentId, String> {
            use nara::tooling::{
                EditorActionOutcome, EditorDocumentOpenResult, EditorProjectBrowserAction,
                EditorProjectBrowserActionOutcome,
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            while self
                .application
                .current_frame()
                .unwrap()
                .project_browser()
                .entry(path)
                .is_none()
            {
                if Instant::now() >= deadline {
                    return Err("the second Scene was not indexed".to_owned());
                }
                self.paint()?;
                std::thread::sleep(Duration::from_millis(1));
            }
            let frame = self.application.current_frame().unwrap();
            let submission = self.application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::ProjectBrowser(
                        EditorProjectBrowserAction::Open {
                            relative_path: Arc::from(path),
                        },
                    )],
                ),
                Duration::ZERO,
            );
            let [
                crate::EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::ProjectBrowser(
                        EditorProjectBrowserActionOutcome::Preparing { request },
                    ),
                ),
            ] = submission.actions()
            else {
                return Err(format!(
                    "second Scene Open was not admitted: {:?}",
                    submission.actions()
                ));
            };
            let request = *request;
            loop {
                let frame = self.application.current_frame().unwrap();
                if let Some(result) = frame.document_open().result() {
                    let EditorDocumentOpenResult::Opened {
                        request: completed,
                        document,
                    } = result
                    else {
                        return Err(format!("second Scene Open failed: {result:?}"));
                    };
                    if completed != request {
                        return Err("second Scene Open returned another request".to_owned());
                    }
                    let acknowledged = self.application.submit_and_drive(
                        EditorActionBatch::new(
                            frame.revision(),
                            [EditorAction::ProjectBrowser(
                                EditorProjectBrowserAction::AcknowledgeOpen { request },
                            )],
                        ),
                        Duration::ZERO,
                    );
                    if !matches!(acknowledged.actions(), [crate::EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged { request: exact }))] if *exact == request)
                    {
                        return Err(format!(
                            "second Scene result acknowledgement failed: {:?}",
                            acknowledged.actions()
                        ));
                    }
                    return Ok(document);
                }
                if Instant::now() >= deadline {
                    return Err("second Scene Open did not finish".to_owned());
                }
                self.application
                    .drive_editor_frame(Duration::ZERO, false, true);
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn close(&mut self, document: EditorDocumentId) -> Result<(), String> {
            let frame = self.application.current_frame().unwrap();
            let submission = self.application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        EditorWorkspaceIntentAction::Request(
                            EditorWorkspaceIntent::CloseDocument { document },
                        ),
                    )],
                ),
                Duration::ZERO,
            );
            if !matches!(
                submission.actions(),
                [crate::EditorApplicationActionDispatch::Dispatched(
                    nara::tooling::EditorActionOutcome::WorkspaceIntent(
                        nara::tooling::EditorWorkspaceIntentRequestResult::Accepted
                    )
                )]
            ) {
                return Err(format!(
                    "Close was not admitted: {:?}",
                    submission.actions()
                ));
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while self
                .application
                .current_frame()
                .is_some_and(|frame| frame.workspace().document(document).is_some())
            {
                if Instant::now() >= deadline {
                    return Err("the clean source did not complete Close".to_owned());
                }
                self.application
                    .drive_editor_frame(Duration::ZERO, false, true);
                std::thread::sleep(Duration::from_millis(1));
            }
            let frame = self.application.current_frame().unwrap();
            let acknowledgement = self.application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        EditorWorkspaceIntentAction::AcknowledgeResult,
                    )],
                ),
                Duration::ZERO,
            );
            if !matches!(
                acknowledgement.actions(),
                [crate::EditorApplicationActionDispatch::Dispatched(
                    nara::tooling::EditorActionOutcome::Acknowledged(
                        nara::tooling::EditorActionAcknowledgement::WorkspaceIntent(true)
                    )
                )]
            ) {
                return Err(format!(
                    "Close result was not acknowledged: {:?}",
                    acknowledgement.actions()
                ));
            }
            Ok(())
        }

        fn run(&mut self, canvas_priority: bool) -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.paint()?;
                if let Some(frame) = self.application.current_frame() {
                    let game = self
                        .application
                        .panel_workspace()
                        .layout()
                        .instances()
                        .iter()
                        .find(|panel| panel.kind() == "nara.game-view" && panel.is_visible())
                        .map(|panel| panel.id());
                    if let Some(game) = game {
                        self.application
                            .panel_workspace_mut()
                            .layout_mut()
                            .close(game);
                        self.native.frontend.reset_layout();
                    } else if self.native.scene_presenter.is_presented(
                        &self.native.scene,
                        &frame,
                        self.native.scene_extent,
                    ) {
                        break;
                    }
                }
                if Instant::now() >= deadline {
                    return Err("the startup Scene has no exact composite presentation".to_owned());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let frame = self.application.current_frame().unwrap();
            let scene = frame
                .scene_preview_document()
                .ok_or("the startup Scene is not bound")?;
            if canvas_priority {
                return self.run_canvas_priority(scene);
            }
            let image = self
                .native
                .scene
                .submitted
                .ok_or("the Scene has no submitted image")?;
            self.open_prefab(scene, "ordinary-anchor")?;
            let frame = self.application.current_frame().unwrap();
            let prefab = frame
                .workspace()
                .active_document
                .ok_or("the Prefab did not open")?;
            if prefab == scene
                || frame
                    .workspace()
                    .active_scene
                    .as_ref()
                    .map(|model| model.kind)
                    != Some(EditorDocumentKind::Prefab)
                || frame.scene_preview_document() != Some(scene)
                || self.application.current_ui_authoring_frame().is_some()
            {
                return Err(
                    "ordinary Prefab navigation changed Scene binding or Canvas priority"
                        .to_owned(),
                );
            }
            let mapping = self
                .native
                .scene_presenter
                .presented
                .as_ref()
                .unwrap()
                .mapping;
            let content = mapping.content_rect();
            let point = [
                (content.min[0] + content.max[0]) * 0.5,
                (content.min[1] + content.max[1]) * 0.5,
            ];
            if !self.native.scene_presenter.is_presented(
                &self.native.scene,
                &frame,
                self.native.scene_extent,
            ) || self
                .native
                .scene_presenter
                .resolve_pick(&frame, point, mapping)
                .is_some()
            {
                return Err(
                    "inactive bound Scene must remain displayed without granting geometry input"
                        .to_owned(),
                );
            }
            self.paint()?;
            let frame = self.application.current_frame().unwrap();
            if self.native.scene.submitted != Some(image)
                || !self.native.scene_presenter.is_presented(
                    &self.native.scene,
                    &frame,
                    self.native.scene_extent,
                )
            {
                return Err(
                    "ordinary Prefab repaint replaced or poisoned the retained Scene receipt"
                        .to_owned(),
                );
            }
            self.navigate(EditorNavigationAction::SetActiveDocument { document: scene });
            let frame = self.application.current_frame().unwrap();
            let mapping = self
                .native
                .scene_presenter
                .presented
                .as_ref()
                .unwrap()
                .mapping;
            let content = mapping.content_rect();
            let point = [
                (content.min[0] + content.max[0]) * 0.5,
                (content.min[1] + content.max[1]) * 0.5,
            ];
            if !matches!(
                self.native.scene_presenter.resolve_pick(&frame, point, mapping),
                Some(EditorAction::Navigate(EditorNavigationAction::SelectEntity { document, .. }))
                    if document == scene
            ) {
                return Err(
                    "explicit Scene activation did not recover the unchanged receipt's picking authority"
                        .to_owned(),
                );
            }
            let second = self.open_scene("scenes/other.scene.json")?;
            let frame = self.application.current_frame().unwrap();
            if second == scene
                || frame.workspace().document(scene).is_none()
                || frame.workspace().document(prefab).is_none()
                || frame.scene_preview_document() != Some(second)
                || self
                    .native
                    .scene_presenter
                    .resolve_pick(&frame, point, mapping)
                    .is_some()
            {
                return Err(
                    "additive Scene Open lost an old document or reused its picking authority"
                        .to_owned(),
                );
            }
            self.paint()?;
            let frame = self.application.current_frame().unwrap();
            if !self.native.scene_presenter.is_presented(
                &self.native.scene,
                &frame,
                self.native.scene_extent,
            ) {
                return Err("the second Scene did not receive its own presentation".to_owned());
            }
            self.navigate(EditorNavigationAction::SetActiveDocument { document: prefab });
            self.close(second)?;
            self.paint()?;
            let frame = self.application.current_frame().unwrap();
            if frame.scene_preview_document().is_some()
                || frame.workspace().active_document != Some(prefab)
                || frame.workspace().document(scene).is_none()
                || self.application.current_ui_authoring_frame().is_some()
            {
                return Err("closing the bound Scene changed the active ordinary Prefab".to_owned());
            }
            self.ensure_retired()
        }

        fn run_canvas_priority(&mut self, scene: EditorDocumentId) -> Result<(), String> {
            self.open_prefab(scene, "ordinary-anchor")?;
            let ordinary = self
                .application
                .current_frame()
                .unwrap()
                .workspace()
                .active_document
                .ok_or("the ordinary Prefab is not active")?;
            self.open_prefab(scene, "ui-anchor")?;
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.paint()?;
                if let Some(authoring) = self.application.current_ui_authoring_frame()
                    && self
                        .native
                        .canvas
                        .is_current(&self.native.scene, &authoring)
                {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(
                        "the UI Prefab did not publish its distinct Canvas authority".to_owned(),
                    );
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let prefab = self
                .application
                .current_frame()
                .unwrap()
                .workspace()
                .active_document
                .ok_or("the UI Prefab is not active")?;
            let frame = self.application.current_frame().unwrap();
            if ordinary == prefab || frame.workspace().document(ordinary).is_none() {
                return Err("opening a UI Prefab retired the unrelated ordinary Prefab".to_owned());
            }
            self.close(ordinary)?;
            self.paint()?;
            let frame = self.application.current_frame().unwrap();
            let authoring = self
                .application
                .current_ui_authoring_frame()
                .ok_or("unrelated Prefab Close lost the UI Canvas")?;
            if frame.workspace().active_document != Some(prefab)
                || frame.scene_preview_document() != Some(scene)
                || !self
                    .native
                    .canvas
                    .is_current(&self.native.scene, &authoring)
            {
                return Err("unrelated Prefab Close changed Canvas or Scene authority".to_owned());
            }
            self.close(scene)?;
            self.paint()?;
            let frame = self.application.current_frame().unwrap();
            let authoring = self
                .application
                .current_ui_authoring_frame()
                .ok_or("closing the bound Scene retired the active UI Canvas")?;
            if frame.scene_preview_document().is_some()
                || !self
                    .native
                    .canvas
                    .is_current(&self.native.scene, &authoring)
                || self.native.scene.image().is_none()
            {
                return Err(
                    "a UI Prefab Canvas must retain priority without a bound Scene".to_owned(),
                );
            }
            self.close(prefab)?;
            self.paint()?;
            self.ensure_retired()
        }

        fn ensure_retired(&self) -> Result<(), String> {
            if self.native.scene.image().is_some()
                || self.native.scene.submitted.is_some()
                || self.native.scene_presenter.prepared.is_some()
                || self.native.scene_presenter.presented.is_some()
            {
                return Err(
                    "the last closed source left a stale Scene image or presenter".to_owned(),
                );
            }
            Ok(())
        }
    }
}
