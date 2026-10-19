use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, MouseButton, TextureId};
use nara::{
    project_host::{EditorUiAuthoringFrame, TrustedCompiledEditorProjectIntent},
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorGestureAction,
        EditorGestureCancelReason, EditorNavigationAction,
    },
    ui::__private::{UiAuthoringDimensionKind, UiAuthoringFieldValue},
};

use super::{
    ImguiEditorFrontend, ImguiFrontendOutput, ImguiViewImage,
    ui_interaction_tests::{UiProject, clear_controls, control_rect},
};
use crate::{EditorApplication, EditorLaunchOptions};

struct CanvasAdmission {
    application: EditorApplication,
    frontend: ImguiEditorFrontend,
    context: Context,
}

impl CanvasAdmission {
    fn new(project: &UiProject) -> Self {
        let mut application = EditorApplication::open_trusted_compiled(
            EditorLaunchOptions::new(&project.0).with_profile("desktop"),
            TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap();
        application.native_shell_ready();
        let frame = application.current_frame().unwrap();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let anchor = SceneEntityId::new("ui-anchor").unwrap();
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
        let authoring = application.current_ui_authoring_frame().unwrap();
        let frame = application.current_frame().unwrap();
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: authoring.document(),
                        entity: Some(SceneEntityId::new("image-node").unwrap()),
                    },
                )],
            ),
            Duration::ZERO,
        );
        let closed = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|instance| instance.kind() != "nara.authoring-view")
            .map(|instance| instance.id())
            .collect::<Vec<_>>();
        for instance in closed {
            application
                .panel_workspace_mut()
                .layout_mut()
                .close(instance);
        }
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE);
        context.io_mut().set_display_size([1600.0, 1100.0]);
        context.io_mut().set_display_framebuffer_scale([1.0, 1.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut interaction = Self {
            application,
            frontend: ImguiEditorFrontend::new(),
            context,
        };
        for _ in 0..6 {
            interaction.step(true);
        }
        interaction
    }

    fn authoring(&self) -> Arc<EditorUiAuthoringFrame> {
        self.application.current_ui_authoring_frame().unwrap()
    }

    fn layout(&self) -> [f64; 4] {
        let frame = self.authoring();
        let projection = frame.projection().unwrap();
        [
            "style.left.kind",
            "style.top.kind",
            "style.width.kind",
            "style.height.kind",
        ]
        .map(|field| {
            let descriptor = projection
                .fields()
                .iter()
                .find(|descriptor| {
                    descriptor.key().entity().as_str() == "image-node"
                        && descriptor.key().primary_field().as_str() == field
                })
                .unwrap();
            let UiAuthoringFieldValue::Dimension {
                kind: UiAuthoringDimensionKind::Pixels,
                value,
            } = descriptor.value()
            else {
                panic!("the selected image node must have absolute pixel dimensions");
            };
            *value
        })
    }

    fn history(&self) -> usize {
        self.application
            .current_frame()
            .unwrap()
            .workspace()
            .document(self.authoring().document())
            .unwrap()
            .history
            .undo_depth
    }

    fn draw(&mut self) -> ImguiFrontendOutput {
        clear_controls();
        let ui = self.context.frame();
        // This CPU adapter checks the real widget/action admission boundary. Its texture identity
        // is local draw data; no GPU submission, native surface or operating-system input is claimed.
        let output = self
            .frontend
            .show_with_views(
                ui,
                &mut self.application,
                Some(ImguiViewImage {
                    texture: TextureId::new(1),
                    extent: [960, 540],
                }),
                None,
                false,
                true,
                "CPU Canvas admission regression",
            )
            .unwrap();
        let _ = self.context.render_legacy();
        output
    }

    fn finish(&mut self, mut output: ImguiFrontendOutput, confirmed: bool) {
        let authoring = self.authoring();
        self.frontend
            .finish_canvas_presentation(&mut output, Some(&authoring), confirmed);
        if let Some(batch) = self.frontend.seal_output(&mut output) {
            self.frontend
                .submit_and_drive(&mut self.application, batch, Duration::ZERO);
        }
    }

    fn step(&mut self, confirmed: bool) {
        let output = self.draw();
        self.finish(output, confirmed);
    }

    fn released_canvas_commit(&mut self) -> ImguiFrontendOutput {
        let selected = control_rect("canvas-selection")
            .expect("the actual selected Canvas overlay must be drawn");
        let pointer = [
            (selected.min[0] + selected.max[0]) * 0.5,
            (selected.min[1] + selected.max[1]) * 0.5,
        ];
        self.context.io_mut().add_mouse_pos_event(pointer);
        self.step(true);
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        self.step(true);
        self.context
            .io_mut()
            .add_mouse_pos_event([pointer[0] + 24.0, pointer[1] + 16.0]);
        let begin = self.draw();
        assert!(
            begin.canvas_actions.iter().any(|action| matches!(
                action,
                EditorAction::Gesture(EditorGestureAction::Begin { .. })
            )),
            "dragging the actual Canvas control must generate Begin"
        );
        self.finish(begin, true);
        let gesture = self.authoring().gesture().unwrap().stamp().id();
        let preview = self.draw();
        assert!(preview.canvas_actions.iter().any(|action| matches!(
            action, EditorAction::Gesture(EditorGestureAction::Preview { gesture: id, .. }) if *id == gesture
        )), "the actual Canvas drag must generate the owner's Preview");
        self.finish(preview, true);
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let released = self.draw();
        assert!(released.canvas_actions.iter().any(|action| matches!(
            action, EditorAction::Gesture(EditorGestureAction::Commit { gesture: id, .. }) if *id == gesture
        )), "the queued mouse release must generate Commit before presentation admission");
        released
    }
}

impl Drop for CanvasAdmission {
    fn drop(&mut self) {
        if self.application.request_exit().exit_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Some(frame) = self.application.current_frame() else {
                return;
            };
            let resolution = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            if self
                .application
                .submit_and_drive(
                    EditorActionBatch::new(frame.revision(), resolution),
                    Duration::ZERO,
                )
                .exit_ready()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            std::thread::panicking(),
            "the Canvas admission Session must retire"
        );
    }
}

#[test]
fn an_unconfirmed_canvas_release_cancels_the_real_owner_gesture_and_restores_its_baseline() {
    let project = UiProject::new();
    let source = project.0.join("prefabs/ui.prefab.json");
    let original = fs::read(&source).unwrap();
    let mut interaction = CanvasAdmission::new(&project);
    let baseline = interaction.layout();
    let history = interaction.history();
    let released = interaction.released_canvas_commit();
    assert_ne!(
        interaction.layout(),
        baseline,
        "the real owner must accept the drag Preview"
    );
    interaction.finish(released, false);
    assert!(
        interaction.authoring().gesture().is_none(),
        "a deferred release cannot leave an orphaned owner gesture"
    );
    assert_eq!(
        interaction.authoring().last_gesture_cancellation(),
        Some(EditorGestureCancelReason::TargetRetired)
    );
    assert_eq!(interaction.layout(), baseline);
    assert_eq!(interaction.history(), history);
    assert_eq!(fs::read(source).unwrap(), original);
}

#[test]
fn a_confirmed_canvas_release_commits_the_real_owner_gesture_once() {
    let project = UiProject::new();
    let source = project.0.join("prefabs/ui.prefab.json");
    let original = fs::read(&source).unwrap();
    let mut interaction = CanvasAdmission::new(&project);
    let baseline = interaction.layout();
    let history = interaction.history();
    let released = interaction.released_canvas_commit();
    let previewed = interaction.layout();
    interaction.finish(released, true);
    assert!(interaction.authoring().gesture().is_none());
    assert_eq!(interaction.layout(), previewed);
    assert_ne!(interaction.layout(), baseline);
    assert_eq!(interaction.history(), history + 1);
    assert_eq!(
        fs::read(source).unwrap(),
        original,
        "accepted in-memory edits still require explicit Save"
    );
}

#[test]
fn a_failed_canvas_frame_rolls_back_the_drag_without_dispatching_unpresented_actions() {
    let project = UiProject::new();
    let source = project.0.join("prefabs/ui.prefab.json");
    let original = fs::read(&source).unwrap();
    let mut interaction = CanvasAdmission::new(&project);
    let baseline = interaction.layout();
    let history = interaction.history();
    let mut released = interaction.released_canvas_commit();
    let frame = interaction.authoring();
    released.actions = Some(EditorActionBatch::new(
        frame.editor_frame(),
        [EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: frame.document(),
                entity: None,
            },
        )],
    ));
    let batch = interaction
        .frontend
        .reject_canvas_frame(released, Some(&frame))
        .unwrap();
    let actions = batch.batch.clone().into_actions();
    assert!(matches!(
        actions.as_slice(),
        [EditorAction::Gesture(EditorGestureAction::Cancel { .. })]
    ));
    interaction
        .frontend
        .submit_and_drive(&mut interaction.application, batch, Duration::ZERO);
    let frame = interaction.authoring();
    assert!(frame.gesture().is_none());
    assert_eq!(
        frame.selection().top_entity().unwrap().as_str(),
        "image-node"
    );
    assert_eq!(interaction.layout(), baseline);
    assert_eq!(interaction.history(), history);
    assert_eq!(fs::read(source).unwrap(), original);
}
