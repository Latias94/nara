use std::collections::BTreeSet;

use nara::{
    reflect::ComponentRegistryBuilder,
    scene::{SceneAuthoringSession, SceneDocument},
    tooling::{
        EditorDocumentDigest, EditorDocumentOpenId, EditorDocumentOpenResult,
        EditorDocumentSourceBinding, EditorFrameClock, EditorPersistenceAttemptId,
        EditorPersistenceOperation, EditorPersistenceResult, EditorPlayView,
        EditorProjectBrowserActionOutcome, EditorProjectBrowserBudgetUsage,
        EditorProjectBrowserLocalAction, EditorProjectBrowserSnapshot, EditorProjectBrowserState,
        EditorProjectContentView, EditorProjectSourceInspectReason, EditorProjectSourceKind,
        EditorProjectView, EditorWorkspace,
    },
};
use nara_ui_kernel::{
    UiInputEvent, UiInteractionSession, UiRouteStamp, UiSeatId, UiSemanticRole, UiSurfaceId,
};

use super::super::{DrawnPanel, PanelControls, input::PanelInput};
use super::*;

fn fixture(count: usize) -> EditorFrameModel {
    let mut workspace = EditorWorkspace::new();
    for _ in 0..count {
        workspace
            .open_scene_session(
                "Repeated title",
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap();
    }
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let mut model = workspace.model(&registry, None);
    for document in &mut model.documents {
        document.source_binding = Some(EditorDocumentSourceBinding::new(format!(
            "scenes/{}.scene.json",
            document.document.raw(),
        )));
    }
    let reopen = model
        .active_document
        .and_then(|document| model.document(document))
        .map_or(EditorReopenTarget::Unavailable, |document| {
            EditorReopenTarget::OpenDocument {
                document: document.document,
                base_revision: document.revision,
            }
        });
    EditorFrameClock::new().publish(
        model,
        EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
            Default::default(),
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::Idle,
            EditorProjectContentView::NotInstalled,
            EditorWorkspaceIntentView::default(),
        ),
        Default::default(),
        reopen,
        None,
    )
}

fn successor(
    model: &EditorFrameModel,
    workspace: nara::tooling::EditorWorkspaceModel,
    persistence: EditorPersistenceView,
    intent: EditorWorkspaceIntentView,
) -> EditorFrameModel {
    EditorFrameClock::new().publish(
        workspace,
        EditorProjectView::new(
            model.project_readiness(),
            Default::default(),
            model.play().clone(),
            persistence,
            model.project_content(),
            intent,
        ),
        Default::default(),
        model.reopen_target(),
        None,
    )
}

fn controller() -> ShellControls {
    ShellControls::new(
        UiRootKey::runtime("editor.shell", "tests").unwrap(),
        UiRootKey::runtime("editor.shell", "tests.menu").unwrap(),
        vec![nara_text::bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 1200.0, 272.0, 1.0, 1, 1, 1, 1).unwrap()
}

fn open_menu(
    shell: &mut ShellControls,
    model: &EditorFrameModel,
    environment: UiEnvironment,
) -> Arc<UiFrame> {
    let mut frame = shell.compile(model, environment).unwrap();
    for _ in 0..64 {
        let target = key(shell, &frame, Control::OpenMenu);
        if frame.semantics().iter().any(|node| {
            node.key() == &target
                && node
                    .clip_rect()
                    .is_none_or(|clip| node.rect().intersect(clip) == Some(node.rect()))
        }) {
            shell.activate(&frame, &target, model).unwrap();
            return shell.compile(model, environment).unwrap();
        }
        assert!(shell.scroll(&frame, 40.0).unwrap());
        frame = shell.compile(model, environment).unwrap();
    }
    panic!("the menu trigger must remain reachable by scrolling the actual shell");
}

fn key(controller: &ShellControls, frame: &Arc<UiFrame>, control: Control) -> UiKey {
    controller
        .retained(frame)
        .unwrap()
        .buttons
        .iter()
        .find(|(_, binding)| binding.control == control)
        .map(|(key, _)| key.clone())
        .expect("the control was compiled")
}

fn source_entry(
    path: &str,
    capability: EditorProjectSourceOpenCapability,
) -> EditorProjectBrowserEntry {
    EditorProjectBrowserEntry::new(
        Arc::from(path),
        match capability {
            EditorProjectSourceOpenCapability::Scene => EditorProjectSourceKind::Scene,
            EditorProjectSourceOpenCapability::Prefab => EditorProjectSourceKind::Prefab,
            _ => EditorProjectSourceKind::AssetSource,
        },
        capability,
    )
}

fn browser(
    generation: u64,
    entries: impl IntoIterator<Item = EditorProjectBrowserEntry>,
) -> EditorProjectBrowserState {
    let mut browser = EditorProjectBrowserState::new();
    browser.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(Arc::new(EditorProjectBrowserSnapshot::new(
            generation,
            entries,
            [],
            EditorProjectBrowserBudgetUsage::default(),
        ))),
        None,
        None,
        false,
        false,
    );
    browser
}

fn with_browser(model: &EditorFrameModel, browser: EditorProjectBrowserView) -> EditorFrameModel {
    EditorFrameClock::new()
        .publish(
            model.workspace().clone(),
            EditorProjectView::new(
                model.project_readiness(),
                browser,
                model.play().clone(),
                model.persistence(),
                model.project_content(),
                model.workspace_intent(),
            ),
            Default::default(),
            model.reopen_target(),
            None,
        )
        .with_document_open(model.document_open().clone())
}

#[test]
fn every_document_is_reachable_by_exact_id_across_bounded_pages() {
    let model = fixture(11);
    let before = model.clone();
    let expected = model
        .workspace()
        .documents
        .iter()
        .map(|tab| tab.document)
        .collect::<BTreeSet<_>>();
    let mut shell = controller();
    let mut seen = BTreeSet::new();
    for page in 0..3 {
        let frame = shell.compile(&model, environment()).unwrap();
        let compiled = shell.retained(&frame).unwrap();
        let documents = compiled
            .buttons
            .values()
            .filter_map(|binding| match binding.control {
                Control::Document(document) => Some(document),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(documents.len() <= DOCUMENTS_PER_PAGE);
        seen.extend(documents);
        if page < 2 {
            let next = key(&shell, &frame, Control::NextDocuments);
            assert!(shell.activate(&frame, &next, &model).unwrap().is_none());
            assert!(shell.activate(&frame, &next, &model).is_err());
        }
    }
    assert_eq!(seen, expected);
    let frame = shell.compile(&model, environment()).unwrap();
    let previous = key(&shell, &frame, Control::PreviousDocuments);
    shell.activate(&frame, &previous, &model).unwrap();
    assert_eq!(shell.document_offset, DOCUMENTS_PER_PAGE);
    assert_eq!(model, before);
}

#[test]
fn selection_uses_the_captured_id_with_duplicate_titles() {
    let model = fixture(4);
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let document = model.workspace().documents[1].document;
    let button = key(&shell, &frame, Control::Document(document));
    assert!(matches!(
        shell.activate(&frame, &button, &model).unwrap(),
        Some(EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: actual,
        })) if actual == document
    ));
    assert!(shell.activate(&frame, &button, &model).is_err());
}

#[test]
fn routed_semantic_activation_uses_the_controlled_button_binding() {
    let model = fixture(2);
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let document = model.workspace().documents[0].document;
    let button = key(&shell, &frame, Control::Document(document));
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let outcome = UiInteractionSession::default()
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::SemanticAction {
                seat: UiSeatId::new(1),
                target: button.clone(),
                action: UiActionId::new("shell.activate").unwrap(),
            },
        )
        .unwrap();
    let batch = outcome
        .consumed()
        .expect("the presented button accepts its semantic action");
    let intents = shell.dispatch(&frame, batch).unwrap();
    assert_eq!(intents.intents().len(), 1);
    assert_eq!(intents.intents()[0].target(), &button);
    assert!(matches!(
        shell
            .activate(&frame, intents.intents()[0].target(), &model)
            .unwrap(),
        Some(EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: actual,
        })) if actual == document
    ));
    assert!(shell.dispatch(&frame, batch).unwrap().intents().is_empty());
}

#[test]
fn observations_require_the_latest_original_frame_and_live_action_owner() {
    let model = fixture(2);
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        1200.0,
        1000.0,
        2.0,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut shell = controller();
    let frame = shell.compile(&model, environment).unwrap();
    let target = key(
        &shell,
        &frame,
        Control::Document(model.workspace().documents[0].document),
    );
    let action = UiActionId::new("shell.activate").unwrap();
    assert!(shell.owns_action(&frame, &target, &action));
    assert!(!shell.owns_action(
        &frame,
        &target,
        &UiActionId::new("foreign.activate").unwrap(),
    ));
    let mut panel = DrawnPanel(PanelControls::Shell(shell));
    let input = PanelInput::default();
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment.surface_id(), 1)
        .unwrap();
    assert!(input.observe(&panel, &journal, &frame).is_err());
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let snapshot = input.observe(&panel, &journal, &frame).unwrap();
    assert_eq!(snapshot.route(), UiRouteStamp::from_receipt(receipt));
    let node = snapshot
        .nodes()
        .iter()
        .find(|node| node.key() == &target)
        .unwrap();
    assert_eq!(node.actions(), std::slice::from_ref(&action));
    assert_eq!(
        Some(node.physical_rect()),
        environment.physical_rect(node.rect()),
    );
    let next_receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 2)
        .unwrap();
    let latest = input.observe(&panel, &journal, &frame).unwrap();
    assert_ne!(snapshot.route(), latest.route());
    assert_eq!(latest.route(), UiRouteStamp::from_receipt(next_receipt));
    let foreign = DrawnPanel(PanelControls::Shell(controller()));
    assert!(input.observe(&foreign, &journal, &frame).is_err());
    panel.cancel_input().unwrap();
    let retired = input.observe(&panel, &journal, &frame).unwrap();
    assert!(retired.nodes().iter().all(|node| node.actions().is_empty()));
    let replacement = panel.compile(&model, None, environment).unwrap();
    assert!(!Arc::ptr_eq(&frame, &replacement));
    assert!(input.observe(&panel, &journal, &replacement).is_err());
    authority
        .submit(&mut journal, Arc::clone(&replacement), 3)
        .unwrap();
    assert!(input.observe(&panel, &journal, &frame).is_err());
    assert!(input.observe(&panel, &journal, &replacement).is_ok());
}

#[test]
fn observations_keep_disabled_and_modal_inert_states_distinct() {
    let model = fixture(1);
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        1200.0,
        1000.0,
        1.0,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut shell = controller();
    let frame = shell.compile(&model, environment).unwrap();
    let undo = key(&shell, &frame, Control::Undo);
    let mut panel = DrawnPanel(PanelControls::Shell(shell));
    let input = PanelInput::default();
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment.surface_id(), 1)
        .unwrap();
    authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let snapshot = input.observe(&panel, &journal, &frame).unwrap();
    let disabled = snapshot
        .nodes()
        .iter()
        .find(|node| node.key() == &undo)
        .unwrap();
    assert!(!disabled.state().enabled());
    assert!(!disabled.state().inert());
    assert!(disabled.actions().is_empty());
    let PanelControls::Shell(shell) = &mut panel.0 else {
        panic!("expected the original shell")
    };
    let modal = open_menu(shell, &model, environment);
    let document = key(
        shell,
        &modal,
        Control::Document(model.workspace().documents[0].document),
    );
    let dismissal = key(shell, &modal, Control::DismissMenu);
    authority
        .submit(&mut journal, Arc::clone(&modal), 2)
        .unwrap();
    let snapshot = input.observe(&panel, &journal, &modal).unwrap();
    let inert = snapshot
        .nodes()
        .iter()
        .find(|node| node.key() == &document)
        .unwrap();
    assert!(inert.state().enabled());
    assert!(inert.state().inert());
    assert!(!inert.state().focusable());
    assert!(inert.actions().is_empty());
    let dismissal = snapshot
        .nodes()
        .iter()
        .find(|node| node.key() == &dismissal)
        .unwrap();
    assert!(dismissal.state().enabled());
    assert!(!dismissal.state().inert());
    assert_eq!(
        dismissal.actions(),
        &[UiActionId::new("shell.activate").unwrap()],
    );
}

#[test]
fn history_and_persistence_actions_keep_the_displayed_document_revision() {
    let original = fixture(1);
    let mut workspace = original.workspace().clone();
    workspace.documents[0].dirty = true;
    workspace.documents[0].history.undo_depth = 2;
    workspace.documents[0].history.redo_depth = 1;
    let model = successor(
        &original,
        workspace,
        EditorPersistenceView::Idle,
        Default::default(),
    );
    let expected = &model.workspace().documents[0];
    for control in [Control::Undo, Control::Redo, Control::Save, Control::Reopen] {
        let mut shell = controller();
        let frame = shell.compile(&model, environment()).unwrap();
        let button = key(&shell, &frame, control);
        let action = shell.activate(&frame, &button, &model).unwrap().unwrap();
        match action {
            EditorAction::Mutate(mutation) => {
                assert_eq!(mutation.document(), expected.document);
                assert_eq!(mutation.base_revision(), expected.revision);
                assert!(matches!(
                    (control, mutation),
                    (Control::Undo, EditorDocumentMutation::Undo { .. })
                        | (Control::Redo, EditorDocumentMutation::Redo { .. })
                ));
            }
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            }) => {
                assert!(control == Control::Save);
                assert_eq!(
                    (document, base_revision),
                    (expected.document, expected.revision),
                );
            }
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }) => {
                assert!(control == Control::Reopen);
                assert_eq!(target, model.reopen_target());
            }
            _ => panic!("the finite toolbar emitted another action kind"),
        }
    }
}

#[test]
fn pending_persistence_keeps_cancel_and_discard_available_without_enabling_save() {
    let original = fixture(1);
    let mut workspace = original.workspace().clone();
    workspace.documents[0].dirty = true;
    workspace.documents[0].history.undo_depth = 1;
    let document = workspace.documents[0].document;
    let revision = workspace.documents[0].revision;
    let attempt = EditorPersistenceAttemptId::new(original.revision(), 1);
    let mut clock = EditorFrameClock::new();
    let prompt = clock.issue_close_prompt(document);
    let intent = EditorWorkspaceIntentView::new(
        Some(EditorWorkspaceIntent::CloseDocument { document }),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision),
        Some(document),
        Some(prompt),
        None,
    );
    for operation in [
        EditorPersistenceOperation::Saving {
            document,
            captured_revision: revision,
        },
        EditorPersistenceOperation::Opening { document },
    ] {
        let persistence = EditorPersistenceView::Pending { attempt, operation };
        let editing = successor(
            &original,
            workspace.clone(),
            persistence,
            Default::default(),
        );
        let mut shell = controller();
        let frame = shell.compile(&editing, environment()).unwrap();
        let undo = key(&shell, &frame, Control::Undo);
        assert_eq!(
            shell.retained(&frame).unwrap().buttons[&undo]
                .button
                .enabled(),
            matches!(operation, EditorPersistenceOperation::Saving { .. }),
        );

        let closing = successor(&original, workspace.clone(), persistence, intent);
        for decision in [EditorCloseDecision::Discard, EditorCloseDecision::Cancel] {
            let mut shell = controller();
            let frame = shell.compile(&closing, environment()).unwrap();
            for control in [
                Control::Save,
                Control::Reopen,
                Control::AcknowledgePersistence,
                Control::SaveBeforeClose,
            ] {
                let button = key(&shell, &frame, control);
                assert!(
                    !shell.retained(&frame).unwrap().buttons[&button]
                        .button
                        .enabled()
                );
            }
            let control = match decision {
                EditorCloseDecision::Discard => Control::DiscardBeforeClose,
                _ => Control::CancelClose,
            };
            let button = key(&shell, &frame, control);
            assert!(matches!(
                shell.activate(&frame, &button, &closing).unwrap(),
                Some(EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve {
                    prompt: actual_prompt,
                    decision: actual_decision,
                })) if actual_prompt == prompt && actual_decision == decision
            ));
        }
    }
}

#[test]
fn old_buttons_refuse_replacement_source_and_changed_active_document() {
    let model = fixture(2);
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let document = model.workspace().documents[0].document;
    let button = key(&shell, &frame, Control::Document(document));
    let mut workspace = model.workspace().clone();
    workspace.documents[0].revision =
        SceneAuthoringSession::new(SceneDocument::default()).revision();
    let replacement = successor(
        &model,
        workspace,
        model.persistence(),
        model.workspace_intent(),
    );
    assert!(shell.activate(&frame, &button, &replacement).is_err());
    let mut workspace = model.workspace().clone();
    workspace.active_document = Some(document);
    for tab in &mut workspace.documents {
        tab.active = tab.document == document;
    }
    let switched = successor(
        &model,
        workspace,
        model.persistence(),
        model.workspace_intent(),
    );
    assert!(shell.activate(&frame, &button, &switched).is_err());
}

#[test]
fn persistence_acknowledgment_cannot_acknowledge_a_successor_attempt() {
    let original = fixture(1);
    let document = &original.workspace().documents[0];
    let result = EditorPersistenceResult::Saved {
        document: document.document,
        revision: document.revision,
        digest: EditorDocumentDigest::new(0, [0; 32]),
    };
    let first = EditorPersistenceAttemptId::new(original.revision(), 1);
    let second = EditorPersistenceAttemptId::new(original.revision(), 2);
    let model = successor(
        &original,
        original.workspace().clone(),
        EditorPersistenceView::Terminal {
            attempt: first,
            result,
        },
        Default::default(),
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let acknowledge = key(&shell, &frame, Control::AcknowledgePersistence);
    let replacement = successor(
        &model,
        model.workspace().clone(),
        EditorPersistenceView::Terminal {
            attempt: second,
            result,
        },
        Default::default(),
    );
    assert!(shell.activate(&frame, &acknowledge, &replacement).is_err());
    assert!(matches!(
        shell.activate(&frame, &acknowledge, &model).unwrap(),
        Some(EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt,
        })) if attempt == first
    ));
}

#[test]
fn a_close_decision_cannot_resolve_a_replacement_prompt() {
    let original = fixture(1);
    let document = original.workspace().documents[0].document;
    let mut clock = EditorFrameClock::new();
    let first = clock.issue_close_prompt(document);
    let second = clock.issue_close_prompt(document);
    let intent = |prompt| {
        EditorWorkspaceIntentView::new(
            Some(EditorWorkspaceIntent::CloseDocument { document }),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision),
            Some(document),
            Some(prompt),
            None,
        )
    };
    let model = successor(
        &original,
        original.workspace().clone(),
        EditorPersistenceView::Idle,
        intent(first),
    );
    let replacement = successor(
        &model,
        model.workspace().clone(),
        EditorPersistenceView::Idle,
        intent(second),
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    for control in [
        Control::SaveBeforeClose,
        Control::DiscardBeforeClose,
        Control::CancelClose,
    ] {
        let button = key(&shell, &frame, control);
        assert!(shell.activate(&frame, &button, &replacement).is_err());
    }
    let cancel = key(&shell, &frame, Control::CancelClose);
    assert!(matches!(
        shell.activate(&frame, &cancel, &model).unwrap(),
        Some(EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve {
            prompt,
            decision: EditorCloseDecision::Cancel,
        })) if prompt == first
    ));
}

#[test]
fn retention_is_bounded_and_complete_basis_controls_reuse() {
    let model = fixture(1);
    let mut shell = controller();
    let first = shell.compile(&model, environment()).unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &shell.compile(&model, environment()).unwrap(),
    ));
    let mut workspace = model.workspace().clone();
    workspace.documents[0].source_binding =
        Some(EditorDocumentSourceBinding::new("other.scene.json"));
    let replacement = successor(
        &model,
        workspace,
        model.persistence(),
        model.workspace_intent(),
    );
    let changed = shell.compile(&replacement, environment()).unwrap();
    assert!(!Arc::ptr_eq(&first, &changed));
    let close = key(&shell, &first, Control::Close);
    assert!(shell.activate(&first, &close, &model).is_err());
    for _ in 0..RETAINED_FRAMES {
        shell.cancel_input().unwrap();
        shell.compile(&replacement, environment()).unwrap();
    }
    assert_eq!(shell.frames.len(), RETAINED_FRAMES);
    assert!(!shell.retains_frame(&first));
}

#[test]
fn long_untrusted_labels_are_bounded_without_changing_document_identity() {
    let model = fixture(1);
    let mut workspace = model.workspace().clone();
    workspace.documents[0].title = "文档\n".repeat(10_000);
    let model = successor(
        &model,
        workspace,
        model.persistence(),
        model.workspace_intent(),
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let document = model.workspace().documents[0].document;
    let button = key(&shell, &frame, Control::Document(document));
    let label = shell.retained(&frame).unwrap().buttons[&button]
        .button
        .label();
    assert!(label.chars().count() <= 81);
    assert!(!label.chars().any(char::is_control));
    assert!(matches!(
        shell.activate(&frame, &button, &model).unwrap(),
        Some(EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: actual,
        })) if actual == document
    ));
}

#[test]
fn every_openable_snapshot_source_is_reachable_across_bounded_pages() {
    let mut entries = (0..11)
        .map(|index| {
            source_entry(
                &format!("sources/{index:02}.json"),
                if index % 2 == 0 {
                    EditorProjectSourceOpenCapability::Scene
                } else {
                    EditorProjectSourceOpenCapability::Prefab
                },
            )
        })
        .collect::<Vec<_>>();
    entries.push(source_entry(
        "assets/icon.png",
        EditorProjectSourceOpenCapability::InspectOnly(
            EditorProjectSourceInspectReason::ReadOnlySource,
        ),
    ));
    entries.push(EditorProjectBrowserEntry::new(
        Arc::from("sources"),
        EditorProjectSourceKind::Directory,
        EditorProjectSourceOpenCapability::InspectOnly(EditorProjectSourceInspectReason::Directory),
    ));
    let mut browser = browser(7, entries);
    browser.apply_local_action(EditorProjectBrowserLocalAction::SetPathFilter {
        value: Arc::from("not-a-visible-row"),
    });
    while browser.advance_query(4).pending() {}
    let model = with_browser(&fixture(0), browser.view());
    assert!(model.project_browser().rows().is_empty());
    let before = model.clone();
    let snapshot = model.project_browser().snapshot().unwrap();
    let expected = snapshot
        .entries()
        .iter()
        .filter(|entry| entry.open_capability().is_document())
        .map(|entry| entry.relative_path().to_owned())
        .collect::<BTreeSet<_>>();
    let mut shell = controller();
    let mut seen = BTreeSet::new();
    for page in 0..3 {
        let frame = shell.compile(&model, environment()).unwrap();
        let sources = shell
            .retained(&frame)
            .unwrap()
            .buttons
            .values()
            .filter_map(|binding| binding.source.clone())
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), if page < 2 { SOURCES_PER_PAGE } else { 3 });
        for source in sources {
            let entry = &snapshot.entries()[source.snapshot_index];
            assert_eq!(source.snapshot_generation, snapshot.generation());
            assert_eq!(source.relative_path.as_ref(), entry.relative_path());
            assert_eq!(source.open_capability, entry.open_capability());
            assert!(seen.insert(source.relative_path.to_string()));
            let frame = shell.compile(&model, environment()).unwrap();
            let button = key(&shell, &frame, Control::OpenSource(source.snapshot_index));
            assert!(matches!(
                shell.activate(&frame, &button, &model).unwrap(),
                Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open { relative_path }))
                    if relative_path == source.relative_path
            ));
        }
        let frame = shell.compile(&model, environment()).unwrap();
        let next = key(&shell, &frame, Control::NextSources);
        if page < 2 {
            assert!(shell.activate(&frame, &next, &model).unwrap().is_none());
            assert!(shell.activate(&frame, &next, &model).is_err());
        } else {
            assert!(
                !shell.retained(&frame).unwrap().buttons[&next]
                    .button
                    .enabled()
            );
        }
    }
    assert_eq!(seen, expected);
    let frame = shell.compile(&model, environment()).unwrap();
    let previous = key(&shell, &frame, Control::PreviousSources);
    shell.activate(&frame, &previous, &model).unwrap();
    assert_eq!(shell.source_offset, SOURCES_PER_PAGE);
    assert_eq!(model, before);
}

#[test]
fn source_pages_are_independent_of_document_pages_and_clamp_after_removal() {
    let entries = (0..9)
        .map(|index| {
            source_entry(
                &format!("sources/{index}.json"),
                EditorProjectSourceOpenCapability::Scene,
            )
        })
        .collect::<Vec<_>>();
    let model = with_browser(&fixture(7), browser(1, entries.clone()).view());
    let mut shell = controller();
    for control in [
        Control::NextDocuments,
        Control::NextSources,
        Control::NextSources,
    ] {
        let frame = shell.compile(&model, environment()).unwrap();
        let button = key(&shell, &frame, control);
        shell.activate(&frame, &button, &model).unwrap();
    }
    assert_eq!(shell.document_offset, DOCUMENTS_PER_PAGE);
    assert_eq!(shell.source_offset, 2 * SOURCES_PER_PAGE);
    let frame = shell.compile(&model, environment()).unwrap();
    let old = key(&shell, &frame, Control::OpenSource(8));
    let replacement = with_browser(&model, browser(2, entries.into_iter().take(2)).view());
    assert!(shell.activate(&frame, &old, &replacement).is_err());
    let current = shell.compile(&replacement, environment()).unwrap();
    assert_eq!(shell.source_offset, 0);
    assert_eq!(shell.document_offset, DOCUMENTS_PER_PAGE);
    assert_eq!(
        shell
            .retained(&current)
            .unwrap()
            .buttons
            .values()
            .filter(|binding| binding.source.is_some())
            .count(),
        2
    );
    let previous = key(&shell, &current, Control::PreviousSources);
    assert!(
        !shell.retained(&current).unwrap().buttons[&previous]
            .button
            .enabled()
    );
    let empty = with_browser(&replacement, Default::default());
    let frame = shell.compile(&empty, environment()).unwrap();
    assert!(
        shell
            .retained(&frame)
            .unwrap()
            .buttons
            .values()
            .all(|binding| binding.source.is_none())
    );
    let next = key(&shell, &frame, Control::NextSources);
    assert!(
        !shell.retained(&frame).unwrap().buttons[&next]
            .button
            .enabled()
    );
}

#[test]
fn equal_bounded_source_labels_keep_distinct_keys_and_complete_paths() {
    let prefix = format!("sources/{}", "目录\n".repeat(1_000));
    let first_path = format!("{prefix}/first.prefab.json");
    let second_path = format!("{prefix}/second.prefab.json");
    let model = with_browser(
        &fixture(0),
        browser(
            4,
            [
                source_entry(&first_path, EditorProjectSourceOpenCapability::Prefab),
                source_entry(&second_path, EditorProjectSourceOpenCapability::Prefab),
            ],
        )
        .view(),
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let first = key(&shell, &frame, Control::OpenSource(0));
    let second = key(&shell, &frame, Control::OpenSource(1));
    assert_ne!(first, second);
    let bindings = &shell.retained(&frame).unwrap().buttons;
    assert_eq!(
        bindings[&first].button.label(),
        bindings[&second].button.label()
    );
    assert!(bindings[&first].button.label().len() <= 128);
    assert!(
        !bindings[&first]
            .button
            .label()
            .chars()
            .any(char::is_control)
    );
    assert_eq!(
        bindings[&first]
            .source
            .as_ref()
            .unwrap()
            .relative_path
            .as_ref(),
        first_path
    );
    assert_eq!(
        bindings[&second]
            .source
            .as_ref()
            .unwrap()
            .relative_path
            .as_ref(),
        second_path
    );
    for (index, expected) in [first_path, second_path].into_iter().enumerate() {
        let frame = shell.compile(&model, environment()).unwrap();
        let button = key(&shell, &frame, Control::OpenSource(index));
        assert!(matches!(
            shell.activate(&frame, &button, &model).unwrap(),
            Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open { relative_path }))
                if relative_path.as_ref() == expected
        ));
    }
}

#[test]
fn source_bindings_refuse_changed_generation_index_path_or_open_capability() {
    let prefix = "sources/same-prefix-".to_owned() + &"a".repeat(180);
    let entries = (0..5)
        .map(|index| {
            source_entry(
                &format!("{prefix}/{index}.json"),
                EditorProjectSourceOpenCapability::Prefab,
            )
        })
        .collect::<Vec<_>>();
    let model = with_browser(&fixture(0), browser(12, entries.clone()).view());
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let next = key(&shell, &frame, Control::NextSources);
    shell.activate(&frame, &next, &model).unwrap();
    let frame = shell.compile(&model, environment()).unwrap();
    let original = key(&shell, &frame, Control::OpenSource(4));
    let mut shifted = entries.clone();
    shifted.push(source_entry(
        "sources/000.json",
        EditorProjectSourceOpenCapability::Scene,
    ));
    let mut renamed = entries.clone();
    renamed[4] = source_entry(
        &format!("{prefix}/5.json"),
        EditorProjectSourceOpenCapability::Prefab,
    );
    let mut changed_kind = entries.clone();
    changed_kind[4] = source_entry(
        entries[4].relative_path(),
        EditorProjectSourceOpenCapability::Scene,
    );
    let mut inspect_only = entries.clone();
    inspect_only[4] = source_entry(
        entries[4].relative_path(),
        EditorProjectSourceOpenCapability::InspectOnly(
            EditorProjectSourceInspectReason::ReadOnlySource,
        ),
    );
    for (generation, entries) in [
        (13, entries.clone()),
        (12, shifted),
        (12, renamed),
        (12, changed_kind),
        (12, inspect_only),
    ] {
        let replacement = with_browser(&model, browser(generation, entries).view());
        assert!(shell.activate(&frame, &original, &replacement).is_err());
    }
    assert!(matches!(
        shell.activate(&frame, &original, &model).unwrap(),
        Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open { relative_path }))
            if relative_path.as_ref() == entries[4].relative_path()
    ));
    let current = shell.compile(&model, environment()).unwrap();
    let replacement = with_browser(&model, browser(13, entries).view());
    let replaced = shell.compile(&replacement, environment()).unwrap();
    assert!(!Arc::ptr_eq(&current, &replaced));
}

#[test]
fn source_semantics_route_the_actual_path_from_the_reserved_shell_area() {
    let model = with_browser(
        &fixture(0),
        browser(
            1,
            [source_entry(
                "prefabs/ui.prefab.json",
                EditorProjectSourceOpenCapability::Prefab,
            )],
        )
        .view(),
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let button = key(&shell, &frame, Control::OpenSource(0));
    let semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &button)
        .unwrap();
    assert_eq!(semantic.role(), UiSemanticRole::Button);
    assert_eq!(semantic.name(), "Open prefabs/ui.prefab.json");
    assert!(semantic.enabled());
    let source_heading = frame
        .layouts()
        .iter()
        .find(|layout| {
            layout.key().is_some_and(|key| {
                key.local_key()
                    == Some(&UiLocalKey::runtime("nara.shell.label", "sources").unwrap())
            })
        })
        .unwrap()
        .rect();
    assert!(semantic.rect().origin().y() >= source_heading.max().y());
    assert!(semantic.rect().max().y() <= environment().logical_size().height());
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let outcome = UiInteractionSession::default()
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::SemanticAction {
                seat: UiSeatId::new(1),
                target: button.clone(),
                action: UiActionId::new("shell.activate").unwrap(),
            },
        )
        .unwrap();
    let intents = shell.dispatch(&frame, outcome.consumed().unwrap()).unwrap();
    assert_eq!(intents.intents().len(), 1);
    assert!(matches!(
        shell.activate(&frame, intents.intents()[0].target(), &model).unwrap(),
        Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open { relative_path }))
            if relative_path.as_ref() == "prefabs/ui.prefab.json"
    ));
}

#[test]
fn source_open_cancellation_keeps_the_exact_request_path_and_preparing_phase() {
    let base = with_browser(
        &fixture(0),
        browser(
            1,
            [source_entry(
                "prefabs/ui.prefab.json",
                EditorProjectSourceOpenCapability::Prefab,
            )],
        )
        .view(),
    );
    let first = EditorDocumentOpenId::new(base.revision(), 9);
    let second = EditorDocumentOpenId::new(fixture(0).revision(), 9);
    assert_ne!(first, second);
    let pending = |request, path: &str, phase| {
        base.clone().with_document_open(EditorDocumentOpenView::new(
            Some((request, Arc::from(path), phase)),
            None,
        ))
    };
    let model = pending(
        first,
        "prefabs/ui.prefab.json",
        EditorDocumentOpenPhase::Preparing,
    );
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let cancel = key(&shell, &frame, Control::CancelOpen);
    for control in [Control::OpenSource(0), Control::AcknowledgeOpen] {
        let button = key(&shell, &frame, control);
        assert!(
            !shell.retained(&frame).unwrap().buttons[&button]
                .button
                .enabled()
        );
    }
    for replacement in [
        pending(
            second,
            "prefabs/ui.prefab.json",
            EditorDocumentOpenPhase::Preparing,
        ),
        pending(
            first,
            "prefabs/other.prefab.json",
            EditorDocumentOpenPhase::Preparing,
        ),
        pending(
            first,
            "prefabs/ui.prefab.json",
            EditorDocumentOpenPhase::Cancelling,
        ),
    ] {
        assert!(shell.activate(&frame, &cancel, &replacement).is_err());
    }
    assert!(matches!(
        shell.activate(&frame, &cancel, &model).unwrap(),
        Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::CancelOpen { request }))
            if request == first
    ));
    let cancelling = pending(
        first,
        "prefabs/ui.prefab.json",
        EditorDocumentOpenPhase::Cancelling,
    );
    let frame = shell.compile(&cancelling, environment()).unwrap();
    let cancel = key(&shell, &frame, Control::CancelOpen);
    assert!(
        !shell.retained(&frame).unwrap().buttons[&cancel]
            .button
            .enabled()
    );
    assert!(shell.activate(&frame, &cancel, &cancelling).is_err());
}

#[test]
fn source_refresh_uses_the_original_browser_and_refuses_pending_or_changed_observations() {
    let mut state = browser(1, []);
    while state.advance_query(4).pending() {}
    let model = with_browser(&fixture(0), state.view());
    let mut shell = controller();
    let frame = shell.compile(&model, environment()).unwrap();
    let refresh = key(&shell, &frame, Control::RefreshSources);
    assert!(
        shell.retained(&frame).unwrap().buttons[&refresh]
            .button
            .enabled()
    );
    state.synchronize(
        EditorProjectBrowserStatus::Ready,
        None,
        None,
        None,
        true,
        false,
    );
    let scanning = with_browser(&model, state.view());
    assert!(shell.activate(&frame, &refresh, &scanning).is_err());
    let scanning_frame = shell.compile(&scanning, environment()).unwrap();
    let current = key(&shell, &scanning_frame, Control::RefreshSources);
    assert!(
        !shell.retained(&scanning_frame).unwrap().buttons[&current]
            .button
            .enabled()
    );
    let request = EditorDocumentOpenId::new(model.revision(), 1);
    let opening = model
        .clone()
        .with_document_open(EditorDocumentOpenView::new(
            Some((
                request,
                Arc::from("prefabs/ui.prefab.json"),
                EditorDocumentOpenPhase::Preparing,
            )),
            None,
        ));
    let opening_frame = shell.compile(&opening, environment()).unwrap();
    let current = key(&shell, &opening_frame, Control::RefreshSources);
    assert!(
        !shell.retained(&opening_frame).unwrap().buttons[&current]
            .button
            .enabled()
    );
    let ready = shell.compile(&model, environment()).unwrap();
    let current = key(&shell, &ready, Control::RefreshSources);
    assert!(matches!(
        shell.activate(&ready, &current, &model).unwrap(),
        Some(EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::Refresh
        ))
    ));
}

#[test]
fn every_source_open_result_acknowledges_only_its_exact_request() {
    let base = with_browser(
        &fixture(0),
        browser(
            1,
            [source_entry(
                "scenes/start.scene.json",
                EditorProjectSourceOpenCapability::Scene,
            )],
        )
        .view(),
    );
    let first = EditorDocumentOpenId::new(base.revision(), 17);
    let second = EditorDocumentOpenId::new(fixture(0).revision(), 17);
    for outcome in 0..3 {
        let terminal = |request| match outcome {
            0 => EditorDocumentOpenResult::Opened {
                request,
                document: EditorDocumentId::from_raw(8),
            },
            1 => EditorDocumentOpenResult::Rejected {
                request,
                code: "project.editor.document-source-changed",
            },
            _ => EditorDocumentOpenResult::Cancelled { request },
        };
        let model = base
            .clone()
            .with_document_open(EditorDocumentOpenView::new(None, Some(terminal(first))));
        let replacement = base
            .clone()
            .with_document_open(EditorDocumentOpenView::new(None, Some(terminal(second))));
        let mut shell = controller();
        let frame = shell.compile(&model, environment()).unwrap();
        let acknowledge = key(&shell, &frame, Control::AcknowledgeOpen);
        let open = key(&shell, &frame, Control::OpenSource(0));
        assert!(
            !shell.retained(&frame).unwrap().buttons[&open]
                .button
                .enabled()
        );
        assert!(shell.activate(&frame, &acknowledge, &replacement).is_err());
        assert!(matches!(
            shell.activate(&frame, &acknowledge, &model).unwrap(),
            Some(EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }))
                if request == first
        ));
        let frame = shell.compile(&base, environment()).unwrap();
        let acknowledge = key(&shell, &frame, Control::AcknowledgeOpen);
        assert!(
            !shell.retained(&frame).unwrap().buttons[&acknowledge]
                .button
                .enabled()
        );
        let open = key(&shell, &frame, Control::OpenSource(0));
        assert!(
            shell.retained(&frame).unwrap().buttons[&open]
                .button
                .enabled()
        );
    }
}

#[test]
fn source_open_feedback_uses_closed_english_messages() {
    let base = fixture(0);
    let request = EditorDocumentOpenId::new(base.revision(), 5);
    let path = "private/文件\n.prefab.json";
    for (phase, expected) in [
        (
            EditorDocumentOpenPhase::Preparing,
            "Opening source (request 5)",
        ),
        (
            EditorDocumentOpenPhase::Cancelling,
            "Cancelling source open (request 5)",
        ),
    ] {
        let view = EditorDocumentOpenView::new(Some((request, Arc::from(path), phase)), None);
        assert_eq!(feedback::open_status(&view), expected);
    }
    let view = EditorDocumentOpenView::new(
        None,
        Some(EditorDocumentOpenResult::Rejected {
            request,
            code: "private/文件\n.prefab.json",
        }),
    );
    assert_eq!(
        feedback::open_status(&view),
        "Source open refused (request 5); review the project source"
    );
    for outcome in [
        EditorProjectBrowserActionOutcome::Preparing { request },
        EditorProjectBrowserActionOutcome::OpenCancellationRequested { request },
        EditorProjectBrowserActionOutcome::OpenAcknowledged { request },
    ] {
        let (applied, message) = feedback::action_result(&PanelActionDispatch::Dispatched(
            nara::tooling::EditorActionOutcome::ProjectBrowser(outcome),
        ));
        assert!(applied);
        assert!(message.contains("request 5"));
        assert!(!message.contains(path));
    }
    let (applied, message) = feedback::action_result(&PanelActionDispatch::Dispatched(
        nara::tooling::EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Rejected {
                code: "private/文件\n.prefab.json",
            },
        ),
    ));
    assert!(!applied);
    assert_eq!(
        message,
        "Source action refused; review the current project source"
    );
}

#[test]
fn anchored_menu_blocks_underlay_and_dismissal_restores_the_current_trigger() {
    let model = fixture(1);
    let mut shell = controller();
    let base = shell.compile(&model, environment()).unwrap();
    let trigger = key(&shell, &base, Control::OpenMenu);
    assert!(shell.activate(&base, &trigger, &model).unwrap().is_none());
    let menu = shell.compile(&model, environment()).unwrap();
    assert!(menu.blocks_unhandled_input());
    assert!(
        menu.root_outcomes()
            .iter()
            .all(|root| root.status() == UiRootStatus::Published)
    );
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&menu), 1)
        .unwrap();
    let underlay = key(&shell, &menu, Control::Close);
    let mut interaction = UiInteractionSession::default();
    let outcome = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::SemanticAction {
                seat: UiSeatId::PRIMARY,
                target: underlay,
                action: UiActionId::new("shell.activate").unwrap(),
            },
        )
        .unwrap();
    if let Some(batch) = outcome.consumed() {
        assert!(shell.dispatch(&menu, batch).unwrap().intents().is_empty());
    }
    let dismiss = key(&shell, &menu, Control::DismissMenu);
    assert!(shell.activate(&menu, &dismiss, &model).unwrap().is_none());
    assert!(shell.activate(&menu, &dismiss, &model).is_err());
    let restored = shell.compile(&model, environment()).unwrap();
    assert!(!restored.blocks_unhandled_input());
    assert_eq!(
        shell.take_focus_request(&restored),
        Some(key(&shell, &restored, Control::OpenMenu))
    );
}

#[test]
fn menu_refuses_changed_action_basis_and_retires_its_anchor_on_resize() {
    let model = fixture(1);
    let mut shell = controller();
    let base = shell.compile(&model, environment()).unwrap();
    let trigger = key(&shell, &base, Control::OpenMenu);
    shell.activate(&base, &trigger, &model).unwrap();
    let menu = shell.compile(&model, environment()).unwrap();
    let close = key(&shell, &menu, Control::MenuClose);
    let mut changed = model.workspace().clone();
    changed.documents[0].dirty = true;
    let changed = successor(
        &model,
        changed,
        EditorPersistenceView::Idle,
        Default::default(),
    );
    assert!(shell.activate(&menu, &close, &changed).is_err());
    let resized = shell
        .compile(
            &model,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 960.0, 320.0, 1.5, 1, 1, 1, 1)
                .unwrap(),
        )
        .unwrap();
    assert!(!resized.blocks_unhandled_input());
    assert!(shell.activate(&menu, &close, &model).is_err());
}

#[test]
fn oversized_menu_scrolls_every_original_action_into_its_clipped_viewport() {
    let model = fixture(1);
    let mut shell = controller();
    shell
        .set_appearance(UiAppearance::default().with_text_scale(4.0).unwrap())
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        1200.0,
        128.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut base = shell.compile(&model, environment).unwrap();
    for _ in 0..32 {
        let trigger = key(&shell, &base, Control::OpenMenu);
        if base.semantics().iter().any(|node| node.key() == &trigger) {
            break;
        }
        assert!(shell.scroll(&base, 40.0).unwrap());
        base = shell.compile(&model, environment).unwrap();
    }
    let trigger = key(&shell, &base, Control::OpenMenu);
    shell.activate(&base, &trigger, &model).unwrap();
    let mut frame = shell.compile(&model, environment).unwrap();
    assert!(shell.menu_scroll_limit > 0.0);
    for control in [
        Control::MenuRefresh,
        Control::MenuReopen,
        Control::MenuClose,
        Control::DismissMenu,
    ] {
        for _ in 0..12 {
            let target = key(&shell, &frame, control);
            let node = frame.semantics().iter().find(|node| node.key() == &target);
            if node.is_some_and(|node| {
                node.clip_rect()
                    .is_some_and(|clip| node.rect().intersect(clip) == Some(node.rect()))
            }) {
                break;
            }
            shell.scroll(&frame, 40.0).unwrap();
            frame = shell.compile(&model, environment).unwrap();
        }
        let target = key(&shell, &frame, control);
        let node = frame
            .semantics()
            .iter()
            .find(|node| node.key() == &target)
            .unwrap();
        assert!(
            node.clip_rect()
                .is_some_and(|clip| node.rect().intersect(clip) == Some(node.rect())),
            "{} must remain reachable",
            control.identity()
        );
    }
    assert!(frame.blocks_unhandled_input());
    assert!(shell.dismiss_menu().unwrap());
}

#[test]
fn menu_keyboard_navigation_reveals_hidden_actions_and_hover_does_not_reverse_wheel_scroll() {
    let model = fixture(1);
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        1200.0,
        128.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut shell = controller();
    shell
        .set_appearance(UiAppearance::default().with_text_scale(2.0).unwrap())
        .unwrap();
    open_menu(&mut shell, &model, environment);
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(environment.surface_id(), 1)
        .unwrap();
    let mut interaction = UiInteractionSession::default();
    let mut epoch = 0;
    let mut present = |shell: &mut ShellControls, event: Option<UiInputEvent>| {
        let frame = shell.compile(&model, environment).unwrap();
        epoch += 1;
        let receipt = authority
            .submit(&mut journal, Arc::clone(&frame), epoch)
            .unwrap();
        let route = UiRouteStamp::from_receipt(receipt);
        if let Some(batch) = interaction
            .reconcile_latest_presentation(&journal, environment.surface_id(), &[])
            .unwrap()
        {
            shell.dispatch(&frame, &batch).unwrap();
        }
        if let Some(target) = shell.take_focus_request(&frame) {
            let outcome = interaction
                .dispatch(
                    &journal,
                    route,
                    UiInputEvent::FocusTarget {
                        seat: UiSeatId::PRIMARY,
                        target,
                    },
                )
                .unwrap();
            shell.dispatch(&frame, outcome.consumed().unwrap()).unwrap();
        }
        if let Some(event) = event {
            let outcome = interaction.dispatch(&journal, route, event).unwrap();
            if let Some(batch) = outcome.consumed() {
                shell.dispatch(&frame, batch).unwrap();
            }
        }
        frame
    };
    let mut frame = present(&mut shell, None);
    for control in [
        Control::MenuRefresh,
        Control::MenuReopen,
        Control::MenuClose,
        Control::DismissMenu,
        Control::MenuRefresh,
    ] {
        let target = key(&shell, &frame, control);
        assert!(
            shell
                .states
                .get(&target)
                .is_some_and(|state| state.focused()),
            "{} must receive actual admitted focus",
            control.identity()
        );
        let semantic = frame
            .semantics()
            .iter()
            .find(|node| node.key() == &target)
            .unwrap();
        assert!(
            semantic
                .clip_rect()
                .is_none_or(|clip| semantic.rect().intersect(clip) == Some(semantic.rect()))
        );
        assert!(shell.navigate_menu(&frame, false).unwrap());
        frame = present(&mut shell, None);
    }
    shell.navigate_menu(&frame, true).unwrap();
    frame = present(&mut shell, None);
    let refresh = key(&shell, &frame, Control::MenuRefresh);
    let rect = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &refresh)
        .unwrap()
        .rect();
    let point = nara_ui_kernel::UiPoint::new(rect.origin().x() + 8.0, rect.origin().y() + 8.0);
    present(
        &mut shell,
        Some(UiInputEvent::Pointer(
            nara_ui_kernel::UiPointerInput::moved(
                UiSeatId::PRIMARY,
                nara_ui_kernel::UiPointerId::PRIMARY,
                point,
            ),
        )),
    );
    shell.scroll(&frame, 42.0).unwrap();
    let requested = shell.menu_scroll;
    present(
        &mut shell,
        Some(UiInputEvent::Pointer(
            nara_ui_kernel::UiPointerInput::moved(
                UiSeatId::PRIMARY,
                nara_ui_kernel::UiPointerId::PRIMARY,
                point,
            ),
        )),
    );
    assert_eq!(shell.menu_scroll, requested);
}

#[test]
fn first_menu_focus_reveals_the_enabled_dismissal_after_disabled_actions() {
    let mut browser = browser(1, []);
    browser.synchronize(
        EditorProjectBrowserStatus::Ready,
        None,
        None,
        None,
        true,
        false,
    );
    let model = with_browser(&fixture(0), browser.view());
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        1200.0,
        128.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut shell = controller();
    shell
        .set_appearance(UiAppearance::default().with_text_scale(2.0).unwrap())
        .unwrap();
    let frame = open_menu(&mut shell, &model, environment);
    let expected = key(&shell, &frame, Control::DismissMenu);
    assert_eq!(shell.take_focus_request(&frame), Some(expected.clone()));
    let node = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &expected)
        .unwrap();
    assert!(
        node.clip_rect()
            .is_none_or(|clip| node.rect().intersect(clip) == Some(node.rect()))
    );
    assert!(shell.menu_scroll > 0.0);
}

#[test]
fn workspace_plain_text_reuses_compatible_measurement_for_palette_and_resize() {
    let model = fixture(3);
    let mut shell = controller();
    let mut samples = Vec::new();
    let mut observe = |label: &str, shell: &mut ShellControls, environment| {
        let start = std::time::Instant::now();
        let frame = shell.compile(&model, environment).unwrap();
        samples.push((
            label.to_owned(),
            start.elapsed().as_nanos(),
            shell.compiler.work_stats(),
        ));
        frame
    };
    let cold = observe("cold", &mut shell, environment());
    let idle = observe("idle", &mut shell, environment());
    assert!(Arc::ptr_eq(&cold, &idle));
    let before = shell.compiler.work_stats();
    let mut palette = shell.appearance.palette();
    palette.raised = nara_ui_kernel::UiColor::rgba(0.07, 0.09, 0.12, 1.0);
    shell
        .set_appearance(shell.appearance.with_palette(palette).unwrap())
        .unwrap();
    let recolored = observe("palette", &mut shell, environment());
    assert!(!Arc::ptr_eq(&idle, &recolored));
    assert_eq!(
        shell.compiler.work_stats().text.layout_attempts,
        before.text.layout_attempts
    );
    assert_eq!(
        cold.semantics()
            .iter()
            .map(|node| (node.name(), node.rect()))
            .collect::<Vec<_>>(),
        recolored
            .semantics()
            .iter()
            .map(|node| (node.name(), node.rect()))
            .collect::<Vec<_>>()
    );
    let wide =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 1300.0, 320.0, 1.0, 1, 1, 1, 1).unwrap();
    observe("resize", &mut shell, wide);
    assert_eq!(
        shell.compiler.work_stats().text.layout_attempts,
        before.text.layout_attempts
    );
    shell
        .set_appearance(shell.appearance.with_text_scale(2.0).unwrap())
        .unwrap();
    observe("text-scale", &mut shell, wide);
    assert!(shell.compiler.work_stats().text.layout_attempts > before.text.layout_attempts);
    for (label, elapsed_ns, work) in samples {
        eprintln!("STYLED_SHELL work={label} one_sample_ns={elapsed_ns} cumulative={work:?}");
    }
}
