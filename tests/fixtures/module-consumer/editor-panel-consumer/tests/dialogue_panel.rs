use std::{
    cell::Cell,
    ops::{Deref, DerefMut},
    rc::Rc,
    time::{Duration, Instant},
};

use nara_editor::{
    EditorApplication, EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel,
    ImguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelSourceKind, EditorPanelTarget,
    },
};
use nara_editor_panel_module_consumer::dialogue::{Dialogue, DialoguePanel, PANEL_KIND, SetLine};
use nara_tooling::{EditorAction, EditorActionBatch, EditorCloseDecision, EditorExtensionCommand};

#[path = "support/dialogue_project.rs"]
mod dialogue_project;

struct Panel {
    dialogue: DialoguePanel,
    drops: Rc<Cell<usize>>,
    paints: Rc<Cell<usize>>,
}
impl ImguiEditorPanel for Panel {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        assert!(
            context
                .model()
                .extension_document()
                .unwrap()
                .get::<Dialogue>()
                .is_some()
        );
        self.paints.set(self.paints.get() + 1);
        self.dialogue.show(ui, context);
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

struct Application(EditorApplication);
impl Deref for Application {
    type Target = EditorApplication;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Application {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Application {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ready = self.0.request_exit().exit_ready();
        while !ready {
            assert!(Instant::now() < deadline, "panel consumer did not retire");
            let Some(frame) = self.0.current_frame() else {
                break;
            };
            let actions = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            ready = self
                .0
                .submit_and_drive(
                    EditorActionBatch::new(frame.revision(), actions),
                    Duration::ZERO,
                )
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn dialogue_panel_retirement_drops_only_its_draft_and_keeps_document_history() {
    let project = dialogue_project::DialogueProject::new();
    let mut application = Application(EditorApplication::new(
        EditorLaunchOptions::new(&project.root).with_profile("desktop"),
    ));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() {
        application.advance_project_open();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    application
        .register_extension_document::<Dialogue>()
        .unwrap();
    let document = application
        .open_extension_document::<Dialogue>("dialogue/intro.json")
        .unwrap();
    let frame = application.current_frame().unwrap();
    let revision = frame.workspace().document(document).unwrap().revision;
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Extension(EditorExtensionCommand::new::<
                Dialogue,
            >(
                document,
                revision,
                SetLine {
                    index: 0,
                    text: "Persist beyond panel".to_owned(),
                },
            ))],
        ),
        Duration::ZERO,
    );
    let before = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert!(before.dirty);
    assert_eq!(before.history.undo_depth, 1);

    let drops = Rc::new(Cell::new(0));
    let paints = Rc::new(Cell::new(0));
    let mut composition = EditorNativeComposition::new();
    let dropped = Rc::clone(&drops);
    let painted = Rc::clone(&paints);
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                PANEL_KIND,
                "Dialogue",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Bottom,
                true,
            ),
            move || Panel {
                dialogue: DialoguePanel::default(),
                drops: Rc::clone(&dropped),
                paints: Rc::clone(&painted),
            },
        )
        .unwrap();
    let mut frontend = composition.install_panels(&mut application).unwrap();
    let instance = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == PANEL_KIND)
        .unwrap()
        .id();
    application.panel_workspace_mut().layout_mut().set_target(
        instance,
        EditorPanelTarget::Source {
            kind: EditorPanelSourceKind::Extension,
            locator: "dialogue/intro.json".to_owned(),
        },
    );
    let mut context = dear_imgui_rs::Context::create();
    context
        .set_ini_filename::<std::path::PathBuf>(None)
        .unwrap();
    context.io_mut().set_display_size([1600.0, 1000.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    context
        .io_mut()
        .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();

    for visible in [true, false, true] {
        application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(instance, visible);
        let frame = context.begin_frame();
        let submission = frontend
            .show(frame.ui(), &mut application)
            .unwrap()
            .unwrap();
        drop(frame);
        frontend.submit_and_drive(&mut application, submission, Duration::ZERO);
        assert_eq!(
            application
                .current_frame()
                .unwrap()
                .workspace()
                .document(document),
            Some(&before)
        );
        if !visible {
            assert_eq!(drops.get(), 1);
        }
    }
    assert_eq!(paints.get(), 2);
    assert_eq!(drops.get(), 1);
    application
        .panel_workspace_mut()
        .layout_mut()
        .close(instance);
    let frame = context.begin_frame();
    let submission = frontend
        .show(frame.ui(), &mut application)
        .unwrap()
        .unwrap();
    drop(frame);
    frontend.submit_and_drive(&mut application, submission, Duration::ZERO);
    assert_eq!(drops.get(), 2);
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document),
        Some(&before)
    );
    let reopened = application.open_extension_document::<Dialogue>("dialogue/intro.json");
    assert_eq!(
        reopened,
        Err(nara::project_host::EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
}
