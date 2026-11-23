use super::*;

use nara::{
    ecs::Component,
    identity::{AuthoredEntityReference, AuthoredReferenceTarget, ReferenceInputId},
    reflect::{
        ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchemaOwnerId,
        PersistentComponent,
    },
    scene::{InMemoryPrefabSourceResolver, PrefabInstanceClaim, PrefabReferenceArguments},
};

const LINK: &str = "consumer.prefab.AppliedPointLink";
const ROOT: &str = "base-root";
const LOCAL_POINT: &str = "local-point";
const SOCKET: &str = "prefabs/apply-socket.prefab.json";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.prefab.AppliedPointLink",
    version = 1,
    alias = "Applied point link",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Link {
    #[nara(
        id = "target",
        alias = "Target",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    target: EntityReference,
}

#[test]
fn a_registered_panel_applies_a_nested_input_from_a_nonactive_scene() {
    apply_owned_point(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_applies_a_nested_input_from_a_nonactive_prefab() {
    apply_owned_point(EditorDocumentKind::Prefab, PREFAB);
}

#[test]
fn a_registered_panel_refuses_a_scene_owned_replacement_with_the_same_endpoint() {
    reject_outer_point(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_refuses_a_prefab_owned_replacement_with_the_same_endpoint() {
    reject_outer_point(EditorDocumentKind::Prefab, PREFAB);
}

fn apply_owned_point(kind: EditorDocumentKind, source: &'static str) {
    let (mut fixture, registry) = with_saved_point(kind, source, false);
    let saved_pin = fixture.stored_sources();
    let containing = fixture.containing;
    let destination = fixture.destination;
    let containing_before = basis(&fixture.app, containing);
    let destination_before = basis(&fixture.app, destination);
    let localized = point_operand(vec![socket_hop()]);
    let revert = instance_patch(&fixture.pinned, &fixture.cleared);
    let restore = instance_patch(&fixture.cleared, &fixture.pinned);

    let (inspection, address) = inspect_left(&mut fixture, true);
    fixture.prepare_apply_review(inspection, address);
    fixture.panel.observe(&mut fixture.app, |context| {
        let review = context.prefab_apply_review(inspection).unwrap();
        assert_eq!(review.containing.document_kind(), kind);
        assert_eq!(review.containing.preview().basis(), &containing_before);
        assert_eq!(review.containing.patch(), &revert);
        assert_eq!(review.containing.inverse(), &restore);
        assert_eq!(review.destination, &destination_before);
        let edit = review
            .destination_edit
            .expect("the destination must change");
        assert_eq!(edit.document_kind(), EditorDocumentKind::Prefab);
        assert_eq!(edit.preview().basis(), &destination_before);
        assert_eq!(edit.patch(), &link_patch(localized.clone()));
        assert_eq!(edit.inverse(), &link_patch(initial_target()));
    });
    assert_eq!(basis(&fixture.app, destination), destination_before);
    assert_eq!(basis(&fixture.app, containing), containing_before);
    assert_eq!(fixture.stored_sources(), saved_pin);

    let published = fixture.commit_destination(inspection, true);
    assert_ne!(published.revision, destination_before.revision);
    fixture.require_destination_save(inspection, published);
    assert_eq!(fixture.stored_sources(), saved_pin);
    fixture.assert_containing(true, (false, 1, 0));
    fixture.assert_destination_history((true, 1, 0));

    save(&mut fixture.app, destination);
    let saved_prefix = fixture.stored_sources();
    assert_eq!(saved_prefix.0, saved_pin.0);
    assert_eq!(
        decode_prefab(&saved_prefix.1),
        destination_document(localized)
    );
    assert_eq!(basis(&fixture.app, containing), containing_before);
    fixture.assert_containing(true, (false, 1, 0));

    fixture.finish(inspection, published);
    fixture.assert_containing(false, (true, 2, 0));
    fixture.assert_destination_history((false, 1, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);

    replay(&mut fixture.app, containing, false);
    fixture.assert_containing(true, (false, 1, 1));
    assert_eq!(
        basis(&fixture.app, destination).revision,
        published.revision
    );
    let undone_containing = basis(&fixture.app, containing);
    replay(&mut fixture.app, destination, false);
    fixture.assert_destination_history((true, 0, 1));
    replay(&mut fixture.app, destination, true);
    assert_eq!(basis(&fixture.app, containing), undone_containing);
    fixture.assert_destination_history((false, 1, 0));
    replay(&mut fixture.app, containing, true);
    fixture.assert_containing(false, (true, 2, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);

    save(&mut fixture.app, containing);
    let saved = fixture.stored_sources();
    assert_saved_source(
        kind,
        &saved.0,
        &containing_document(&fixture.cleared, &fixture.sibling),
    );
    assert_eq!(saved.1, saved_prefix.1);
    assert_compiled_points(&fixture, &registry);
    reopen(&mut fixture.app, destination);
    reopen(&mut fixture.app, containing);
    fixture.assert_containing(false, (false, 0, 0));
    assert!(fixture.app.close());
}

fn reject_outer_point(kind: EditorDocumentKind, source: &'static str) {
    let (mut fixture, _) = with_saved_point(kind, source, true);
    let saved_pin = fixture.stored_sources();
    let containing_before = basis(&fixture.app, fixture.containing);
    let destination_before = basis(&fixture.app, fixture.destination);
    let (inspection, address) = inspect_left(&mut fixture, true);
    assert_eq!(
        fixture.prepare(inspection, address, fixture.destination),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::InvalidProperty,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    assert!(
        frame(&fixture.app)
            .prefab_inspection()
            .apply_destination
            .is_none()
    );
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
    fixture.assert_containing(true, (false, 1, 0));
    fixture.assert_destination_history((false, 0, 0));
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    assert_eq!(basis(&fixture.app, fixture.destination), destination_before);
    assert_eq!(fixture.stored_sources(), saved_pin);
    assert!(fixture.app.close());
}

fn with_saved_point(
    kind: EditorDocumentKind,
    source: &'static str,
    outer_replacement: bool,
) -> (ApplyFixture, ComponentRegistrySnapshot) {
    let project = text_project::TextProject::new();
    let mut registry = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new(OWNER),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry.register_persistent_component::<Link>().unwrap();
    let registry = registry.freeze().unwrap();
    let schema = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        registry.catalog().clone(),
        None,
    )])
    .unwrap();

    let sibling = PrefabInstance::root(
        AssetRef::path("inspection-base.prefab.json").unwrap(),
        base_key(),
        PrefabEntityTarget::source(id(ROOT)),
    );
    let mut cleared = sibling.clone();
    let mut overlay = PrefabOverlay::new(base_key());
    if outer_replacement {
        overlay.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(id("socket")),
            value: Some(socket_instance(routed_entity(LOCAL_POINT))),
        });
    }
    cleared.overrides = Some(overlay);
    let override_value = point_operand(vec![left_hop(), socket_hop()]);
    let mut pinned = cleared.clone();
    pinned
        .overrides
        .as_mut()
        .unwrap()
        .components
        .push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id(ROOT)),
            component: ComponentTypeId::new(LINK),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: property("target"),
                intent: PrefabPropertyIntent::Set(override_value.clone()),
            }],
        });
    let mut socket =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("socket-body"))]);
    socket.inputs.insert(point());
    write_containing_source(
        &project,
        kind,
        source,
        containing_document(&cleared, &sibling),
    );
    for (path, bytes) in [
        (SOCKET, socket.to_json_string().unwrap()),
        (
            BASE,
            destination_document(initial_target())
                .to_json_string()
                .unwrap(),
        ),
        (
            "schema/authoring.bundle.json",
            schema.to_json_string().unwrap(),
        ),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
    }

    let mut fixture = ApplyFixture::open(project, kind, source, pinned, cleared, sibling);
    let initial = fixture.stored_sources();
    let containing = fixture.containing;
    let (inspection, address) = inspect_left(&mut fixture, false);
    assert_eq!(
        fixture
            .panel
            .dispatch(&mut fixture.app, |_| Action::PreparePropertyWrite {
                inspection,
                address,
                intent: PrefabPropertyIntent::Set(override_value),
                limits: Default::default(),
            }),
        Outcome::Accepted {
            inspection,
            phase: Phase::PreparingEdit,
        }
    );
    assert_review(
        &mut fixture.panel,
        &mut fixture.app,
        inspection,
        &instance_patch(&fixture.cleared, &fixture.pinned),
    );
    fixture.commit(inspection, Phase::Terminal);
    assert_eq!(
        frame(&fixture.app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Written {
            inspection,
            document: containing,
            revision: basis(&fixture.app, containing).revision,
            kind: AuthoringPrefabPropertyClaimKind::Set,
        })
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
    assert_eq!(fixture.stored_sources(), initial);
    save(&mut fixture.app, containing);
    let saved = fixture.stored_sources();
    assert_saved_source(
        kind,
        &saved.0,
        &containing_document(&fixture.pinned, &fixture.sibling),
    );
    assert_eq!(saved.1, initial.1);
    fixture.assert_containing(true, (false, 1, 0));
    (fixture, registry)
}

fn inspect_left(
    fixture: &mut ApplyFixture,
    pinned: bool,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    let inspection = start(
        &mut fixture.panel,
        &mut fixture.app,
        fixture.containing,
        "left",
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    let address = EditorPrefabPropertyAddress {
        component: ComponentTypeId::new(LINK),
        ..address("left", "target")
    };
    fixture.panel.observe(&mut fixture.app, |context| {
        let snapshot = context.prefab_effective_observation(inspection).unwrap();
        let target = snapshot.target(&address.entity).unwrap();
        let read = target
            .component(&address.component)
            .unwrap()
            .property_value(
                &address.property,
                &mut ComponentPropertyReadBudget::new(
                    ItemLimit::new(16).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(
            read.revert_claim().map(|claim| claim.kind),
            pinned.then_some(AuthoringPrefabPropertyClaimKind::Set)
        );
        if pinned {
            let AuthoringPrefabPropertyValue::Present(value) = read.value else {
                panic!("the saved point must resolve in the selected occurrence");
            };
            assert_eq!(
                value.copy_value(Default::default()).unwrap(),
                ComponentValue::EntityReference(routed_entity(LOCAL_POINT))
            );
        }
    });
    (inspection, address)
}

fn assert_compiled_points(fixture: &ApplyFixture, registry: &ComponentRegistrySnapshot) {
    let (containing, destination) = fixture.stored_sources();
    let socket = fs::read(fixture.project.root.join(SOCKET)).unwrap();
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            fixture.sibling.source.clone(),
            base_key(),
            decode_prefab(&destination),
        )
        .with_prefab(
            socket_asset(),
            socket_hop().expected_base,
            decode_prefab(&socket),
        );
    let report = match fixture.kind {
        EditorDocumentKind::Scene => SceneDocumentCandidate::decode_json_bytes(&containing)
            .unwrap()
            .into_composition_source()
            .unwrap()
            .compile(registry, &sources),
        EditorDocumentKind::Prefab => decode_prefab(&containing).compile(registry, &sources),
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let compiled = report.document().unwrap();
    let [left, right] = [id("left"), id("right")].map(|anchor| {
        let root = compiled
            .entities()
            .iter()
            .find(|entity| entity.id() == &anchor)
            .unwrap();
        let point = root
            .children()
            .iter()
            .find(|child| {
                let origin = &compiled.canonical_provenance()[*child];
                origin.local_entity_id() == &id(LOCAL_POINT)
                    && origin.projection_path().unwrap().root_anchor() == &anchor
            })
            .unwrap();
        assert_eq!(
            root.components()[&ComponentTypeId::new(LINK)]
                .value()
                .field_entity_reference("target")
                .unwrap(),
            &EntityReference::SceneLocal {
                entity: point.clone(),
            },
            "compiled reference at {anchor}"
        );
        point
    });
    assert_ne!(left, right);
}

fn containing_document(left: &PrefabInstance, right: &PrefabInstance) -> SceneDocument {
    SceneDocument::from_ordered_roots([("left", left), ("right", right)].map(
        |(anchor, instance)| {
            let mut record = SceneEntityRecord::new(id(anchor));
            record.prefab = Some(instance.clone());
            record
        },
    ))
}

fn destination_document(value: ComponentValue) -> PrefabDocument {
    let root = SceneEntityRecord::new(id(ROOT)).with_component(
        ComponentTypeId::new(LINK),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("target", value)]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    );
    let mut socket = SceneEntityRecord::new(id("socket"));
    socket.prefab = Some(socket_instance(EntityReference::SceneLocal {
        entity: id(LOCAL_POINT),
    }));
    let children = ["socket", LOCAL_POINT].map(|child| {
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id(child)),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id(ROOT),
            ))),
        )
    });
    PrefabDocument::new(
        [SceneEntityPlacement::root(id(ROOT))]
            .into_iter()
            .chain(children),
        [root, socket, SceneEntityRecord::new(id(LOCAL_POINT))],
    )
}

fn decode_prefab(bytes: &[u8]) -> PrefabDocument {
    PrefabDocumentCandidate::decode_json_bytes(bytes)
        .unwrap()
        .into_composition_source()
        .unwrap()
}

fn link_patch(value: ComponentValue) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id(ROOT),
        component: ComponentTypeId::new(LINK),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("target"),
        value,
    }])
}

fn initial_target() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(ROOT) })
}

fn point() -> ReferenceInputId {
    ReferenceInputId::new("point").unwrap()
}

fn point_operand(through: Vec<PrefabInstanceHop>) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference {
            through,
            target: AuthoredReferenceTarget::Input(point()),
        },
    })
}

fn routed_entity(entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: PrefabEntityTarget {
            through: vec![left_hop()],
            entity: PrefabEntityOrigin::SourceEntity(id(entity)),
        }
        .into(),
    }
}

fn left_hop() -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("left")),
        expected_base: base_key(),
    }
}

fn socket_asset() -> AssetRef {
    AssetRef::path("apply-socket.prefab.json").unwrap()
}

fn socket_hop() -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("socket")),
        expected_base: SourceDocumentKey::project_location(
            "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
            AssetSourcePath::new(SOCKET).unwrap(),
            SourceDocumentKind::Prefab,
        ),
    }
}

fn socket_instance(binding: EntityReference) -> PrefabInstance {
    let mut instance = PrefabInstance::forest(socket_asset());
    instance.arguments = Some(PrefabReferenceArguments {
        base: socket_hop().expected_base,
        values: [(point(), binding)].into(),
    });
    instance
}
