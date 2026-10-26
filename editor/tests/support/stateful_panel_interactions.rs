use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{Context, MouseButton, Ui};
use nara::tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction,
};
use nara_editor::{
    EditorApplication, EditorLaunchOptions, EditorNativeComposition, ImguiEditorFrontend,
    ImguiEditorPanel, ImguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelSourceKind, EditorPanelTarget,
    },
};

#[derive(Default)]
struct Observations {
    created: u64,
    drafts: BTreeMap<EditorPanelInstanceId, (u64, String, [f32; 2])>,
    dropped: Vec<u64>,
}

struct DraftPanel {
    serial: u64,
    draft: String,
    observations: Rc<RefCell<Observations>>,
}

impl Drop for DraftPanel {
    fn drop(&mut self) {
        self.observations.borrow_mut().dropped.push(self.serial);
    }
}

impl ImguiEditorPanel for DraftPanel {
    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        if ui.button("Append to draft") {
            self.draft.push('x');
        }
        let (min, max) = ui.item_rect();
        self.observations.borrow_mut().drafts.insert(
            context.model().instance().id(),
            (
                self.serial,
                self.draft.clone(),
                [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            ),
        );
        ui.text(&self.draft);
    }
}

fn draw(
    context: &mut Context,
    frontend: &mut ImguiEditorFrontend,
    application: &mut EditorApplication,
) {
    if let Some(batch) = super::imgui::draw(context, frontend, application) {
        frontend.submit_and_drive(application, batch, Duration::ZERO);
    }
}

fn click(
    context: &mut Context,
    frontend: &mut ImguiEditorFrontend,
    application: &mut EditorApplication,
    point: [f32; 2],
) {
    context.io_mut().add_mouse_pos_event(point);
    draw(context, frontend, application);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(context, frontend, application);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    draw(context, frontend, application);
}

struct KindCheckedPanel {
    kind: &'static str,
    serial: u64,
    observed: Rc<RefCell<KindObservations>>,
}

#[derive(Default)]
struct KindObservations {
    created: u64,
    painted: BTreeMap<String, u64>,
    dropped: Vec<u64>,
}

impl ImguiEditorPanel for KindCheckedPanel {
    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        assert_eq!(context.model().instance().kind(), self.kind);
        self.observed
            .borrow_mut()
            .painted
            .insert(self.kind.to_owned(), self.serial);
        ui.text(self.kind);
    }
}

impl Drop for KindCheckedPanel {
    fn drop(&mut self) {
        self.observed.borrow_mut().dropped.push(self.serial);
    }
}

#[test]
fn public_layout_reset_and_restore_retire_objects_before_reusing_instance_ids() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(fixture).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() && Instant::now() < deadline {
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let observed = Rc::new(RefCell::new(KindObservations::default()));
    let mut composition = EditorNativeComposition::new();
    for kind in ["a.custom", "z.custom"] {
        let shared = Rc::clone(&observed);
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    kind,
                    kind,
                    EditorPanelMultiplicity::Multiple,
                    EditorPanelPlacement::Floating,
                    true,
                ),
                move || {
                    let serial = {
                        let mut observed = shared.borrow_mut();
                        observed.created += 1;
                        observed.created
                    };
                    KindCheckedPanel {
                        kind,
                        serial,
                        observed: Rc::clone(&shared),
                    }
                },
            )
            .unwrap();
    }
    let mut frontend = composition.install_panels(&mut application).unwrap();
    let mut context = super::imgui::context();
    for _ in 0..4 {
        draw(&mut context, &mut frontend, &mut application);
    }
    let first = observed.borrow().painted.clone();
    assert_eq!(first.len(), 2);
    let old_first_builtin = application.panel_workspace().layout().instances()[0].clone();

    application.panel_workspace_mut().reset();
    let reset_custom = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "a.custom")
        .unwrap();
    assert_eq!(
        reset_custom.id(),
        old_first_builtin.id(),
        "reset reassigns the former built-in ID to an earlier-sorting custom kind"
    );
    observed.borrow_mut().painted.clear();
    for _ in 0..4 {
        draw(&mut context, &mut frontend, &mut application);
    }
    let reset = observed.borrow().painted.clone();
    assert_eq!(
        reset.len(),
        2,
        "both reset instances must invoke their own factories"
    );
    assert_eq!(observed.borrow().created, 4);
    for kind in ["a.custom", "z.custom"] {
        assert_ne!(reset[kind], first[kind]);
        assert!(observed.borrow().dropped.contains(&first[kind]));
    }

    // An identical restore keeps every numeric ID/kind/target/document. It still retires drafts.
    let saved = application.panel_workspace().layout().clone();
    application
        .panel_workspace_mut()
        .restore(saved.clone(), "dear-imgui", 1, &[]);
    assert_eq!(application.panel_workspace().layout(), &saved);
    observed.borrow_mut().painted.clear();
    for _ in 0..4 {
        draw(&mut context, &mut frontend, &mut application);
    }
    let restored = observed.borrow().painted.clone();
    assert_eq!(restored.len(), 2);
    assert_eq!(observed.borrow().created, 6);
    for kind in ["a.custom", "z.custom"] {
        assert_ne!(restored[kind], reset[kind]);
        assert!(observed.borrow().dropped.contains(&reset[kind]));
    }

    // Direct use of the public layout reset follows the same lifetime contract.
    let registry = application.panel_workspace().registry().clone();
    application
        .panel_workspace_mut()
        .layout_mut()
        .reset(&registry);
    observed.borrow_mut().painted.clear();
    for _ in 0..4 {
        draw(&mut context, &mut frontend, &mut application);
    }
    assert_eq!(observed.borrow().created, 8);
    assert_eq!(observed.borrow().painted.len(), 2);
    for serial in restored.values() {
        assert!(observed.borrow().dropped.contains(serial));
    }
    drop(frontend);
    assert_eq!(observed.borrow().dropped.len(), 8);
    let mut closed = application.request_exit().exit_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !closed {
        let update = if let Some(frame) = application.current_frame() {
            application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                .update()
        } else {
            application.advance_project_open()
        };
        closed = update.exit_ready();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(closed);
}

#[test]
fn contributed_drafts_follow_instance_target_document_and_project_lifetimes() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&fixture).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() && Instant::now() < deadline {
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let before = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    let observations = Rc::new(RefCell::new(Observations::default()));
    let shared = Rc::clone(&observations);
    let mut composition = EditorNativeComposition::new();
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                "sample.draft",
                "Draft",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Left,
                true,
            ),
            move || {
                let serial = {
                    let mut observed = shared.borrow_mut();
                    observed.created += 1;
                    observed.created
                };
                DraftPanel {
                    serial,
                    draft: String::new(),
                    observations: Rc::clone(&shared),
                }
            },
        )
        .unwrap();
    let mut frontend = composition.install_panels(&mut application).unwrap();
    let first = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "sample.draft")
        .unwrap()
        .id();
    let builtin_ids = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .filter(|panel| panel.kind() != "sample.draft")
        .map(|panel| panel.id())
        .collect::<Vec<_>>();
    for id in builtin_ids {
        application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(id, false);
    }
    let second = application
        .panel_workspace_mut()
        .open("sample.draft", EditorPanelTarget::ActiveDocument)
        .unwrap();
    application
        .panel_workspace_mut()
        .layout_mut()
        .place(second, EditorPanelPlacement::Right);
    let mut context = super::imgui::context();
    for _ in 0..4 {
        draw(&mut context, &mut frontend, &mut application);
    }
    let first_before = observations.borrow().drafts[&first].clone();
    let second_before = observations.borrow().drafts[&second].clone();
    click(
        &mut context,
        &mut frontend,
        &mut application,
        first_before.2,
    );
    click(
        &mut context,
        &mut frontend,
        &mut application,
        second_before.2,
    );
    click(
        &mut context,
        &mut frontend,
        &mut application,
        second_before.2,
    );
    assert_eq!(observations.borrow().drafts[&first].1, "x");
    assert_eq!(observations.borrow().drafts[&second].1, "xx");
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document),
        Some(&before),
        "frontend drafts must not dirty or revise the source document"
    );

    application.panel_workspace_mut().layout_mut().close(first);
    draw(&mut context, &mut frontend, &mut application);
    assert_eq!(observations.borrow().dropped, vec![first_before.0]);
    assert_eq!(observations.borrow().drafts[&second].0, second_before.0);
    application.panel_workspace_mut().layout_mut().reopen(first);
    draw(&mut context, &mut frontend, &mut application);
    let reopened = observations.borrow().drafts[&first].0;
    assert_ne!(reopened, first_before.0);
    assert!(observations.borrow().drafts[&first].1.is_empty());

    application.panel_workspace_mut().layout_mut().set_target(
        first,
        EditorPanelTarget::Source {
            kind: EditorPanelSourceKind::Scene,
            locator: "missing.scene.json".to_owned(),
        },
    );
    draw(&mut context, &mut frontend, &mut application);
    assert!(
        observations.borrow().dropped.contains(&reopened),
        "an unresolved retarget must retire the old state even though its painter is gated"
    );
    let created = observations.borrow().created;
    draw(&mut context, &mut frontend, &mut application);
    assert_eq!(observations.borrow().created, created);
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(first, EditorPanelTarget::ActiveDocument);
    draw(&mut context, &mut frontend, &mut application);
    let retargeted = observations.borrow().drafts[&first].0;
    assert_ne!(retargeted, reopened);

    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                    document,
                }),
            )],
        ),
        Duration::ZERO,
    );
    draw(&mut context, &mut frontend, &mut application);
    assert!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_document
            .is_none()
    );
    assert!(observations.borrow().dropped.contains(&retargeted));
    assert!(observations.borrow().dropped.contains(&second_before.0));
    let closed_first = observations.borrow().drafts[&first].0;
    let closed_second = observations.borrow().drafts[&second].0;

    assert!(application.request_project_replacement(&fixture, Some("desktop".to_owned())));
    let deadline = Instant::now() + Duration::from_secs(10);
    while application
        .current_frame()
        .is_none_or(|frame| frame.workspace().active_document.is_none())
    {
        application.advance_project_open();
        if let Some(frame) = application.current_frame() {
            application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        }
        assert!(
            Instant::now() < deadline,
            "the replacement must publish its fresh workspace"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    draw(&mut context, &mut frontend, &mut application);
    assert!(observations.borrow().dropped.contains(&closed_first));
    assert!(observations.borrow().dropped.contains(&closed_second));
    assert_ne!(observations.borrow().drafts[&first].0, closed_first);
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_document,
        Some(document),
        "even reused document IDs must get fresh project drafts"
    );

    let mut closed = application.request_exit().exit_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !closed {
        let update = if let Some(frame) = application.current_frame() {
            let actions = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            application
                .submit_and_drive(
                    EditorActionBatch::new(frame.revision(), actions),
                    Duration::ZERO,
                )
                .update()
        } else {
            application.advance_project_open()
        };
        closed = update.exit_ready();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
