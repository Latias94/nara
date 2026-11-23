use super::{
    set::{instance_patch, with_claim_intents},
    *,
};

use nara::{
    diagnostic::DiagnosticValueRef,
    identity::AuthoredEntityReference,
    reflect::{
        ComponentPropertyKey, ComponentPropertyStep, ComponentShapeMember, ComponentValueShape,
    },
    scene::SceneAuthoringRevision,
};

const ROUTES: &str = "routes";
const KEY: &str = "primary";
const SOCKET: &str = "socket";

#[test]
fn a_registered_panel_sets_and_reverts_a_structured_property_on_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_sets_and_reverts_a_structured_property_on_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let right = PrefabInstance::root(
        AssetRef::path("inspection-base.prefab.json").unwrap(),
        base_key(),
        PrefabEntityTarget::source(id("base-root")),
    );
    let note = PrefabPropertyIntent::Set(ComponentValue::String("left note".into()));
    let left = with_claim_intents(&right, [("note", note)]);
    let containing = |left: &PrefabInstance| {
        SceneDocument::from_ordered_roots([("left", left), ("right", &right)].map(
            |(anchor, instance)| {
                let mut record = SceneEntityRecord::new(id(anchor));
                record.prefab = Some(instance.clone());
                record
            },
        ))
    };
    let original = containing(&left);
    let original_bytes = match kind {
        EditorDocumentKind::Scene => original.to_json_string(),
        EditorDocumentKind::Prefab => {
            PrefabDocument::new(original.placements.clone(), original.entities.clone())
                .to_json_string()
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    let initial_base = base_source(INITIAL_SCORE);
    let source_file = project.root.join(source);
    let base_file = project.root.join(BASE);
    let schema = structured_schema().to_json_string().unwrap();
    for (path, bytes) in [
        (source, &original_bytes),
        (BASE, &initial_base),
        (STARTUP, &startup),
        ("schema/authoring.bundle.json", &schema),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
    }

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
    let activate = EditorNavigationAction::SetActiveDocument { document: active };
    assert!(matches!(
        submit(&mut app, EditorAction::Navigate(activate)).actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
            if report.applied
    ));
    let assert_history = |app: &EditorApplication, expected| {
        let current = frame(app);
        let workspace = current.workspace();
        let tab = workspace.document(document).unwrap();
        assert_eq!(workspace.active_document, Some(active));
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(tab.kind, kind);
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            expected
        );
    };
    let captured = basis(&app, document);
    let assert_unpublished = |app: &EditorApplication| {
        assert_eq!(basis(app, document), captured);
        assert_history(app, (false, 0, 0));
        assert_eq!(fs::read(&source_file).unwrap(), original_bytes.as_bytes());
        assert_eq!(fs::read(&base_file).unwrap(), initial_base.as_bytes());
    };

    let mut panel = PanelDriver::register(&mut app);
    let select_left = |panel: &mut PanelDriver, app: &mut EditorApplication, value, pinned| {
        inspect_row(panel, app, document, "left", value, pinned)
    };
    let inherited = projected_inheritance("left", INITIAL_SCORE);
    let raw_alias = authored(address("right", ROUTES).entity);
    let proposed = |key, second| pinned_row(key, raw_alias.clone(), second);
    let raw = proposed(KEY, local("left"));
    let projected = pinned_row(
        KEY,
        authored(PrefabEntityTarget::source(id("right"))),
        authored(PrefabEntityTarget::source(id("left"))),
    );
    assert_ne!(raw, projected);
    let mut pinned = left.clone();
    let [PrefabComponentIntent::Properties { claims, .. }] =
        pinned.overrides.as_mut().unwrap().components.as_mut_slice()
    else {
        unreachable!();
    };
    claims.push(PrefabPropertyClaim {
        path: row_property(),
        intent: PrefabPropertyIntent::Set(raw.clone()),
    });

    let (inspection, selected) = select_left(&mut panel, &mut app, &inherited, false);
    assert_eq!(
        prepare_set(
            &mut panel,
            &mut app,
            inspection,
            &selected,
            proposed(KEY, ComponentValue::String("left".into())),
        ),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::InvalidProperty,
        },
    );
    expect_phase(&mut app, inspection, Phase::Ready);
    assert_unpublished(&app);
    cancel(&mut panel, &mut app, inspection);
    let missing_route = authored(occurrence_target("right", "missing-socket"));
    for (value, failure) in [
        (proposed("renamed", local("left")), "component"),
        (proposed(KEY, missing_route), "missing-target"),
    ] {
        let (inspection, selected) = select_left(&mut panel, &mut app, &inherited, false);
        assert_eq!(
            prepare_set(&mut panel, &mut app, inspection, &selected, value),
            preparing(inspection)
        );
        reject_candidate(&mut panel, &mut app, inspection, failure);
        assert_unpublished(&app);
    }

    let assert_rows = |panel: &mut PanelDriver, app: &mut EditorApplication, pinned, weight| {
        for (anchor, claimed) in [("left", pinned), ("right", false)] {
            let inherited = projected_inheritance(anchor, weight);
            let expected = if claimed { &projected } else { &inherited };
            let (inspection, _) = inspect_row(panel, app, document, anchor, expected, claimed);
            cancel(panel, app, inspection);
        }
    };
    let (writing, selected) = select_left(&mut panel, &mut app, &inherited, false);
    let declaration_limits = PrefabDeclarationLimits::default();
    let submit_with_depth = |panel: &mut PanelDriver, app: &mut EditorApplication, depth| {
        let limits = PrefabDeclarationLimits::new(
            declaration_limits.max_nodes(),
            declaration_limits.max_logical_bytes(),
            depth,
        )
        .unwrap();
        panel.dispatch(app, |_| Action::PreparePropertyWrite {
            inspection: writing,
            address: selected.clone(),
            intent: PrefabPropertyIntent::Set(raw.clone()),
            limits,
        })
    };
    assert_eq!(
        submit_with_depth(&mut panel, &mut app, 6),
        Outcome::Rejected {
            inspection: Some(writing),
            reason: Rejection::BudgetExceeded,
        }
    );
    expect_phase(&mut app, writing, Phase::Ready);
    assert_unpublished(&app);
    assert_eq!(
        submit_with_depth(&mut panel, &mut app, 7),
        preparing(writing)
    );
    assert_reviewed_row(&mut panel, &mut app, writing, &left, &pinned, Some(&raw));
    panel.observe(&mut app, |context| {
        let effective = context.prefab_effective_observation(writing).unwrap();
        let component = effective
            .target(&selected.entity)
            .unwrap()
            .component(&selected.component)
            .unwrap();
        let observed = component
            .property_value(&selected.property, &mut read_budget())
            .unwrap();
        let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
            panic!("the candidate must contain the resolved keyed record");
        };
        assert_eq!(value.copy_value(Default::default()).unwrap(), projected);
        assert_eq!(observed.revert_claim().unwrap().path, selected.property);
    });
    assert_unpublished(&app);
    let revision = commit(&mut panel, &mut app, writing, document);
    assert_eq!(
        frame(&app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Written {
            inspection: writing,
            document,
            revision,
            kind: AuthoringPrefabPropertyClaimKind::Set,
        })
    );
    acknowledge(&mut panel, &mut app, writing);
    assert_history(&app, (true, 1, 0));
    assert_rows(&mut panel, &mut app, true, INITIAL_SCORE);
    for (redo, history) in [(false, (false, 0, 1)), (true, (true, 1, 0))] {
        replay(&mut app, document, redo);
        assert_history(&app, history);
        assert_rows(&mut panel, &mut app, redo, INITIAL_SCORE);
    }
    assert_eq!(fs::read(&source_file).unwrap(), original_bytes.as_bytes());

    save(&mut app, document);
    let saved = fs::read(&source_file).unwrap();
    assert_saved_source(kind, &saved, &containing(&pinned));
    reopen(&mut app, document);
    assert_history(&app, (false, 0, 0));
    let changed_base = base_source(CHANGED_SCORE);
    assert_eq!(fs::read(&base_file).unwrap(), initial_base.as_bytes());
    fs::write(&base_file, &changed_base).unwrap();

    let (reverting, selected) = select_left(&mut panel, &mut app, &projected, true);
    let mut member = selected.clone();
    member
        .property
        .steps
        .push(ComponentPropertyStep::Member(ComponentFieldId::new(
            "weight",
        )));
    assert_eq!(
        panel.dispatch(&mut app, |_| Action::PrepareRevert {
            inspection: reverting,
            address: member,
            limits: Default::default(),
        }),
        Outcome::Rejected {
            inspection: Some(reverting),
            reason: Rejection::NoCurrentClaim,
        }
    );
    let before_revert = basis(&app, document);
    prepare_revert(
        &mut panel,
        &mut app,
        reverting,
        selected,
        Default::default(),
    );
    assert_reviewed_row(&mut panel, &mut app, reverting, &pinned, &left, None);
    assert_eq!(basis(&app, document), before_revert);
    assert_history(&app, (false, 0, 0));
    let revision = commit(&mut panel, &mut app, reverting, document);
    assert_eq!(
        frame(&app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Reverted {
            inspection: reverting,
            document,
            revision,
        })
    );
    acknowledge(&mut panel, &mut app, reverting);
    assert_history(&app, (true, 1, 0));
    assert_rows(&mut panel, &mut app, false, CHANGED_SCORE);
    assert_eq!(fs::read(&source_file).unwrap(), saved);
    save(&mut app, document);
    assert_saved_source(kind, &fs::read(&source_file).unwrap(), &original);
    reopen(&mut app, document);
    assert_history(&app, (false, 0, 0));
    assert_rows(&mut panel, &mut app, false, CHANGED_SCORE);
    assert_eq!(fs::read(&base_file).unwrap(), changed_base.as_bytes());
    assert_eq!(
        fs::read(project.root.join(STARTUP)).unwrap(),
        startup.as_bytes()
    );
    assert!(app.close());
}

fn structured_schema() -> AuthoringSchemaBundle {
    let bundle = schema();
    let owner = &bundle.owners()[0];
    let mut catalog = owner.current().clone();
    let settings = catalog
        .components
        .iter_mut()
        .find(|schema| schema.id().as_str() == COMPONENT)
        .unwrap();
    let key = ComponentFieldId::new("id");
    let item = ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_key", ComponentValueKind::String).with_id(key.clone()),
        ComponentShapeMember::new("wire_weight", ComponentValueKind::U64)
            .with_id(ComponentFieldId::new("weight")),
        ComponentShapeMember::new(
            "wire_links",
            ComponentValueShape::map(ComponentValueShape::optional(
                ComponentValueShape::EntityRef,
            )),
        ),
    ]);
    settings.fields.push(
        ComponentFieldSchema::required(
            ComponentFieldId::new(ROUTES),
            "Routes",
            ComponentFieldPath::from_fields(["wire_routes"]),
            ComponentValueShape::keyed_list(item, key),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef),
    );
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.owner_id().clone(),
        catalog,
        None,
    )])
    .unwrap()
}

fn row(key: &str, weight: u64, links: [(&str, ComponentValue); 3]) -> ComponentValue {
    ComponentValue::map([
        ("wire_key", ComponentValue::String(key.into())),
        ("wire_weight", ComponentValue::U64(weight)),
        ("wire_links", ComponentValue::map(links)),
    ])
}

fn pinned_row(key: &str, first: ComponentValue, second: ComponentValue) -> ComponentValue {
    row(
        key,
        37,
        [
            ("first", first),
            ("second", second),
            ("fallback", ComponentValue::Null),
        ],
    )
}

fn inherited_row(weight: u64, root: ComponentValue, socket: ComponentValue) -> ComponentValue {
    row(
        KEY,
        weight,
        [
            ("first", root),
            ("second", socket.clone()),
            ("legacy", socket),
        ],
    )
}

fn base_source(weight: u64) -> String {
    let mut document = base(INITIAL_SCORE);
    document.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["wire_routes"]),
            ComponentValue::List(vec![inherited_row(
                weight,
                local("base-root"),
                local(SOCKET),
            )]),
        )
        .unwrap();
    document.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(SOCKET)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("base-root"),
        ))),
    ));
    document.entities.push(SceneEntityRecord::new(id(SOCKET)));
    document.to_json_string().unwrap()
}

fn local(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(entity) })
}

fn authored(target: PrefabEntityTarget) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::from(target),
    })
}

fn occurrence_target(anchor: &str, entity: &str) -> PrefabEntityTarget {
    let mut target = address(anchor, ROUTES).entity;
    target.entity = PrefabEntityOrigin::SourceEntity(id(entity));
    target
}

fn projected_inheritance(anchor: &str, weight: u64) -> ComponentValue {
    inherited_row(
        weight,
        authored(PrefabEntityTarget::source(id(anchor))),
        authored(occurrence_target(anchor, SOCKET)),
    )
}

fn row_property() -> ComponentPropertyPath {
    property(ROUTES).with_steps([ComponentPropertyStep::ListItem {
        by: ComponentFieldId::new("id"),
        key: ComponentPropertyKey::String(KEY.into()),
    }])
}

fn inspect_row(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    document: EditorDocumentId,
    anchor: &str,
    expected: &ComponentValue,
    pinned: bool,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    let inspection = start(panel, app, document, anchor);
    expect_phase(app, inspection, Phase::Ready);
    let selected = EditorPrefabPropertyAddress {
        property: row_property(),
        ..address(anchor, ROUTES)
    };
    panel.observe(app, |context| {
        let observation = context.prefab_effective_observation(inspection).unwrap();
        let component = observation
            .target(&selected.entity)
            .unwrap()
            .component(&selected.component)
            .unwrap();
        let observed = component
            .property_value(&selected.property, &mut read_budget())
            .unwrap();
        assert_eq!(observed.controlling_claim(), observed.revert_claim());
        assert_eq!(observed.revert_claim().is_some(), pinned);
        if let Some(claim) = observed.revert_claim() {
            assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Set);
            assert_eq!(claim.path, selected.property);
            assert_eq!(
                claim.location.source(),
                &PrefabDeclarationSource::Containing
            );
            assert_eq!(claim.location.path(), &PrefabInstancePath::new(id(anchor)));
        }
        let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
            panic!("the selected keyed record must be present: {observed:?}");
        };
        let limits = ComponentValueMaterializationLimits::new(
            ItemLimit::new(32).unwrap(),
            ByteLimit::new(4096).unwrap(),
            DepthLimit::new(8).unwrap(),
        );
        assert_eq!(&value.copy_value(limits).unwrap(), expected);
    });
    (inspection, selected)
}

fn read_budget() -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(ItemLimit::new(32).unwrap(), ByteLimit::new(4096).unwrap())
}

fn prepare_set(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    selected: &EditorPrefabPropertyAddress,
    value: ComponentValue,
) -> Outcome {
    panel.dispatch(app, |_| Action::PreparePropertyWrite {
        inspection,
        address: selected.clone(),
        intent: PrefabPropertyIntent::Set(value),
        limits: Default::default(),
    })
}

fn preparing(inspection: EditorPrefabInspectionId) -> Outcome {
    Outcome::Accepted {
        inspection,
        phase: Phase::PreparingEdit,
    }
}

fn reject_candidate(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    failure_kind: &str,
) {
    expect_phase(app, inspection, Phase::Terminal);
    let current = frame(app);
    let result = &current.prefab_inspection().result;
    let Some(EditorPrefabInspectionResult::Failed {
        inspection: actual,
        reason: Rejection::SourceRejected,
        diagnostics,
    }) = result
    else {
        panic!("the structured candidate must be rejected: {result:?}");
    };
    assert_eq!(*actual, inspection);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "failure-kind"
                        && field.value() == DiagnosticValueRef::Identifier(failure_kind)
                })
        }),
        "{diagnostics:?}"
    );
    acknowledge(panel, app, inspection);
}

fn assert_reviewed_row(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    before: &PrefabInstance,
    after: &PrefabInstance,
    raw: Option<&ComponentValue>,
) {
    expect_phase(app, inspection, Phase::Review);
    panel.observe(app, |context| {
        let review = context.prefab_inspection_review(inspection).unwrap();
        assert_eq!(review.patch(), &instance_patch(before, after));
        assert_eq!(review.inverse(), &instance_patch(after, before));
        let preview = review.preview();
        let page = preview
            .query_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: preview.basis().clone(),
                    entity: id("left"),
                    projection: EditorAuthoringProjection::PrefabPropertyClaims { steps: vec![] },
                }),
                limits: EditorAuthoringQueryLimits::new(16, 8, 4096).unwrap(),
            })
            .unwrap();
        assert!(page.next.is_none());
        let path = row_property();
        let value = page.rows.iter().find_map(|row| match row {
            EditorAuthoringRow::PrefabPropertyClaim {
                claim,
                property,
                has_value: true,
                ..
            } if property == &path => Some(
                preview
                    .read_prefab_property_claim(
                        claim,
                        EditorPropertyReadLimits::new(1024, 32, 4096).unwrap(),
                    )
                    .unwrap(),
            ),
            _ => None,
        });
        assert_eq!(page.rows.len(), if raw.is_some() { 2 } else { 1 });
        assert_eq!(value, raw.cloned().map(PrefabPropertyIntent::Set));
    });
}

fn commit(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    document: EditorDocumentId,
) -> SceneAuthoringRevision {
    let before = basis(app, document).revision;
    assert_eq!(
        panel.dispatch(app, |_| Action::Commit { inspection }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Validating,
        }
    );
    expect_phase(app, inspection, Phase::Terminal);
    let revision = basis(app, document).revision;
    assert_ne!(revision, before);
    revision
}

fn reopen(app: &mut EditorApplication, document: EditorDocumentId) {
    let before = basis(app, document).revision;
    let result = persist(
        app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: before,
            },
        },
    );
    let reopened = basis(app, document).revision;
    assert!(
        matches!(result, EditorPersistenceResult::Opened {
            document: actual, revision, ..
        } if actual == document && revision == reopened),
        "{result:?}"
    );
    assert_ne!(reopened.source_id(), before.source_id());
}
