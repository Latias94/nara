use dear_imgui_rs::Ui;
use nara::tooling::{
    EditorAction, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation, EditorFrameModel,
    EditorNavigationAction, EditorPersistenceAction, EditorPersistenceRejection,
    EditorPersistenceRequestKind, EditorPersistenceResult, EditorPersistenceView, EditorPlayState,
    EditorReopenTarget, EditorRuntimeAvailability, EditorSceneModel, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction,
};

use crate::frontend::controls::field_drafts::FieldEditors;
use crate::frontend::controls::{editing_enabled, persistence_allows};

pub(super) mod recursive_controls;

#[derive(Default)]
pub(super) struct ImguiAuthoringPanels {
    scope: Option<EditorDocumentId>,
    pub(super) fields: FieldEditors,
    pub(super) source_tool: super::source_tool::SourceToolWidgets,
    read_only: bool,
}

impl ImguiAuthoringPanels {
    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub(super) fn retire(&mut self) {
        self.scope = None;
        self.fields.clear();
        self.source_tool = Default::default();
    }

    fn synchronize(&mut self, scene: Option<&EditorSceneModel>) {
        let scope = scene.map(|scene| scene.document);
        if self.scope.is_some() && scope.is_some() && self.scope != scope {
            self.retire();
        }
        self.scope = scope;
    }

    pub(super) fn show_document_controls(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
    ) {
        self.synchronize(frame.workspace().active_scene.as_ref());
        let document = frame
            .workspace()
            .active_document
            .and_then(|id| frame.workspace().document(id));
        let editing = !self.read_only && editing_enabled(frame);
        let title = document.map_or("No active document", |document| document.title.as_str());
        let combo = ui.begin_combo("Document", title);
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, "Document selector");
        if let Some(_combo) = combo {
            for tab in &frame.workspace().documents {
                let _id = ui.push_id(format!("document:{}", tab.document.raw()).as_str());
                let clicked = ui
                    .selectable_config(format!(
                        "{}{}",
                        tab.title,
                        if tab.dirty { " *" } else { "" }
                    ))
                    .selected(tab.active)
                    .disabled(self.read_only || frame.workspace_intent().intent().is_some())
                    .build();
                #[cfg(test)]
                super::ui_interaction_tests::record_control(
                    ui,
                    format!("document:{}", tab.document.raw()),
                );
                if clicked && (!tab.active || tab.kind == EditorDocumentKind::Scene) {
                    actions.push(EditorAction::Navigate(
                        EditorNavigationAction::SetActiveDocument {
                            document: tab.document,
                        },
                    ));
                }
            }
        }
        if document.is_some_and(|document| document.kind == EditorDocumentKind::Prefab)
            && let Some(document) = frame.scene_preview_document()
        {
            ui.same_line();
            if enabled_button(ui, !self.read_only, "Back to Scene") {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SetActiveDocument { document },
                ));
            }
        }
        if let Some(document) = document {
            ui.text(format!(
                "{}{}",
                document.title,
                if document.dirty { " *" } else { "" }
            ));
            if enabled_button(ui, editing && document.history.undo_depth > 0, "Undo") {
                actions.push(EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            if enabled_button(ui, editing && document.history.redo_depth > 0, "Redo") {
                actions.push(EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            let persistence = frame.persistence();
            let idle = !self.read_only
                && frame.workspace_intent().intent().is_none()
                && persistence.operation().is_none()
                && (document.kind != EditorDocumentKind::Scene
                    || frame.play().state() == EditorPlayState::Empty
                    || matches!(
                        frame.runtime_availability(),
                        EditorRuntimeAvailability::ExternalSupervised
                    ));
            let save_allowed = persistence_allows(
                persistence.result(),
                EditorPersistenceRequestKind::Save,
                document.document,
            );
            if enabled_button(ui, idle && save_allowed && document.dirty, "Save") {
                actions.push(EditorAction::Persistence(EditorPersistenceAction::Save {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            let reopen_allowed = match frame.reopen_target() {
                EditorReopenTarget::OpenDocument { document, .. } => persistence_allows(
                    persistence.result(),
                    EditorPersistenceRequestKind::Reopen,
                    document,
                ),
                EditorReopenTarget::Unavailable => false,
            };
            if enabled_button(ui, idle && reopen_allowed, "Reopen") {
                actions.push(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: frame.reopen_target(),
                }));
            }
            ui.same_line();
            if enabled_button(
                ui,
                !self.read_only && frame.workspace_intent().intent().is_none(),
                "Close Document",
            ) {
                actions.push(EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                        document: document.document,
                    }),
                ));
            }
        } else {
            ui.text_disabled("No active document");
        }
        if let EditorPersistenceView::Terminal { attempt, result } = frame.persistence() {
            match result {
                EditorPersistenceResult::Saved { .. } => ui.text("Saved"),
                EditorPersistenceResult::Opened { .. } => ui.text("Reopened"),
                EditorPersistenceResult::Rejected {
                    reason: EditorPersistenceRejection::RequiresReconcile,
                    ..
                } => ui.text_wrapped(
                    "Saving could not be confirmed. Reopen this document to reconcile its source.",
                ),
                EditorPersistenceResult::Rejected { reason, .. } => {
                    ui.text_wrapped(format!("Persistence rejected: {reason:?}"));
                }
                EditorPersistenceResult::Failed { stage, .. } => {
                    ui.text_wrapped(format!("Persistence failed: {stage:?}"));
                }
                EditorPersistenceResult::PersistenceUncertain { .. } => {
                    ui.text_wrapped("Saving could not be confirmed. Reopen this document to reconcile its source.");
                }
            }
            if enabled_button(
                ui,
                frame.persistence().operation().is_none(),
                "Dismiss persistence result",
            ) {
                actions.push(EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult { attempt },
                ));
            }
        }
    }
}

fn enabled_button(ui: &Ui, enabled: bool, label: &str) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    #[cfg(test)]
    if matches!(
        label,
        "Back to Scene" | "Undo" | "Redo" | "Save" | "Reopen" | "Dismiss persistence result"
    ) {
        super::interaction_tests::record_control(ui, label);
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::controls::compose_action;
    use nara::{
        reflect::{ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentTypeId},
        scene::{SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord},
        tooling::{
            EditorExternalReloadState, EditorPersistenceOperation, PrefabCompositionCommand,
            SceneCompositionCommand, SceneEditorState,
        },
    };

    fn frozen_empty_registry() -> ComponentRegistrySnapshot {
        ComponentRegistryBuilder::new().freeze().unwrap()
    }

    fn scene(kind: EditorDocumentKind) -> EditorSceneModel {
        let session = SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("owner").unwrap()),
        ]));
        EditorSceneModel {
            document: EditorDocumentId::from_raw(7),
            kind,
            title: "Test document".to_owned(),
            dirty: false,
            revision: session.revision(),
            saved_revision: session.revision(),
            external_reload: EditorExternalReloadState::Clean,
            selection: Default::default(),
            editor: SceneEditorState::new().model(&session, &frozen_empty_registry(), None),
        }
    }

    fn document_frame(kind: EditorDocumentKind) -> EditorFrameModel {
        use nara::{
            scene::{PrefabAuthoringSession, PrefabDocument},
            tooling::{
                EditorAuthoringReadiness, EditorDocumentSourceBinding, EditorFrameClock,
                EditorPersistenceView, EditorPlayView, EditorProjectContentView,
                EditorProjectReadinessView, EditorProjectView, EditorWorkspace,
                EditorWorkspaceIntentView,
            },
        };

        let mut workspace = EditorWorkspace::new();
        for title in ["First Scene", "Second Scene"] {
            workspace
                .open_scene_session(title, SceneAuthoringSession::new(SceneDocument::default()))
                .unwrap();
        }
        if kind == EditorDocumentKind::Prefab {
            workspace
                .open_prefab_session(
                    "Ordinary Prefab",
                    EditorDocumentSourceBinding::new("ordinary.prefab.json"),
                    PrefabAuthoringSession::new(PrefabDocument::default()),
                )
                .unwrap();
        }
        EditorFrameClock::new().publish(
            workspace.model(&frozen_empty_registry(), None),
            EditorProjectView::new(
                EditorProjectReadinessView::new(
                    EditorAuthoringReadiness::Complete,
                    EditorRuntimeAvailability::TrustedInProcess,
                ),
                Default::default(),
                EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                EditorPersistenceView::Idle,
                EditorProjectContentView::NotInstalled,
                EditorWorkspaceIntentView::new(None, None, None, None, None),
            ),
            Default::default(),
            EditorReopenTarget::Unavailable,
            None,
        )
    }

    fn draw_document_controls(
        context: &mut dear_imgui_rs::Context,
        panels: &mut ImguiAuthoringPanels,
        frame: &EditorFrameModel,
    ) -> Vec<EditorAction> {
        super::super::ui_interaction_tests::clear_controls();
        let ui = context.frame();
        let mut actions = Vec::new();
        ui.window("Document navigation")
            .position([0.0, 0.0], dear_imgui_rs::Condition::Always)
            .size([900.0, 500.0], dear_imgui_rs::Condition::Always)
            .build(|| panels.show_document_controls(ui, frame, &mut actions));
        let _ = context.render_legacy();
        actions
    }

    fn click_document_control(
        context: &mut dear_imgui_rs::Context,
        panels: &mut ImguiAuthoringPanels,
        frame: &EditorFrameModel,
        label: &str,
    ) -> Vec<EditorAction> {
        let rect = super::super::ui_interaction_tests::control_rect(label)
            .unwrap_or_else(|| panic!("the document control {label:?} was not painted"));
        context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        let mut actions = draw_document_controls(context, panels, frame);
        for down in [true, false] {
            context
                .io_mut()
                .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, down);
            actions.extend(draw_document_controls(context, panels, frame));
        }
        actions
    }

    fn document_context() -> dear_imgui_rs::Context {
        let mut context = dear_imgui_rs::Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([1000.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context
    }

    #[test]
    fn document_selector_reselects_active_scene_without_reselecting_active_prefab() {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let frame = document_frame(kind);
            assert_eq!(frame.scene_preview_document(), None);
            let document = frame.workspace().active_document.unwrap();
            let mut context = document_context();
            let mut panels = ImguiAuthoringPanels::default();
            for _ in 0..3 {
                assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
            }
            assert!(
                click_document_control(&mut context, &mut panels, &frame, "Document selector")
                    .is_empty()
            );
            for _ in 0..2 {
                assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
            }
            let actions = click_document_control(
                &mut context,
                &mut panels,
                &frame,
                &format!("document:{}", document.raw()),
            );
            if kind == EditorDocumentKind::Scene {
                assert!(matches!(
                    actions.as_slice(),
                    [EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                        document: selected,
                    })] if *selected == document
                ));
            } else {
                assert!(
                    actions.is_empty(),
                    "active Prefab selection must remain a no-op"
                );
            }
        }
    }

    #[test]
    fn back_to_scene_is_absent_without_a_retained_binding_even_with_open_scenes() {
        let frame = document_frame(EditorDocumentKind::Prefab);
        assert_eq!(frame.workspace().scenes.len(), 2);
        assert_eq!(frame.scene_preview_document(), None);
        let mut context = document_context();
        let mut panels = ImguiAuthoringPanels::default();
        for _ in 0..3 {
            assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
        }
        assert!(super::super::ui_interaction_tests::control_rect("Back to Scene").is_none());
    }

    #[test]
    fn back_to_scene_activates_the_bound_scene_instead_of_the_first_open_scene() {
        let frame = document_frame(EditorDocumentKind::Prefab);
        let document = frame.workspace().scenes[1].document;
        assert_ne!(frame.workspace().scenes[0].document, document);
        let frame = frame.with_scene_preview_document(Some(document));
        let mut context = document_context();
        let mut panels = ImguiAuthoringPanels::default();
        for _ in 0..3 {
            assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
        }
        let actions = click_document_control(&mut context, &mut panels, &frame, "Back to Scene");
        assert!(matches!(
            actions.as_slice(),
            [EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                document: selected,
            })] if *selected == document
        ));
    }

    #[test]
    fn component_actions_keep_the_exact_document_kind_revision_and_stable_component_id() {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let scene = scene(kind);
            let entity = SceneEntityId::new("owner").unwrap();
            let component = ComponentTypeId::new("external.authoring.Custom");
            let action = compose_action(
                &scene,
                SceneCompositionCommand::AddComponent {
                    entity: entity.clone(),
                    component: component.clone(),
                },
            );
            match action {
                EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                    document,
                    base_revision,
                    command:
                        SceneCompositionCommand::AddComponent {
                            entity: actual_entity,
                            component: actual_component,
                        },
                }) => {
                    assert_eq!(kind, EditorDocumentKind::Scene);
                    assert_eq!((document, base_revision), (scene.document, scene.revision));
                    assert_eq!((actual_entity, actual_component), (entity, component));
                }
                EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                    document,
                    base_revision,
                    command:
                        PrefabCompositionCommand::AddComponent {
                            entity: actual_entity,
                            component: actual_component,
                        },
                }) => {
                    assert_eq!(kind, EditorDocumentKind::Prefab);
                    assert_eq!((document, base_revision), (scene.document, scene.revision));
                    assert_eq!((actual_entity, actual_component), (entity, component));
                }
                other => panic!("component action changed document authority: {other:?}"),
            }
        }
    }

    #[test]
    fn an_uncertain_save_allows_only_reopening_its_exact_document() {
        let scene = scene(EditorDocumentKind::Scene);
        for result in [
            Some(EditorPersistenceResult::PersistenceUncertain {
                document: scene.document,
                revision: scene.revision,
                digest: nara::tooling::EditorDocumentDigest::new(0, [0; 32]),
            }),
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(scene.document),
                reason: EditorPersistenceRejection::RequiresReconcile,
            }),
        ] {
            assert!(!persistence_allows(
                result,
                EditorPersistenceRequestKind::Save,
                scene.document
            ));
            assert!(persistence_allows(
                result,
                EditorPersistenceRequestKind::Reopen,
                scene.document
            ));
            assert!(!persistence_allows(
                result,
                EditorPersistenceRequestKind::Reopen,
                EditorDocumentId::from_raw(8)
            ));
        }
        assert!(!persistence_allows(
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: None,
                reason: EditorPersistenceRejection::RequiresReconcile,
            }),
            EditorPersistenceRequestKind::Reopen,
            scene.document,
        ));
    }

    #[test]
    fn an_ordinary_save_keeps_the_authoring_frame_editable_but_reopen_does_not() {
        use nara::tooling::{
            EditorAuthoringReadiness, EditorFrameClock, EditorPersistenceView, EditorPlayView,
            EditorProjectContentView, EditorProjectReadinessView, EditorProjectView,
            EditorWorkspace, EditorWorkspaceIntentView,
        };
        let mut workspace = EditorWorkspace::new();
        let report = workspace
            .open_scene_session(
                "Editable",
                SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
                    SceneEntityRecord::new(SceneEntityId::new("owner").unwrap()),
                ])),
            )
            .unwrap();
        let document = report.opened_document.unwrap();
        let revision = workspace.scene(document).unwrap().revision();
        let mut clock = EditorFrameClock::new();
        let source = clock.publish(
            workspace.model(&frozen_empty_registry(), None),
            EditorProjectView::new(
                EditorProjectReadinessView::new(
                    EditorAuthoringReadiness::Complete,
                    EditorRuntimeAvailability::TrustedInProcess,
                ),
                Default::default(),
                EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                EditorPersistenceView::Idle,
                EditorProjectContentView::NotInstalled,
                EditorWorkspaceIntentView::new(None, None, None, None, None),
            ),
            Default::default(),
            EditorReopenTarget::OpenDocument {
                document,
                base_revision: revision,
            },
            None,
        );
        let attempt = nara::tooling::EditorPersistenceAttemptId::new(source.revision(), 1);
        for (operation, editable) in [
            (None, true),
            (
                Some(EditorPersistenceOperation::Saving {
                    document,
                    captured_revision: revision,
                }),
                true,
            ),
            (
                Some(EditorPersistenceOperation::Opening { document }),
                false,
            ),
        ] {
            let frame = clock.publish(
                workspace.model(&frozen_empty_registry(), None),
                EditorProjectView::new(
                    EditorProjectReadinessView::new(
                        EditorAuthoringReadiness::Complete,
                        EditorRuntimeAvailability::TrustedInProcess,
                    ),
                    Default::default(),
                    EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                    match operation {
                        Some(operation) => EditorPersistenceView::Pending { attempt, operation },
                        None => EditorPersistenceView::Idle,
                    },
                    EditorProjectContentView::NotInstalled,
                    EditorWorkspaceIntentView::new(None, None, None, None, None),
                ),
                Default::default(),
                EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: revision,
                },
                None,
            );
            assert_eq!(editing_enabled(&frame), editable, "{operation:?}");
        }
    }
}
