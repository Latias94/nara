//! Selected demand comparisons use the production staged compiler and exact Schema admission.

use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    sync::Arc,
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticValueRef;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    __private::plan_declared_entity_references, AuthoringSchemaBundle, AuthoringSchemaBundleOwner,
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringRecordReadiness, AuthoringSchemaBasis, PrefabAddition, PrefabComponentIntent,
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    PrefabOverlay, PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent,
    ResolvedPrefabSource, SceneCompilationFailureKind, SceneCompilationLimits,
    SceneCompilationOptions, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
    prefab::{
        AuthoringSceneCompilationOmissionReason, SelectedInstanceCompilation,
        SelectedInstanceConsumer, SourceDocumentInput, try_compile_authoring_scene_with_loader,
        try_compile_selected_authoring_instance_with_loader,
    },
};

const CONVERSION: SelectedInstanceConsumer<'static> = SelectedInstanceConsumer::Conversion {
    reference_queries: &[],
    initial_reference_work: (0, 0),
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/selected/{value}.ron")).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "862aa6e4-e568-408a-abd4-69e7d7d5a53a".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.SelectedSemanticData")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.selected-data").unwrap()
}

fn local(value: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(value) })
}

fn persistent() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("game.selected").unwrap(),
            PersistentRuntimeId::parse_str("7882b70a-55cf-4ae7-9172-a6bd0956d8d7").unwrap(),
        ),
    })
}

fn image() -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        (
            "value",
            ComponentValue::String("textures/selected.png".into()),
        ),
    ])
}

fn schema() -> ComponentSchema {
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    ComponentSchema::new(
        component(),
        "Selected semantic data",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(capabilities)
    .with_fields([
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
            local("socket"),
        )
        .with_capabilities(capabilities)
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("persistent"),
            "Persistent",
            ComponentFieldPath::from_fields(["persistent"]),
            ComponentValueShape::EntityRef,
            persistent(),
        )
        .with_capabilities(capabilities)
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("score"),
            "Score",
            ComponentFieldPath::from_fields(["score"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(capabilities),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueShape::from(ComponentValueKind::AssetRef),
            image(),
        )
        .with_capabilities(capabilities)
        .with_capability(ComponentCapability::AssetRef),
    ])
}

fn basis() -> AuthoringSchemaBasis {
    basis_with_schema(schema())
}

fn basis_with_schema(schema: ComponentSchema) -> AuthoringSchemaBasis {
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
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    assert!(basis.has_authoring_adapter(&component()));
    assert!(!basis.has_native_preview_adapter(&component()));
    basis
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
        .with_owner(owner())
        .with_contextual_default(ComponentFieldId::new("target"), local("socket"))
}

fn empty() -> ComponentValue {
    ComponentValue::map([] as [(&str, ComponentValue); 0])
}

fn data(value: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(value)).with_component(component(), record(empty()))
}

fn anchor(value: &str, source: &str, overlay: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(value));
    entity.prefab = Some(PrefabInstance {
        overrides: overlay,
        ..PrefabInstance::forest(request(source))
    });
    entity
}

fn placement(target: PrefabEntityTarget, parent: ScenePlacementParent) -> SceneEntityPlacement {
    SceneEntityPlacement::new(target, ScenePlacement::last(parent))
}

fn authored(reference: AuthoredEntityReference) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored { reference })
}

fn nested_target(terminal: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("left")),
                expected_base: key("branch"),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(id("graft")),
                expected_base: key("leaf"),
            },
        ],
        entity: PrefabEntityOrigin::SourceEntity(id(terminal)),
    }
}

struct Fixture {
    scene: SceneDocument,
    sources: HashMap<AssetRef, (SourceDocumentKey, Arc<PrefabDocument>)>,
}

impl Fixture {
    fn new() -> Self {
        let branch_overlay = |name: &str| {
            let mut branch_overlay = PrefabOverlay::new(key("branch"));
            let mut badge = PrefabAddition::new(id("badge"));
            badge.components.insert(
                component(),
                record(empty()).with_contextual_default(
                    ComponentFieldId::new("target"),
                    authored(AuthoredEntityReference {
                        through: vec![PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id(name)),
                            expected_base: key("branch"),
                        }],
                        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(
                            id("socket"),
                        )),
                    }),
                ),
            );
            branch_overlay.placements.push(placement(
                PrefabEntityTarget::addition(id("badge")),
                ScenePlacementParent::Entity(PrefabEntityTarget::source(id("socket"))),
            ));
            branch_overlay.additions.push(badge);
            let mut graft = PrefabAddition::new(id("graft"));
            graft.prefab = Some(PrefabInstance::forest(request("leaf")));
            branch_overlay.placements.push(placement(
                PrefabEntityTarget::addition(id("graft")),
                ScenePlacementParent::Root,
            ));
            branch_overlay.additions.push(graft);
            branch_overlay
        };

        let base = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("host")),
                SceneEntityPlacement::root(id("other")),
                crate::tests::child_placement(id("socket"), id("host")),
                crate::tests::child_placement(id("left"), id("host")),
                crate::tests::child_placement(id("right"), id("host")),
            ],
            [
                data("host"),
                SceneEntityRecord::new(id("socket")),
                anchor("left", "branch", Some(branch_overlay("left"))),
                anchor("right", "branch", Some(branch_overlay("right"))),
                SceneEntityRecord::new(id("other")),
            ],
        );
        let mut overlay = PrefabOverlay::new(key("base"));
        let mut extra = PrefabAddition::new(id("extra"));
        extra.components.insert(
            component(),
            record(empty()).with_contextual_default(
                ComponentFieldId::new("target"),
                authored(AuthoredEntityReference {
                    through: vec![
                        PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("A")),
                            expected_base: key("base"),
                        },
                        PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("left")),
                            expected_base: key("branch"),
                        },
                        PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::InstanceAddition(id("graft")),
                            expected_base: key("leaf"),
                        },
                    ],
                    target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                        "socket",
                    ))),
                }),
            ),
        );
        overlay.placements.push(placement(
            PrefabEntityTarget::addition(id("extra")),
            ScenePlacementParent::Root,
        ));
        overlay.additions.push(extra);
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(42)),
            }],
        });
        // Physical movement does not change the mixed typed ownership path.
        overlay.placements.push(placement(
            nested_target("leaf"),
            ScenePlacementParent::Entity(PrefabEntityTarget::source(id("host"))),
        ));
        let scene = SceneDocument::new(
            [
                SceneEntityPlacement::root(id("A")),
                SceneEntityPlacement::root(id("0_B")),
                SceneEntityPlacement::root(id("prefab_0")),
                crate::tests::child_placement(id("A_child"), id("A")),
                crate::tests::child_placement(id("skeleton_0"), id("A")),
            ],
            [
                anchor("A", "base", Some(overlay)),
                anchor("A_child", "independent", None),
                SceneEntityRecord::new(id("skeleton_0")),
                anchor("0_B", "unrelated", None),
                SceneEntityRecord::new(id("prefab_0")),
            ],
        );
        let sources = [
            ("base", base),
            (
                "branch",
                PrefabDocument::from_ordered_roots([
                    data("socket"),
                    anchor("nested", "leaf", None),
                ]),
            ),
            (
                "leaf",
                PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("socket")),
                    data("leaf"),
                ]),
            ),
            (
                "unrelated",
                PrefabDocument::from_ordered_roots([data("socket")]),
            ),
            (
                "independent",
                PrefabDocument::from_ordered_roots([data("socket")]),
            ),
        ]
        .into_iter()
        .map(|(name, document)| (request(name), (key(name), Arc::new(document))))
        .collect();
        Self { scene, sources }
    }

    fn load(
        &self,
        reference: &AssetRef,
        basis: &AuthoringSchemaBasis,
    ) -> Option<ResolvedPrefabSource> {
        self.sources.get(reference).map(|(key, document)| {
            ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                basis.clone(),
                key.clone(),
            )
        })
    }

    fn selected(
        &self,
        basis: &AuthoringSchemaBasis,
        limits: SceneCompilationLimits,
    ) -> (SelectedInstanceCompilation, Vec<AssetRef>) {
        let mut reads = Vec::new();
        let report = try_compile_selected_authoring_instance_with_loader(
            SourceDocumentInput::Scene(&self.scene),
            &id("A"),
            CONVERSION,
            basis,
            SceneCompilationOptions::default().with_limits(limits),
            None,
            |reference, _| {
                reads.push(reference.clone());
                Ok::<_, Infallible>(self.load(reference, basis))
            },
        )
        .unwrap();
        (report, reads)
    }
}

fn assert_success(report: &SelectedInstanceCompilation) {
    assert!(report.fragment.is_some(), "{report:?}");
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert!(report.failure.is_none(), "{report:?}");
    assert!(!report.budget_failed);
    assert!(report.work_bytes.is_some_and(|bytes| bytes > 0));
}

#[test]
fn selected_demand_does_not_read_missing_or_conflicted_siblings_or_local_child_instances() {
    let basis = basis();
    for conflicted in [false, true] {
        let mut fixture = Fixture::new();
        fixture.sources.remove(&request("independent"));
        if conflicted {
            fixture
                .scene
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("0_B"))
                .unwrap()
                .prefab
                .as_mut()
                .unwrap()
                .overrides = Some(PrefabOverlay::new(key("wrong-base")));
        } else {
            fixture.sources.remove(&request("unrelated"));
        }
        // Neither known malformed values nor unknown raw data acquire interpretation demand.
        for entity in &mut fixture.scene.entities {
            entity
                .components
                .insert(component(), record(ComponentValue::Bool(false)));
            entity.components.insert(
                ComponentTypeId::new("game.UnavailableRaw"),
                record(ComponentValue::String("preserve exactly".into())),
            );
        }
        let before = fixture.scene.clone();
        let (selected, reads) = fixture.selected(&basis, Default::default());
        assert_success(&selected);
        assert!(
            reads.iter().all(|read| {
                [request("base"), request("branch"), request("leaf")].contains(read)
            })
        );
        assert_eq!(
            reads
                .iter()
                .filter(|read| **read == request("branch"))
                .count(),
            2
        );
        assert_eq!(
            reads
                .iter()
                .filter(|read| **read == request("leaf"))
                .count(),
            4
        );
        assert!(selected.omissions.iter().all(|fact| {
            fact.reason == AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
        }));
        assert_eq!(fixture.scene, before);
        let full = try_compile_authoring_scene_with_loader(
            &fixture.scene,
            &basis,
            Default::default(),
            None,
            |reference, _| Ok::<_, Infallible>(fixture.load(reference, &basis)),
        )
        .unwrap();
        assert!(full.candidate().is_none(), "{full:?}");
        assert_eq!(
            full.failure().unwrap().reason(),
            if conflicted {
                &SceneCompilationFailureKind::SourceChanged
            } else {
                &SceneCompilationFailureKind::OwnerAdmission
            },
        );
    }
}

#[test]
fn selected_semantics_match_full_values_order_references_assets_and_typed_ancestry() {
    let basis = basis();
    let fixture = Fixture::new();
    let (selected, _) = fixture.selected(&basis, Default::default());
    assert_success(&selected);
    let fragment = selected.fragment.as_ref().unwrap();
    let full = try_compile_authoring_scene_with_loader(
        &fixture.scene,
        &basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(fixture.load(reference, &basis)),
    )
    .unwrap();
    assert!(full.candidate().is_some(), "{full:?}");
    let document = full.candidate().unwrap();
    let provenance = document.provenance();
    let mut remap = BTreeMap::new();
    for entity in &fixture.scene.entities {
        remap.insert(entity.id.clone(), entity.id.clone());
    }
    for (selected_id, selected_origin) in &fragment.provenance {
        let (full_id, _) = provenance
            .iter()
            .find(|(_, origin)| *origin == selected_origin)
            .unwrap();
        remap.insert(full_id.clone(), selected_id.clone());
    }
    assert_eq!(fragment.entities.len(), 22);
    assert_eq!(fragment.provenance.len(), fragment.entities.len());
    assert_eq!(fragment.roots.len(), 3);
    let full_anchor = document
        .entities()
        .iter()
        .find(|record| record.id() == &id("A"))
        .unwrap();
    assert_eq!(
        full_anchor.children()[..fragment.roots.len()]
            .iter()
            .map(|child| remap[child].clone())
            .collect::<Vec<_>>(),
        fragment.roots,
    );
    assert_eq!(
        full_anchor.children()[fragment.roots.len()..],
        [id("A_child"), id("skeleton_0")],
    );
    let copy_components = |entity: &crate::ExpandedSceneEntity| {
        entity
            .components()
            .iter()
            .map(|(component, record)| {
                let mut copied =
                    SceneComponentRecord::new(record.version, record.value().to_owned());
                copied.owner = record.owner.clone();
                copied.contextual_defaults = record.contextual_defaults.clone();
                (component.clone(), copied)
            })
            .collect::<BTreeMap<_, _>>()
    };
    for selected_entity in &fragment.entities {
        let origin = &fragment.provenance[selected_entity.id()];
        let (full_id, _) = provenance
            .iter()
            .find(|(_, actual)| *actual == origin)
            .unwrap();
        let expected = document
            .entities()
            .iter()
            .find(|entity| entity.id() == full_id)
            .unwrap();
        assert_eq!(&remap[expected.id()], selected_entity.id());
        assert_eq!(
            expected
                .children()
                .iter()
                .map(|child| remap[child].clone())
                .collect::<Vec<_>>(),
            selected_entity.children(),
        );
        let mut expected_components = copy_components(expected);
        for payload in expected_components.values_mut() {
            let plan = plan_declared_entity_references::<Infallible>(
                &schema(),
                payload.value(),
                Default::default(),
            )
            .unwrap();
            plan.rewrite_in_place(payload.value_mut(), |_, reference| {
                Ok::<_, Infallible>(match reference {
                    EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                        entity: remap[entity].clone(),
                    },
                    EntityReference::Persistent { .. } => reference.clone(),
                    EntityReference::Authored { .. } => {
                        panic!("published references must be lowered")
                    }
                })
            })
            .unwrap();
        }
        let mut actual_components = copy_components(selected_entity);
        for payload in actual_components.values_mut() {
            assert!(
                payload
                    .contextual_defaults
                    .contains_key(&ComponentFieldId::new("target"))
            );
            // Selected source conversion retains separately proven default snapshots.
            // This comparison covers its effective values and ordered projection.
            payload.contextual_defaults.clear();
        }
        assert_eq!(expected_components, actual_components);
        assert!(
            origin
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &id("A"))
        );
        assert!(
            !fixture
                .scene
                .entities
                .iter()
                .any(|entity| &entity.id == selected_entity.id())
        );
    }
    let components = fragment
        .entities
        .iter()
        .map(|entity| entity.components().len())
        .sum::<usize>();
    assert_eq!(components, 10);
    for entity in &fragment.entities {
        for payload in entity.components().values() {
            let score = if fragment.provenance[entity.id()].local_entity_id() == &id("host") {
                42
            } else {
                7
            };
            assert_eq!(
                payload.value().get("score").map(|value| value.to_owned()),
                Some(ComponentValue::U64(score))
            );
            assert_eq!(
                payload
                    .value()
                    .get("persistent")
                    .map(|value| value.to_owned()),
                Some(persistent())
            );
            assert_eq!(
                payload.value().get("image").map(|value| value.to_owned()),
                Some(image())
            );
        }
    }
    assert_eq!(selected.asset_references.len(), components);
    assert!(selected.asset_references.iter().all(|reference| {
        reference.asset_ref() == &AssetRef::path("textures/selected.png").unwrap()
    }));
    assert_eq!(selected.omissions.len(), components);
    for fact in &selected.omissions {
        assert!(full.omissions().iter().any(|full_fact| {
            full_fact.reason == fact.reason
                && full_fact.component == fact.component
                && provenance[&full_fact.entity] == fragment.provenance[&fact.entity]
        }));
    }
    let mixed = fragment.provenance.values().filter(|origin| {
        origin.projection_path().is_some_and(|path| {
            matches!(path.steps(), [
                PrefabProjectionStep::SourceEntity { entity: left, .. },
                PrefabProjectionStep::InstanceAddition { entity: graft },
                PrefabProjectionStep::SourceEntity { entity: leaf, .. },
            ] if [id("left"), id("right")].contains(left) && graft == &id("graft") && leaf == &id("leaf"))
        })
    }).count();
    assert_eq!(mixed, 2);
    // Native preview filtering cannot supply these complete source-conversion values.
    let preview = full.into_native_preview().unwrap().candidate;
    assert!(
        preview
            .entities()
            .iter()
            .all(|entity| entity.components().is_empty())
    );
}

#[test]
fn nominal_prefab_input_uses_its_actual_topology_without_instantiating_a_scene() {
    let fixture = Fixture::new();
    let basis = basis();
    let prefab = PrefabDocument::new(
        fixture.scene.placements.clone(),
        fixture.scene.entities.clone(),
    );
    let before = prefab.clone();
    let from_prefab = try_compile_selected_authoring_instance_with_loader(
        SourceDocumentInput::Prefab(&prefab),
        &id("A"),
        CONVERSION,
        &basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(fixture.load(reference, &basis)),
    )
    .unwrap();
    let (from_scene, _) = fixture.selected(&basis, Default::default());
    assert_success(&from_prefab);
    let actual = from_prefab.fragment.unwrap();
    let expected = from_scene.fragment.unwrap();
    assert_eq!(actual.roots, expected.roots);
    assert_eq!(actual.entities, expected.entities);
    assert_eq!(actual.provenance, expected.provenance);
    assert_eq!(prefab, before);
}

#[test]
fn necessary_selected_dependencies_fail_without_reading_unrelated_sources() {
    let basis = basis();
    for missing in ["base", "branch", "leaf"] {
        let mut fixture = Fixture::new();
        fixture.sources.remove(&request(missing));
        let (report, reads) = fixture.selected(&basis, Default::default());
        assert!(report.fragment.is_none(), "{report:?}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-missing" })
        );
        assert!(reads.contains(&request(missing)));
        assert!(!reads.contains(&request("unrelated")));
        assert!(!reads.contains(&request("independent")));
    }
}

#[test]
fn selected_unavailable_survivor_keeps_an_omission_with_its_typed_owner() {
    let basis = basis();
    let mut fixture = Fixture::new();
    let unknown = ComponentTypeId::new("game.UnavailableProjected");
    let opaque = record(ComponentValue::String(
        "cannot localize without Schema".into(),
    ));
    {
        let (_, source) = fixture.sources.get_mut(&request("leaf")).unwrap();
        Arc::make_mut(source)
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("leaf"))
            .unwrap()
            .components
            .insert(unknown.clone(), opaque.clone());
    }
    let (report, _) = fixture.selected(&basis, Default::default());
    assert_success(&report);
    let fragment = report.fragment.as_ref().unwrap();
    let facts = report
        .omissions
        .iter()
        .filter(|fact| fact.component == unknown)
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 4);
    for fact in facts {
        assert_eq!(
            fact.reason,
            AuthoringSceneCompilationOmissionReason::RecordUnavailable(
                AuthoringRecordReadiness::UnknownSchema
            ),
        );
        assert!(
            fragment.provenance[&fact.entity]
                .projection_path()
                .is_some()
        );
        assert!(fragment.entities.iter().any(|entity| {
            entity.id() == &fact.entity && !entity.components().contains_key(&unknown)
        }));
    }
    assert!(
        fixture.sources[&request("leaf")]
            .1
            .entities
            .iter()
            .any(|entity| {
                entity.id == id("leaf") && entity.components.get(&unknown) == Some(&opaque)
            })
    );
}

#[test]
fn every_opened_selected_scope_requires_the_exact_shared_basis() {
    let fixture = Fixture::new();
    let basis = basis();
    let other_basis = self::basis();
    assert_ne!(basis, other_basis);
    for wrong in ["base", "branch", "leaf"] {
        let report = try_compile_selected_authoring_instance_with_loader(
            SourceDocumentInput::Scene(&fixture.scene),
            &id("A"),
            CONVERSION,
            &basis,
            Default::default(),
            None,
            |reference, _| {
                Ok::<_, Infallible>(fixture.load(
                    reference,
                    if reference == &request(wrong) {
                        &other_basis
                    } else {
                        &basis
                    },
                ))
            },
        )
        .unwrap();
        assert!(report.fragment.is_none(), "{report:?}");
        assert_eq!(
            report.failure.unwrap().reason(),
            &SceneCompilationFailureKind::OwnerAdmission
        );
    }
}

#[test]
fn selection_rejects_non_instance_or_projected_locators_without_loading() {
    let fixture = Fixture::new();
    let basis = basis();
    for selected in ["absent", "prefab_0", "socket", "skeleton_0"] {
        let report = try_compile_selected_authoring_instance_with_loader(
            SourceDocumentInput::Scene(&fixture.scene),
            &id(selected),
            CONVERSION,
            &basis,
            Default::default(),
            None,
            |_, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
                panic!("only a directly owned instance anchor can create source demand")
            },
        )
        .unwrap();
        assert!(report.fragment.is_none());
        assert_eq!(
            report.failure.unwrap().reason(),
            &SceneCompilationFailureKind::MissingTarget
        );
    }
}

#[test]
fn selected_work_reuses_compiler_caps_and_honors_loader_restrictions() {
    let fixture = Fixture::new();
    let basis = basis();
    for limits in [
        SceneCompilationLimits::default().with_materialized_entities(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_materialized_components(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_materialized_value_nodes(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_materialized_value_bytes(ByteLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_resolved_instances(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_overlay_declarations(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default()
            .with_generated_identifier_bytes(ByteLimit::new(1).unwrap()),
        SceneCompilationLimits::default()
            .with_single_generated_identifier_bytes(ByteLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_provenance_bytes(ByteLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_reference_work_nodes(ItemLimit::new(1).unwrap()),
        SceneCompilationLimits::default().with_reference_work_bytes(ByteLimit::new(1).unwrap()),
    ] {
        let (report, _) = fixture.selected(&basis, limits);
        assert!(report.fragment.is_none(), "{report:?}");
        assert!(report.budget_failed);
        assert_eq!(
            report.failure.unwrap().reason(),
            &SceneCompilationFailureKind::WorkLimit
        );
    }
    let report = try_compile_selected_authoring_instance_with_loader(
        SourceDocumentInput::Scene(&fixture.scene),
        &id("A"),
        CONVERSION,
        &basis,
        Default::default(),
        None,
        |reference, working| {
            working.restrict_working_bytes(
                ByteLimit::new(working.checked_work_bytes().unwrap()).unwrap(),
            );
            Ok::<_, Infallible>(fixture.load(reference, &basis))
        },
    )
    .unwrap();
    assert!(report.fragment.is_none());
    assert!(report.budget_failed);
    let error = try_compile_selected_authoring_instance_with_loader(
        SourceDocumentInput::Scene(&fixture.scene),
        &id("A"),
        CONVERSION,
        &basis,
        Default::default(),
        None,
        |_, working| {
            working.restrict_working_bytes(ByteLimit::new(1).unwrap());
            Err::<Option<ResolvedPrefabSource>, _>("selected-loader-failure")
        },
    )
    .unwrap_err();
    assert_eq!(error, "selected-loader-failure");
}

#[test]
fn only_retained_selected_references_open_a_structurally_suppressed_dependency() {
    let basis = basis();
    for mode in ["missing", "discarded", "repaired"] {
        let mut fixture = Fixture::new();
        let mut inner = PrefabOverlay::new(key("branch"));
        inner.components.push(PrefabComponentIntent::Set {
            mode: crate::PrefabComponentSetMode::Add,
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            record: record(ComponentValue::map([(
                "target",
                authored(AuthoredEntityReference {
                    through: vec![
                        PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("inner")),
                            expected_base: key("branch"),
                        },
                        PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("dead")),
                            expected_base: key("needed-by-binding"),
                        },
                    ],
                    target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                        "socket",
                    ))),
                }),
            )]))
            .with_contextual_default(
                ComponentFieldId::new("target"),
                authored(AuthoredEntityReference {
                    through: vec![PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("inner")),
                        expected_base: key("branch"),
                    }],
                    target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                        "host",
                    ))),
                }),
            ),
        });
        fixture.sources.insert(
            request("branch"),
            (
                key("branch"),
                Arc::new(PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("host")),
                    anchor("dead", "needed-by-binding", None),
                ])),
            ),
        );
        fixture.sources.insert(
            request("base"),
            (
                key("base"),
                Arc::new(PrefabDocument::from_ordered_roots([anchor(
                    "inner",
                    "branch",
                    Some(inner),
                )])),
            ),
        );
        let mut outer = PrefabOverlay::new(key("base"));
        let inside = |entity: &str| PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("inner")),
                expected_base: key("branch"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id(entity)),
        };
        outer.suppressions.push(inside("dead"));
        if mode == "discarded" {
            outer.components.push(PrefabComponentIntent::Suppress {
                target: inside("host"),
                component: component(),
            });
        } else if mode == "repaired" {
            outer.components.push(PrefabComponentIntent::Properties {
                target: inside("host"),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(authored(AuthoredEntityReference {
                        through: vec![
                            PrefabInstanceHop {
                                anchor: PrefabEntityOrigin::SourceEntity(id("A")),
                                expected_base: key("base"),
                            },
                            PrefabInstanceHop {
                                anchor: PrefabEntityOrigin::SourceEntity(id("inner")),
                                expected_base: key("branch"),
                            },
                        ],
                        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(
                            id("host"),
                        )),
                    })),
                }],
            });
        }
        fixture
            .scene
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("A"))
            .unwrap()
            .prefab
            .as_mut()
            .unwrap()
            .overrides = Some(outer);
        let (report, reads) = fixture.selected(&basis, Default::default());
        assert_eq!(
            reads.contains(&request("needed-by-binding")),
            mode == "missing",
            "only the surviving reference can demand its nested source"
        );
        if mode == "discarded" {
            assert_success(&report);
            assert!(
                report
                    .fragment
                    .unwrap()
                    .entities
                    .iter()
                    .all(|entity| entity.components().is_empty())
            );
        } else if mode == "repaired" {
            assert_success(&report);
            let fragment = report.fragment.unwrap();
            assert!(fragment.provenance.values().all(|origin| {
                origin
                    .projection_path()
                    .unwrap()
                    .steps()
                    .iter()
                    .all(|step| {
                        !matches!(step, PrefabProjectionStep::SourceEntity { canonical_source, .. }
                        if canonical_source == &key("needed-by-binding"))
                    })
            }));
            let host = fragment
                .entities
                .iter()
                .find(|entity| fragment.provenance[entity.id()].local_entity_id() == &id("host"))
                .unwrap();
            assert_eq!(
                host.components()[&component()]
                    .value()
                    .get("target")
                    .map(|value| value.to_owned()),
                Some(local(host.id().as_str()))
            );
        } else {
            assert!(report.fragment.is_none());
            assert!(
                report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.prefab-source-missing"
                })
            );
        }
        assert!(!reads.contains(&request("unrelated")));
        assert!(!reads.contains(&request("independent")));
    }
}

#[test]
fn selected_invalid_values_layout_and_final_reference_survival_are_not_output_filters() {
    let basis = basis();
    for case in ["value", "layout", "reference"] {
        let mut fixture = Fixture::new();
        if case == "value" {
            let (_, base) = fixture.sources.get_mut(&request("base")).unwrap();
            Arc::make_mut(base)
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("host"))
                .unwrap()
                .components
                .insert(component(), record(ComponentValue::Bool(false)));
        } else {
            let overlay = fixture
                .scene
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("A"))
                .unwrap()
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap();
            if case == "layout" {
                overlay.placements.push(placement(
                    PrefabEntityTarget::source(id("host")),
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(id("socket"))),
                ));
            } else {
                // host's complete default still refers to socket after socket is removed.
                overlay
                    .suppressions
                    .push(PrefabEntityTarget::source(id("socket")));
            }
        }
        let before = fixture.scene.clone();
        let (report, reads) = fixture.selected(&basis, Default::default());
        assert!(report.fragment.is_none(), "{case}: {report:?}");
        assert!(report.diagnostics.has_errors());
        assert!(!reads.contains(&request("unrelated")));
        assert!(!reads.contains(&request("independent")));
        assert_eq!(fixture.scene, before);
    }
}

#[test]
fn final_surviving_shells_and_root_bindings_share_the_entity_cap_without_copying_raw_values() {
    let basis = basis();
    let mut fixture = Fixture::new();
    fixture.sources.insert(
        request("base"),
        (
            key("base"),
            Arc::new(PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("selected")),
            ])),
        ),
    );
    fixture
        .scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("A"))
        .unwrap()
        .prefab
        .as_mut()
        .unwrap()
        .overrides = None;
    for entity in &mut fixture.scene.entities {
        entity.components.insert(
            ComponentTypeId::new("game.LargeUnavailableRaw"),
            record(ComponentValue::String("x".repeat(128 * 1024))),
        );
    }
    for root_bound in [false, true] {
        if root_bound {
            let anchor = fixture
                .scene
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("A"))
                .unwrap();
            anchor.prefab = Some(PrefabInstance::root(
                request("base"),
                key("base"),
                PrefabEntityTarget::source(id("selected")),
            ));
            // The merged anchor is selected value demand; only the other shells stay opaque.
            anchor.components.clear();
        }
        let final_count = fixture.scene.entities.len() + usize::from(!root_bound);
        let before = fixture.scene.clone();
        for allowed in [final_count - 1, final_count] {
            let limits = SceneCompilationLimits::default()
                .with_materialized_entities(ItemLimit::new(allowed).unwrap())
                .with_materialized_components(ItemLimit::new(1).unwrap())
                .with_materialized_value_nodes(ItemLimit::new(1).unwrap())
                .with_materialized_value_bytes(ByteLimit::new(1).unwrap())
                .with_resolved_instances(ItemLimit::new(1).unwrap());
            let (report, reads) = fixture.selected(&basis, limits);
            assert_eq!(reads, [request("base")]);
            if allowed == final_count {
                assert_success(&report);
                let fragment = report.fragment.unwrap();
                assert_eq!(fragment.entities.len(), 1);
                assert_eq!(fragment.provenance.len(), 1);
                assert_eq!(fragment.roots, [fragment.entities[0].id().clone()]);
                if root_bound {
                    assert_eq!(fragment.entities[0].id(), &id("A"));
                    assert!(fragment.provenance[&id("A")].projection_path().is_none());
                }
                assert!(report.omissions.is_empty());
            } else {
                assert!(report.fragment.is_none());
                assert!(report.budget_failed);
                assert_eq!(
                    report.failure.as_ref().unwrap().reason(),
                    &SceneCompilationFailureKind::WorkLimit,
                );
                let diagnostic = report
                    .diagnostics
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
                    })
                    .unwrap();
                for (name, value) in [
                    (
                        "budget-kind",
                        DiagnosticValueRef::Identifier("materialized-entities"),
                    ),
                    ("observed", DiagnosticValueRef::Unsigned(final_count as u64)),
                    ("maximum", DiagnosticValueRef::Unsigned(allowed as u64)),
                ] {
                    assert!(
                        diagnostic.fields().iter().any(|field| {
                            field.key().as_str() == name && field.value() == value
                        }),
                        "{diagnostic:?}"
                    );
                }
            }
            assert_eq!(fixture.scene, before);
        }
    }
}

#[test]
fn absent_write_requires_an_exact_claim_even_when_the_effective_root_is_already_absent() {
    use crate::{
        __private::try_inspect_authoring_prefab_instance_with_loader, AuthoringDocumentSource,
        AuthoringPrefabPropertyClaimKind, AuthoringPrefabPropertyValue, PrefabDeclarationLimits,
        SceneAuthoringSession, SceneDocumentCandidate,
    };
    use nara_reflect::ComponentPropertyReadBudget;

    let capabilities = ComponentCapability::SCENE_AUTHORING;
    let basis = basis_with_schema(
        ComponentSchema::new(component(), "Notes", ComponentSchemaVersion::ONE)
            .with_capabilities(capabilities)
            .with_fields([
                ComponentFieldSchema::optional(
                    ComponentFieldId::new("note"),
                    "Note",
                    ComponentFieldPath::from_fields(["details", "wire_note"]),
                    ComponentValueKind::String,
                )
                .with_capabilities(capabilities),
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("score"),
                    "Score",
                    ComponentFieldPath::from_fields(["wire_score"]),
                    ComponentValueKind::U64,
                    ComponentValue::U64(7),
                )
                .with_capabilities(capabilities),
            ]),
    );
    let inner = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty()).with_owner(owner()),
        ),
    ]));
    let mut inherited = PrefabOverlay::new(key("inner"));
    inherited
        .components
        .push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("root")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("note")),
                intent: PrefabPropertyIntent::Absent,
            }],
        });
    let base = Arc::new(PrefabDocument::from_ordered_roots([anchor(
        "inner",
        "inner",
        Some(inherited),
    )]));
    let scene = SceneDocument::from_ordered_roots([anchor("A", "alias", None)]);
    let inspect = |scene: &SceneDocument| {
        try_inspect_authoring_prefab_instance_with_loader(
            SourceDocumentInput::Scene(scene),
            &id("A"),
            &basis,
            Default::default(),
            |reference, _| {
                let (source, source_key) = if reference == &request("alias") {
                    (&base, key("base"))
                } else {
                    assert_eq!(reference, &request("inner"));
                    (&inner, key("inner"))
                };
                Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                    Arc::clone(source),
                    basis.clone(),
                    source_key,
                )))
            },
        )
        .unwrap()
    };
    let published =
        SceneDocumentCandidate::decode_json_bytes(scene.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/main.scene.json").unwrap()),
                &basis,
            )
            .unwrap()
            .publish();
    let session = SceneAuthoringSession::from_published_authoring_document(published);
    let original = inspect(session.document());
    let target = original
        .targets()
        .find(|target| target.entity == PrefabEntityOrigin::SourceEntity(id("root")))
        .unwrap();
    let selected = original
        .target(target)
        .unwrap()
        .component(&component())
        .unwrap();
    let limits = PrefabDeclarationLimits::default();
    for (root, expect_default) in [("note", false), ("score", true)] {
        let property = ComponentPropertyPath::new(ComponentFieldId::new(root));
        let mut read = ComponentPropertyReadBudget::new(
            ItemLimit::new(10).unwrap(),
            ByteLimit::new(4096).unwrap(),
        );
        let observed = selected.property_value(&property, &mut read).unwrap();
        assert!(observed.revert_claim().is_none());
        assert_eq!(
            observed.controlling_claim().map(|claim| claim.kind),
            (!expect_default).then_some(AuthoringPrefabPropertyClaimKind::Absent)
        );
        assert!(matches!(
            (expect_default, observed.value),
            (false, AuthoringPrefabPropertyValue::Missing)
                | (true, AuthoringPrefabPropertyValue::Defaulted(_))
        ));
        let edit = original
            .plan_property_write(
                target,
                &component(),
                &property,
                PrefabPropertyIntent::Absent,
                limits,
                &mut read,
            )
            .unwrap();
        assert_eq!(edit.kind(), AuthoringPrefabPropertyClaimKind::Absent);
        assert!(!original.verifies_property_write(&edit, &mut read).unwrap());

        let prepared = session
            .prepare_prefab_property_write(session.revision(), &edit, limits)
            .unwrap();
        let candidate = inspect(prepared.document());
        assert!(candidate.verifies_property_write(&edit, &mut read).unwrap());
    }
}

#[test]
fn inspection_preserves_root_aliases_ordered_claims_and_typed_reference_reads() {
    use crate::{
        __private::try_inspect_authoring_prefab_instance_with_loader,
        AuthoringPrefabPropertyClaimKind, AuthoringPrefabPropertyValue,
        AuthoringPrefabRecordOrigin, AuthoringPrefabValueError, AuthoringPropertyChildrenPosition,
        AuthoringReadError, PrefabDeclarationSource, PrefabInstancePath, PrefabOverlaySlot,
    };
    use nara_reflect::{
        ComponentPropertyReadBudget, ComponentPropertyStep, ComponentValueMaterializationLimits,
    };

    let property = |root| ComponentPropertyPath::new(ComponentFieldId::new(root));
    let pin = |root, value| PrefabPropertyClaim {
        path: property(root),
        intent: PrefabPropertyIntent::Set(value),
    };
    let properties = |target, claims| PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id(target)),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims,
    };
    let basis = basis_with_schema(
        schema().with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("links"),
                "Links",
                ComponentFieldPath::from_fields(["links"]),
                ComponentValueShape::map(ComponentValueShape::EntityRef),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("annotation"),
                "Annotation",
                ComponentFieldPath::from_fields(["annotation"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]),
    );
    let inherited = SceneEntityRecord::new(id("host")).with_component(
        component(),
        record(ComponentValue::map([
            ("score", ComponentValue::U64(7)),
            ("links", ComponentValue::map([("main", local("socket"))])),
        ])),
    );
    let branch = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("host")),
            crate::tests::child_placement(id("socket"), id("host")),
        ],
        [inherited, SceneEntityRecord::new(id("socket"))],
    );
    let mut inner = PrefabOverlay::new(key("branch"));
    inner.components.push(properties(
        "host",
        vec![
            pin("score", ComponentValue::U64(7)),
            pin("persistent", persistent()),
        ],
    ));
    let mut bridge = SceneEntityRecord::new(id("bridge"));
    bridge.prefab = Some(PrefabInstance {
        overrides: Some(inner),
        ..PrefabInstance::root(
            request("branch"),
            key("branch"),
            PrefabEntityTarget::source(id("host")),
        )
    });
    let base = PrefabDocument::from_ordered_roots([bridge]);
    let root_hop = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("A")),
        expected_base: key("base"),
    };
    let inner_hop = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("bridge")),
        expected_base: key("branch"),
    };
    let root = PrefabEntityTarget {
        through: vec![root_hop.clone()],
        entity: PrefabEntityOrigin::SourceEntity(id("bridge")),
    };
    let nested = |name| PrefabEntityTarget {
        through: vec![root_hop.clone(), inner_hop.clone()],
        entity: PrefabEntityOrigin::SourceEntity(id(name)),
    };
    let host = nested("host");
    let socket = nested("socket");
    let typed_ref = authored(socket.clone().into());
    let links = ComponentValue::map([("main", typed_ref.clone())]);
    let mut outer = PrefabOverlay::new(key("base"));
    outer.components.push(properties(
        "bridge",
        vec![
            pin("score", ComponentValue::U64(7)),
            PrefabPropertyClaim {
                path: property("image"),
                intent: PrefabPropertyIntent::Absent,
            },
            PrefabPropertyClaim {
                path: property("annotation"),
                intent: PrefabPropertyIntent::Absent,
            },
            pin("links", links.clone()),
        ],
    ));
    let unknown = ComponentTypeId::new("game.InspectionOpaque");
    let mut containing = SceneEntityRecord::new(id("A")).with_component(
        unknown.clone(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    );
    containing.prefab = Some(PrefabInstance {
        overrides: Some(outer),
        ..PrefabInstance::root(
            request("alias"),
            key("base"),
            PrefabEntityTarget::source(id("bridge")),
        )
    });
    let source =
        PrefabDocument::from_ordered_roots([containing, anchor("sibling", "missing", None)]);
    let sources = [
        (request("alias"), (key("base"), Arc::new(base))),
        (request("branch"), (key("branch"), Arc::new(branch))),
    ];
    let mut reads = Vec::new();
    let snapshot = try_inspect_authoring_prefab_instance_with_loader(
        SourceDocumentInput::Prefab(&source),
        &id("A"),
        &basis,
        Default::default(),
        |reference, _| {
            reads.push(reference.clone());
            Ok::<_, Infallible>(
                sources
                    .iter()
                    .find(|(request, _)| request == reference)
                    .map(|(_, (key, source))| {
                        ResolvedPrefabSource::bound_authoring_source(
                            Arc::clone(source),
                            basis.clone(),
                            key.clone(),
                        )
                    }),
            )
        },
    )
    .unwrap();
    assert_eq!(reads, [request("alias"), request("branch")]);
    assert_eq!(snapshot.schema_basis(), &basis);
    assert!(
        snapshot
            .checked_retained_bytes()
            .is_some_and(|bytes| bytes > 0)
    );
    assert_eq!(
        snapshot.targets().collect::<Vec<_>>(),
        [&root, &host, &socket]
    );
    let root = snapshot.target(&root).unwrap();
    let alias = snapshot.target(&host).unwrap();
    assert_eq!(root.component_count(), 2);
    let opaque = alias.component(&unknown).unwrap();
    assert_eq!(
        opaque.fact().record(),
        &AuthoringPrefabRecordOrigin::SourceEntity {
            source: PrefabDeclarationSource::Containing,
            entity: id("A"),
        }
    );
    assert!(matches!(
        opaque.root_fields(),
        Err(AuthoringReadError::UnavailableComponent {
            readiness: AuthoringRecordReadiness::UnknownSchema,
        })
    ));
    let root = root.component(&component()).unwrap();
    let alias = alias.component(&component()).unwrap();
    assert!(std::ptr::eq(root.fact(), alias.fact()));
    assert_eq!(
        root.fact().record(),
        &AuthoringPrefabRecordOrigin::SourceEntity {
            source: PrefabDeclarationSource::Prefab(key("branch")),
            entity: id("host"),
        }
    );
    assert_eq!(root.property_claims().len(), 6);
    assert_eq!(
        root.property_claims()[0].location.source(),
        &PrefabDeclarationSource::Prefab(key("base"))
    );
    let mut scan = ComponentPropertyReadBudget::new(
        ItemLimit::new(100).unwrap(),
        ByteLimit::new(4096).unwrap(),
    );
    let score = root.property_value(&property("score"), &mut scan).unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = score.value else {
        panic!("equal pin must be explicit")
    };
    assert_eq!(
        value.copy_value(Default::default()).unwrap(),
        ComponentValue::U64(7)
    );
    let claim = score.revert_claim().unwrap();
    assert_eq!(claim.location.path(), &PrefabInstancePath::new(id("A")));
    assert_eq!(
        claim.slot,
        PrefabOverlaySlot::Property {
            component_index: 0,
            claim_index: 0
        }
    );
    let inherited_claim = root
        .property_value(&property("persistent"), &mut scan)
        .unwrap();
    assert!(inherited_claim.controlling_claim().is_some());
    assert!(inherited_claim.revert_claim().is_none());
    let absent = root.property_value(&property("image"), &mut scan).unwrap();
    assert!(matches!(
        absent.value,
        AuthoringPrefabPropertyValue::Defaulted(_)
    ));
    assert_eq!(
        absent.revert_claim().unwrap().kind,
        AuthoringPrefabPropertyClaimKind::Absent
    );
    let absent = root
        .property_value(&property("annotation"), &mut scan)
        .unwrap();
    assert!(matches!(
        absent.value,
        AuthoringPrefabPropertyValue::Missing
    ));
    assert_eq!(
        absent.revert_claim().unwrap().kind,
        AuthoringPrefabPropertyClaimKind::Absent
    );
    let target = root.property_value(&property("target"), &mut scan).unwrap();
    let AuthoringPrefabPropertyValue::Defaulted(target) = target.value else {
        panic!("default must retain its provenance")
    };
    assert_eq!(target.copy_value(Default::default()).unwrap(), typed_ref);
    assert_eq!(
        target
            .copy_value(ComponentValueMaterializationLimits::default().with_nodes(ItemLimit::ONE)),
        Err(AuthoringPrefabValueError::BudgetExceeded),
    );
    let page = root
        .property_children(
            &property("links"),
            AuthoringPropertyChildrenPosition::Initial,
            32,
            8,
            4096,
        )
        .unwrap();
    let entry = ComponentPropertyStep::MapEntry("main".into());
    assert_eq!(page.children[0].to_step(), entry);
    let member = root
        .property_value(&property("links").with_steps([entry]), &mut scan)
        .unwrap();
    assert_eq!(member.controlling_claim().unwrap().path, property("links"));
    assert!(member.revert_claim().is_none());
    let AuthoringPrefabPropertyValue::Present(value) = root
        .property_value(&property("links"), &mut scan)
        .unwrap()
        .value
    else {
        panic!("map pin must be explicit")
    };
    assert_eq!(value.copy_value(Default::default()).unwrap(), links);
}

#[test]
fn inspection_resolves_a_contextual_default_only_when_it_supplies_the_effective_value() {
    use crate::{
        __private::{
            AuthoringPrefabInspectionError, try_inspect_authoring_prefab_instance_with_loader,
        },
        AuthoringPrefabPropertyValue,
    };
    use nara_reflect::ComponentPropertyReadBudget;

    let basis = basis();
    let mut fixture = Fixture::new();
    let root = PrefabEntityTarget::source(id("host"));
    fixture.sources.insert(
        request("base"),
        (
            key("base"),
            Arc::new(PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("host")),
            ])),
        ),
    );
    fixture.sources.remove(&request("unrelated"));
    let selected = fixture
        .scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("A"))
        .unwrap();
    selected.prefab = Some(PrefabInstance::root(request("base"), key("base"), root));
    let contextual = authored(
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("0_B")),
                expected_base: key("unrelated"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("socket")),
        }
        .into(),
    );
    let with_target = |value| {
        record(value).with_contextual_default(ComponentFieldId::new("target"), contextual.clone())
    };
    selected.components.insert(
        component(),
        with_target(ComponentValue::map([("target", persistent())])),
    );
    let inspect = |fixture: &Fixture, reads: &mut Vec<AssetRef>| {
        try_inspect_authoring_prefab_instance_with_loader(
            SourceDocumentInput::Scene(&fixture.scene),
            &id("A"),
            &basis,
            Default::default(),
            |request, _| {
                reads.push(request.clone());
                Ok::<_, Infallible>(fixture.load(request, &basis))
            },
        )
    };
    let mut reads = Vec::new();
    let snapshot = inspect(&fixture, &mut reads).unwrap();
    assert_eq!(reads, [request("base")]);
    let view = snapshot.target(snapshot.targets().next().unwrap()).unwrap();
    let component_view = view.component(&component()).unwrap();
    let mut budget = ComponentPropertyReadBudget::new(ItemLimit::ONE, ByteLimit::new(128).unwrap());
    let property = component_view
        .property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("target")),
            &mut budget,
        )
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = property.value else {
        panic!("an explicit value shadows its declared default")
    };
    assert_eq!(value.copy_value(Default::default()).unwrap(), persistent());

    fixture
        .scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("A"))
        .unwrap()
        .components
        .insert(component(), with_target(empty()));
    reads.clear();
    let failure = inspect(&fixture, &mut reads).unwrap_err();
    let AuthoringPrefabInspectionError::Compilation { diagnostics, .. } = failure else {
        panic!("expected compiler failure")
    };
    assert_eq!(reads, [request("base"), request("unrelated")]);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-source-missing")
    );
}

#[test]
fn inspection_copies_repeated_references_at_the_exact_projected_value_limit() {
    use crate::{
        __private::try_inspect_authoring_prefab_instance_with_loader, AuthoringPrefabPropertyValue,
        AuthoringPrefabValueError,
    };
    use nara_core::DepthLimit;
    use nara_reflect::{ComponentPropertyReadBudget, ComponentValueMaterializationLimits};

    let field = ComponentFieldId::new("links");
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    let basis = basis_with_schema(
        ComponentSchema::new(component(), "Links", ComponentSchemaVersion::ONE)
            .with_capabilities(capabilities)
            .with_fields([ComponentFieldSchema::required(
                field.clone(),
                "Links",
                ComponentFieldPath::from_fields(["links"]),
                ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::EntityRef)),
            )
            .with_capabilities(capabilities)
            .with_capability(ComponentCapability::EntityRef)]),
    );
    let map_key = "k".repeat(4096);
    let authored_value = ComponentValue::map([(
        "links",
        ComponentValue::map([(
            map_key.clone(),
            ComponentValue::List(vec![local("root"); 3]),
        )]),
    )]);
    let base = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, authored_value)
                .with_owner(owner()),
        ),
    ]));
    let mut instance = SceneEntityRecord::new(id("p"));
    instance.prefab = Some(PrefabInstance::root(
        request("base"),
        key("base"),
        PrefabEntityTarget::source(id("root")),
    ));
    let scene = SceneDocument::from_ordered_roots([instance]);
    let snapshot = try_inspect_authoring_prefab_instance_with_loader(
        SourceDocumentInput::Scene(&scene),
        &id("p"),
        &basis,
        Default::default(),
        |reference, _| {
            assert_eq!(reference, &request("base"));
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(&base),
                basis.clone(),
                key("base"),
            )))
        },
    )
    .unwrap();
    let target = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("p")),
            expected_base: key("base"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("root")),
    };
    let view = snapshot
        .target(&target)
        .unwrap()
        .component(&component())
        .unwrap();
    let mut budget = ComponentPropertyReadBudget::new(ItemLimit::ONE, ByteLimit::ONE);
    let observation = view
        .property_value(&ComponentPropertyPath::new(field), &mut budget)
        .unwrap();
    let AuthoringPrefabPropertyValue::Present(value) = observation.value else {
        panic!("expected the authored Map")
    };

    let expected = ComponentValue::map([(
        map_key,
        ComponentValue::List(vec![authored(AuthoredEntityReference::source(id("p"))); 3]),
    )]);
    let exact = ComponentValueMaterializationLimits::new(
        ItemLimit::new(8).unwrap(),
        ByteLimit::new(4099).unwrap(),
        DepthLimit::new(3).unwrap(),
    );
    assert_eq!(value.copy_value(exact).unwrap(), expected);
    for limits in [
        exact.with_nodes(ItemLimit::new(7).unwrap()),
        exact.with_bytes(ByteLimit::new(4098).unwrap()),
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(8).unwrap(),
            ByteLimit::new(4099).unwrap(),
            DepthLimit::new(2).unwrap(),
        ),
    ] {
        assert_eq!(
            value.copy_value(limits),
            Err(AuthoringPrefabValueError::BudgetExceeded)
        );
    }
}
