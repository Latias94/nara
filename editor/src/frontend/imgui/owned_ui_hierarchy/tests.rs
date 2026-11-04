use std::{
    fs,
    time::{Duration, Instant},
};

use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    scene::{PrefabEntityTarget, SceneEntityPlacement, ScenePlacementParent},
    tooling::{
        EditorActionBatch, EditorCloseDecision, EditorDocumentMutation, EditorFrameClock,
        EditorPersistenceAction, EditorReopenTarget, PrefabCompositionCommand,
    },
};
use nara_ui_kernel::{
    UiActivationSource, UiFrameJournal, UiInputEvent, UiInteractionSession, UiPresentationReceipt,
    UiRouteStamp, UiSeatId, UiSemanticRole, UiSurfaceId,
};
use serde_json::{Value, json};

use super::*;
use crate::frontend::imgui::{
    PanelActionNotDispatched,
    submission::{self, PanelOrigin, Recipient},
    ui_interaction_tests::{UiProject, setup_ui_prefab_source},
};
use crate::{EditorApplication, EditorApplicationActionDispatch, EditorLaunchOptions};

mod boundaries;
mod input;
mod projection;
mod workflows;

struct Fixture {
    application: EditorApplication,
    project: UiProject,
}

impl Fixture {
    fn new() -> Self {
        Self::open(UiProject::new())
    }

    fn open(project: UiProject) -> Self {
        let mut application = EditorApplication::open_trusted_compiled(
            EditorLaunchOptions::new(&project.0).with_profile("desktop"),
            TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap();
        application.native_shell_ready();
        setup_ui_prefab_source(&mut application);
        Self {
            application,
            project,
        }
    }

    fn model(&self) -> Arc<EditorFrameModel> {
        self.application.current_frame().unwrap()
    }
    fn ui(&self) -> Arc<EditorUiAuthoringFrame> {
        self.application.current_ui_authoring_frame().unwrap()
    }
    fn source(&self) -> Vec<u8> {
        fs::read(self.project.0.join("prefabs/ui.prefab.json")).unwrap()
    }
    fn history(&self) -> usize {
        self.model()
            .workspace()
            .document(self.ui().document())
            .unwrap()
            .history
            .undo_depth
    }

    fn send(&mut self, action: EditorAction) -> EditorActionOutcome {
        let model = self.model();
        let dispatch = self.application.submit_and_drive(
            EditorActionBatch::new(model.revision(), [action]),
            Duration::ZERO,
        );
        let EditorApplicationActionDispatch::Dispatched(outcome) = &dispatch.actions()[0] else {
            panic!("the Session did not dispatch the fixture action")
        };
        outcome.clone()
    }

    fn select(&mut self, entity: &str) {
        let outcome = self.send(EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: self.ui().document(),
                entity: Some(SceneEntityId::new(entity).unwrap()),
            },
        ));
        assert!(matches!(outcome, EditorActionOutcome::Workspace(report) if report.applied));
    }

    fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.application.submit_and_drive(
                EditorActionBatch::new(self.model().revision(), []),
                Duration::ZERO,
            );
            let model = self.model();
            if model.persistence().operation().is_none() && model.project_content().is_current() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the real Prefab persistence/content work did not settle");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.application.request_exit().exit_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Some(model) = self.application.current_frame() else {
                return;
            };
            let decision = model.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            if self
                .application
                .submit_and_drive(
                    EditorActionBatch::new(model.revision(), decision),
                    Duration::ZERO,
                )
                .update()
                .exit_ready()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            std::thread::panicking(),
            "the UI owner fixture did not retire its Session"
        );
    }
}

fn controller() -> OwnedUiHierarchy {
    OwnedUiHierarchy::new(
        UiRootKey::runtime("editor.hierarchy", "ui-owner-test").unwrap(),
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

fn paint(owner: &mut OwnedUiHierarchy, fixture: &Fixture, height: f32) -> Arc<UiFrame> {
    owner
        .compile(
            &fixture.model(),
            &fixture.ui(),
            environment(height),
            UiTheme::default(),
            false,
        )
        .unwrap()
}

fn button(frame: &UiFrame, name: &str) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Button && node.name() == name)
        .unwrap_or_else(|| panic!("missing UI Hierarchy button: {name}"))
        .key()
        .clone()
}

fn field(frame: &UiFrame, name: &str) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::TextInput && node.name() == name)
        .unwrap_or_else(|| panic!("missing UI Hierarchy text field: {name}"))
        .key()
        .clone()
}

fn enabled(frame: &UiFrame, name: &str) -> bool {
    let key = button(frame, name);
    frame
        .semantics()
        .iter()
        .find(|node| node.key() == &key)
        .unwrap()
        .enabled()
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
        Recipient::Hierarchy,
    )
    .unwrap()
}

fn click(
    owner: &mut OwnedUiHierarchy,
    routing: &mut RoutedInput,
    frame: &Arc<UiFrame>,
    name: &str,
    model: &EditorFrameModel,
    ui: &EditorUiAuthoringFrame,
) -> Option<UiHierarchySubmission> {
    let key = button(frame, name);
    let intents = routing.click(owner, frame, &key);
    assert_eq!(
        intents.intents().len(),
        1,
        "the real CPU journal admits {name}"
    );
    assert_eq!(intents.intents()[0].target(), &key);
    owner
        .activate(frame, intents.intents()[0].target(), model, ui, false)
        .unwrap()
}

fn click_current(
    owner: &mut OwnedUiHierarchy,
    routing: &mut RoutedInput,
    fixture: &Fixture,
    name: &str,
) -> Option<UiHierarchySubmission> {
    let frame = paint(owner, fixture, 800.0);
    click(
        owner,
        routing,
        &frame,
        name,
        &fixture.model(),
        &fixture.ui(),
    )
}

fn set_field(
    owner: &mut OwnedUiHierarchy,
    routing: &mut RoutedInput,
    frame: &Arc<UiFrame>,
    name: &str,
    value: &str,
) -> UiTextInputToken {
    let key = field(frame, name);
    routing.focus(owner, frame, &key);
    let token = owner.input_token(frame, &key).unwrap().unwrap();
    owner
        .edit(frame, &key, token, &TextEdit::SelectAll)
        .unwrap();
    owner
        .edit(frame, &key, token, &TextEdit::Insert(value.to_owned()))
        .unwrap();
    token
}

fn apply(
    owner: &mut OwnedUiHierarchy,
    fixture: &mut Fixture,
    prepared: UiHierarchySubmission,
    serial: u64,
) -> EditorDocumentMutation {
    let action = prepared
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    let EditorAction::Mutate(mutation) = action else {
        panic!("the form lowers to one nominal document mutation")
    };
    let own = id(serial);
    owner.admitted(Ok(own));
    let outcome = fixture.send(EditorAction::Mutate(mutation.clone()));
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
    owner.action_finished(own, &PanelActionDispatch::Dispatched(outcome));
    mutation
}

fn install_tree(project: &UiProject, ids: &[SceneEntityId], mixed: bool) {
    let source = project.0.join("prefabs/ui.prefab.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    let root = document["payload"]["entities"][0].clone();
    let leaf = document["payload"]["entities"][1].clone();
    let container = json!({"id": "template", "components": {"nara.ui.UiNode": root["components"]["nara.ui.UiNode"].clone()}});
    let mut entities = vec![root];
    let mut placements = vec![SceneEntityPlacement::root(
        SceneEntityId::new("ui-root").unwrap(),
    )];
    for (index, entity) in ids.iter().enumerate() {
        let mut record = if index == 0 {
            container.clone()
        } else {
            leaf.clone()
        };
        record["id"] = json!(entity.as_str());
        entities.push(record);
        let parent = if index == 1 {
            ids[0].clone()
        } else {
            SceneEntityId::new("ui-root").unwrap()
        };
        placements.push(SceneEntityPlacement::new(
            PrefabEntityTarget::source(entity.clone()),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                parent,
            ))),
        ));
    }
    if mixed {
        entities.push(json!({"id": "omitted-source-descendant", "components": {}}));
        placements.push(SceneEntityPlacement::new(
            PrefabEntityTarget::source(SceneEntityId::new("omitted-source-descendant").unwrap()),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                ids[0].clone(),
            ))),
        ));
    }
    document["payload"]["entities"] = json!(entities);
    document["payload"]["placements"] = serde_json::to_value(placements).unwrap();
    fs::write(source, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
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
        owner: &mut OwnedUiHierarchy,
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
            let batch = outcome
                .consumed()
                .unwrap_or_else(|| panic!("journal refused {event:?}: {:?}", outcome.rejection()));
            let intents = owner.dispatch(frame, batch).unwrap();
            if !batch.is_reconciliation() {
                return intents;
            }
            assert_eq!(attempt, 0);
            assert!(intents.intents().is_empty());
        }
        unreachable!()
    }

    fn focus(&mut self, owner: &mut OwnedUiHierarchy, frame: &Arc<UiFrame>, key: &UiKey) {
        assert!(
            self.route(
                owner,
                frame,
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target: key.clone()
                }
            )
            .intents()
            .is_empty()
        );
    }

    fn click(
        &mut self,
        owner: &mut OwnedUiHierarchy,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> UiIntentBatch {
        self.focus(owner, frame, key);
        self.route(
            owner,
            frame,
            UiInputEvent::ActivateFocused {
                seat: UiSeatId::PRIMARY,
                source: UiActivationSource::Keyboard,
            },
        )
    }
}
