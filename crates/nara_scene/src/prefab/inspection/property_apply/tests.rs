use std::{convert::Infallible, sync::Arc};

use nara_ecs::identity::{AuthoredReferenceTarget, ReferenceInputId};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentSchemaCatalog,
    ComponentShapeMember, ComponentValueKind, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    AuthoringDocumentSource, PrefabAddition, PrefabComponentIntent, PrefabDocument,
    PrefabDocumentCandidate, PrefabInstance, PrefabInstanceClaim, PrefabInstanceHop,
    PrefabInstancePath, PrefabOverlay, PrefabPropertyClaim, PrefabReferenceArguments, PrefabRevert,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKind, authoring::AuthoringAdapterReceipt,
};

const SLOT: PrefabOverlaySlot = PrefabOverlaySlot::Property {
    component_index: 0,
    claim_index: 0,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "862aa6e4-e568-408a-abd4-69e7d7d5a53a".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn asset(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap()
}

fn property(name: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(name))
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.Promotable")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.promotion").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    let optional = |name: &str, shape| {
        ComponentFieldSchema::optional(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            shape,
        )
        .with_capabilities(capabilities)
    };
    let schema = ComponentSchema::new(component(), "Promotable", ComponentSchemaVersion::ONE)
        .with_capabilities(capabilities)
        .with_fields([
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("score"),
                "Score",
                ComponentFieldPath::from_fields(["score"]),
                ComponentValueKind::U64,
                ComponentValue::U64(7),
            )
            .with_capabilities(capabilities),
            optional("float", ComponentValueKind::F64.into()),
            optional(
                "nullable",
                ComponentValueShape::Optional {
                    value: Box::new(ComponentValueKind::U64.into()),
                },
            ),
            optional(
                "weights",
                ComponentValueShape::Map {
                    value: Box::new(ComponentValueKind::U64.into()),
                },
            ),
            optional("target", ComponentValueShape::EntityRef)
                .with_capability(ComponentCapability::EntityRef),
            optional(
                "groups",
                ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::EntityRef)),
            )
            .with_capability(ComponentCapability::EntityRef),
            optional(
                "binding",
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("weight", ComponentValueKind::F64),
                    ComponentShapeMember::new(
                        "links",
                        ComponentValueShape::map(ComponentValueShape::optional(
                            ComponentValueShape::EntityRef,
                        )),
                    ),
                ]),
            )
            .with_capability(ComponentCapability::EntityRef),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn read() -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(ItemLimit::new(64).unwrap(), ByteLimit::new(8192).unwrap())
}

fn hop(anchor: &str, base: &str) -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
        expected_base: key(base),
    }
}

fn overlay(
    base: &str,
    target: PrefabEntityTarget,
    property: &ComponentPropertyPath,
    intent: PrefabPropertyIntent,
) -> PrefabOverlay {
    let mut overlay = PrefabOverlay::new(key(base));
    overlay.components.push(PrefabComponentIntent::Properties {
        target,
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: property.clone(),
            intent,
        }],
    });
    overlay
}

struct Fixture {
    basis: AuthoringSchemaBasis,
    branch: Arc<PrefabDocument>,
    destination: PrefabAuthoringSession,
    scene: SceneAuthoringSession,
    target: PrefabEntityTarget,
}

impl Fixture {
    fn new(
        initial: ComponentValue,
        property: &ComponentPropertyPath,
        claim: PrefabPropertyIntent,
        inherited: Option<PrefabPropertyIntent>,
    ) -> Self {
        let basis = basis();
        let hero = PrefabEntityTarget::source(id("hero"));
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("hero"))
            .with_component(
                component(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, initial).with_owner(owner()),
            )]);
        let destination =
            PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(asset("destination")),
                    &basis,
                )
                .unwrap()
                .publish();
        let mut nested = SceneEntityRecord::new(id("nested"));
        nested.prefab = Some(PrefabInstance {
            overrides: inherited
                .map(|intent| overlay("destination", hero.clone(), property, intent)),
            ..PrefabInstance::root(asset("destination"), key("destination"), hero.clone())
        });
        let branch = Arc::new(PrefabDocument::from_ordered_roots([nested]));
        let inside_branch = PrefabEntityTarget {
            through: vec![hop("nested", "destination")],
            entity: hero.entity,
        };
        let mut mount = SceneEntityRecord::new(id("mount"));
        mount.prefab = Some(PrefabInstance {
            overrides: Some(overlay("branch", inside_branch.clone(), property, claim)),
            ..PrefabInstance::forest(asset("branch"))
        });
        let scene =
            SceneDocument::from_ordered_roots([mount, SceneEntityRecord::new(id("external"))]);
        let scene = scene_session(&scene, &basis);
        let mut target = inside_branch;
        target.through.insert(0, hop("mount", "branch"));
        Self {
            basis,
            branch,
            destination: PrefabAuthoringSession::from_published_authoring_document(destination),
            scene,
            target,
        }
    }

    fn inspect(
        &self,
        scene: &SceneDocument,
        destination: Arc<PrefabDocument>,
    ) -> AuthoringPrefabInstanceSnapshot {
        try_inspect_authoring_prefab_instance_with_loader(
            PrefabInspectionInput::Borrowed(SourceDocumentInput::Scene(scene)),
            &id("mount"),
            &self.basis,
            Default::default(),
            |request, _| {
                let (document, key) = if request == &asset("branch") {
                    (Arc::clone(&self.branch), key("branch"))
                } else if request == &asset("leaf") {
                    let mut leaf =
                        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("socket"))]);
                    leaf.inputs.insert(ReferenceInputId::new("point").unwrap());
                    (Arc::new(leaf), key("leaf"))
                } else {
                    assert_eq!(request, &asset("destination"));
                    (Arc::clone(&destination), key("destination"))
                };
                Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                    document,
                    self.basis.clone(),
                    key,
                )))
            },
        )
        .unwrap()
    }

    fn replace_destination(&mut self, document: PrefabDocument) {
        let published = PrefabDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(asset("destination")),
            &self.basis,
        )
        .unwrap()
        .publish();
        self.destination = PrefabAuthoringSession::from_published_authoring_document(published);
    }

    fn claim(&self) -> PrefabPropertyClaimRef<'_> {
        self.scene
            .prefab_property_claims(&id("mount"), &[])
            .unwrap()
            .claim_at(SLOT)
            .unwrap()
    }

    fn plan(&self, property: &ComponentPropertyPath) -> ObservedPrefabPropertyApply {
        self.inspect(self.scene.document(), self.destination.shared_document())
            .plan_property_apply(
                &self.target,
                &component(),
                property,
                self.claim(),
                &key("destination"),
                Default::default(),
                &mut read(),
            )
            .unwrap()
    }

    fn prepare(
        &self,
        edit: &ObservedPrefabPropertyApply,
    ) -> (
        crate::PreparedSceneAuthoringEdit,
        Option<PreparedPrefabAuthoringEdit>,
    ) {
        let source = self
            .scene
            .prepare_prefab_revert(
                self.scene.revision(),
                &PrefabInstancePath {
                    anchor: edit.anchor().clone(),
                    steps: Vec::new(),
                },
                PrefabRevert::Property(self.claim()),
                Default::default(),
            )
            .unwrap();
        let destination = self
            .destination
            .prepare_prefab_property_apply(self.destination.revision(), edit)
            .unwrap();
        (source, destination)
    }
}

fn scene_session(scene: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published =
        SceneDocumentCandidate::decode_json_bytes(scene.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/main.scene.json").unwrap()),
                basis,
            )
            .unwrap()
            .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

#[test]
fn nested_scalar_and_nullable_set_lower_to_ordinary_field_edits() {
    let nested = property("weights").with_steps([ComponentPropertyStep::MapEntry("α".into())]);
    let fixture = Fixture::new(
        ComponentValue::map([(
            "weights",
            ComponentValue::map([("α", ComponentValue::U64(3))]),
        )]),
        &nested,
        PrefabPropertyIntent::Set(ComponentValue::U64(4)),
        None,
    );
    let edit = fixture.plan(&nested);
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::ReplaceProperty { property, value: ComponentValue::U64(4), .. }]
            if property == &nested
    ));
    assert!(matches!(
        destination.inverse().operations.as_slice(),
        [ScenePatchOperation::ReplaceProperty {
            value: ComponentValue::U64(3),
            ..
        }]
    ));
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
    assert!(
        !fixture
            .inspect(fixture.scene.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );

    let nullable = property("nullable");
    for initial in [ComponentValue::U64(9), ComponentValue::Null] {
        let unchanged = initial == ComponentValue::Null;
        let fixture = Fixture::new(
            ComponentValue::map([("nullable", initial)]),
            &nullable,
            PrefabPropertyIntent::Set(ComponentValue::Null),
            None,
        );
        let edit = fixture.plan(&nullable);
        let (source, destination) = fixture.prepare(&edit);
        assert_eq!(destination.is_none(), unchanged);
        let candidate = destination.as_ref().map_or_else(
            || fixture.destination.shared_document(),
            PreparedPrefabAuthoringEdit::shared_document,
        );
        assert!(
            fixture
                .inspect(source.document(), candidate)
                .verifies_property_apply(&edit, &mut read())
                .unwrap()
        );
    }
}

#[test]
fn absent_uses_remove_field_and_preserves_default_origin() {
    let score = property("score");
    let fixture = Fixture::new(
        ComponentValue::map([("score", ComponentValue::U64(7))]),
        &score,
        PrefabPropertyIntent::Absent,
        None,
    );
    let edit = fixture.plan(&score);
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::RemoveField { field, .. }] if field == &score.root
    ));
    assert!(
        !fixture
            .inspect(source.document(), fixture.destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );

    for inherited in [None, Some(PrefabPropertyIntent::Absent)] {
        let covered = inherited.is_some();
        let fixture = Fixture::new(
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
            &score,
            PrefabPropertyIntent::Absent,
            inherited,
        );
        let edit = fixture.plan(&score);
        let (source, destination) = fixture.prepare(&edit);
        assert!(destination.is_none());
        assert_eq!(
            fixture
                .inspect(source.document(), fixture.destination.shared_document())
                .verifies_property_apply(&edit, &mut read())
                .unwrap(),
            !covered
        );
    }
}

#[test]
fn authored_reference_rebases_through_root_binding_and_keeps_the_normalized_endpoint() {
    let property = property("target");
    let raw = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![hop("mount", "branch"), hop("nested", "destination")],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("hero"))),
        },
    };
    let fixture = Fixture::new(
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        &property,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(raw.clone())),
        None,
    );
    let original = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    let observed = original
        .target(&fixture.target)
        .unwrap()
        .component(&component())
        .unwrap()
        .property_value(&property, &mut read())
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
        panic!("the authored claim must resolve");
    };
    let normalized = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![hop("mount", "branch")],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("nested"))),
        },
    };
    assert_ne!(normalized, raw);
    assert_eq!(
        value.copy_value(Default::default()).unwrap(),
        ComponentValue::EntityReference(normalized.clone()),
    );
    assert!(matches!(
        original.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("branch"),
            Default::default(),
            &mut read(),
        ),
        Err(AuthoringPrefabPropertyApplyError::InvalidDestination)
    ));
    let nodes = 2 + 2 * fixture.target.through.len() + raw.cost_nodes() + normalized.cost_nodes();
    let bytes = fixture.target.logical_bytes_up_to(8192).unwrap()
        + property.logical_bytes().unwrap()
        + component().as_str().len()
        + raw.cost().logical_bytes()
        + normalized.cost().logical_bytes();
    let plan = |nodes, bytes| {
        original.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            PrefabDeclarationLimits::new(nodes, bytes, 64).unwrap(),
            &mut read(),
        )
    };
    for (nodes, bytes) in [(nodes - 1, bytes), (nodes, bytes - 1)] {
        assert!(matches!(
            plan(nodes, bytes),
            Err(AuthoringPrefabPropertyApplyError::BudgetExceeded)
        ));
    }
    let edit = plan(nodes, bytes).unwrap();
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { value: ComponentValue::EntityReference(EntityReference::Authored { reference }), .. }]
            if reference == &AuthoredEntityReference::source(id("hero"))
    ));
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );

    let escaping = EntityReference::Authored {
        reference: AuthoredEntityReference::source(id("external")),
    };
    let fixture = Fixture::new(
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        &property,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(escaping)),
        None,
    );
    assert!(matches!(
        fixture
            .inspect(
                fixture.scene.document(),
                fixture.destination.shared_document()
            )
            .plan_property_apply(
                &fixture.target,
                &component(),
                &property,
                fixture.claim(),
                &key("destination"),
                Default::default(),
                &mut read()
            ),
        Err(AuthoringPrefabPropertyApplyError::InvalidProperty)
    ));
}

#[test]
fn apply_uses_the_physical_record_for_either_root_bound_alias() {
    let property = property("score");
    let mut fixture = Fixture::new(
        ComponentValue::map([("score", ComponentValue::U64(3))]),
        &property,
        PrefabPropertyIntent::Set(ComponentValue::U64(9)),
        None,
    );
    let mut scene = fixture.scene.document().clone();
    let mount = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("mount"))
        .unwrap();
    let overlay = mount.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
    let PrefabComponentIntent::Properties { target, .. } = &mut overlay.components[0] else {
        panic!("fixture stores one property group");
    };
    *target = PrefabEntityTarget::source(id("nested"));
    fixture.scene = scene_session(&scene, &fixture.basis);

    let alias = PrefabEntityTarget {
        through: vec![hop("mount", "branch")],
        entity: PrefabEntityOrigin::SourceEntity(id("nested")),
    };
    let original = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    for target in [&fixture.target, &alias] {
        let edit = original
            .plan_property_apply(
                target,
                &component(),
                &property,
                fixture.claim(),
                &key("destination"),
                Default::default(),
                &mut read(),
            )
            .unwrap();
        let (source, destination) = fixture.prepare(&edit);
        let destination = destination.unwrap();
        assert!(matches!(
            destination.patch().operations.as_slice(),
            [ScenePatchOperation::SetField { entity, value: ComponentValue::U64(9), .. }]
                if entity == &id("hero")
        ));
        assert!(
            fixture
                .inspect(source.document(), destination.shared_document())
                .verifies_property_apply(&edit, &mut read())
                .unwrap()
        );
    }
}

#[test]
fn apply_refuses_a_reference_to_an_addition_owned_by_the_outer_overlay() {
    let property = property("target");
    let addition = PrefabEntityOrigin::InstanceAddition(id("outside"));
    let endpoint = PrefabEntityTarget {
        through: vec![hop("mount", "branch"), hop("nested", "destination")],
        entity: addition.clone(),
    };
    let raw = EntityReference::Authored {
        reference: endpoint.into(),
    };
    let mut fixture = Fixture::new(
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        &property,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(raw.clone())),
        None,
    );
    let mut overlay = PrefabOverlay::new(key("destination"));
    overlay.additions.push(PrefabAddition::new(id("outside")));
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget {
            through: Vec::new(),
            entity: addition,
        },
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("hero"),
        ))),
    ));
    Arc::make_mut(&mut fixture.branch).entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides = Some(overlay);

    let original = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    let observed = original
        .target(&fixture.target)
        .unwrap()
        .component(&component())
        .unwrap()
        .property_value(&property, &mut read())
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
        panic!("the selected occurrence can resolve the added entity");
    };
    assert_eq!(
        value.copy_value(Default::default()).unwrap(),
        ComponentValue::EntityReference(raw)
    );
    assert!(matches!(
        original.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            Default::default(),
            &mut read(),
        ),
        Err(AuthoringPrefabPropertyApplyError::InvalidProperty)
    ));
}

#[test]
fn apply_requires_the_destination_to_own_the_nested_instance_route() {
    let property = property("target");
    let raw = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                hop("mount", "branch"),
                hop("nested", "destination"),
                hop("hero", "leaf"),
            ],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("socket"))),
        },
    };
    let mut fixture = Fixture::new(
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        &property,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(raw)),
        None,
    );
    let leaf = PrefabInstance::forest(asset("leaf"));
    let mut overlay = PrefabOverlay::new(key("destination"));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("hero")),
        value: Some(leaf.clone()),
    });
    Arc::make_mut(&mut fixture.branch).entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides = Some(overlay);
    let original = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    assert!(matches!(
        original.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            Default::default(),
            &mut read(),
        ),
        Err(AuthoringPrefabPropertyApplyError::InvalidProperty)
    ));

    Arc::make_mut(&mut fixture.branch).entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides = None;
    let mut destination = fixture.destination.document().clone();
    destination.entities[0].prefab = Some(leaf);
    fixture.replace_destination(destination);
    let edit = fixture.plan(&property);
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { value: ComponentValue::EntityReference(EntityReference::Authored { reference }), .. }]
            if reference.through == [hop("hero", "leaf")]
                && reference.target == AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("socket")))
    ));
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );

    let different_endpoint = fixture
        .destination
        .prepare_authoring_property_edit(
            fixture.destination.revision(),
            &[AuthoringPropertyEdit {
                entity: id("hero"),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                property,
                value: ComponentValue::EntityReference(EntityReference::Authored {
                    reference: AuthoredEntityReference::source(id("hero")),
                }),
            }],
        )
        .unwrap();
    assert!(
        !fixture
            .inspect(source.document(), different_endpoint.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
}

#[test]
fn apply_requires_each_input_binding_to_belong_to_the_destination_even_at_the_same_endpoint() {
    let property = property("target");
    let point = ReferenceInputId::new("point").unwrap();
    let outer = ReferenceInputId::new("outer").unwrap();
    let raw = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                hop("mount", "branch"),
                hop("nested", "destination"),
                hop("hero", "leaf"),
            ],
            target: AuthoredReferenceTarget::Input(point.clone()),
        },
    });
    let mut fixture = Fixture::new(
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        &property,
        PrefabPropertyIntent::Set(raw),
        None,
    );
    let build_destination = |argument| {
        let mut destination = fixture.destination.document().clone();
        destination.entities[0].prefab = Some(PrefabInstance {
            arguments: Some(PrefabReferenceArguments {
                base: key("leaf"),
                values: [(point.clone(), argument)].into(),
            }),
            ..PrefabInstance::forest(asset("leaf"))
        });
        destination.inputs.insert(outer.clone());
        destination
    };
    let local_destination = build_destination(EntityReference::SceneLocal { entity: id("hero") });
    let forwarded_destination = build_destination(EntityReference::Authored {
        reference: AuthoredEntityReference::input(outer.clone()),
    });
    Arc::make_mut(&mut fixture.branch).entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .arguments = Some(PrefabReferenceArguments {
        base: key("destination"),
        values: [(
            outer,
            EntityReference::Authored {
                reference: AuthoredEntityReference {
                    through: vec![hop("nested", "destination")],
                    target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                        "hero",
                    ))),
                },
            },
        )]
        .into(),
    });
    let read_resolved = |snapshot: &AuthoringPrefabInstanceSnapshot,
                         target: &PrefabEntityTarget| {
        let read = snapshot
            .target(target)
            .unwrap()
            .component(&component())
            .unwrap()
            .property_value(&property, &mut read())
            .unwrap();
        let AuthoringPrefabPropertyValue::Present(value) = read.value else {
            panic!("the nested input must resolve");
        };
        value.copy_value(Default::default()).unwrap()
    };

    fixture.replace_destination(forwarded_destination);
    let original = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    let forwarded_endpoint = read_resolved(&original, &fixture.target);
    assert!(matches!(
        original.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            Default::default(),
            &mut read(),
        ),
        Err(AuthoringPrefabPropertyApplyError::InvalidProperty)
    ));

    fixture.replace_destination(local_destination);
    let local = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    assert_eq!(read_resolved(&local, &fixture.target), forwarded_endpoint);
    let edit = fixture.plan(&property);
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { value: ComponentValue::EntityReference(EntityReference::Authored { reference }), .. }]
            if reference.through == [hop("hero", "leaf")]
                && reference.target == AuthoredReferenceTarget::Input(point)
    ));
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
}

#[test]
fn structured_apply_admits_both_values_and_verifies_each_rebased_leaf() {
    let property = property("binding");
    let projected_ref = |through, entity| {
        ComponentValue::EntityReference(EntityReference::Authored {
            reference: AuthoredEntityReference {
                through,
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                    entity,
                ))),
            },
        })
    };
    let binding = |weight, through: &[PrefabInstanceHop], other| {
        ComponentValue::map([
            ("weight", ComponentValue::f64(weight).unwrap()),
            (
                "links",
                ComponentValue::map([
                    ("root", projected_ref(through.to_vec(), "hero")),
                    ("other", projected_ref(through.to_vec(), other)),
                    ("optional", ComponentValue::Null),
                ]),
            ),
        ])
    };
    let through = [hop("mount", "branch"), hop("nested", "destination")];
    let raw = binding(-0.0, &through, "other");
    let mut fixture = Fixture::new(
        ComponentValue::map([("binding", binding(1.0, &[], "hero"))]),
        &property,
        PrefabPropertyIntent::Set(raw.clone()),
        None,
    );
    let mut destination = fixture.destination.document().clone();
    destination
        .entities
        .push(SceneEntityRecord::new(id("other")));
    destination.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(id("other")),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("hero"),
        ))),
    ));
    fixture.replace_destination(destination);
    let snapshot = fixture.inspect(
        fixture.scene.document(),
        fixture.destination.shared_document(),
    );
    let selected = snapshot
        .target(&fixture.target)
        .unwrap()
        .component(&component())
        .unwrap()
        .property_value(&property, &mut read())
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = selected.value else {
        panic!("the structured claim must supply a composed value");
    };
    let projected = value.copy_value(Default::default()).unwrap();
    let raw_cost = raw.cost();
    let projected_cost = projected.cost();
    let nodes = 2 + 2 * fixture.target.through.len() + raw_cost.nodes() + projected_cost.nodes();
    let bytes = fixture.target.logical_bytes_up_to(8192).unwrap()
        + property.logical_bytes().unwrap()
        + component().as_str().len()
        + raw_cost.logical_bytes()
        + projected_cost.logical_bytes();
    let plan = |nodes, bytes| {
        snapshot.plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            PrefabDeclarationLimits::new(nodes, bytes, 64).unwrap(),
            &mut read(),
        )
    };
    for (nodes, bytes) in [(nodes - 1, bytes), (nodes, bytes - 1)] {
        assert!(matches!(
            plan(nodes, bytes),
            Err(AuthoringPrefabPropertyApplyError::BudgetExceeded)
        ));
    }
    let edit = plan(nodes, bytes).unwrap();
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { value, .. }]
            if component_values_exactly_match(value, &binding(-0.0, &[], "other"))
    ));
    let verifies = |document| {
        fixture
            .inspect(source.document(), document)
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    };
    assert!(verifies(destination.shared_document()));
    for value in [binding(0.0, &[], "other"), binding(-0.0, &[], "hero")] {
        let wrong = fixture
            .destination
            .prepare_authoring_property_edit(
                fixture.destination.revision(),
                &[AuthoringPropertyEdit {
                    entity: id("hero"),
                    component: component(),
                    component_version: ComponentSchemaVersion::ONE,
                    property: property.clone(),
                    value,
                }],
            )
            .unwrap();
        assert!(!verifies(wrong.shared_document()));
    }
}

#[test]
fn structured_apply_copies_many_references_under_one_long_map_key() {
    let property = property("groups");
    let group = |through| {
        let leaf = ComponentValue::EntityReference(EntityReference::Authored {
            reference: AuthoredEntityReference {
                through,
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                    "hero",
                ))),
            },
        });
        ComponentValue::map([("g".repeat(16_384), ComponentValue::List(vec![leaf; 8]))])
    };
    let initial = ComponentValue::map([("groups", ComponentValue::Map(Default::default()))]);
    let raw = group(vec![hop("mount", "branch"), hop("nested", "destination")]);
    let fixture = Fixture::new(initial, &property, PrefabPropertyIntent::Set(raw), None);
    let limits = PrefabDeclarationLimits::new(128, 36 * 1024, 64).unwrap();
    let edit = fixture
        .inspect(
            fixture.scene.document(),
            fixture.destination.shared_document(),
        )
        .plan_property_apply(
            &fixture.target,
            &component(),
            &property,
            fixture.claim(),
            &key("destination"),
            limits,
            &mut read(),
        )
        .unwrap();
    let (source, destination) = fixture.prepare(&edit);
    let destination = destination.unwrap();
    assert!(matches!(
        destination.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { value, .. }]
            if component_values_exactly_match(value, &group(Vec::new()))
    ));
    assert!(
        fixture
            .inspect(source.document(), destination.shared_document())
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
}

#[test]
fn structured_apply_refuses_equal_values_still_pinned_by_a_descendant_claim() {
    let weights = property("weights");
    let value = |first| ComponentValue::map([("first", ComponentValue::U64(first))]);
    let mut fixture = Fixture::new(
        ComponentValue::map([("weights", value(3))]),
        &weights,
        PrefabPropertyIntent::Set(value(4)),
        None,
    );
    Arc::make_mut(&mut fixture.branch).entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides = Some(overlay(
        "destination",
        PrefabEntityTarget::source(id("hero")),
        &weights
            .clone()
            .with_steps([ComponentPropertyStep::MapEntry("first".into())]),
        PrefabPropertyIntent::Set(ComponentValue::U64(4)),
    ));
    let edit = fixture.plan(&weights);
    let (source, destination) = fixture.prepare(&edit);
    let candidate = fixture.inspect(source.document(), destination.unwrap().shared_document());
    let observed = candidate
        .target(&fixture.target)
        .unwrap()
        .component(&component())
        .unwrap()
        .property_value(&weights, &mut read())
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(actual) = observed.value else {
        panic!("the remaining descendant must supply its value");
    };
    assert_eq!(actual.copy_value(Default::default()).unwrap(), value(4));
    assert!(observed.controlling_claim().is_none());
    assert!(
        !candidate
            .verifies_property_apply(&edit, &mut read())
            .unwrap()
    );
}

#[test]
fn apply_preserves_the_sign_of_zero_in_preparation_and_verification() {
    let property = property("float");
    for before in [0.0_f64, -0.0] {
        let fixture = Fixture::new(
            ComponentValue::map([("float", ComponentValue::f64(before).unwrap())]),
            &property,
            PrefabPropertyIntent::Set(ComponentValue::f64(-0.0).unwrap()),
            None,
        );
        let edit = fixture.plan(&property);
        let (source, destination) = fixture.prepare(&edit);
        assert_eq!(destination.is_none(), before.is_sign_negative());
        if let Some(destination) = destination.as_ref() {
            assert!(matches!(
                destination.patch().operations.as_slice(),
                [ScenePatchOperation::SetField { value: ComponentValue::F64(value), .. }]
                    if value.get().to_bits() == (-0.0_f64).to_bits()
            ));
            assert!(matches!(
                destination.inverse().operations.as_slice(),
                [ScenePatchOperation::SetField { value: ComponentValue::F64(value), .. }]
                    if value.get().to_bits() == before.to_bits()
            ));
        }
        let unchanged = fixture.destination.shared_document();
        assert_eq!(
            fixture
                .inspect(source.document(), Arc::clone(&unchanged))
                .verifies_property_apply(&edit, &mut read())
                .unwrap(),
            before.is_sign_negative()
        );
        let candidate = destination
            .as_ref()
            .map_or_else(|| unchanged, PreparedPrefabAuthoringEdit::shared_document);
        assert!(
            fixture
                .inspect(source.document(), candidate)
                .verifies_property_apply(&edit, &mut read())
                .unwrap()
        );
    }
}
