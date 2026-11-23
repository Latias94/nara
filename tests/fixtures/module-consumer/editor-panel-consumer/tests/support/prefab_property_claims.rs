use super::*;

use nara::{
    asset::{AssetRef, AssetSourcePath},
    project_host::EditorEditPreparationError,
    reflect::ComponentValueShape,
    scene::{
        PrefabComponentIntent, PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance,
        PrefabInstancePath, PrefabOverlay, PrefabOverlaySlot, PrefabPropertyClaim,
        PrefabPropertyIntent, PrefabRevert, SceneDocumentCandidate, SourceDocumentKey,
        SourceDocumentKind,
    },
};
use nara_editor::{EditorPanel, EditorPanelContext, EditorPanelHost, workspace::EditorPanelTarget};
use nara_tooling::{
    EditorAuthoringCursor, EditorAuthoringPage, EditorPersistenceAttemptId, EditorPersistenceView,
    EditorPrefabPropertyClaimTarget,
};

#[path = "prefab_property_claims/nested_revert.rs"]
mod nested_revert;

const PANEL: &str = "consumer.prefab-property-claims";
const COMPONENT: &str = "consumer.prefab.Settings";
const UNKNOWN: &str = "consumer.prefab.Unavailable";
const STARTUP: &str = "scenes/startup.scene.json";
const SCENE: &str = "scenes/prefab-property-claims.scene.json";
const PREFAB: &str = "prefabs/prefab-property-claims.prefab.json";
const BASE: &str = "prefabs/claims.prefab.json";
const LEFT_SCORE: u64 = 77;
const RIGHT_SCORE: u64 = 41;

fn schemas() -> AuthoringSchemaBundle {
    let settings = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Settings",
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
        ComponentFieldSchema::optional(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["wire_note"]),
            ComponentValueShape::optional(ComponentValueKind::String.into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![settings],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn property(field: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(field))
}

fn score(value: u64) -> PrefabPropertyIntent {
    PrefabPropertyIntent::Set(ComponentValue::U64(value))
}

fn opaque_value() -> PrefabPropertyIntent {
    PrefabPropertyIntent::Set(ComponentValue::String(
        "preserve unavailable payload".into(),
    ))
}

fn properties(
    target: &str,
    component: &str,
    version: ComponentSchemaVersion,
    claims: impl IntoIterator<Item = (&'static str, PrefabPropertyIntent)>,
) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id(target)),
        component: ComponentTypeId::new(component),
        version,
        claims: claims
            .into_iter()
            .map(|(field, intent)| PrefabPropertyClaim {
                path: property(field),
                intent,
            })
            .collect(),
    }
}

fn instances() -> [PrefabInstance; 3] {
    let key = SourceDocumentKey::project_location(
        "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
        AssetSourcePath::new(BASE).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let instance = |components| {
        let mut overlay = PrefabOverlay::new(key.clone());
        overlay.components = components;
        let mut instance = PrefabInstance::forest(AssetRef::path("claims.prefab.json").unwrap());
        instance.overrides = Some(overlay);
        instance
    };
    let known = |target, claims| properties(target, COMPONENT, ComponentSchemaVersion::ONE, claims);
    let guard = known("guard", vec![("score", score(12))]);
    let absent = ("note", PrefabPropertyIntent::Absent);
    let unknown = properties(
        "leaf",
        UNKNOWN,
        ComponentSchemaVersion(7),
        [("score", opaque_value())],
    );
    let left = instance(vec![
        guard.clone(),
        known("leaf", vec![("score", score(LEFT_SCORE)), absent.clone()]),
        unknown.clone(),
    ]);
    let right = instance(vec![
        guard.clone(),
        known(
            "leaf",
            vec![
                ("score", score(RIGHT_SCORE)),
                ("note", PrefabPropertyIntent::Set(ComponentValue::Null)),
            ],
        ),
    ]);
    let reverted = instance(vec![guard, known("leaf", vec![absent]), unknown]);
    [left, right, reverted]
}

fn scene(left: &PrefabInstance, right: &PrefabInstance) -> SceneDocument {
    SceneDocument::from_ordered_roots([("left", left), ("right", right)].map(|(name, instance)| {
        let mut anchor = SceneEntityRecord::new(id(name));
        anchor.prefab = Some(instance.clone());
        anchor
    }))
}

fn base() -> PrefabDocument {
    PrefabDocument::from_ordered_roots(["guard", "leaf"].map(|name| {
        SceneEntityRecord::new(id(name)).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_score", ComponentValue::U64(20)),
                    ("wire_note", ComponentValue::String("base note".into())),
                ]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
        )
    }))
}

fn query_limits() -> EditorAuthoringQueryLimits {
    EditorAuthoringQueryLimits::new(8, 1, 1024).unwrap()
}

fn read_limits() -> EditorPropertyReadLimits {
    EditorPropertyReadLimits::new(1024, 1, 8).unwrap()
}

fn discover(
    basis: &EditorDocumentQueryBasis,
    path: &PrefabInstancePath,
    query: impl Fn(EditorAuthoringQuery) -> EditorAuthoringPage,
) -> (Vec<EditorAuthoringRow>, EditorAuthoringCursor) {
    let target = EditorAuthoringQueryTarget {
        basis: basis.clone(),
        entity: path.anchor.clone(),
        projection: EditorAuthoringProjection::PrefabPropertyClaims {
            steps: path.steps.clone(),
        },
    };
    let limits = query_limits();
    let mut page = query(EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(target.clone()),
        limits,
    });
    let continuation = page.next.clone().expect("one row cannot fit all claims");
    let mut rows = Vec::new();
    for _ in 0..8 {
        assert_eq!(page.target.as_ref(), &target);
        assert!(page.rows.len() <= limits.max_rows());
        assert!(page.scanned_entries <= limits.max_scan_entries());
        assert!(page.metadata_bytes <= limits.max_metadata_bytes());
        for row in &page.rows {
            let EditorAuthoringRow::PrefabPropertyClaim { claim, .. } = row else {
                panic!("the fixture must return complete claim headers: {row:?}");
            };
            assert_eq!(claim.basis(), basis);
            assert_eq!(claim.path(), path);
        }
        rows.extend(page.rows);
        let Some(next) = page.next else {
            return (rows, continuation);
        };
        page = query(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(next),
            limits,
        });
    }
    panic!("claim pagination did not finish for {path:?}");
}

fn find_claim(
    rows: &[EditorAuthoringRow],
    entity: &str,
    component_name: &str,
    schema_version: ComponentSchemaVersion,
    field: &str,
    expected_value: bool,
) -> EditorPrefabPropertyClaimTarget {
    let expected_target = PrefabEntityTarget::source(id(entity));
    let expected_component = ComponentTypeId::new(component_name);
    let expected_property = property(field);
    let mut selected = rows.iter().filter_map(|row| match row {
        EditorAuthoringRow::PrefabPropertyClaim {
            claim,
            target,
            component,
            version,
            property,
            has_value,
        } if target == &expected_target
            && component == &expected_component
            && *version == schema_version
            && property == &expected_property =>
        {
            assert_eq!(*has_value, expected_value);
            Some(claim)
        }
        _ => None,
    });
    let claim = selected
        .next()
        .expect("the named claim must be discoverable");
    assert!(selected.next().is_none());
    claim.clone()
}

struct Review {
    selected: EditorPrefabPropertyClaimTarget,
    competing: EditorPrefabPropertyClaimTarget,
    continuation: EditorAuthoringCursor,
    patch: ScenePatchDocument,
    inverse: ScenePatchDocument,
    retained: PreparedEditorDocumentEdit,
    preview_claim: EditorPrefabPropertyClaimTarget,
    preview_continuation: EditorAuthoringQuery,
}

#[derive(Default)]
struct Evidence {
    review: Option<Review>,
    held_frames: usize,
    submissions: Option<[PanelSubmissionId; 2]>,
    outcomes: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct ClaimPanel {
    document: EditorDocumentId,
    pending: Option<[PreparedEditorDocumentEdit; 2]>,
    evidence: Rc<RefCell<Evidence>>,
}

impl ClaimPanel {
    fn review(&mut self, context: &EditorPanelContext<'_, '_>) {
        let basis = context.document_query_basis(self.document).unwrap();
        let left = PrefabInstancePath::new(id("left"));
        let right = PrefabInstancePath::new(id("right"));
        let query = |request| context.query_document_authoring(request).unwrap();
        let (left_rows, continuation) = discover(&basis, &left, query);
        let (right_rows, _) = discover(&basis, &right, query);
        assert_eq!(left_rows.len(), 4);
        assert_eq!(right_rows.len(), 3);

        let known = |rows: &[EditorAuthoringRow], entity, field, has_value| {
            find_claim(
                rows,
                entity,
                COMPONENT,
                ComponentSchemaVersion::ONE,
                field,
                has_value,
            )
        };
        let read = |claim: &EditorPrefabPropertyClaimTarget| {
            context
                .read_prefab_property_claim(claim, read_limits())
                .unwrap()
        };
        let left_claim = known(&left_rows, "leaf", "score", true);
        let right_claim = known(&right_rows, "leaf", "score", true);
        assert_eq!(left_claim.slot(), right_claim.slot());
        assert_ne!(left_claim, right_claim);
        let guard = known(&left_rows, "guard", "score", true);
        assert_ne!(left_claim.slot(), guard.slot());
        assert_eq!(read(&guard), score(12));
        let absent = known(&left_rows, "leaf", "note", false);
        let null = known(&right_rows, "leaf", "note", true);
        assert_eq!(absent.slot(), null.slot());
        assert_eq!(read(&absent), PrefabPropertyIntent::Absent);
        assert_eq!(read(&null), PrefabPropertyIntent::Set(ComponentValue::Null));

        let unavailable = find_claim(
            &left_rows,
            "leaf",
            UNKNOWN,
            ComponentSchemaVersion(7),
            "score",
            true,
        );
        assert_ne!(unavailable.slot(), left_claim.slot());
        let opaque_limits = EditorPropertyReadLimits::new(1024, 1, 64).unwrap();
        assert_eq!(
            context
                .read_prefab_property_claim(&unavailable, opaque_limits)
                .unwrap(),
            opaque_value()
        );
        let [
            PrefabPropertyIntent::Set(ComponentValue::U64(left_score)),
            PrefabPropertyIntent::Set(ComponentValue::U64(right_score)),
        ] = [read(&left_claim), read(&right_claim)]
        else {
            panic!("both selected claims must provide their stored scores");
        };
        assert_eq!((left_score, right_score), (LEFT_SCORE, RIGHT_SCORE));
        let (selected, competing) = if left_score > right_score {
            (left_claim, right_claim)
        } else {
            (right_claim, left_claim)
        };
        let prepare = |claim: &EditorPrefabPropertyClaimTarget| {
            context
                .prepare_prefab_property_revert(claim, Default::default())
                .unwrap()
        };
        drop(prepare(&selected));
        let selected_edit = prepare(&selected);
        let competing_edit = prepare(&competing);
        let retained = prepare(&selected);
        for edit in [&selected_edit, &competing_edit, &retained] {
            assert!(edit.is_prefab_revert());
            assert_eq!(edit.document(), self.document);
            assert_eq!(edit.base_revision(), basis.revision);
        }

        let preview = retained.preview();
        assert_eq!(preview.basis(), &basis);
        let (rows, preview_cursor) = discover(preview.basis(), selected.path(), |query| {
            preview.query_authoring(query).unwrap()
        });
        assert_eq!(rows.len(), 3);
        let preview_guard = known(&rows, "guard", "score", true);
        let preview_claim = known(&rows, "leaf", "note", false);
        let preview_unavailable = find_claim(
            &rows,
            "leaf",
            UNKNOWN,
            ComponentSchemaVersion(7),
            "score",
            true,
        );
        assert_eq!(preview_claim.slot(), selected.slot());
        assert_eq!(
            preview.read_prefab_property_claim(&preview_guard, read_limits()),
            Ok(score(12))
        );
        assert_eq!(
            preview.read_prefab_property_claim(&preview_claim, read_limits()),
            Ok(PrefabPropertyIntent::Absent)
        );
        assert_eq!(
            preview.read_prefab_property_claim(&preview_unavailable, opaque_limits),
            Ok(opaque_value())
        );

        let sibling = selected_edit.preview();
        let wrong_source = EditorDocumentQueryRejection::WrongPreparedEdit;
        for (reader, result) in [
            (
                "workspace",
                context.read_prefab_property_claim(&preview_claim, read_limits()),
            ),
            (
                "sibling",
                sibling.read_prefab_property_claim(&preview_claim, read_limits()),
            ),
            (
                "preview",
                preview.read_prefab_property_claim(&selected, read_limits()),
            ),
        ] {
            assert_eq!(result, Err(wrong_source.clone()), "{reader}");
        }
        let continue_from = |cursor| EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(cursor),
            limits: query_limits(),
        };
        let preview_continuation = continue_from(preview_cursor);
        for (reader, result) in [
            (
                "workspace",
                context.query_document_authoring(preview_continuation.clone()),
            ),
            (
                "sibling",
                sibling.query_authoring(preview_continuation.clone()),
            ),
            (
                "preview",
                preview.query_authoring(continue_from(continuation.clone())),
            ),
        ] {
            assert_eq!(result, Err(wrong_source.clone()), "{reader}");
        }
        assert_eq!(
            context
                .prepare_prefab_property_revert(&preview_claim, Default::default())
                .unwrap_err(),
            EditorEditPreparationError::QueryBasis(wrong_source)
        );
        assert_eq!(read(&selected), score(LEFT_SCORE));

        let [
            ScenePatchOperation::SetPrefabInstance {
                value: Some(candidate_instance),
                ..
            },
        ] = retained.patch().operations.as_slice()
        else {
            panic!("the retained Revert must replace one instance");
        };
        let preview_overlay = candidate_instance.overrides.as_ref().unwrap();
        let preview_declaration = preview_overlay
            .property_claim_at(preview_claim.slot())
            .unwrap();
        let PrefabOverlaySlot::Property {
            component_index, ..
        } = preview_claim.slot()
        else {
            panic!("the candidate claim must identify a property");
        };
        let live_observation = context
            .observe_prefab_instance(&basis, selected.path(), Default::default())
            .unwrap();
        let live_declaration = live_observation
            .instance()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .property_claim_at(selected.slot())
            .unwrap();
        assert_ne!(preview_declaration, live_declaration);
        for (action, expected_code) in [
            (
                PrefabRevert::Intent(preview_claim.slot()),
                "scene.authoring-prefab-property-claim-required",
            ),
            (
                PrefabRevert::Property(preview_declaration),
                "scene.authoring-prefab-stale-observed-intent",
            ),
            (
                PrefabRevert::Intent(PrefabOverlaySlot::Component {
                    index: component_index,
                }),
                "scene.authoring-prefab-component-claim-required",
            ),
            (
                PrefabRevert::Property(live_declaration),
                "scene.authoring-prefab-stale-observed-intent",
            ),
        ] {
            let error = context
                .prepare_prefab_revert(&basis, selected.path(), action, Default::default())
                .unwrap_err();
            let EditorEditPreparationError::Document(rejected) = error else {
                panic!("the source owner must reject Revert with {expected_code}: {error:?}");
            };
            assert!(!rejected.applied);
            assert_eq!(rejected.document, Some(self.document));
            assert_eq!(rejected.revision, Some(basis.revision));
            let patch_report = rejected.patch_report.as_ref().unwrap();
            assert!(patch_report.inverse.is_none());
            assert!(
                patch_report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == expected_code),
                "{rejected:?}"
            );
            assert_eq!(context.document_query_basis(self.document).unwrap(), basis);
            assert_eq!(read(&selected), score(LEFT_SCORE));
            assert_eq!(read(&competing), score(RIGHT_SCORE));
            assert_eq!(read(&absent), PrefabPropertyIntent::Absent);
        }
        let observed_edit = context
            .prepare_prefab_revert(
                &basis,
                selected.path(),
                live_observation.revert(0, Some(selected.slot())).unwrap(),
                Default::default(),
            )
            .unwrap();
        assert_eq!(observed_edit.patch(), selected_edit.patch());
        assert_eq!(observed_edit.inverse(), selected_edit.inverse());

        self.evidence.borrow_mut().review = Some(Review {
            selected,
            competing,
            continuation,
            patch: selected_edit.patch().clone(),
            inverse: selected_edit.inverse().clone(),
            retained,
            preview_claim,
            preview_continuation,
        });
        self.pending = Some([selected_edit, competing_edit]);
    }

    fn prepare_frame(&mut self, context: &mut EditorPanelContext<'_, '_>) {
        assert!(context.model().document().is_none());
        if self.evidence.borrow().review.is_none() {
            self.review(context);
            return;
        }
        let mut evidence = self.evidence.borrow_mut();
        if let Some(pending) = self.pending.take() {
            if evidence.held_frames < 2 {
                evidence.held_frames += 1;
                self.pending = Some(pending);
                return;
            }
            evidence.submissions =
                Some(pending.map(|edit| context.commit_authoring_edit(edit).unwrap()));
        }
    }
}

impl EditorPanel for ClaimPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.evidence.borrow_mut().outcomes.push((id, result));
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
        panic!("Browser Open refused the claim fixture: {opening:?}");
    };
    let request = *request;
    wait(app, |current| current.document_open().result().is_some());
    let result = frame(app).document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        panic!("Browser Open did not publish the claim fixture: {result:?}");
    };
    assert_eq!(completed, request);
    let acknowledged = submit(
        app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: actual }
        ))
    ] if *actual == request));
    document
}

fn persist(
    app: &mut EditorApplication,
    action: EditorPersistenceAction,
) -> (EditorPersistenceAttemptId, EditorPersistenceResult) {
    let submission = submit(app, EditorAction::Persistence(action));
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { attempt },
        )),
    ] = submission.actions()
    else {
        panic!("persistence request was not accepted: {submission:?}");
    };
    let attempt = *attempt;
    settle_persistence(app);
    let EditorPersistenceView::Terminal {
        attempt: completed,
        result,
    } = frame(app).persistence()
    else {
        panic!("the accepted attempt must publish a terminal result");
    };
    assert_eq!(completed, attempt);
    let acknowledged = submit(
        app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt: actual }
        ))
    ] if *actual == attempt));
    (attempt, result)
}

#[test]
fn a_registered_panel_discovers_stored_prefab_claims_and_reverts_one_on_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_discovers_stored_prefab_claims_and_reverts_one_on_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let [left, right, reverted] = instances();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    let original_document = scene(&left, &right);
    let original = match kind {
        EditorDocumentKind::Scene => original_document.to_json_string(),
        EditorDocumentKind::Prefab => {
            PrefabDocument::new(original_document.placements, original_document.entities)
                .to_json_string()
        }
        EditorDocumentKind::Extension(_) => unreachable!("Scene and Prefab claim fixtures"),
    }
    .unwrap();
    let prototype = base().to_json_string().unwrap();
    fs::write(project.root.join(STARTUP), &startup).unwrap();
    let source_file = project.root.join(source);
    fs::write(&source_file, &original).unwrap();
    fs::write(project.root.join(BASE), &prototype).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas().to_json_string().unwrap(),
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
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
            ] if report.applied)
        );
    }
    let initial = frame(&app);
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, kind);
    assert!(initial_tab.source_binding.is_some());
    assert!(!initial_tab.dirty);
    assert_eq!(initial_tab.history.undo_depth, 0);
    assert_eq!(initial_tab.history.redo_depth, 0);
    assert_eq!(initial.scene_preview_document(), Some(active));
    let captured = basis(&app, document);
    let assert_navigation = |app: &EditorApplication| {
        let current = frame(app);
        assert_eq!(current.workspace().active_document, Some(active));
        assert_eq!(
            current.workspace().active_scene,
            initial.workspace().active_scene
        );
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(
            current.workspace().documents.len(),
            initial.workspace().documents.len()
        );
        assert_eq!(
            current.workspace().document(active),
            initial.workspace().document(active)
        );
        assert_eq!(
            current
                .workspace()
                .document(document)
                .unwrap()
                .source_binding,
            initial_tab.source_binding
        );
    };

    let evidence = Rc::new(RefCell::new(Evidence::default()));
    let shared = Rc::clone(&evidence);
    let mut host = EditorPanelHost::new();
    host.register_panel(
        &mut app,
        EditorPanelDescriptor::new(
            PANEL,
            "Prefab property claims",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            true,
        ),
        move || ClaimPanel {
            document,
            pending: None,
            evidence: Rc::clone(&shared),
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
    for _ in 0..4 {
        let candidate = host.prepare(&app, ClaimPanel::prepare_frame).unwrap();
        assert_eq!(frame(&app).workspace(), initial.workspace());
        assert!(evidence.borrow().outcomes.is_empty());
        assert_eq!(fs::read_to_string(&source_file).unwrap(), original);
        let submissions = evidence.borrow().submissions;
        if let Some([selected, competing]) = submissions {
            assert_eq!(
                candidate.submissions().collect::<Vec<_>>(),
                [(panel, selected), (panel, competing)]
            );
        } else {
            assert_eq!(candidate.submissions().len(), 0);
        }
        host.submit_and_drive(&mut app, candidate, Duration::ZERO);
        if submissions.is_none() {
            assert_eq!(frame(&app).workspace(), initial.workspace());
        }
        assert_navigation(&app);
    }
    drop(host);

    let mut evidence = evidence.borrow_mut();
    assert_eq!(evidence.held_frames, 2);
    let [accepted_id, stale_id] = evidence.submissions.unwrap();
    assert_ne!(accepted_id, stale_id);
    let [
        (accepted, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(applied))),
        (stale, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(rejected))),
    ] = evidence.outcomes.as_slice()
    else {
        panic!("both original panel submissions must complete");
    };
    assert_eq!((*accepted, *stale), (accepted_id, stale_id));
    assert!(applied.applied, "{:?}", applied.diagnostics);
    assert_eq!(applied.document, Some(document));
    assert_eq!(applied.active_document, Some(active));
    assert!(!rejected.applied);
    assert_eq!(rejected.document, Some(document));
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
    }));
    let review = evidence.review.take().unwrap();
    drop(evidence);
    assert_eq!(review.selected.path(), &PrefabInstancePath::new(id("left")));
    assert_eq!(
        review.competing.path(),
        &PrefabInstancePath::new(id("right"))
    );
    assert_eq!(review.selected.basis(), &captured);
    assert_eq!(review.competing.basis(), &captured);
    let patch = |expected: &PrefabInstance, value: &PrefabInstance| {
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: id("left"),
            expected: Some(expected.clone()),
            value: Some(value.clone()),
        }])
    };
    assert_eq!(review.patch, patch(&left, &reverted));
    assert_eq!(review.inverse, patch(&reverted, &left));

    let stale_query = EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Continue(review.continuation),
        limits: query_limits(),
    };
    let assert_stale = |app: &EditorApplication, rejection: EditorDocumentQueryRejection| {
        assert_eq!(
            app.query_document_authoring(stale_query.clone()),
            Err(rejection.clone())
        );
        for claim in [&review.selected, &review.competing] {
            assert_eq!(
                app.read_prefab_property_claim(claim, read_limits()),
                Err(rejection.clone())
            );
            assert_eq!(
                app.prepare_prefab_property_revert(claim, Default::default())
                    .unwrap_err(),
                EditorEditPreparationError::QueryBasis(rejection.clone())
            );
        }
    };
    assert_stale(&app, EditorDocumentQueryRejection::StaleRevision);
    let assert_claims = |app: &EditorApplication, is_reverted| {
        let current = basis(app, document);
        let (expected, field, intent) = if is_reverted {
            (&reverted, "note", PrefabPropertyIntent::Absent)
        } else {
            (&left, "score", score(LEFT_SCORE))
        };
        for (original, expected_instance, field, expected_intent) in [
            (&review.selected, expected, field, intent),
            (&review.competing, &right, "score", score(RIGHT_SCORE)),
        ] {
            let path = original.path();
            let observation = app
                .observe_prefab_instance(&current, path, Default::default())
                .unwrap();
            assert_eq!(observation.instance(), Some(expected_instance));
            let (rows, _) = discover(&current, path, |query| {
                app.query_document_authoring(query).unwrap()
            });
            let claim = find_claim(
                &rows,
                "leaf",
                COMPONENT,
                ComponentSchemaVersion::ONE,
                field,
                expected_intent != PrefabPropertyIntent::Absent,
            );
            assert_eq!(claim.slot(), original.slot());
            assert_ne!(&claim, original);
            assert_eq!(
                app.read_prefab_property_claim(&claim, read_limits()),
                Ok(expected_intent)
            );
        }
        assert_navigation(app);
    };
    let assert_history = |app: &EditorApplication, dirty, undo, redo| {
        let current = frame(app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            (dirty, undo, redo)
        );
    };
    let replay = |app: &mut EditorApplication, redo| {
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
        assert!(
            matches!(submit(app, EditorAction::Mutate(mutation)).actions(), [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
            ] if report.applied && report.document == Some(document))
        );
        assert_claims(app, redo);
    };
    assert_claims(&app, true);
    assert_history(&app, true, 1, 0);
    replay(&mut app, false);
    assert_history(&app, false, 0, 1);

    let preview = review.retained.preview();
    assert_eq!(preview.basis(), &captured);
    assert_ne!(preview.basis(), &basis(&app, document));
    let page = preview
        .query_authoring(review.preview_continuation)
        .unwrap();
    assert!(matches!(
        page.rows.as_slice(),
        [EditorAuthoringRow::PrefabPropertyClaim { claim, .. }] if claim == &review.preview_claim
    ));
    assert_eq!(
        preview.read_prefab_property_claim(&review.preview_claim, read_limits()),
        Ok(PrefabPropertyIntent::Absent)
    );
    drop(review.retained);

    replay(&mut app, true);
    assert_history(&app, true, 1, 0);
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original);

    let saving = basis(&app, document);
    let (saved_attempt, saved) = persist(
        &mut app,
        EditorPersistenceAction::Save {
            document,
            base_revision: saving.revision,
        },
    );
    assert!(matches!(saved, EditorPersistenceResult::Saved {
            document: actual, revision, ..
        } if actual == document && revision == saving.revision));
    assert_history(&app, false, 1, 0);
    let saved_bytes = fs::read(&source_file).unwrap();
    let expected = scene(&reverted, &right);
    match kind {
        EditorDocumentKind::Scene => assert_eq!(
            SceneDocumentCandidate::decode_json_bytes(&saved_bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            expected,
        ),
        EditorDocumentKind::Prefab => assert_eq!(
            PrefabDocumentCandidate::decode_json_bytes(&saved_bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            PrefabDocument::new(expected.placements, expected.entities),
        ),
        EditorDocumentKind::Extension(_) => unreachable!("Scene and Prefab claim fixtures"),
    }

    replay(&mut app, false);
    assert_history(&app, true, 0, 1);
    assert_eq!(fs::read(&source_file).unwrap(), saved_bytes);
    let reopening = basis(&app, document);
    let (reopened_attempt, reopened) = persist(
        &mut app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: reopening.revision,
            },
        },
    );
    assert_ne!(reopened_attempt, saved_attempt);
    let reopened_basis = basis(&app, document);
    assert!(matches!(reopened, EditorPersistenceResult::Opened {
            document: actual, revision, ..
        } if actual == document && revision == reopened_basis.revision));
    assert_ne!(
        reopened_basis.revision.source_id(),
        captured.revision.source_id()
    );
    assert_stale(&app, EditorDocumentQueryRejection::StaleIncarnation);
    assert_claims(&app, true);
    assert_history(&app, false, 0, 0);
    assert_eq!(fs::read(&source_file).unwrap(), saved_bytes);
    assert_eq!(
        fs::read_to_string(project.root.join(BASE)).unwrap(),
        prototype
    );
    assert_eq!(
        fs::read_to_string(project.root.join(STARTUP)).unwrap(),
        startup
    );
    assert!(app.close());
}
