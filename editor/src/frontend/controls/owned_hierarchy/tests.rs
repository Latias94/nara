use std::{
    fs,
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaCatalog, ComponentTypeId,
        ComponentValue, PersistentComponentSchemaOwnerId,
    },
    scene::{
        __private::AuthoringAdapterReceipt, AuthoringSchemaBasis, PrefabDocument,
        PrefabEntityOrigin, PrefabInstanceHop, SceneDocument, SceneDocumentCandidate,
        SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
    },
    tooling::{
        EditorActionBatch, EditorCloseDecision, EditorDocumentMutation, EditorFrameClock,
        EditorPersistenceAction, EditorPersistenceResult, EditorReopenTarget,
        PrefabCompositionCommand,
    },
};
use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiFrameJournal, UiInputEvent, UiInteractionSession,
    UiPointerButton, UiPointerId, UiPresentationReceipt, UiRouteStamp, UiSeatId, UiSemanticRole,
    UiSurfaceId,
};

use super::*;
use crate::frontend::imgui::interaction_tests::{CUSTOM_COMPONENT, InteractionProject};
use crate::panels::{
    PanelActionNotDispatched,
    submission::{self, PanelOrigin},
};
use crate::{EditorApplication, EditorApplicationActionDispatch, EditorLaunchOptions};

mod boundaries;
mod filter;
mod long_content;
mod reuse;
mod work_costs;
mod workflows;

fn entity(index: usize) -> SceneEntityId {
    SceneEntityId::new(format!("entity-{index:04}")).unwrap()
}

fn application(kind: EditorDocumentKind, count: usize) -> (InteractionProject, TestApplication) {
    let project = InteractionProject::new();
    let rows = (0..count)
        .map(|index| SceneEntityRecord::new(entity(index)))
        .collect::<Vec<_>>();
    let source = match kind {
        EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(rows)
            .to_json_string()
            .unwrap(),
        EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(rows)
            .to_json_string()
            .unwrap(),
        _ => unreachable!(),
    };
    fs::write(project.source(kind), source).unwrap();
    let mut application = open(&project);
    if kind == EditorDocumentKind::Prefab {
        select(
            &mut application,
            SceneEntityId::new("prefab-anchor").unwrap(),
        );
        let model = application.current_frame().unwrap();
        let scene = model.workspace().active_scene.as_ref().unwrap();
        send(
            &mut application,
            EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                document: scene.document,
                base_revision: scene.revision,
                entity: SceneEntityId::new("prefab-anchor").unwrap(),
            }),
        );
    }
    let model = application.current_frame().unwrap();
    assert_eq!(model.workspace().active_scene.as_ref().unwrap().kind, kind);
    if count > 0 {
        select(&mut application, entity(0));
    }
    (project, application)
}

fn open(project: &InteractionProject) -> TestApplication {
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() && Instant::now() < deadline {
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        application.current_frame().is_some(),
        "the ordinary InteractionProject opens; state={:?}, recovery={:?}",
        application.state(),
        match application.project_shell_model() {
            Some(crate::app::EditorProjectShellModel::Recovery { diagnostics, .. }) => {
                Some(diagnostics)
            }
            _ => None,
        },
    );
    TestApplication(application)
}

/// Retire the real Session even when a test assertion unwinds.
struct TestApplication(EditorApplication);

impl Deref for TestApplication {
    type Target = EditorApplication;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TestApplication {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for TestApplication {
    fn drop(&mut self) {
        if self.0.request_exit().exit_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Some(model) = self.0.current_frame() else {
                return;
            };
            let decision = model.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            let update = self.0.submit_and_drive(
                EditorActionBatch::new(model.revision(), decision),
                Duration::ZERO,
            );
            if update.update().exit_ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            std::thread::panicking(),
            "the fixture did not retire its real Session"
        );
    }
}

fn controller() -> OwnedHierarchy {
    OwnedHierarchy::new(
        UiRootKey::runtime("editor.hierarchy", "owner-test").unwrap(),
        vec![nara_text::bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn environment(height: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(97).unwrap(),
        700.0,
        height,
        1.5,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn paint(controller: &mut OwnedHierarchy, model: &EditorFrameModel) -> Arc<UiFrame> {
    controller
        .compile(model, environment(800.0), UiTheme::default(), false)
        .unwrap()
}

fn button(frame: &Arc<UiFrame>, name: &str) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Button && node.name() == name)
        .unwrap_or_else(|| panic!("missing Hierarchy button: {name}"))
        .key()
        .clone()
}

fn input(frame: &Arc<UiFrame>) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::TextInput && node.name() == "Filter entities")
        .expect("the real panel-local text input is present")
        .key()
        .clone()
}

fn labels(frame: &Arc<UiFrame>) -> Vec<String> {
    frame
        .semantics()
        .iter()
        .map(|node| node.name().to_owned())
        .collect()
}

fn send(application: &mut EditorApplication, action: EditorAction) {
    let model = application.current_frame().unwrap();
    let result = application.submit_and_drive(
        EditorActionBatch::new(model.revision(), [action]),
        Duration::ZERO,
    );
    assert!(
        matches!(&result.actions()[0],
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied),
        "the real Session accepted the action: {:?}",
        result.actions()
    );
}

fn select(application: &mut EditorApplication, entity: SceneEntityId) {
    let model = application.current_frame().unwrap();
    send(
        application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: model.workspace().active_scene.as_ref().unwrap().document,
            entity: Some(entity),
        }),
    );
}

fn id(mut serial: u64) -> PanelSubmissionId {
    submission::enqueue(
        &mut serial,
        &mut Vec::new(),
        PanelOrigin {
            instance: crate::workspace::EditorPanelInstanceId(1),
            incarnation: Default::default(),
        },
        0,
        (),
    )
    .unwrap()
}

fn apply(
    controller: &mut OwnedHierarchy,
    application: &mut EditorApplication,
    prepared: HierarchySubmission,
) {
    let model = application.current_frame().unwrap();
    let action = prepared.into_action(&model, false).unwrap();
    let own = id(1);
    controller.admitted(Ok(own));
    let result = application.submit_and_drive(
        EditorActionBatch::new(model.revision(), [action]),
        Duration::ZERO,
    );
    let EditorApplicationActionDispatch::Dispatched(result) = &result.actions()[0] else {
        panic!("the Session did not dispatch the owner action")
    };
    assert!(
        matches!(result, EditorActionOutcome::Workspace(report) if report.applied),
        "{result:?}"
    );
    controller.action_finished(own, &PanelActionDispatch::Dispatched(result.clone()));
}

fn activate(
    controller: &mut OwnedHierarchy,
    frame: &Arc<UiFrame>,
    name: &str,
    model: &EditorFrameModel,
) -> Option<HierarchySubmission> {
    controller
        .activate(frame, &button(frame, name), model, false)
        .unwrap()
}

fn republish(
    model: &EditorFrameModel,
    workspace: nara::tooling::EditorWorkspaceModel,
) -> EditorFrameModel {
    EditorFrameClock::new().publish(
        workspace,
        model.project().clone(),
        Default::default(),
        EditorReopenTarget::Unavailable,
        None,
    )
}

struct RoutedInput {
    journal: UiFrameJournal,
    authority: nara_ui_kernel::__private::UiFrameJournalAuthority,
    session: UiInteractionSession,
    epoch: u64,
    presented: Option<(Arc<UiFrame>, UiPresentationReceipt)>,
}

impl RoutedInput {
    fn new(frame: &Arc<UiFrame>) -> Self {
        let (mut journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
        journal
            .install_surface(
                frame.environment().surface_id(),
                frame.environment().slot_generation().get(),
            )
            .unwrap();
        Self {
            journal,
            authority,
            session: UiInteractionSession::default(),
            epoch: 0,
            presented: None,
        }
    }

    fn route(
        &mut self,
        controller: &mut OwnedHierarchy,
        frame: &Arc<UiFrame>,
        event: UiInputEvent,
    ) -> UiIntentBatch {
        let receipt = if let Some((current, receipt)) = &self.presented
            && Arc::ptr_eq(current, frame)
        {
            *receipt
        } else {
            self.epoch += 1;
            let receipt = self
                .authority
                .submit(&mut self.journal, Arc::clone(frame), self.epoch)
                .unwrap();
            self.presented = Some((Arc::clone(frame), receipt));
            receipt
        };
        for attempt in 0..2 {
            let outcome = self
                .session
                .dispatch(
                    &self.journal,
                    UiRouteStamp::from_receipt(receipt),
                    event.clone(),
                )
                .unwrap();
            let batch = outcome.consumed().unwrap_or_else(|| {
                panic!(
                    "the hosted journal refused {event:?}: {:?}",
                    outcome.rejection()
                )
            });
            let intents = controller.dispatch(frame, batch).unwrap();
            if !batch.is_reconciliation() {
                return intents;
            }
            assert_eq!(attempt, 0);
            assert!(intents.intents().is_empty());
        }
        unreachable!()
    }

    fn focus(&mut self, controller: &mut OwnedHierarchy, frame: &Arc<UiFrame>, key: &UiKey) {
        assert!(
            self.route(
                controller,
                frame,
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target: key.clone(),
                }
            )
            .intents()
            .is_empty()
        );
    }

    fn click(
        &mut self,
        controller: &mut OwnedHierarchy,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> UiIntentBatch {
        self.focus(controller, frame, key);
        self.route(
            controller,
            frame,
            UiInputEvent::ActivateFocused {
                seat: UiSeatId::PRIMARY,
                source: UiActivationSource::Keyboard,
            },
        )
    }
}
