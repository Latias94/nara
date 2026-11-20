use super::*;

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    core::{ByteLimit, ItemLimit},
    reflect::ComponentPropertyReadBudget,
    scene::{
        AuthoringDocumentReadiness, AuthoringPropertyValue, AuthoringReadError,
        AuthoringRecordReadiness, SourceDocumentKey, SourceDocumentKind,
    },
};
use nara_editor::{
    EditorPanel, EditorPanelContext, EditorPanelHost, EditorPanelSubmission,
    workspace::{EditorPanelSourceKind, EditorPanelTarget},
};
use nara_tooling::{
    EditorDocumentOpenId, EditorPersistenceView, EditorProjectBrowserAction as BrowserAction,
    EditorProjectBrowserActionOutcome as BrowserOutcome,
};

const PANEL: &str = "consumer.unopened-authoring";
const STARTUP: &str = "scenes/startup.scene.json";
const LOCKED: &str = "prefabs/panel-binding.prefab.json";
const SOURCE_ID: &str = "be8c37d3-2cd3-4cdf-9479-004f3cd0866d";
const EXTERNAL_ID: &str = "c535c4c0-8d2b-4a78-af2f-6112d26fb914";
const INITIAL: u64 = 17;

#[derive(Default)]
struct InspectionPanel {
    completion: Option<(PanelSubmissionId, PanelActionDispatch)>,
}

impl EditorPanel for InspectionPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, dispatch: PanelActionDispatch) {
        assert!(self.completion.replace((id, dispatch)).is_none());
    }
}

struct LockedPanel {
    host: EditorPanelHost<InspectionPanel>,
    document: EditorDocumentId,
}

impl LockedPanel {
    fn register(app: &mut EditorApplication, document: EditorDocumentId) -> Self {
        let mut host = EditorPanelHost::new();
        host.register_panel(
            app,
            EditorPanelDescriptor::new(
                PANEL,
                "Unopened source authoring",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Floating,
                false,
            ),
            InspectionPanel::default,
        )
        .unwrap();
        app.panel_workspace_mut()
            .open(
                PANEL,
                EditorPanelTarget::Source {
                    kind: EditorPanelSourceKind::Prefab,
                    locator: LOCKED.into(),
                },
            )
            .unwrap();
        Self { host, document }
    }

    fn prepare<R>(
        &mut self,
        app: &EditorApplication,
        visit: impl FnOnce(&mut EditorPanelContext<'_, '_>) -> R,
    ) -> (EditorPanelSubmission, R) {
        let mut visit = Some(visit);
        let mut observed = None;
        let document = self.document;
        let candidate = self
            .host
            .prepare(app, |panel, context| {
                let model = context.model();
                assert_eq!(model.document().map(|tab| tab.document), Some(document));
                assert_ne!(model.frame().workspace().active_document, Some(document));
                assert!(model.selection().is_none());
                panel.completion = None;
                observed = Some(visit.take().unwrap()(context));
            })
            .unwrap();
        (
            candidate,
            observed.expect("the locked panel must be visited"),
        )
    }

    fn observe<R>(
        &mut self,
        app: &EditorApplication,
        read: impl FnOnce(&EditorPanelContext<'_, '_>) -> R,
    ) -> R {
        let (candidate, observed) = self.prepare(app, |context| read(context));
        assert_eq!(candidate.submissions().len(), 0);
        observed
    }

    fn dispatch(
        &mut self,
        app: &mut EditorApplication,
        enqueue: impl FnOnce(&mut EditorPanelContext<'_, '_>) -> PanelSubmissionId,
    ) -> EditorActionOutcome {
        let (candidate, (token, submitted)) =
            self.prepare(app, |context| (context.instance_token(), enqueue(context)));
        assert_eq!(candidate.submissions().len(), 1);
        assert_eq!(
            candidate.submissions().next(),
            Some((token.instance(), submitted))
        );
        self.host.submit_and_drive(app, candidate, Duration::ZERO);
        self.host
            .inspect(app, &token, |panel, _| {
                let Some((finished, PanelActionDispatch::Dispatched(outcome))) = &panel.completion
                else {
                    panic!(
                        "the locked panel lost its exact dispatch: {:?}",
                        panel.completion
                    );
                };
                assert_eq!(*finished, submitted);
                outcome.clone()
            })
            .expect("the unrelated panel binding must survive the action")
    }

    fn browser(&mut self, app: &mut EditorApplication, action: BrowserAction) -> BrowserOutcome {
        let outcome = self.dispatch(app, |context| {
            context
                .submit(EditorAction::ProjectBrowser(action))
                .unwrap()
        });
        let EditorActionOutcome::ProjectBrowser(outcome) = outcome else {
            panic!("expected a Project Browser dispatch: {outcome:?}");
        };
        outcome
    }

    fn promote(
        &mut self,
        app: &mut EditorApplication,
        request: EditorDocumentOpenId,
    ) -> EditorDocumentOpenId {
        let opening = preparing(self.browser(app, BrowserAction::OpenInspectedSource { request }));
        assert_ne!(opening, request);
        opening
    }

    fn acknowledge(&mut self, app: &mut EditorApplication, request: EditorDocumentOpenId) {
        assert_eq!(
            self.browser(app, BrowserAction::AcknowledgeOpen { request }),
            BrowserOutcome::OpenAcknowledged { request }
        );
        assert!(frame(app).document_open().result().is_none());
        assert!(app.inspected_authoring_source(request).is_none());
    }
}

fn metadata(path: &str, kind: AssetSourceKind, stable_id: &str) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(path.rsplit_once('/').unwrap().1).unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap()
}

fn contents(kind: SourceDocumentKind, amount: u64) -> String {
    let entities = [
        SceneEntityRecord::new(id("tail")),
        SceneEntityRecord::new(id("anchor"))
            .with_component(
                ComponentTypeId::new("consumer.Link"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        (
                            "target",
                            ComponentValue::EntityReference(EntityReference::SceneLocal {
                                entity: id("tail"),
                            }),
                        ),
                        ("amount", ComponentValue::U64(amount)),
                    ]),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            )
            .with_component(
                ComponentTypeId::new("consumer.Unknown"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("preserve this record".into()),
                ),
            ),
    ];
    match kind {
        SourceDocumentKind::Scene => SceneDocument::from_ordered_roots(entities).to_json_string(),
        SourceDocumentKind::Prefab => PrefabDocument::from_ordered_roots(entities).to_json_string(),
    }
    .unwrap()
}

fn preparing(outcome: BrowserOutcome) -> EditorDocumentOpenId {
    let BrowserOutcome::Preparing { request } = outcome else {
        panic!("source preparation was refused: {outcome:?}");
    };
    request
}

fn assert_unchanged(current: &EditorFrameModel, before: &EditorFrameModel) {
    assert_eq!(current.workspace(), before.workspace());
    assert_eq!(
        current.scene_preview_document(),
        before.scene_preview_document()
    );
    assert_eq!(current.persistence(), before.persistence());
}

fn terminal_open(
    app: &mut EditorApplication,
    request: EditorDocumentOpenId,
    before: Option<&EditorFrameModel>,
) -> EditorDocumentOpenResult {
    wait(app, |current| {
        if let Some(before) = before {
            assert_unchanged(current, before);
        }
        current.document_open().result().is_some()
    });
    let completed = frame(app);
    assert!(!completed.document_open().has_pending_work());
    let result = completed.document_open().result().unwrap();
    assert_eq!(result.request(), request);
    result
}

fn inspect_source(
    panel: &mut LockedPanel,
    app: &mut EditorApplication,
    action: BrowserAction,
    before: &EditorFrameModel,
) -> EditorDocumentOpenId {
    let request = preparing(panel.browser(app, action));
    assert_eq!(
        terminal_open(app, request, Some(before)),
        EditorDocumentOpenResult::Inspected { request }
    );
    request
}

#[derive(Debug, PartialEq)]
struct InspectedAmount {
    entity: SceneEntityId,
    component: ComponentTypeId,
    version: ComponentSchemaVersion,
    property: ComponentPropertyPath,
    value: u64,
}

fn discover(
    panel: &mut LockedPanel,
    app: &EditorApplication,
    request: EditorDocumentOpenId,
    path: &str,
    key: &SourceDocumentKey,
) -> InspectedAmount {
    let retained = app.inspected_authoring_source(request).unwrap();
    assert_eq!(retained.source_key(), key);

    panel.observe(app, |context| {
        let source = context.inspected_authoring_source(request).unwrap();
        assert_eq!(source.relative_path(), path);
        assert_eq!(source.source_key(), key);
        assert_eq!(source.readiness(), AuthoringDocumentReadiness::Degraded);
        let mut entities = source.entities_after(None);
        assert_eq!(entities.len(), 2);
        let entity = entities.next().unwrap();
        assert_eq!(entity.as_str(), "anchor");
        let mut remaining = source.entities_after(Some(entity));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining.next().unwrap().as_str(), "tail");

        let record = source.entity(entity).unwrap();
        let component = record
            .components_after(None)
            .find(|component| component.fact().readiness() == AuthoringRecordReadiness::Available)
            .unwrap();
        let fact = component.fact();
        assert_eq!(fact.component().as_str(), "consumer.Link");
        let field = component
            .root_fields()
            .unwrap()
            .iter()
            .find(|field| {
                field.value_kind() == ComponentValueKind::U64
                    && field.has_capability(ComponentCapability::Edit)
            })
            .unwrap();
        assert_eq!(field.id().as_str(), "amount");
        let property = ComponentPropertyPath::new(field.id().clone());
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(4).unwrap(),
            ByteLimit::new(64).unwrap(),
        );
        assert_eq!(
            component.property_value(
                &ComponentPropertyPath::new(ComponentFieldId::new("unknown")),
                &mut budget,
            ),
            Err(AuthoringReadError::FieldNotFound)
        );
        let value = component.property_value(&property, &mut budget).unwrap();
        let AuthoringPropertyValue::Present(ComponentValue::U64(amount)) = value else {
            panic!("the discovered amount must contain a stored integer: {value:?}");
        };
        assert_eq!(*amount, INITIAL);
        InspectedAmount {
            entity: entity.clone(),
            component: fact.component().clone(),
            version: fact.version(),
            property,
            value: *amount,
        }
    })
}

fn apply(app: &mut EditorApplication, action: EditorAction) {
    let submitted = submit(app, action);
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] =
        submitted.actions()
    else {
        panic!("workspace command was not dispatched: {submitted:?}");
    };
    assert!(report.applied, "{:?}", report.diagnostics);
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
    apply(app, EditorAction::Mutate(mutation));
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
        panic!("persistence was refused: {started:?}");
    };
    settle_persistence(app);
    let EditorPersistenceView::Terminal {
        attempt: completed,
        result,
    } = frame(app).persistence()
    else {
        panic!("persistence must complete the accepted attempt");
    };
    assert_eq!(completed, *attempt);
    let acknowledged = submit(
        app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt: completed,
        }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt },
        )),
    ] if *attempt == completed));
    result
}

#[test]
fn a_locked_panel_round_trips_an_unopened_scene() {
    round_trip(SourceDocumentKind::Scene, "scenes/unopened.scene.json");
}

#[test]
fn a_locked_panel_round_trips_an_unopened_prefab() {
    round_trip(SourceDocumentKind::Prefab, "prefabs/unopened.prefab.json");
}

fn round_trip(kind: SourceDocumentKind, path: &str) {
    let project = text_project::TextProject::new();
    let (document_kind, asset_kind) = match kind {
        SourceDocumentKind::Scene => (EditorDocumentKind::Scene, AssetSourceKind::Scene),
        SourceDocumentKind::Prefab => (EditorDocumentKind::Prefab, AssetSourceKind::Prefab),
    };
    let manifest = project.root.join("nara.toml");
    let stable = fs::read_to_string(&manifest)
        .unwrap()
        .replace("project_location", "stable_metadata");
    fs::write(&manifest, stable).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        bundle().to_json_string().unwrap(),
    )
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))]);
    let startup_text = startup.to_json_string().unwrap();
    let locked_text = PrefabDocument::default().to_json_string().unwrap();
    let original = contents(kind, INITIAL);
    let source_file = project.root.join(path);
    for (path, kind, stable_id, bytes) in [
        (
            STARTUP,
            AssetSourceKind::Scene,
            "a48d60e2-8ede-4103-b721-f6a0533fb99f",
            &startup_text,
        ),
        (
            LOCKED,
            AssetSourceKind::Prefab,
            "64a2efa1-70cb-4779-8386-dda7c9541bd2",
            &locked_text,
        ),
        (path, asset_kind.clone(), SOURCE_ID, &original),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
        fs::write(
            project.root.join(format!("{path}.meta")),
            metadata(path, kind, stable_id),
        )
        .unwrap();
    }
    let sidecar = project.root.join(format!("{path}.meta"));
    let key = SourceDocumentKey::stable_asset(StableAssetId::parse_str(SOURCE_ID).unwrap(), kind);

    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |current| {
        !current.project_browser().has_pending_work()
    });
    let active = frame(&app).workspace().active_document.unwrap();
    let opening = submit(
        &mut app,
        EditorAction::ProjectBrowser(BrowserAction::Open {
            relative_path: LOCKED.into(),
        }),
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(outcome))] =
        opening.actions()
    else {
        panic!("expected to start Open for the panel binding: {opening:?}");
    };
    let locked_request = preparing(*outcome);
    let EditorDocumentOpenResult::Opened {
        document: locked, ..
    } = terminal_open(&mut app, locked_request, None)
    else {
        panic!("the unrelated panel source must open");
    };
    apply(
        &mut app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    );
    let mut panel = LockedPanel::register(&mut app, locked);
    panel.acknowledge(&mut app, locked_request);

    let base_revision = basis(&app, active).revision;
    apply(
        &mut app,
        EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document: active,
            base_revision,
            patch: add_root_patch(&startup.placements, "active-draft"),
        }),
    );
    apply(
        &mut app,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("active-draft")),
        }),
    );
    let before = frame(&app);
    assert_eq!(before.workspace().documents.len(), 2);
    assert_eq!(before.workspace().active_document, Some(active));
    assert_eq!(before.scene_preview_document(), Some(active));
    let selected = &before.workspace().active_scene.as_ref().unwrap().selection;
    assert_eq!(selected.top_entity(), Some(&id("active-draft")));
    let draft = before.workspace().document(active).unwrap();
    assert!(draft.dirty);
    assert_eq!(draft.history.undo_depth, 1);

    let inspect = || BrowserAction::InspectSource {
        relative_path: path.into(),
    };
    let stale = inspect_source(&mut panel, &mut app, inspect(), &before);
    assert!(app.inspected_authoring_source(stale).is_some());
    panel.acknowledge(&mut app, stale);
    assert_unchanged(&frame(&app), &before);

    let original_sidecar = fs::read_to_string(&sidecar).unwrap();
    let external = contents(kind, INITIAL + 10);
    assert_eq!(original.len(), external.len());
    for (file, original, replacement) in [
        (&source_file, original.as_str(), external),
        (
            &sidecar,
            original_sidecar.as_str(),
            metadata(path, asset_kind, EXTERNAL_ID),
        ),
    ] {
        let captured = inspect_source(&mut panel, &mut app, inspect(), &before);
        let observation = discover(&mut panel, &app, captured, path, &key);
        fs::write(file, &replacement).unwrap();
        assert_eq!(
            discover(&mut panel, &app, captured, path, &key),
            observation
        );
        assert_unchanged(&frame(&app), &before);
        let opening = panel.promote(&mut app, captured);
        let result = terminal_open(&mut app, opening, Some(&before));
        assert!(
            matches!(result, EditorDocumentOpenResult::Rejected { .. }),
            "{file:?}: {result:?}"
        );
        assert!(app.inspected_authoring_source(captured).is_none());
        assert_eq!(fs::read_to_string(file).unwrap(), replacement);
        panel.acknowledge(&mut app, opening);
        assert_unchanged(&frame(&app), &before);
        fs::write(file, original).unwrap();
    }

    let inspected = inspect_source(&mut panel, &mut app, inspect(), &before);
    assert_ne!(inspected, stale);
    for action in [
        BrowserAction::CancelOpen { request: stale },
        BrowserAction::AcknowledgeOpen { request: stale },
        BrowserAction::OpenInspectedSource { request: stale },
    ] {
        assert!(matches!(
            panel.browser(&mut app, action),
            BrowserOutcome::Rejected { .. }
        ));
        assert_eq!(
            frame(&app).document_open().result(),
            Some(EditorDocumentOpenResult::Inspected { request: inspected })
        );
        assert_unchanged(&frame(&app), &before);
    }
    assert!(app.inspected_authoring_source(stale).is_none());
    panel.observe(&app, |context| {
        assert!(context.inspected_authoring_source(stale).is_none());
    });
    let observation = discover(&mut panel, &app, inspected, path, &key);
    assert_unchanged(&frame(&app), &before);
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);

    let opening = panel.promote(&mut app, inspected);
    let EditorDocumentOpenResult::Opened { document, .. } = terminal_open(&mut app, opening, None)
    else {
        panic!("the exact inspected source must open on request");
    };
    assert!(app.inspected_authoring_source(inspected).is_none());
    panel.observe(&app, |context| {
        assert!(context.inspected_authoring_source(inspected).is_none());
        assert!(context.inspected_authoring_source(opening).is_none());
    });
    let opened = frame(&app);
    assert_eq!(
        opened.workspace().documents.len(),
        before.workspace().documents.len() + 1
    );
    assert_eq!(opened.workspace().active_document, Some(document));
    assert_eq!(
        opened.scene_preview_document(),
        Some(match kind {
            SourceDocumentKind::Scene => document,
            SourceDocumentKind::Prefab => active,
        })
    );
    let tab = opened.workspace().document(document).unwrap();
    assert_eq!(tab.kind, document_kind);
    assert_eq!(tab.source_binding.as_ref().unwrap().locator(), path);
    assert_eq!(tab.revision, tab.saved_revision);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
    assert!(!tab.dirty);
    panel.acknowledge(&mut app, opening);

    apply(
        &mut app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    );
    let inactive = frame(&app);
    assert_eq!(inactive.workspace().document(active), Some(draft));
    assert_eq!(
        &inactive
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection,
        selected
    );
    assert_eq!(inactive.scene_preview_document(), Some(active));
    assert_eq!(
        panel.browser(&mut app, inspect()),
        BrowserOutcome::Rejected {
            code: "project.editor.document-source-already-open",
        }
    );
    assert_unchanged(&frame(&app), &inactive);
    assert!(!frame(&app).document_open().has_pending_work());
    assert!(frame(&app).document_open().result().is_none());

    let request = |basis: EditorDocumentQueryBasis| EditorPropertyRead {
        target: EditorPropertyTarget {
            basis,
            entity: observation.entity.clone(),
            component: observation.component.clone(),
            property: observation.property.clone(),
        },
        limits: EditorPropertyReadLimits::new(1024, 1, 8).unwrap(),
    };
    let assert_value = |app: &EditorApplication, value| {
        assert_eq!(
            app.read_document_property(request(basis(app, document)))
                .unwrap()
                .value,
            EditorPropertyValue::Present(ComponentValue::U64(value))
        );
    };
    let (captured, prepared, replacement) = panel.observe(&app, |context| {
        let basis = context.document_query_basis(document).unwrap();
        let current = context
            .read_document_property(request(basis.clone()))
            .unwrap();
        assert_eq!(current.target.basis, basis);
        let EditorPropertyValue::Present(ComponentValue::U64(amount)) = current.value else {
            panic!("the promoted source must retain the inspected scalar: {current:?}");
        };
        assert_eq!(amount, observation.value);
        let replacement = amount.checked_add(2).unwrap();
        let edit = AuthoringPropertyEdit {
            entity: current.target.entity,
            component: current.target.component,
            component_version: observation.version,
            property: current.target.property,
            value: ComponentValue::U64(replacement),
        };
        let prepared = context
            .prepare_authoring_property_edit(&basis, &[edit])
            .unwrap();
        (basis, prepared, replacement)
    });
    assert_eq!(captured, basis(&app, document));
    assert_eq!(prepared.document(), document);
    assert_eq!(prepared.document_kind(), document_kind);
    assert_eq!(prepared.base_revision(), captured.revision);
    assert_eq!(prepared.patch().operations.len(), 1);
    assert_eq!(prepared.inverse().operations.len(), 1);
    assert_unchanged(&frame(&app), &inactive);
    let committed = panel.dispatch(&mut app, |context| {
        context.commit_authoring_edit(prepared).unwrap()
    });
    assert!(
        matches!(&committed, EditorActionOutcome::Workspace(report)
            if report.applied && report.document == Some(document)
                && report.active_document == Some(active)),
        "{committed:?}"
    );
    for (redo, value, depths) in [
        (None, replacement, (1, 0)),
        (Some(false), observation.value, (0, 1)),
        (Some(true), replacement, (1, 0)),
    ] {
        if let Some(redo) = redo {
            replay(&mut app, document, redo);
        }
        assert_value(&app, value);
        let current = frame(&app);
        let history = current.workspace().document(document).unwrap().history;
        assert_eq!((history.undo_depth, history.redo_depth), depths);
        assert_eq!(
            current.workspace().active_scene,
            inactive.workspace().active_scene
        );
    }
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);

    let saving = basis(&app, document);
    let saved = persist(
        &mut app,
        EditorPersistenceAction::Save {
            document,
            base_revision: saving.revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = saved
    else {
        panic!("the edited source must save: {saved:?}");
    };
    assert_eq!((saved_document, revision), (document, saving.revision));
    assert!(!frame(&app).workspace().document(document).unwrap().dirty);
    let saved_bytes = fs::read(&source_file).unwrap();
    let saved_json: serde_json::Value = serde_json::from_slice(&saved_bytes).unwrap();
    let expected_json: serde_json::Value =
        serde_json::from_str(&contents(kind, replacement)).unwrap();
    assert_eq!(saved_json["payload"], expected_json["payload"]);

    replay(&mut app, document, false);
    assert_value(&app, observation.value);
    assert!(frame(&app).workspace().document(document).unwrap().dirty);
    let reopening = basis(&app, document);
    let reopened = persist(
        &mut app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: reopening.revision,
            },
        },
    );
    let current = frame(&app);
    let tab = current.workspace().document(document).unwrap();
    assert_eq!(
        reopened,
        EditorPersistenceResult::Opened {
            document,
            revision: tab.revision,
            digest,
        }
    );
    assert_ne!(tab.revision.source_id(), reopening.revision.source_id());
    assert_eq!(tab.source_binding.as_ref().unwrap().locator(), path);
    assert_eq!(tab.revision, tab.saved_revision);
    assert!(!tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
    assert_eq!(
        app.read_document_property(request(captured)),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
    assert_value(&app, replacement);
    assert_eq!(fs::read(&source_file).unwrap(), saved_bytes);
    assert_eq!(fs::read_to_string(&sidecar).unwrap(), original_sidecar);
    assert_eq!(
        fs::read_to_string(project.root.join(STARTUP)).unwrap(),
        startup_text
    );
    assert_eq!(
        current.workspace().active_scene,
        inactive.workspace().active_scene
    );
    assert_eq!(current.workspace().document(active), Some(draft));
    assert_eq!(current.scene_preview_document(), Some(active));
    assert!(
        app.close(),
        "the independent authoring project must physically retire"
    );
}
