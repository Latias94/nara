use super::{set::instance_patch, *};

use nara::{
    diagnostic::DiagnosticValueRef,
    ecs::Component,
    identity::AuthoredEntityReference,
    reflect::{
        ComponentCodecError, ComponentRegistryBuilder, ComponentRegistrySnapshot,
        ComponentSchemaOwnerId, ComponentValueRef, ComponentValueShape,
    },
    scene::{InMemoryPrefabSourceResolver, SceneCompilationReport},
};

const LINK: &str = "consumer.prefab.EffectiveLink";
const BASE_ROOT: &str = "base-root";
const TARGET: &str = "target";
const DELETED: &str = "deleted-target";

#[derive(Component)]
struct Link {
    wire_target: Option<EntityReference>,
}

impl Link {
    fn schema() -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new(LINK),
            "Effective link",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new(TARGET),
            "Target",
            ComponentFieldPath::from_fields(["wire_target"]),
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)])
    }

    fn decode(value: ComponentValueRef<'_>) -> Result<Self, ComponentCodecError> {
        let wire_target = match value.get("wire_target") {
            Some(value) if value.kind() != ComponentValueKind::Null => {
                Some(value.as_entity_reference().cloned().ok_or_else(|| {
                    ComponentCodecError::invalid_field("wire_target", "entity reference")
                })?)
            }
            _ => None,
        };
        Ok(Self { wire_target })
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::map(self.wire_target.iter().map(
            |reference| {
                (
                    "wire_target",
                    ComponentValue::EntityReference(reference.clone()),
                )
            },
        )))
    }
}

fn local_reference(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn authored_reference(route: PrefabEntityTarget) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference::from(route),
    }
}

fn route(anchor: &str, entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: base_key(),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn reference_base() -> PrefabDocument {
    let link = Link {
        wire_target: Some(local_reference(TARGET)),
    };
    let children = [TARGET, DELETED].map(|child| {
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id(child)),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id(BASE_ROOT),
            ))),
        )
    });
    PrefabDocument::new(
        [SceneEntityPlacement::root(id(BASE_ROOT))]
            .into_iter()
            .chain(children),
        [
            SceneEntityRecord::new(id(BASE_ROOT)).with_component(
                ComponentTypeId::new(LINK),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, link.encode().unwrap())
                    .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            ),
            SceneEntityRecord::new(id(TARGET)),
            SceneEntityRecord::new(id(DELETED)),
        ],
    )
}

fn mount(overlay: PrefabOverlay) -> PrefabInstance {
    let mut instance = PrefabInstance::root(
        AssetRef::path("inspection-base.prefab.json").unwrap(),
        base_key(),
        PrefabEntityTarget::source(id(BASE_ROOT)),
    );
    instance.overrides = Some(overlay);
    instance
}

fn left_mount(intent: PrefabPropertyIntent) -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(base_key());
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id(BASE_ROOT)),
        component: ComponentTypeId::new(LINK),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: property(TARGET),
            intent,
        }],
    });
    mount(overlay)
}

fn right_mount() -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(base_key());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id(DELETED)));
    mount(overlay)
}

fn inspect_link(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    document: EditorDocumentId,
    kind: AuthoringPrefabPropertyClaimKind,
    endpoint: Option<EntityReference>,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    let inspection = start(panel, app, document, "left");
    expect_phase(app, inspection, Phase::Ready);
    let address = panel.observe(app, |context| {
        let effective = context.prefab_effective_observation(inspection).unwrap();
        let expected = route("left", BASE_ROOT);
        let entity = effective
            .targets()
            .find(|entity| *entity == &expected)
            .unwrap();
        let component = effective
            .target(entity)
            .unwrap()
            .component(&ComponentTypeId::new(LINK))
            .unwrap();
        let address = EditorPrefabPropertyAddress {
            entity: entity.clone(),
            component: component.fact().component().clone(),
            property: property(TARGET),
        };
        let read = component
            .property_value(
                &address.property,
                &mut ComponentPropertyReadBudget::new(
                    ItemLimit::new(16).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                ),
            )
            .unwrap();
        let claim = read.revert_claim().unwrap();
        assert_eq!(claim.kind, kind);
        assert_eq!(claim.path, address.property);
        assert_eq!(
            claim.location.source(),
            &PrefabDeclarationSource::Containing
        );
        assert_eq!(claim.location.path(), &PrefabInstancePath::new(id("left")));
        assert_eq!(read.controlling_claim(), Some(claim));
        match (endpoint, read.value) {
            (None, AuthoringPrefabPropertyValue::Missing) => {}
            (Some(endpoint), AuthoringPrefabPropertyValue::Present(value)) => assert_eq!(
                value.copy_value(Default::default()).unwrap(),
                ComponentValue::EntityReference(endpoint)
            ),
            (_, value) => panic!("unexpected effective reference: {value:?}"),
        }
        address
    });
    (inspection, address)
}

fn prepare_set(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    address: EditorPrefabPropertyAddress,
    reference: &EntityReference,
) {
    let intent = PrefabPropertyIntent::Set(ComponentValue::EntityReference(reference.clone()));
    assert_eq!(
        panel.dispatch(app, |_| Action::PreparePropertyWrite {
            inspection,
            address,
            intent,
            limits: PrefabDeclarationLimits::default(),
        }),
        Outcome::Accepted {
            inspection,
            phase: Phase::PreparingEdit,
        }
    );
}

fn assert_reference_review(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    basis: &EditorDocumentQueryBasis,
    before: &PrefabInstance,
    after: &PrefabInstance,
    reference: &EntityReference,
) {
    expect_phase(app, inspection, Phase::Review);
    panel.observe(app, |context| {
        let review = context.prefab_inspection_review(inspection).unwrap();
        assert_eq!(review.patch(), &instance_patch(before, after));
        assert_eq!(review.inverse(), &instance_patch(after, before));
        let live = context
            .observe_prefab_instance(
                basis,
                &PrefabInstancePath::new(id("left")),
                Default::default(),
            )
            .unwrap();
        assert_eq!(live.instance.as_ref(), Some(before));
        let preview = review.preview();
        assert_eq!(preview.basis(), basis);
        let page = preview
            .query_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: basis.clone(),
                    entity: id("left"),
                    projection: EditorAuthoringProjection::PrefabPropertyClaims { steps: vec![] },
                }),
                limits: EditorAuthoringQueryLimits::new(16, 8, 4096).unwrap(),
            })
            .unwrap();
        assert!(page.next.is_none());
        let [
            EditorAuthoringRow::PrefabPropertyClaim {
                claim,
                property: selected,
                has_value: true,
                ..
            },
        ] = page.rows.as_slice()
        else {
            panic!(
                "the review must contain the sole Set claim: {:?}",
                page.rows
            );
        };
        assert_eq!(selected, &property(TARGET));
        assert_eq!(
            preview
                .read_prefab_property_claim(
                    claim,
                    EditorPropertyReadLimits::new(1024, 16, 1024).unwrap(),
                )
                .unwrap(),
            PrefabPropertyIntent::Set(ComponentValue::EntityReference(reference.clone())),
        );
    });
}

fn commit_set(
    panel: &mut PanelDriver,
    app: &mut EditorApplication,
    inspection: EditorPrefabInspectionId,
    before: &EditorDocumentQueryBasis,
) {
    assert_eq!(
        panel.dispatch(app, |_| Action::Commit { inspection }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Validating,
        }
    );
    expect_phase(app, inspection, Phase::Terminal);
    let revision = basis(app, before.document).revision;
    assert_ne!(revision, before.revision);
    assert_eq!(
        frame(app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Written {
            inspection,
            document: before.document,
            revision,
            kind: AuthoringPrefabPropertyClaimKind::Set,
        })
    );
    acknowledge(panel, app, inspection);
}

fn compile_saved(
    kind: EditorDocumentKind,
    bytes: &[u8],
    registry: &ComponentRegistrySnapshot,
    sources: &InMemoryPrefabSourceResolver,
) -> SceneCompilationReport {
    match kind {
        EditorDocumentKind::Scene => SceneDocumentCandidate::decode_json_bytes(bytes)
            .unwrap()
            .into_composition_source()
            .unwrap()
            .compile(registry, sources),
        EditorDocumentKind::Prefab => PrefabDocumentCandidate::decode_json_bytes(bytes)
            .unwrap()
            .into_composition_source()
            .unwrap()
            .compile(registry, sources),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
}

fn assert_compiled_links(report: SceneCompilationReport, endpoint: &str) {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let compiled = report.document().unwrap();
    let entity = |name| {
        let id = id(name);
        compiled
            .entities()
            .iter()
            .find(|entity| entity.id() == &id)
            .unwrap()
    };
    let left = entity("left");
    let right = entity("right");
    let [right_target] = right.children() else {
        panic!("the sibling must contain one surviving target");
    };
    let endpoint = entity(endpoint).id();
    assert!(compiled.roots().contains(endpoint));
    assert_ne!(right_target, &id(TARGET));
    let component = ComponentTypeId::new(LINK);
    for (record, target) in [(left, endpoint), (right, right_target)] {
        assert_eq!(
            record.components()[&component]
                .value()
                .field_entity_reference("wire_target")
                .unwrap(),
            &EntityReference::SceneLocal {
                entity: target.clone(),
            },
        );
    }
}

#[test]
fn a_registered_panel_sets_effective_entity_references_on_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_sets_effective_entity_references_on_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    use AuthoringPrefabPropertyClaimKind::{Absent, Set};

    let project = text_project::TextProject::new();
    let absent = left_mount(PrefabPropertyIntent::Absent);
    let right = right_mount();
    let containing = |left: &PrefabInstance| {
        let mut left_anchor = SceneEntityRecord::new(id("left"));
        left_anchor.prefab = Some(left.clone());
        let mut right_anchor = SceneEntityRecord::new(id("right"));
        right_anchor.prefab = Some(right.clone());
        SceneDocument::from_ordered_roots([
            left_anchor,
            right_anchor,
            SceneEntityRecord::new(id(TARGET)),
        ])
    };
    let original = containing(&absent);
    let original_bytes = match kind {
        EditorDocumentKind::Scene => original.to_json_string(),
        EditorDocumentKind::Prefab => {
            PrefabDocument::new(original.placements, original.entities).to_json_string()
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    let base_bytes = reference_base().to_json_string().unwrap();
    let link_schema = Link::schema();
    let catalog = ComponentSchemaCatalog {
        components: vec![link_schema.clone()],
        ..Default::default()
    };
    let mut registry = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new(OWNER),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry
        .register_persistent_component_with_codec(link_schema, Link::decode, Link::encode)
        .unwrap();
    let registry = registry.freeze().unwrap();
    let schema = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        catalog,
        None,
    )])
    .unwrap()
    .to_json_string()
    .unwrap();
    for (path, bytes) in [
        (source, &original_bytes),
        (BASE, &base_bytes),
        (STARTUP, &startup),
        ("schema/authoring.bundle.json", &schema),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
    }
    let source_file = project.root.join(source);
    let base_file = project.root.join(BASE);
    let base = PrefabDocumentCandidate::decode_json_bytes(&fs::read(&base_file).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    let sources =
        InMemoryPrefabSourceResolver::new().with_prefab(right.source.clone(), base_key(), base);

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
        assert!(matches!(
            submit(&mut app, EditorAction::Navigate(action)).actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Workspace(report)
            )] if report.applied
        ));
    }
    let initial = frame(&app);
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, kind);
    assert!(initial_tab.source_binding.is_some());
    let assert_workspace = |app: &EditorApplication, dirty, undo, redo| {
        let current = frame(app);
        let workspace = current.workspace();
        let tab = workspace.document(document).unwrap();
        assert_eq!(workspace.active_document, Some(active));
        assert_eq!(workspace.active_scene, initial.workspace().active_scene);
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(tab.source_binding, initial_tab.source_binding);
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            (dirty, undo, redo)
        );
    };
    let assert_unwritten = |expected: &[u8]| {
        assert_eq!(fs::read(&source_file).unwrap(), expected);
        assert_eq!(fs::read(&base_file).unwrap(), base_bytes.as_bytes());
    };
    let mut panel = PanelDriver::register(&mut app);
    let before = basis(&app, document);

    let (rejected, selected) = inspect_link(&mut panel, &mut app, document, Absent, None);
    let deleted = authored_reference(route("right", DELETED));
    prepare_set(&mut panel, &mut app, rejected, selected, &deleted);
    expect_phase(&mut app, rejected, Phase::Terminal);
    let terminal = frame(&app);
    let failure = &terminal.prefab_inspection().result;
    let Some(EditorPrefabInspectionResult::Failed {
        inspection,
        reason: Rejection::SourceRejected,
        diagnostics,
    }) = failure
    else {
        panic!("the deleted endpoint must reject preparation: {failure:?}");
    };
    assert_eq!(*inspection, rejected);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "failure-kind"
                        && field.value() == DiagnosticValueRef::Identifier("missing-target")
                })
        }),
        "{diagnostics:?}"
    );
    acknowledge(&mut panel, &mut app, rejected);
    assert_eq!(basis(&app, document), before);
    assert_workspace(&app, false, 0, 0);
    assert_unwritten(original_bytes.as_bytes());

    let sibling = authored_reference(route("right", BASE_ROOT));
    let sibling_endpoint = authored_reference(PrefabEntityTarget::source(id("right")));
    let sibling_mount = left_mount(PrefabPropertyIntent::Set(ComponentValue::EntityReference(
        sibling.clone(),
    )));
    let (restoring, selected) = inspect_link(&mut panel, &mut app, document, Absent, None);
    prepare_set(&mut panel, &mut app, restoring, selected, &sibling);
    assert_reference_review(
        &mut panel,
        &mut app,
        restoring,
        &before,
        &absent,
        &sibling_mount,
        &sibling,
    );
    assert_eq!(basis(&app, document), before);
    assert_workspace(&app, false, 0, 0);
    assert_unwritten(original_bytes.as_bytes());
    commit_set(&mut panel, &mut app, restoring, &before);
    let restored = basis(&app, document);
    assert_workspace(&app, true, 1, 0);
    assert_unwritten(original_bytes.as_bytes());

    let (repeating, selected) = inspect_link(
        &mut panel,
        &mut app,
        document,
        Set,
        Some(sibling_endpoint.clone()),
    );
    prepare_set(&mut panel, &mut app, repeating, selected, &sibling);
    assert_failure(
        &mut app,
        repeating,
        Rejection::AlreadyWritten,
        "scene.authoring-prefab-already-written",
    );
    acknowledge(&mut panel, &mut app, repeating);
    assert_eq!(basis(&app, document), restored);
    assert_workspace(&app, true, 1, 0);
    assert_unwritten(original_bytes.as_bytes());

    replay(&mut app, document, false);
    let (undone, _) = inspect_link(&mut panel, &mut app, document, Absent, None);
    cancel(&mut panel, &mut app, undone);
    assert_workspace(&app, false, 0, 1);
    replay(&mut app, document, true);
    assert_workspace(&app, true, 1, 0);
    save(&mut app, document);
    let sibling_bytes = fs::read(&source_file).unwrap();
    assert_saved_source(kind, &sibling_bytes, &containing(&sibling_mount));
    assert_compiled_links(
        compile_saved(kind, &sibling_bytes, &registry, &sources),
        "right",
    );
    assert_workspace(&app, false, 1, 0);

    let local = local_reference(TARGET);
    let local_endpoint = authored_reference(PrefabEntityTarget::source(id(TARGET)));
    let local_mount = left_mount(PrefabPropertyIntent::Set(ComponentValue::EntityReference(
        local.clone(),
    )));
    let before_local = basis(&app, document);
    let (retargeting, selected) = inspect_link(
        &mut panel,
        &mut app,
        document,
        Set,
        Some(sibling_endpoint.clone()),
    );
    prepare_set(&mut panel, &mut app, retargeting, selected, &local);
    assert_reference_review(
        &mut panel,
        &mut app,
        retargeting,
        &before_local,
        &sibling_mount,
        &local_mount,
        &local,
    );
    assert_eq!(basis(&app, document), before_local);
    assert_workspace(&app, false, 1, 0);
    assert_unwritten(&sibling_bytes);
    commit_set(&mut panel, &mut app, retargeting, &before_local);
    assert_workspace(&app, true, 2, 0);
    assert_unwritten(&sibling_bytes);
    save(&mut app, document);
    let local_bytes = fs::read(&source_file).unwrap();
    assert_saved_source(kind, &local_bytes, &containing(&local_mount));
    assert_compiled_links(
        compile_saved(kind, &local_bytes, &registry, &sources),
        TARGET,
    );
    assert_workspace(&app, false, 2, 0);

    replay(&mut app, document, false);
    let (unsaved, _) = inspect_link(&mut panel, &mut app, document, Set, Some(sibling_endpoint));
    cancel(&mut panel, &mut app, unsaved);
    assert_workspace(&app, true, 1, 1);
    let reopening = basis(&app, document).revision;
    let opened = persist(
        &mut app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: reopening,
            },
        },
    );
    let reopened = basis(&app, document).revision;
    assert!(
        matches!(opened, EditorPersistenceResult::Opened { document: actual, revision, .. }
            if actual == document && revision == reopened),
        "{opened:?}"
    );
    assert_ne!(reopening.source_id(), reopened.source_id());
    let (saved, _) = inspect_link(&mut panel, &mut app, document, Set, Some(local_endpoint));
    cancel(&mut panel, &mut app, saved);
    assert_workspace(&app, false, 0, 0);
    assert_unwritten(&local_bytes);
    assert_eq!(
        fs::read(project.root.join(STARTUP)).unwrap(),
        startup.as_bytes()
    );
    assert!(app.close());
}
