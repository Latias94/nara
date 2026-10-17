use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton};
use nara::{
    project_host::{EditorUiAuthoringFrame, TrustedCompiledEditorProjectIntent},
    scene::{SceneEntityId, ScenePlacementAnchor},
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorFrameModel,
        EditorPersistenceOperation, EditorPersistenceResult, EditorWorkspaceIntentAction,
        EditorWorkspaceIntentPhase,
    },
};
use serde_json::{Value, json};

use super::{
    ImguiEditorFrontend,
    ui_interaction_tests::{UiProject, clear_controls, control_rect},
};
use crate::{EditorApplication, EditorLaunchOptions};

struct StructureInteraction {
    application: EditorApplication,
    frontend: ImguiEditorFrontend,
    context: Context,
}

impl StructureInteraction {
    fn new(project: &UiProject) -> Self {
        let mut application = EditorApplication::open_trusted_compiled(
            EditorLaunchOptions::new(&project.0).with_profile("desktop"),
            TrustedCompiledEditorProjectIntent::new(),
        )
        .expect("the built-in UI fixture opens through its trusted lane");
        application.native_shell_ready();
        let hidden = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|instance| !matches!(instance.kind(), "nara.hierarchy" | "nara.inspector"))
            .map(|instance| instance.id())
            .collect::<Vec<_>>();
        for instance in hidden {
            application
                .panel_workspace_mut()
                .layout_mut()
                .close(instance);
        }
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE | ConfigFlags::NAV_ENABLE_KEYBOARD);
        context.io_mut().set_display_size([1800.0, 6000.0]);
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
        for _ in 0..4 {
            interaction.render();
        }
        interaction.click("entity:ui-anchor");
        interaction.click("Open Prefab Source");
        interaction
    }

    fn frame(&self) -> Arc<EditorFrameModel> {
        self.application.current_frame().unwrap()
    }

    fn authoring(&self) -> Arc<EditorUiAuthoringFrame> {
        self.application.current_ui_authoring_frame().unwrap()
    }

    fn render(&mut self) {
        clear_controls();
        let ui = self.context.frame();
        let batch = self.frontend.show(ui, &mut self.application).unwrap();
        let _ = self.context.render_legacy();
        if let Some(batch) = batch {
            self.frontend
                .submit_and_drive(&mut self.application, batch, Duration::ZERO);
        }
    }

    fn click(&mut self, label: &str) {
        let rect = control_rect(label)
            .unwrap_or_else(|| panic!("the real structure widget {label:?} was not painted"));
        self.context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        self.render();
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        self.render();
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        self.render();
        for _ in 0..3 {
            self.render();
        }
    }

    fn press(&mut self, key: Key) {
        self.context.io_mut().add_key_event(key, true);
        self.render();
        self.context.io_mut().add_key_event(key, false);
        self.render();
    }

    fn move_group_after(&mut self, parent: &str, sibling: &str) {
        self.click("ui-entity:moving-group");
        let authoring = self.authoring();
        let candidates = authoring
            .projection()
            .unwrap()
            .structure_placement_candidates(Some(&SceneEntityId::new("moving-group").unwrap()))
            .unwrap();
        let current = candidates
            .iter()
            .position(|candidate| candidate.is_current())
            .unwrap();
        let destination = candidates
            .iter()
            .position(|candidate| {
                candidate.parent().as_str() == parent
                    && matches!(
                        &candidate.placement().anchor,
                        ScenePlacementAnchor::After { sibling: anchor }
                            if anchor.as_str() == sibling
                    )
            })
            .expect("the exact requested parent/sibling anchor is offered");
        assert_ne!(current, destination);
        let revision = authoring.revision().generation();
        let history = self.undo_depth();
        self.click("Move subtree");
        self.click("Placement");
        let key = if destination > current {
            Key::DownArrow
        } else {
            Key::UpArrow
        };
        for _ in 0..destination.abs_diff(current) {
            self.press(key);
        }
        self.press(Key::Enter);
        for _ in 0..3 {
            self.render();
        }
        self.click("Apply structure change");
        assert_eq!(self.authoring().revision().generation(), revision + 1);
        assert_eq!(self.undo_depth(), history + 1);
        assert_eq!(
            self.authoring().selection().top_entity().unwrap().as_str(),
            "moving-group"
        );
    }

    fn undo_depth(&self) -> usize {
        self.frame()
            .workspace()
            .document(self.authoring().document())
            .unwrap()
            .history
            .undo_depth
    }

    fn assert_tree(&self, expected: &[(&str, Option<&str>, u16)]) {
        let authoring = self.authoring();
        let actual = authoring
            .projection()
            .unwrap()
            .hierarchy()
            .iter()
            .map(|node| {
                (
                    node.entity().as_str(),
                    node.parent().map(SceneEntityId::as_str),
                    node.depth(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    fn settle_persistence(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.frame().persistence().operation() != EditorPersistenceOperation::Idle
            && Instant::now() < deadline
        {
            self.render();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            self.frame().persistence().operation(),
            EditorPersistenceOperation::Idle
        );
        self.render();
    }
}

impl Drop for StructureInteraction {
    fn drop(&mut self) {
        if self.application.request_exit().exit_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Some(frame) = self.application.current_frame() else {
                return;
            };
            let decision = (frame.workspace_intent().phase()
                == Some(EditorWorkspaceIntentPhase::AwaitingDecision))
            .then_some(EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Discard),
            ));
            if self
                .application
                .submit_and_drive(
                    EditorActionBatch::new(frame.revision(), decision),
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
            "the structure fixture did not retire its Session"
        );
    }
}

fn install_tree(project: &UiProject) -> Value {
    let source = project.0.join("prefabs/ui.prefab.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    let mut root = document["payload"]["entities"][0].clone();
    let leaf = document["payload"]["entities"][1].clone();
    let container = json!({
        "id": "template",
        "children": [],
        "components": { "nara.ui.UiNode": root["components"]["nara.ui.UiNode"].clone() },
    });
    root["children"] = json!(["source-parent", "destination-parent", "unrelated-node"]);
    let mut entities = vec![root];
    for (id, children, template) in [
        (
            "source-parent",
            vec!["moving-group", "source-tail"],
            &container,
        ),
        ("moving-group", vec!["moving-leaf"], &container),
        ("moving-leaf", vec![], &leaf),
        ("source-tail", vec![], &leaf),
        (
            "destination-parent",
            vec!["dest-first", "dest-second"],
            &container,
        ),
        ("dest-first", vec![], &leaf),
        ("dest-second", vec![], &leaf),
        ("unrelated-node", vec![], &leaf),
    ] {
        let mut record = template.clone();
        record["id"] = json!(id);
        record["children"] = json!(children);
        entities.push(record);
    }
    document["payload"]["entities"] = json!(entities);
    fs::write(source, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    document
}

fn records(document: &Value) -> BTreeMap<&str, &Value> {
    document["payload"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| (record["id"].as_str().unwrap(), record))
        .collect()
}

#[test]
fn specialized_ui_move_and_placement_widgets_reparent_reorder_undo_and_save_reopen_subtree() {
    let project = UiProject::new();
    let mut expected_source = install_tree(&project);
    let source = project.0.join("prefabs/ui.prefab.json");
    let original_bytes = fs::read(&source).unwrap();
    let initial = [
        ("ui-root", None, 0),
        ("source-parent", Some("ui-root"), 1),
        ("moving-group", Some("source-parent"), 2),
        ("moving-leaf", Some("moving-group"), 3),
        ("source-tail", Some("source-parent"), 2),
        ("destination-parent", Some("ui-root"), 1),
        ("dest-first", Some("destination-parent"), 2),
        ("dest-second", Some("destination-parent"), 2),
        ("unrelated-node", Some("ui-root"), 1),
    ];
    let reparented = [
        initial[0],
        initial[1],
        initial[4],
        initial[5],
        initial[6],
        ("moving-group", Some("destination-parent"), 2),
        initial[3],
        initial[7],
        initial[8],
    ];
    let reordered = [
        initial[0],
        initial[1],
        initial[4],
        initial[5],
        initial[6],
        initial[7],
        ("moving-group", Some("destination-parent"), 2),
        initial[3],
        initial[8],
    ];
    let mut interaction = StructureInteraction::new(&project);
    assert!(interaction.authoring().structure_editing_enabled());
    interaction.assert_tree(&initial);
    assert_eq!(interaction.undo_depth(), 0);
    interaction.move_group_after("destination-parent", "dest-first");
    interaction.assert_tree(&reparented);
    interaction.move_group_after("destination-parent", "dest-second");
    interaction.assert_tree(&reordered);
    interaction.click("Undo");
    interaction.assert_tree(&reparented);
    assert_eq!(interaction.undo_depth(), 1);
    interaction.click("Undo");
    interaction.assert_tree(&initial);
    assert_eq!(interaction.undo_depth(), 0);
    interaction.click("Redo");
    interaction.assert_tree(&reparented);
    assert_eq!(interaction.undo_depth(), 1);
    interaction.click("Redo");
    interaction.assert_tree(&reordered);
    assert_eq!(interaction.undo_depth(), 2);
    assert_eq!(fs::read(&source).unwrap(), original_bytes);

    let document = interaction.authoring().document();
    interaction.click("Save");
    interaction.settle_persistence();
    assert!(matches!(interaction.frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document));
    assert!(
        !interaction
            .frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    for record in expected_source["payload"]["entities"]
        .as_array_mut()
        .unwrap()
    {
        if record["id"] == "source-parent" {
            record["children"] = json!(["source-tail"]);
        } else if record["id"] == "destination-parent" {
            record["children"] = json!(["dest-first", "dest-second", "moving-group"]);
        }
    }
    let stored: Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    assert_eq!(
        stored["payload"]["roots"],
        expected_source["payload"]["roots"]
    );
    assert_eq!(
        records(&stored),
        records(&expected_source),
        "only the two parent child lists change; the complete subtree and unrelated records survive"
    );

    interaction.click("Dismiss persistence result");
    interaction.click("Reopen");
    interaction.settle_persistence();
    assert!(matches!(
        interaction.frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    interaction.assert_tree(&reordered);
    assert_eq!(interaction.undo_depth(), 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(source).unwrap()).unwrap(),
        stored
    );
}
