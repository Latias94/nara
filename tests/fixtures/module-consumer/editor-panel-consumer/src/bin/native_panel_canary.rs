//! Executes the public native runner from a separately compiled source contribution.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use nara_editor::{
    EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext,
    PanelActionDispatch, PanelSubmissionId,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};
use nara_tooling::{
    EditorAction, EditorDocumentTabModel, EditorNavigationAction, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction,
};

const PANEL_KIND: &str = "sample.native-runner-canary";
const DRAFT: &str = "external panel draft survives its admitted navigation";

#[derive(Debug, Default)]
struct Evidence {
    factories: usize,
    painted: usize,
    selection_acknowledged: bool,
    selection_dispatched: bool,
    exit_requested: bool,
    dropped: usize,
}

struct NativePanelCanary {
    evidence: Rc<RefCell<Evidence>>,
    draft: String,
    selected: Option<String>,
    document_before: Option<EditorDocumentTabModel>,
    selection_submission: Option<PanelSubmissionId>,
}

impl ImguiEditorPanel for NativePanelCanary {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        self.evidence.borrow_mut().painted += 1;
        let model = context.model();
        assert_eq!(model.instance().kind(), PANEL_KIND);
        let document = model
            .document()
            .expect("ordinary Open must publish the panel document");
        let scene = model
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .expect("ordinary Open must publish the source Scene projection");
        assert_eq!(scene.document, document.document);
        ui.text("External native runner canary");

        if let Some(before) = &self.document_before {
            assert!(self.evidence.borrow().selection_dispatched);
            assert_eq!(
                document, before,
                "panel navigation must preserve source revision, dirty state and history"
            );
            assert_eq!(
                self.draft, DRAFT,
                "the native runner must retain the same panel draft between frames"
            );
            assert_eq!(
                model
                    .selection()
                    .and_then(|selection| selection.top_entity())
                    .map(|entity| entity.as_str()),
                self.selected.as_deref(),
                "a later native frame must acknowledge the exact emitted selection",
            );
            if !self.evidence.borrow().exit_requested {
                self.evidence.borrow_mut().selection_acknowledged = true;
                self.evidence.borrow_mut().exit_requested = true;
                eprintln!("native panel acknowledged selection; requesting normal workspace exit");
                context
                    .submit(EditorAction::WorkspaceIntent(
                        EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
                    ))
                    .expect("the ordinary panel submission budget must admit Exit");
            }
        } else {
            assert!(!document.dirty, "the source fixture must start clean");
            assert!(
                model
                    .selection()
                    .is_some_and(|selection| selection.is_empty())
            );
            let entity = scene
                .editor
                .inspector
                .entities
                .first()
                .expect("the source fixture must expose an entity")
                .id
                .clone();
            self.draft = DRAFT.to_owned();
            self.selected = Some(entity.as_str().to_owned());
            self.document_before = Some(document.clone());
            let document = document.document;
            eprintln!("native panel emitted a real selection from the public Scene projection");
            self.selection_submission = Some(
                context
                    .submit(EditorAction::Navigate(
                        EditorNavigationAction::SelectEntity {
                            document,
                            entity: Some(entity),
                        },
                    ))
                    .expect("the ordinary panel submission budget must admit selection"),
            );
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        if self.selection_submission == Some(id) {
            assert!(matches!(
                result,
                PanelActionDispatch::Dispatched(nara_tooling::EditorActionOutcome::Workspace(report))
                    if report.applied
            ));
            assert!(!self.evidence.borrow().selection_dispatched);
            self.evidence.borrow_mut().selection_dispatched = true;
            self.selection_submission = None;
        }
    }
}

impl Drop for NativePanelCanary {
    fn drop(&mut self) {
        self.evidence.borrow_mut().dropped += 1;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("the native panel canary requires a project root argument")?,
    );
    let evidence = Rc::new(RefCell::new(Evidence::default()));
    let shared = Rc::clone(&evidence);
    let mut composition = EditorNativeComposition::new();
    composition.register_panel(
        EditorPanelDescriptor::new(
            PANEL_KIND,
            "External native panel",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            true,
        ),
        move || {
            eprintln!("public native runner created the external panel object");
            shared.borrow_mut().factories += 1;
            NativePanelCanary {
                evidence: Rc::clone(&shared),
                draft: String::new(),
                selected: None,
                document_before: None,
                selection_submission: None,
            }
        },
    )?;
    eprintln!("starting the public native runner; its ordinary window may be briefly visible");
    // A binary main owns the process's main thread, as required by the public Winit runner.
    // No application, frontend, native Host or synthetic OS input is constructed by the fixture.
    composition.run(EditorLaunchOptions::new(project).with_profile("desktop"))?;
    let evidence = evidence.borrow();
    assert_eq!(
        evidence.factories, 1,
        "native startup must create one source-built panel object"
    );
    assert!(
        evidence.painted >= 2,
        "selection must be acknowledged by a subsequent native frame"
    );
    assert!(
        evidence.selection_dispatched && evidence.selection_acknowledged && evidence.exit_requested
    );
    assert_eq!(
        evidence.dropped, 1,
        "normal runner return must retire the panel object"
    );
    println!("NARA_EXTERNAL_NATIVE_PANEL_OK {evidence:?}");
    Ok(())
}
