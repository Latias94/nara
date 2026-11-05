use std::{
    fs,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePlacement,
        ScenePlacementParent,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentMutation, EditorFrameModel,
        EditorNavigationAction, SceneCompositionCommand,
    },
};
use nara_ui_kernel::{
    UiActivationSource, UiInputEvent, UiInteractionSession, UiRouteStamp, UiSeatId, UiSemanticRole,
    UiSurfaceId,
};

use super::super::{DrawnPanel, PanelControls, input::clipboard_edit, text_controls::TextControls};
use super::*;
use crate::{
    EditorApplication, EditorApplicationActionDispatch, EditorLaunchOptions, EditorPanelHost,
    EditorPanelSubmission,
    app::PersistenceFixture,
    native::{EditorClipboard, EditorClipboardAccess},
    panels::PanelActionNotDispatched,
};

const COMPONENT: &str = "test.drawn.Refusals";
const ENTITY: &str = "editable";
const SOURCE: &str = "worlds/entry.scene.json";

struct Fixture {
    host: EditorPanelHost<DrawnPanel>,
    application: EditorApplication,
    project: PersistenceFixture,
}

impl Fixture {
    fn new() -> Self {
        let project = PersistenceFixture::authored_upgrade();
        let schema = ComponentSchema::new(
            ComponentTypeId::new(COMPONENT),
            "Drawn refusals",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("label"),
                "Label",
                ComponentFieldPath::from_fields(["label"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::from_fields(["value"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
        .with_authoring_initial_value(ComponentValue::map([
            ("label", ComponentValue::String("before".into())),
            ("value", ComponentValue::U64(10)),
        ]));
        let owner = PersistentComponentSchemaOwnerId::try_new("test.drawn.components").unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        fs::write(
            project
                .project_root()
                .join("catalogs/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new(ENTITY).unwrap()).with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        ("label", ComponentValue::String("before".into())),
                        ("value", ComponentValue::U64(10)),
                    ]),
                )
                .with_owner(owner),
            ),
            SceneEntityRecord::new(SceneEntityId::new("other").unwrap()),
        ]);
        fs::write(
            project.project_root().join(SOURCE),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        let mut application = EditorApplication::new(
            EditorLaunchOptions::new(project.project_root()).with_profile("desktop"),
        );
        application.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while application.current_frame().is_none() {
            assert!(
                Instant::now() < deadline,
                "the original Session did not open"
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        let model = application.current_frame().unwrap();
        let document = model.workspace().active_scene.as_ref().unwrap().document;
        application.submit_and_drive(
            EditorActionBatch::new(
                model.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: Some(SceneEntityId::new(ENTITY).unwrap()),
                    },
                )],
            ),
            Duration::ZERO,
        );
        let mut host = EditorPanelHost::new();
        host.install_registered_panel(&application, "nara.inspector", || {
            DrawnPanel::inspector(
                UiRootKey::runtime("drawn.refusals", "inspector").unwrap(),
                vec![nara_text::bundled_regular_font_face().unwrap()],
            )
            .unwrap()
        })
        .unwrap();
        Self {
            host,
            application,
            project,
        }
    }

    fn visit<R>(
        &mut self,
        callback: impl FnOnce(&mut InspectorControls, &mut EditorPanelContext<'_, '_>) -> R,
    ) -> (R, Option<EditorPanelSubmission>) {
        let mut callback = Some(callback);
        let mut result = None;
        let submission = self.host.prepare(&self.application, |panel, context| {
            let PanelControls::Inspector(owner) = &mut panel.0 else {
                panic!("unexpected panel")
            };
            result = Some(callback.take().unwrap()(owner, context));
        });
        (
            result.expect("the original Inspector instance was visited"),
            submission.filter(EditorPanelSubmission::has_submissions_for_test),
        )
    }

    fn compile(&mut self) -> Arc<UiFrame> {
        let (frame, submission) =
            self.visit(|owner, context| owner.compile(context, environment()).unwrap());
        assert!(submission.is_none());
        frame
    }

    fn activate(&mut self, frame: &Arc<UiFrame>, label: &str) -> Option<EditorPanelSubmission> {
        let target = frame
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::Button && node.name() == label)
            .unwrap()
            .key()
            .clone();
        let (mut journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
        journal
            .install_surface(frame.environment().surface_id(), 1)
            .unwrap();
        let receipt = authority
            .submit(&mut journal, Arc::clone(frame), 1)
            .unwrap();
        let mut interaction = UiInteractionSession::default();
        let focused = interaction
            .dispatch(
                &journal,
                UiRouteStamp::from_receipt(receipt),
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target,
                },
            )
            .unwrap();
        let activated = interaction
            .dispatch(
                &journal,
                UiRouteStamp::from_receipt(receipt),
                UiInputEvent::ActivateFocused {
                    seat: UiSeatId::PRIMARY,
                    source: UiActivationSource::Keyboard,
                },
            )
            .unwrap();
        self.visit(|owner, context| {
            owner
                .reduce(frame, focused.consumed().unwrap(), context)
                .unwrap();
            owner
                .reduce(frame, activated.consumed().unwrap(), context)
                .unwrap();
        })
        .1
    }

    fn source(&self) -> Vec<u8> {
        fs::read(self.project.project_root().join(SOURCE)).unwrap()
    }
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 600.0, 2000.0, 1.0, 1, 1, 1, 1).unwrap()
}

fn input(frame: &Arc<UiFrame>, label: &str) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::TextInput && node.name() == label)
        .unwrap()
        .key()
        .clone()
}

fn has_text(frame: &UiFrame, needle: &str) -> bool {
    frame
        .semantics()
        .iter()
        .any(|node| node.name().contains(needle))
}

fn component_action(model: &EditorFrameModel, command: SceneCompositionCommand) -> EditorAction {
    let scene = model.workspace().active_scene.as_ref().unwrap();
    EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
        document: scene.document,
        base_revision: scene.revision,
        command,
    })
}

#[test]
fn unchanged_enter_and_escape_keep_the_original_field_and_source() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let field = input(&frame, "label");
    let source = fixture.source();
    let revision = fixture.application.current_frame().unwrap().revision();
    let ((key, token), _) = fixture.visit(|owner, _| {
        let InspectorBinding::Field(key) = owner.inspector.binding(&frame, &field).unwrap() else {
            panic!("expected field")
        };
        (key, owner.inspector.focus(&frame, &field).unwrap())
    });
    for discard in [false, true] {
        let (_, submission) = fixture.visit(|owner, context| {
            owner
                .finish_input(&frame, &field, discard, context)
                .unwrap();
            let draft = &owner.fields.drafts[&key];
            assert_eq!(draft.candidate.copy_text(), "before");
            assert!(!draft.dirty && !draft.queued && draft.pending.is_none());
            assert_eq!(owner.input_token(&frame, &field).unwrap(), Some(token));
        });
        assert!(submission.is_none());
    }
    assert_eq!(
        fixture.application.current_frame().unwrap().revision(),
        revision
    );
    assert_eq!(fixture.source(), source);
}

#[test]
fn invalid_numeric_enter_retains_the_draft_and_can_be_corrected_and_applied() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let field = input(&frame, "value");
    let source = fixture.source();
    let ((key, token), _) = fixture.visit(|owner, _| {
        let InspectorBinding::Field(key) = owner.inspector.binding(&frame, &field).unwrap() else {
            panic!("expected field")
        };
        let token = owner.inspector.focus(&frame, &field).unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::SelectAll)
            .unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::Insert("-".into()))
            .unwrap();
        (key, token)
    });
    let frame = fixture.compile();
    let (_, submission) = fixture.visit(|owner, context| {
        owner.finish_input(&frame, &field, false, context).unwrap();
        let draft = &owner.fields.drafts[&key];
        assert_eq!(draft.candidate.copy_text(), "-");
        assert!(draft.dirty && !draft.queued && draft.pending.is_none());
        assert!(draft.error.as_ref().unwrap().contains("invalid"));
        assert_eq!(owner.input_token(&frame, &field).unwrap(), Some(token));
        owner
            .edit(&frame, &field, token, &TextEdit::SelectAll)
            .unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::Insert("17".into()))
            .unwrap();
    });
    assert!(submission.is_none());
    let frame = fixture.compile();
    let (_, submission) =
        fixture.visit(|owner, context| owner.finish_input(&frame, &field, false, context).unwrap());
    fixture.host.submit_and_drive(
        &mut fixture.application,
        submission.unwrap(),
        Duration::ZERO,
    );
    let frame = fixture.compile();
    assert_eq!(
        frame
            .semantics()
            .iter()
            .find(|node| node.name() == "value" && node.role() == UiSemanticRole::TextInput)
            .unwrap()
            .value(),
        Some("17")
    );
    assert_eq!(fixture.source(), source);
}

struct RefusingClipboard;

impl EditorClipboardAccess for RefusingClipboard {
    fn read(&mut self) -> Result<String, String> {
        Err("provider refused read".into())
    }
    fn write(&mut self, _: &str) -> Result<(), String> {
        Err("provider refused write".into())
    }
}

#[test]
fn clipboard_denial_keeps_text_selection_and_focus_and_allows_retry() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let field = input(&frame, "label");
    let source = fixture.source();
    let clipboard = EditorClipboard::with_access(RefusingClipboard);
    let (_, submission) = fixture.visit(|owner, _| {
        let InspectorBinding::Field(key) = owner.inspector.binding(&frame, &field).unwrap() else {
            panic!("expected field")
        };
        let token = owner.inspector.focus(&frame, &field).unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::SelectAll)
            .unwrap();
        for shortcut in ["v", "c", "x"] {
            let edit = clipboard_edit(
                &mut TextControls::Inspector(owner),
                &frame,
                &field,
                token,
                shortcut,
                || clipboard.read(),
                |text| clipboard.write(text),
            )
            .unwrap();
            assert!(edit.is_none());
            assert_eq!(
                owner
                    .selected_text(&frame, &field, token)
                    .unwrap()
                    .as_deref(),
                Some("before")
            );
            assert_eq!(owner.input_token(&frame, &field).unwrap(), Some(token));
            let draft = &owner.fields.drafts[&key];
            assert_eq!(draft.candidate.copy_text(), "before");
            assert!(!draft.dirty && draft.pending.is_none());
            assert!(draft.error.as_ref().unwrap().contains("provider refused"));
        }
        let clipboard = EditorClipboard::memory("replacement");
        let edit = clipboard_edit(
            &mut TextControls::Inspector(owner),
            &frame,
            &field,
            token,
            "v",
            || clipboard.read(),
            |text| clipboard.write(text),
        )
        .unwrap()
        .unwrap();
        owner.edit(&frame, &field, token, &edit).unwrap();
        assert_eq!(
            owner.fields.drafts[&key].candidate.copy_text(),
            "replacement"
        );
        owner.edit(&frame, &field, token, &TextEdit::Undo).unwrap();
        assert_eq!(owner.fields.drafts[&key].candidate.copy_text(), "before");
    });
    assert!(submission.is_none());
    assert_eq!(fixture.source(), source);
}

#[test]
fn observed_component_actions_require_the_original_live_behavior_owner() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let menu = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Button && node.name() == "Add Component")
        .unwrap();
    let removal = frame
        .semantics()
        .iter()
        .find(|node| {
            node.role() == UiSemanticRole::Button && node.name() == format!("Remove {COMPONENT}")
        })
        .unwrap();
    let menu_key = menu.key().clone();
    let menu_action = menu.actions()[0].clone();
    let removal_key = removal.key().clone();
    let removal_action = removal.actions()[0].clone();
    let (_, submission) = fixture.visit(|owner, context| {
        assert!(owner.owns_action(&frame, &menu_key, &menu_action));
        assert!(owner.owns_action(&frame, &removal_key, &removal_action));
        assert!(!owner.owns_action(&frame, &menu_key, &removal_action));
        assert!(!owner.owns_action(&frame, &input(&frame, "label"), &menu_action));
        assert!(
            owner
                .inspector
                .activate_component(&frame, &menu_key, context.model().frame())
                .unwrap()
                .is_none()
        );
        assert!(!owner.owns_action(&frame, &menu_key, &menu_action));
        assert!(!owner.owns_action(&frame, &removal_key, &removal_action));
    });
    assert!(submission.is_none());
    let current = fixture.compile();
    let (_, submission) = fixture.visit(|owner, _| {
        assert!(owner.owns_action(&current, &menu_key, &menu_action));
        assert!(owner.owns_action(&current, &removal_key, &removal_action));
        assert!(!owner.owns_action(&frame, &menu_key, &menu_action));
    });
    assert!(submission.is_none());
}

#[test]
fn stale_component_activation_reports_locally_without_submitting() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let model = fixture.application.current_frame().unwrap();
    let result = fixture.application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [component_action(
                &model,
                SceneCompositionCommand::MoveEntity {
                    target: nara::scene::PrefabEntityTarget::source(
                        SceneEntityId::new(ENTITY).unwrap(),
                    ),
                    to: ScenePlacement::last(ScenePlacementParent::Root),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(
        matches!(&result.actions()[0], EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
    );
    let revision = fixture.application.current_frame().unwrap().revision();
    assert!(
        fixture
            .activate(&frame, &format!("Remove {COMPONENT}"))
            .is_none()
    );
    let current = fixture.compile();
    assert!(has_text(&current, "Action was not applied"));
    assert_eq!(
        fixture.application.current_frame().unwrap().revision(),
        revision
    );
    let (_, submission) = fixture.visit(|owner, _| {
        assert!(
            owner
                .activation_refused(&current, None, OwnedInspectorError::ForeignDispatch)
                .is_err()
        );
        assert!(
            owner
                .activation_refused(&current, None, OwnedInspectorError::UnknownControl)
                .is_err()
        );
    });
    assert!(submission.is_none());
}

#[test]
fn component_supersession_is_reported_and_late_submission_cannot_mutate() {
    let mut fixture = Fixture::new();
    let frame = fixture.compile();
    let revision = fixture.application.current_frame().unwrap().revision();
    let candidate = fixture
        .activate(&frame, &format!("Remove {COMPONENT}"))
        .unwrap();
    let current = fixture.compile();
    assert!(has_text(
        &current,
        "Component action was not dispatched: Superseded"
    ));
    let (pending, _) = fixture.visit(|owner, _| owner.component_pending.len());
    assert_eq!(pending, 0);
    fixture
        .host
        .submit_and_drive(&mut fixture.application, candidate, Duration::ZERO);
    assert_eq!(
        fixture.application.current_frame().unwrap().revision(),
        revision
    );
    let current = fixture.compile();
    let candidate = fixture
        .activate(&current, &format!("Remove {COMPONENT}"))
        .unwrap();
    fixture
        .host
        .submit_and_drive(&mut fixture.application, candidate, Duration::ZERO);
    let current = fixture.compile();
    assert!(
        !current
            .semantics()
            .iter()
            .any(|node| node.name() == format!("Remove {COMPONENT}"))
    );
    assert!(!has_text(&current, "Component action was not applied"));
    let (pending, _) = fixture.visit(|owner, _| owner.component_pending.len());
    assert_eq!(pending, 0);
}

#[test]
fn component_failed_application_is_correlated_to_its_exact_terminal_id() {
    let mut fixture = Fixture::new();
    fixture.compile();
    let revision = fixture
        .application
        .current_frame()
        .unwrap()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .revision;
    let source = fixture.source();
    let (tracked, candidate) = fixture.visit(|owner, context| {
        let command = SceneCompositionCommand::AddComponent {
            entity: SceneEntityId::new(ENTITY).unwrap(),
            component: ComponentTypeId::new(COMPONENT),
        };
        let unrelated_action = component_action(context.model().frame(), command.clone());
        let tracked_action = component_action(context.model().frame(), command);
        let unrelated = context.submit(unrelated_action).unwrap();
        let tracked = context.submit(tracked_action).unwrap();
        owner.component_pending.insert(tracked);
        owner.action_finished(
            unrelated,
            &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
        );
        assert!(owner.component_pending.contains(&tracked));
        let current = owner.compile(context, environment()).unwrap();
        assert!(!has_text(&current, "Component action was not dispatched"));
        tracked
    });
    fixture
        .host
        .submit_and_drive(&mut fixture.application, candidate.unwrap(), Duration::ZERO);
    let (current, _) = fixture.visit(|owner, context| {
        assert!(!owner.component_pending.contains(&tracked));
        assert!(has_text(
            &owner.compile(context, environment()).unwrap(),
            "Component action was not applied"
        ));
        owner.action_finished(
            tracked,
            &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
        );
        owner.compile(context, environment()).unwrap()
    });
    assert!(!has_text(
        &current,
        "Component action was not dispatched: Closing"
    ));
    assert_eq!(
        fixture
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        revision
    );
    assert_eq!(fixture.source(), source);
}
