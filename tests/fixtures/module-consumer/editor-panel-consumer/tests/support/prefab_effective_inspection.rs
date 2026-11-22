use super::*;

use nara::{
    asset::{AssetRef, AssetSourcePath},
    core::{ByteLimit, DepthLimit, ItemLimit},
    reflect::{ComponentPropertyReadBudget, ComponentValueMaterializationLimits},
    scene::{
        AuthoringPrefabInstanceSnapshot, AuthoringPrefabPropertyClaimKind,
        AuthoringPrefabPropertyValue, AuthoringPrefabRecordOrigin, PrefabComponentIntent,
        PrefabDeclarationLimits, PrefabDeclarationSource, PrefabDocumentCandidate,
        PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
        PrefabInstancePath, PrefabOverlay, PrefabOverlaySlot, PrefabPropertyClaim,
        PrefabPropertyIntent, SceneDocumentCandidate, SourceDocumentKey, SourceDocumentKind,
    },
};
use nara_editor::{EditorPanel, EditorPanelContext, EditorPanelHost, workspace::EditorPanelTarget};
use nara_tooling::{
    EditorPersistenceView, EditorPrefabInspectionAction as Action, EditorPrefabInspectionId,
    EditorPrefabInspectionOutcome as Outcome, EditorPrefabInspectionPhase as Phase,
    EditorPrefabInspectionRejection as Rejection, EditorPrefabInspectionResult,
    EditorPrefabPropertyAddress,
};

#[path = "prefab_effective_inspection/absent.rs"]
mod absent;
#[path = "prefab_effective_inspection/apply.rs"]
mod apply;
#[path = "prefab_effective_inspection/recursive.rs"]
mod recursive;
#[path = "prefab_effective_inspection/reference.rs"]
mod reference;
#[path = "prefab_effective_inspection/set.rs"]
mod set;

const PANEL: &str = "consumer.prefab-effective-inspection";
const COMPONENT: &str = "consumer.prefab.EffectiveSettings";
const ANCHOR_COMPONENT: &str = "consumer.prefab.AnchorLabel";
const STARTUP: &str = "scenes/startup.scene.json";
const SCENE: &str = "scenes/effective-inspection.scene.json";
const PREFAB: &str = "prefabs/effective-inspection.prefab.json";
const BASE: &str = "prefabs/inspection-base.prefab.json";
const UNRELATED: &str = "prefabs/unrelated-invalid.prefab.json";
const INITIAL_SCORE: u64 = 20;
const CHANGED_SCORE: u64 = 63;
const SIBLING_NOTE_BYTES: usize = 140_000;

fn schema() -> AuthoringSchemaBundle {
    let settings = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Effective settings",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("score"),
            "Score",
            ComponentFieldPath::from_fields(["wire_score"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["wire_note"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ]);
    let anchor_label = ComponentSchema::new(
        ComponentTypeId::new(ANCHOR_COMPONENT),
        "Anchor label",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("text"),
        "Text",
        ComponentFieldPath::from_fields(["wire_text"]),
        ComponentValueKind::String,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![settings, anchor_label],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn property(field: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(field))
}

fn base_key() -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
        AssetSourcePath::new(BASE).unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn base(score: u64) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("wire_score", ComponentValue::U64(score)),
                ("wire_note", ComponentValue::String("base note".into())),
            ]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )])
}

fn address(anchor: &str, field: &str) -> EditorPrefabPropertyAddress {
    EditorPrefabPropertyAddress {
        entity: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: base_key(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("base-root")),
        },
        component: ComponentTypeId::new(COMPONENT),
        property: property(field),
    }
}

fn mounts() -> (PrefabInstance, PrefabInstance, PrefabInstance) {
    let source = AssetRef::path("inspection-base.prefab.json").unwrap();
    let key = base_key();
    let root = PrefabEntityTarget::source(id("base-root"));
    let mut left = PrefabInstance::root(source.clone(), key.clone(), root.clone());
    let right = PrefabInstance::forest(source);
    let note = PrefabPropertyClaim {
        path: property("note"),
        intent: PrefabPropertyIntent::Set(ComponentValue::String("n".repeat(SIBLING_NOTE_BYTES))),
    };
    let overlay = |claims| {
        let mut overlay = PrefabOverlay::new(key.clone());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: root.clone(),
            component: ComponentTypeId::new(COMPONENT),
            version: ComponentSchemaVersion::ONE,
            claims,
        });
        Some(overlay)
    };
    let mut reverted = left.clone();
    reverted.overrides = overlay(vec![note.clone()]);
    left.overrides = overlay(vec![
        PrefabPropertyClaim {
            path: property("score"),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(INITIAL_SCORE)),
        },
        note,
    ]);
    (left, right, reverted)
}

fn containing_source(left: &PrefabInstance, right: &PrefabInstance) -> SceneDocument {
    let mut left_anchor = SceneEntityRecord::new(id("left")).with_component(
        ComponentTypeId::new(ANCHOR_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("wire_text", ComponentValue::String("left anchor".into()))]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    );
    left_anchor.prefab = Some(left.clone());
    let mut right_anchor = SceneEntityRecord::new(id("right"));
    right_anchor.prefab = Some(right.clone());
    let mut unrelated = SceneEntityRecord::new(id("unrelated"));
    unrelated.prefab = Some(PrefabInstance::forest(
        AssetRef::path("unrelated-invalid.prefab.json").unwrap(),
    ));
    SceneDocument::from_ordered_roots([left_anchor, right_anchor, unrelated])
}

struct InspectionPanel {
    completion: Rc<RefCell<Option<(PanelSubmissionId, PanelActionDispatch)>>>,
}

impl EditorPanel for InspectionPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        assert!(self.completion.borrow_mut().replace((id, result)).is_none());
    }
}

struct PanelDriver {
    host: EditorPanelHost<InspectionPanel>,
    completion: Rc<RefCell<Option<(PanelSubmissionId, PanelActionDispatch)>>>,
}

impl PanelDriver {
    fn register(app: &mut EditorApplication) -> Self {
        let completion = Rc::new(RefCell::new(None));
        let shared = Rc::clone(&completion);
        let mut host = EditorPanelHost::new();
        host.register_panel(
            app,
            EditorPanelDescriptor::new(
                PANEL,
                "Effective Prefab inspection",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Floating,
                true,
            ),
            move || InspectionPanel {
                completion: Rc::clone(&shared),
            },
        )
        .unwrap();
        let panel = app
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == PANEL)
            .unwrap()
            .id();
        assert!(
            app.panel_workspace_mut()
                .layout_mut()
                .set_target(panel, EditorPanelTarget::Workspace)
        );
        Self { host, completion }
    }

    fn dispatch(
        &mut self,
        app: &mut EditorApplication,
        action: impl FnOnce(&EditorPanelContext<'_, '_>) -> Action,
    ) -> Outcome {
        let mut action = Some(action);
        let mut submitted = None;
        let candidate = self
            .host
            .prepare(app, |_, context| {
                assert!(context.model().document().is_none());
                let action = action.take().unwrap()(context);
                submitted = Some(
                    context
                        .submit(EditorAction::PrefabInspection(action))
                        .unwrap(),
                );
            })
            .unwrap();
        let submitted = submitted.expect("the registered workspace panel must submit once");
        assert_eq!(candidate.submissions().len(), 1);
        self.host.submit_and_drive(app, candidate, Duration::ZERO);
        let completion = self.completion.borrow_mut().take();
        let Some((
            finished,
            PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabInspection(outcome)),
        )) = completion
        else {
            panic!("Prefab inspection lost its original panel result: {completion:?}");
        };
        assert_eq!(finished, submitted);
        outcome
    }

    fn observe<R>(
        &mut self,
        app: &mut EditorApplication,
        read: impl FnOnce(&EditorPanelContext<'_, '_>) -> R,
    ) -> R {
        let mut read = Some(read);
        let mut observed = None;
        let candidate = self
            .host
            .prepare(app, |_, context| {
                assert!(context.model().document().is_none());
                observed = Some(read.take().unwrap()(context));
            })
            .unwrap();
        assert_eq!(candidate.submissions().len(), 0);
        self.host.submit_and_drive(app, candidate, Duration::ZERO);
        observed.expect("the registered workspace panel must observe once")
    }
}

fn open_source(app: &mut EditorApplication, source: &str) -> EditorDocumentId {
    let opening = submit(
        app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: source.into(),
        }),
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Preparing { request },
        )),
    ] = opening.actions()
    else {
        panic!("Browser Open refused the effective inspection fixture: {opening:?}");
    };
    wait(app, |current| current.document_open().result().is_some());
    let result = frame(app).document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        panic!("Browser Open did not publish the effective inspection fixture: {result:?}");
    };
    assert_eq!(completed, *request);
    let acknowledged = submit(
        app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
            request: *request,
        }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: actual }
        ))
    ] if actual == request));
    document
}

fn persist(
    app: &mut EditorApplication,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    let started = submit(app, EditorAction::Persistence(action));
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { attempt },
        )),
    ] = started.actions()
    else {
        panic!("persistence request was not accepted: {started:?}");
    };
    settle_persistence(app);
    let EditorPersistenceView::Terminal {
        attempt: completed,
        result,
    } = frame(app).persistence()
    else {
        panic!("the accepted persistence attempt must publish its result");
    };
    assert_eq!(completed, *attempt);
    let acknowledged = submit(
        app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt: *attempt }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt: actual }
        ))
    ] if actual == attempt));
    result
}

fn assert_score(
    snapshot: &AuthoringPrefabInstanceSnapshot,
    anchor: &str,
    expected_score: u64,
    pinned: bool,
) -> EditorPrefabPropertyAddress {
    let expected = address(anchor, "score");
    let entity = snapshot
        .targets()
        .find(|target| *target == &expected.entity)
        .expect("discovery must retain the containing-source route through this mount");
    let other = if anchor == "left" { "right" } else { "left" };
    assert!(snapshot.target(&address(other, "score").entity).is_none());
    let target = snapshot.target(entity).unwrap();
    let anchor_label = target.component(&ComponentTypeId::new(ANCHOR_COMPONENT));
    if anchor == "left" {
        assert_eq!(
            anchor_label.unwrap().fact().record(),
            &AuthoringPrefabRecordOrigin::SourceEntity {
                source: PrefabDeclarationSource::Containing,
                entity: id("left"),
            }
        );
    } else {
        assert!(anchor_label.is_err());
    }

    let component = target.component(&expected.component).unwrap();
    let field = component
        .root_fields()
        .unwrap()
        .iter()
        .find(|field| field.id() == &expected.property.root)
        .expect("the admitted component must expose the stable score root");
    let selected = EditorPrefabPropertyAddress {
        entity: entity.clone(),
        component: component.fact().component().clone(),
        property: ComponentPropertyPath::new(field.id().clone()),
    };
    let mut budget =
        ComponentPropertyReadBudget::new(ItemLimit::new(8).unwrap(), ByteLimit::new(1024).unwrap());
    let observation = component
        .property_value(&selected.property, &mut budget)
        .unwrap();
    assert_eq!(
        observation.record,
        &AuthoringPrefabRecordOrigin::SourceEntity {
            source: PrefabDeclarationSource::Prefab(base_key()),
            entity: id("base-root"),
        }
    );
    let AuthoringPrefabPropertyValue::Present(value) = observation.value else {
        panic!("the selected score must be present: {observation:?}");
    };
    let limits = ComponentValueMaterializationLimits::new(
        ItemLimit::ONE,
        ByteLimit::new(128).unwrap(),
        DepthLimit::ONE,
    );
    assert_eq!(
        value.copy_value(limits).unwrap(),
        ComponentValue::U64(expected_score)
    );

    if pinned {
        let claim = observation
            .controlling_claim()
            .expect("the equal-value pin is authored");
        assert_eq!(claim.path, selected.property);
        assert_eq!(
            claim.location.source(),
            &PrefabDeclarationSource::Containing
        );
        assert_eq!(claim.location.path(), &PrefabInstancePath::new(id(anchor)));
        assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Set);
        assert_eq!(
            claim.slot,
            PrefabOverlaySlot::Property {
                component_index: 0,
                claim_index: 0,
            }
        );
        assert_eq!(observation.revert_claim(), Some(claim));
    } else {
        assert!(observation.controlling_claim().is_none());
        assert!(observation.revert_claim().is_none());
    }
    selected
}

fn expect_phase(app: &mut EditorApplication, inspection: EditorPrefabInspectionId, phase: Phase) {
    wait(app, |current| {
        let view = current.prefab_inspection();
        assert_eq!(view.inspection, Some(inspection));
        view.phase == phase || view.phase == Phase::Terminal
    });
    let current = frame(app);
    let view = current.prefab_inspection();
    assert_eq!(view.phase, phase, "{view:?}");
    assert_eq!(view.result.is_some(), phase == Phase::Terminal);
}

fn start(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    document: EditorDocumentId,
    anchor: &str,
) -> EditorPrefabInspectionId {
    let outcome = panel.dispatch(app, |context| Action::Inspect {
        basis: context.document_query_basis(document).unwrap(),
        anchor: id(anchor),
    });
    let Outcome::Accepted {
        inspection,
        phase: Phase::Inspecting,
    } = outcome
    else {
        panic!("the named Prefab inspection must start: {outcome:?}");
    };
    inspection
}

fn inspect_score(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    document: EditorDocumentId,
    anchor: &str,
    score: u64,
    pinned: bool,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    let inspection = start(panel, app, document, anchor);
    expect_phase(app, inspection, Phase::Ready);
    let selected = panel.observe(app, |context| {
        let observation = context.prefab_effective_observation(inspection).unwrap();
        assert_eq!(
            Some(observation.schema_basis()),
            context
                .document_query_basis(document)
                .unwrap()
                .schema
                .as_ref()
        );
        assert_score(observation, anchor, score, pinned)
    });
    (inspection, selected)
}

fn acknowledge(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
) {
    assert_eq!(
        panel.dispatch(app, |_| Action::Acknowledge { inspection }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Idle,
        }
    );
    assert_eq!(frame(app).prefab_inspection(), &Default::default());
}

fn cancel(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
) {
    assert_eq!(
        panel.dispatch(app, |_| Action::Cancel { inspection }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Terminal,
        }
    );
    expect_phase(app, inspection, Phase::Terminal);
    assert_eq!(
        frame(app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Cancelled { inspection })
    );
    acknowledge(panel, app, inspection);
}

fn assert_failure(
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    expected_reason: Rejection,
    diagnostic_code: &str,
) {
    expect_phase(app, inspection, Phase::Terminal);
    let current = frame(app);
    let result = &current.prefab_inspection().result;
    let Some(EditorPrefabInspectionResult::Failed {
        inspection: actual,
        reason,
        diagnostics,
    }) = result
    else {
        panic!("the selected inspection must fail: {result:?}");
    };
    assert_eq!(*actual, inspection);
    assert_eq!(*reason, expected_reason, "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == diagnostic_code),
        "{diagnostics:?}"
    );
}

fn prepare_revert(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    address: EditorPrefabPropertyAddress,
    limits: PrefabDeclarationLimits,
) {
    assert_eq!(
        panel.dispatch(app, |_| Action::PrepareRevert {
            inspection,
            address,
            limits,
        }),
        Outcome::Accepted {
            inspection,
            phase: Phase::PreparingEdit,
        }
    );
}

fn assert_review(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    expected_patch: &ScenePatchDocument,
) {
    expect_phase(app, inspection, Phase::Review);
    panel.observe(app, |context| {
        assert_eq!(
            context
                .prefab_inspection_review(inspection)
                .unwrap()
                .patch(),
            expected_patch
        );
    });
}

fn replay(app: &mut EditorApplication, document: EditorDocumentId, redo: bool) {
    let base_revision = basis(app, document).revision;
    let mutation = if redo {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    };
    let result = submit(app, EditorAction::Mutate(mutation));
    assert!(
        matches!(result.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
    ] if report.applied && report.document == Some(document)),
        "{result:?}"
    );
}

fn save(app: &mut EditorApplication, document: EditorDocumentId) {
    let base_revision = basis(app, document).revision;
    let result = persist(
        app,
        EditorPersistenceAction::Save {
            document,
            base_revision,
        },
    );
    assert!(
        matches!(result, EditorPersistenceResult::Saved {
        document: actual, revision, ..
    } if actual == document && revision == base_revision),
        "{result:?}"
    );
}

fn assert_saved_source(kind: EditorDocumentKind, bytes: &[u8], expected: &SceneDocument) {
    match kind {
        EditorDocumentKind::Scene => assert_eq!(
            &SceneDocumentCandidate::decode_json_bytes(bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            expected,
        ),
        EditorDocumentKind::Prefab => assert_eq!(
            PrefabDocumentCandidate::decode_json_bytes(bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            PrefabDocument::new(expected.placements.clone(), expected.entities.clone()),
        ),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
}

#[test]
fn a_registered_panel_inspects_and_reverts_an_effective_prefab_on_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_inspects_and_reverts_an_effective_prefab_on_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let (left, right, reverted) = mounts();
    let original_document = containing_source(&left, &right);
    let reverted_document = containing_source(&reverted, &right);
    let original = match kind {
        EditorDocumentKind::Scene => original_document.to_json_string(),
        EditorDocumentKind::Prefab => PrefabDocument::new(
            original_document.placements.clone(),
            original_document.entities.clone(),
        )
        .to_json_string(),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    let initial_base = base(INITIAL_SCORE).to_json_string().unwrap();
    let changed_base = base(CHANGED_SCORE).to_json_string().unwrap();
    assert_eq!(initial_base.len(), changed_base.len());
    assert_ne!(initial_base, changed_base);
    let unrelated =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-root"))])
            .to_json_string()
            .unwrap();
    let source_file = project.root.join(source);
    let base_file = project.root.join(BASE);
    let unrelated_file = project.root.join(UNRELATED);
    fs::write(&source_file, &original).unwrap();
    fs::write(&base_file, &initial_base).unwrap();
    fs::write(&unrelated_file, &unrelated).unwrap();
    fs::write(project.root.join(STARTUP), &startup).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schema().to_json_string().unwrap(),
    )
    .unwrap();

    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |current| {
        !current.project_browser().has_pending_work()
    });
    let active = frame(&app).workspace().active_document.unwrap();
    let document = open_source(&mut app, source);
    assert_ne!(document, active);
    for action in [
        EditorNavigationAction::SetActiveDocument { document: active },
        EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("active")),
        },
    ] {
        assert!(
            matches!(submit(&mut app, EditorAction::Navigate(action)).actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
        ] if report.applied)
        );
    }
    let initial = frame(&app);
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, kind);
    assert!(initial_tab.source_binding.is_some());
    let captured = basis(&app, document);
    let assert_navigation = |app: &EditorApplication| {
        let current = frame(app);
        let workspace = current.workspace();
        assert_eq!(workspace.active_document, Some(active));
        assert_eq!(workspace.active_scene, initial.workspace().active_scene);
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(
            workspace.documents.len(),
            initial.workspace().documents.len()
        );
        assert_eq!(
            workspace.document(active),
            initial.workspace().document(active)
        );
        assert_eq!(
            workspace.document(document).unwrap().source_binding,
            initial_tab.source_binding
        );
    };
    let assert_history = |app: &EditorApplication, dirty, undo, redo| {
        let current = frame(app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            (dirty, undo, redo)
        );
        assert_navigation(app);
    };
    assert_history(&app, false, 0, 0);
    let mut panel = PanelDriver::register(&mut app);
    let default_limits = PrefabDeclarationLimits::default();
    let large_limits = PrefabDeclarationLimits::new(
        default_limits.max_nodes(),
        1024 * 1024,
        default_limits.max_depth(),
    )
    .unwrap();

    let (right_id, right_address) = inspect_score(
        &mut panel,
        &mut app,
        document,
        "right",
        INITIAL_SCORE,
        false,
    );
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::PrepareRevert {
            inspection: right_id,
            address: right_address.clone(),
            limits: default_limits,
        }),
        Outcome::Rejected {
            inspection: Some(right_id),
            reason: Rejection::NoCurrentClaim,
        }
    );
    cancel(&mut panel, &mut app, right_id);

    let (limited, selected) =
        inspect_score(&mut panel, &mut app, document, "left", INITIAL_SCORE, true);
    assert_ne!(right_id, limited);
    let mut missing_target = selected.clone();
    missing_target.entity.entity = PrefabEntityOrigin::SourceEntity(id("missing-target"));
    for address in [right_address, missing_target] {
        assert_eq!(
            panel.dispatch(&mut app, |_| Action::PrepareRevert {
                inspection: limited,
                address,
                limits: default_limits,
            }),
            Outcome::Rejected {
                inspection: Some(limited),
                reason: Rejection::InvalidProperty,
            }
        );
        assert_eq!(frame(&app).prefab_inspection().phase, Phase::Ready);
    }

    prepare_revert(&mut panel, &mut app, limited, selected, default_limits);
    assert_failure(
        &mut app,
        limited,
        Rejection::BudgetExceeded,
        "scene.authoring-prefab-budget-exceeded",
    );
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);
    assert_eq!(fs::read(&source_file).unwrap(), original.as_bytes());
    acknowledge(&mut panel, &mut app, limited);

    let (before_prepare, selected) =
        inspect_score(&mut panel, &mut app, document, "left", INITIAL_SCORE, true);
    fs::write(&base_file, &changed_base).unwrap();
    prepare_revert(&mut panel, &mut app, before_prepare, selected, large_limits);
    assert_failure(
        &mut app,
        before_prepare,
        Rejection::StaleSource,
        "project.editor.authoring-source-changed",
    );
    acknowledge(&mut panel, &mut app, before_prepare);
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);

    fs::write(&base_file, &initial_base).unwrap();
    let expected_patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("left"),
        expected: Some(left),
        value: Some(reverted),
    }]);
    let (reviewed, selected) =
        inspect_score(&mut panel, &mut app, document, "left", INITIAL_SCORE, true);
    prepare_revert(&mut panel, &mut app, reviewed, selected, large_limits);
    assert_review(&mut panel, &mut app, reviewed, &expected_patch);
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);

    fs::write(&base_file, &changed_base).unwrap();
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::Commit {
            inspection: reviewed,
        }),
        Outcome::Accepted {
            inspection: reviewed,
            phase: Phase::Validating,
        }
    );
    assert_failure(
        &mut app,
        reviewed,
        Rejection::StaleSource,
        "project.editor.authoring-source-changed",
    );
    acknowledge(&mut panel, &mut app, reviewed);
    assert_eq!(
        app.prefab_inspection_review(reviewed).unwrap_err(),
        Rejection::WrongInspection
    );
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);

    // A broken sibling is demanded only when it is the selected mount.
    fs::write(&unrelated_file, b"{").unwrap();
    let (fresh_right, _) = inspect_score(
        &mut panel,
        &mut app,
        document,
        "right",
        CHANGED_SCORE,
        false,
    );
    cancel(&mut panel, &mut app, fresh_right);
    let invalid = start(&mut panel, &mut app, document, "unrelated");
    assert_failure(
        &mut app,
        invalid,
        Rejection::SourceRejected,
        "project.editor.authoring-source-rejected",
    );
    acknowledge(&mut panel, &mut app, invalid);
    fs::write(&unrelated_file, &unrelated).unwrap();

    fs::remove_file(&base_file).unwrap();
    let missing = start(&mut panel, &mut app, document, "left");
    assert_failure(
        &mut app,
        missing,
        Rejection::SourceRejected,
        "scene.prefab-source-missing",
    );
    acknowledge(&mut panel, &mut app, missing);
    fs::write(&base_file, &changed_base).unwrap();
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);

    let (inspection, selected) =
        inspect_score(&mut panel, &mut app, document, "left", INITIAL_SCORE, true);
    prepare_revert(&mut panel, &mut app, inspection, selected, large_limits);
    assert_review(&mut panel, &mut app, inspection, &expected_patch);
    assert_eq!(
        app.prefab_inspection_review(inspection).unwrap().patch(),
        &expected_patch
    );
    assert_eq!(basis(&app, document), captured);
    assert_history(&app, false, 0, 0);
    let base_before_revert = fs::read(&base_file).unwrap();
    assert_eq!(
        panel.dispatch(&mut app, |context| {
            assert_eq!(
                context.model().frame().prefab_inspection().phase,
                Phase::Review
            );
            assert_eq!(
                context
                    .prefab_inspection_review(inspection)
                    .unwrap()
                    .patch(),
                &expected_patch
            );
            Action::Commit { inspection }
        }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Validating,
        }
    );
    expect_phase(&mut app, inspection, Phase::Terminal);
    let published = basis(&app, document);
    assert_ne!(published.revision, captured.revision);
    assert_eq!(
        frame(&app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Reverted {
            inspection,
            document,
            revision: published.revision,
        })
    );
    acknowledge(&mut panel, &mut app, inspection);
    assert_eq!(
        app.prefab_effective_observation(inspection).unwrap_err(),
        Rejection::WrongInspection
    );
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::Inspect {
            basis: captured.clone(),
            anchor: id("left"),
        }),
        Outcome::Rejected {
            inspection: None,
            reason: Rejection::StaleSource,
        }
    );
    assert_history(&app, true, 1, 0);
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);
    assert_eq!(fs::read(&base_file).unwrap(), base_before_revert);

    let assert_left = |panel: &mut PanelDriver, app: &mut EditorApplication, score, pinned| {
        let (inspection, _) = inspect_score(panel, app, document, "left", score, pinned);
        cancel(panel, app, inspection);
        assert_navigation(app);
    };
    save(&mut app, document);
    assert_saved_source(kind, &fs::read(&source_file).unwrap(), &reverted_document);
    assert_history(&app, false, 1, 0);

    let (before_undo, inherited) =
        inspect_score(&mut panel, &mut app, document, "left", CHANGED_SCORE, false);
    replay(&mut app, document, false);
    assert_history(&app, true, 0, 1);
    assert_eq!(
        app.prefab_effective_observation(before_undo).unwrap_err(),
        Rejection::StaleSource
    );
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::PrepareRevert {
            inspection: before_undo,
            address: inherited,
            limits: large_limits,
        }),
        Outcome::Rejected {
            inspection: Some(before_undo),
            reason: Rejection::StaleSource,
        }
    );
    cancel(&mut panel, &mut app, before_undo);
    assert_left(&mut panel, &mut app, INITIAL_SCORE, true);
    save(&mut app, document);
    assert_saved_source(kind, &fs::read(&source_file).unwrap(), &original_document);
    assert_history(&app, false, 0, 1);

    replay(&mut app, document, true);
    assert_history(&app, true, 1, 0);
    assert_left(&mut panel, &mut app, CHANGED_SCORE, false);
    save(&mut app, document);
    let saved = fs::read(&source_file).unwrap();
    assert_saved_source(kind, &saved, &reverted_document);
    assert_history(&app, false, 1, 0);

    replay(&mut app, document, false);
    assert_history(&app, true, 0, 1);
    let reopening = basis(&app, document);
    let result = persist(
        &mut app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: reopening.revision,
            },
        },
    );
    let reopened = basis(&app, document);
    assert!(
        matches!(result, EditorPersistenceResult::Opened {
        document: actual, revision, ..
    } if actual == document && revision == reopened.revision),
        "{result:?}"
    );
    assert_ne!(
        reopened.revision.source_id(),
        reopening.revision.source_id()
    );
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::Inspect {
            basis: reopening,
            anchor: id("left"),
        }),
        Outcome::Rejected {
            inspection: None,
            reason: Rejection::StaleSource,
        }
    );
    panel.observe(&mut app, |context| {
        for entity in &reverted_document.entities {
            let observed = context
                .observe_prefab_instance(
                    &reopened,
                    &PrefabInstancePath::new(entity.id.clone()),
                    Default::default(),
                )
                .unwrap();
            assert_eq!(observed.instance.as_ref(), entity.prefab.as_ref());
        }
    });
    assert_left(&mut panel, &mut app, CHANGED_SCORE, false);
    assert_history(&app, false, 0, 0);
    assert_eq!(fs::read(&source_file).unwrap(), saved);
    assert_eq!(fs::read(&base_file).unwrap(), base_before_revert);
    assert_eq!(fs::read_to_string(&unrelated_file).unwrap(), unrelated);
    assert_eq!(
        fs::read_to_string(project.root.join(STARTUP)).unwrap(),
        startup
    );
    assert!(app.close());
}
