#![cfg(any(target_os = "windows", target_os = "linux"))]

use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

use nara::{
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{AuthoringFieldEdit, SceneEntityId},
};
use nara_editor::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, EditorNativeComposition,
    ImguiEditorPanel, ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelTarget,
    },
};
use nara_editor_panel_module_consumer::{PANEL_KIND, clear_selection, selected_count};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
    EditorNavigationAction,
};

#[derive(Default)]
struct DispatchEvidence {
    queued: Vec<PanelSubmissionId>,
    finished: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct ClearSelected(Rc<RefCell<DispatchEvidence>>);

impl ImguiEditorPanel for ClearSelected {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        assert_eq!(context.model().instance().kind(), PANEL_KIND);
        assert_eq!(selected_count(context.model()), 1);
        let entity = context
            .model()
            .selection()
            .unwrap()
            .top_entity()
            .unwrap()
            .clone();
        context.edit_field(
            ui,
            &entity,
            &ComponentTypeId::new("nara.scene.Name"),
            &ComponentFieldId::new("value"),
        );
        for _ in 0..2 {
            let id = context
                .submit(clear_selection(context.model()).unwrap())
                .unwrap();
            self.0.borrow_mut().queued.push(id);
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.0.borrow_mut().finished.push((id, result));
    }
}

#[test]
fn a_separately_compiled_imgui_companion_reopens_without_owning_document_selection_or_history() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if application.current_frame().is_some() {
            break;
        }
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let mut composition = EditorNativeComposition::new();
    let dispatch_evidence = Rc::new(RefCell::new(DispatchEvidence::default()));
    let shared_evidence = Rc::clone(&dispatch_evidence);
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                PANEL_KIND,
                "Selection summary",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Bottom,
                true,
            ),
            move || ClearSelected(Rc::clone(&shared_evidence)),
        )
        .unwrap();
    let mut frontend = composition.install_panels(&mut application).unwrap();
    let instance_id = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == PANEL_KIND)
        .unwrap()
        .id();
    let frame = application.current_frame().unwrap();
    let document = frame.workspace().active_document.unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("signal-node").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    let selected = application.current_frame().unwrap();
    let scene = selected.workspace().active_scene.as_ref().unwrap();
    let selection = scene.selection.clone();
    application.submit_and_drive(
        EditorActionBatch::new(
            selected.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyAuthoringFieldEdit {
                    document,
                    base_revision: scene.revision,
                    edit: AuthoringFieldEdit::single(
                        SceneEntityId::new("signal-node").unwrap(),
                        ComponentTypeId::new("nara.scene.Name"),
                        ComponentSchemaVersion::ONE,
                        ComponentFieldId::new("value"),
                        ComponentValue::String("ImGui panel lifetime proof".to_owned()),
                    )
                    .unwrap(),
                },
            )],
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
    let target = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap()
        .target()
        .clone();
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
    let panel_closed = application
        .panel_workspace_mut()
        .layout_mut()
        .close(instance_id);
    let frame = context.begin_frame();
    let batch = frontend
        .show(frame.ui(), &mut application)
        .unwrap()
        .unwrap();
    drop(frame);
    frontend.submit_and_drive(&mut application, batch, Duration::ZERO);
    let closed_frame = application.current_frame().unwrap();
    let closed_panel = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap();
    let panel_was_hidden = !closed_panel.is_open() && !closed_panel.is_visible();
    let closed_document_unchanged = closed_frame.workspace().document(document) == Some(&before);
    let closed_selection_unchanged = closed_frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        == selection;
    let panel_reopened = application
        .panel_workspace_mut()
        .layout_mut()
        .reopen(instance_id);
    let reopened_panel = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap();
    let panel_was_reopened = reopened_panel.is_open()
        && reopened_panel.is_visible()
        && reopened_panel.target() == &target;
    let frame = context.begin_frame();
    let batch = frontend
        .show(frame.ui(), &mut application)
        .unwrap()
        .unwrap();
    drop(frame);
    let before_dispatch = application.current_frame().unwrap();
    let painting_preserved_selection = before_dispatch
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        == selection;
    frontend.submit_and_drive(&mut application, batch, Duration::ZERO);
    {
        let evidence = dispatch_evidence.borrow();
        assert_eq!(evidence.queued.len(), 2);
        assert_eq!(evidence.finished.len(), 2);
        assert_ne!(evidence.queued[0], evidence.queued[1]);
        for (queued, (finished, result)) in evidence.queued.iter().zip(&evidence.finished) {
            assert_eq!(queued, finished);
            assert!(matches!(
                result,
                PanelActionDispatch::Dispatched(nara_tooling::EditorActionOutcome::Workspace(report))
                    if report.applied
            ));
        }
    }
    let after = application.current_frame().unwrap();
    let selection_cleared = after
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .is_empty();
    let duplicate = application
        .panel_workspace_mut()
        .open(PANEL_KIND, EditorPanelTarget::ActiveDocument)
        .unwrap();
    let document_unchanged = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        == Some(&before);
    let duplicate_target_unchanged = application
        .panel_workspace()
        .layout()
        .instance(duplicate)
        .unwrap()
        .target()
        == &target;
    let mut closed = application.request_exit().exit_ready();
    if !closed && let Some(frame) = application.current_frame() {
        closed = application
            .submit_and_drive(
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
            .exit_ready();
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if closed {
            break;
        }
        let Some(frame) = application.current_frame() else {
            break;
        };
        closed = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
            .exit_ready();
        if !closed {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(closed);
    assert!(before.dirty);
    assert_eq!(before.history.undo_depth, 1);
    assert!(panel_closed && panel_was_hidden);
    assert!(closed_document_unchanged && closed_selection_unchanged);
    assert!(panel_reopened && panel_was_reopened);
    assert!(painting_preserved_selection);
    assert!(selection_cleared);
    assert!(document_unchanged);
    assert_ne!(instance_id, duplicate);
    assert!(duplicate_target_unchanged);
}

struct FieldWriter {
    evidence: Rc<RefCell<DispatchEvidence>>,
    submitted: bool,
}

impl ImguiEditorPanel for FieldWriter {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let model = context.model();
        let document = model.document().unwrap().clone();
        let entity = model.selection().unwrap().top_entity().unwrap().clone();
        context.edit_field(
            ui,
            &entity,
            &ComponentTypeId::new("nara.scene.Name"),
            &ComponentFieldId::new("value"),
        );
        if self.submitted {
            return;
        }
        self.submitted = true;
        // An advanced tool deliberately submits two writes from one basis. Correlation does
        // not turn their ordered batch into an atomic patch or silently rebase the second.
        for value in ["External accepted field", "External conflicting field"] {
            let id = context
                .submit(EditorAction::Mutate(
                    EditorDocumentMutation::ApplyAuthoringFieldEdit {
                        document: document.document,
                        base_revision: document.revision,
                        edit: AuthoringFieldEdit::single(
                            entity.clone(),
                            ComponentTypeId::new("nara.scene.Name"),
                            ComponentSchemaVersion::ONE,
                            ComponentFieldId::new("value"),
                            ComponentValue::String(value.to_owned()),
                        )
                        .unwrap(),
                    },
                ))
                .unwrap();
            self.evidence.borrow_mut().queued.push(id);
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.evidence.borrow_mut().finished.push((id, result));
    }
}

#[test]
fn an_external_field_tool_observes_acceptance_and_same_basis_conflict_separately() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if application.current_frame().is_some() {
            break;
        }
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let evidence = Rc::new(RefCell::new(DispatchEvidence::default()));
    let shared = Rc::clone(&evidence);
    let mut composition = EditorNativeComposition::new();
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                "sample.field-writer",
                "Field writer",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Floating,
                true,
            ),
            move || FieldWriter {
                evidence: Rc::clone(&shared),
                submitted: false,
            },
        )
        .unwrap();
    let mut frontend = composition.install_panels(&mut application).unwrap();
    let frame = application.current_frame().unwrap();
    let document = frame.workspace().active_document.unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("signal-node").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    let before = application.current_frame().unwrap();
    let before_tab = before.workspace().document(document).unwrap();
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
    for _ in 0..4 {
        let frame = context.begin_frame();
        let candidate = frontend.show(frame.ui(), &mut application).unwrap();
        drop(frame);
        if let Some(candidate) = candidate {
            frontend.submit_and_drive(&mut application, candidate, Duration::ZERO);
        }
    }
    let evidence = evidence.borrow();
    assert_eq!(evidence.queued.len(), 2);
    assert_eq!(
        evidence
            .finished
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        evidence.queued
    );
    assert!(matches!(&evidence.finished[0].1,
        PanelActionDispatch::Dispatched(nara_tooling::EditorActionOutcome::Workspace(report)) if report.applied));
    let after = application.current_frame().unwrap();
    let after_tab = after.workspace().document(document).unwrap();
    assert_eq!(
        evidence.finished[1].1,
        PanelActionDispatch::Dispatched(nara_tooling::EditorActionOutcome::Rejected(
            nara_tooling::EditorActionRejection::StaleDocumentFact {
                document,
                expected: before_tab.revision,
                actual: Some(after_tab.revision),
            }
        ),)
    );
    assert!(after_tab.dirty);
    assert_eq!(
        after_tab.history.undo_depth,
        before_tab.history.undo_depth + 1
    );
    let entity = after
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let name = entity
        .components
        .iter()
        .find(|component| component.component.as_str() == "nara.scene.Name")
        .unwrap();
    assert_eq!(
        name.fields
            .iter()
            .find(|field| field.id.as_str() == "value")
            .unwrap()
            .value,
        Some(ComponentValue::String("External accepted field".to_owned()))
    );
}
