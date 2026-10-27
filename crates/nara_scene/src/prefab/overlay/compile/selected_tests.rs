//! Selected demand comparisons use the production staged compiler and exact Schema admission.

use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    sync::Arc,
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
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
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabExpansionFailureKind,
    PrefabExpansionLimits, PrefabExpansionOptions, PrefabInstance, PrefabInstanceHop,
    PrefabOverlay, PrefabPlacement, PrefabPlacementParent, PrefabPlacementSlot,
    PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceBinding,
    ResolvedPrefabSource, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SourceDocumentKey, SourceDocumentKind,
    prefab::{
        AuthoringPrefabOmissionReason, SelectedInstanceCompilation, SelectedInstanceSource,
        try_compile_selected_authoring_instance_with_loader,
        try_expand_authoring_prefabs_with_loader,
    },
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
            ComponentValueShape::AssetRef,
            image(),
        )
        .with_capabilities(capabilities)
        .with_capability(ComponentCapability::AssetRef),
    ])
}

fn basis() -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema()],
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
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
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
        source: request(source),
        overrides: overlay,
    });
    entity
}

fn placement(target: PrefabEntityTarget, parent: PrefabPlacementParent) -> PrefabPlacement {
    PrefabPlacement {
        target,
        parent,
        slot: PrefabPlacementSlot::Last,
    }
}

fn binding(target: PrefabEntityTarget) -> PrefabReferenceBinding {
    PrefabReferenceBinding {
        slot: ComponentFieldPath::from_fields(["target"]),
        expected: id("socket"),
        target,
    }
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
        let mut branch_overlay = PrefabOverlay::new(key("branch"));
        let mut badge = PrefabAddition::new(id("badge"));
        badge.components.insert(component(), record(empty()));
        badge.bindings.insert(
            component(),
            vec![binding(PrefabEntityTarget::source(id("socket")))],
        );
        branch_overlay.placements.push(placement(
            PrefabEntityTarget::addition(id("badge")),
            PrefabPlacementParent::Entity(PrefabEntityTarget::source(id("socket"))),
        ));
        branch_overlay.additions.push(badge);
        let mut graft = PrefabAddition::new(id("graft"));
        graft.prefab = Some(PrefabInstance {
            source: request("leaf"),
            overrides: None,
        });
        branch_overlay.placements.push(placement(
            PrefabEntityTarget::addition(id("graft")),
            PrefabPlacementParent::Root,
        ));
        branch_overlay.additions.push(graft);

        let base = PrefabDocument::new(
            [id("host"), id("other")],
            [
                data("host").with_children([id("socket"), id("left"), id("right")]),
                SceneEntityRecord::new(id("socket")),
                anchor("left", "branch", Some(branch_overlay.clone())),
                anchor("right", "branch", Some(branch_overlay)),
                SceneEntityRecord::new(id("other")),
            ],
        );
        let mut overlay = PrefabOverlay::new(key("base"));
        let mut extra = PrefabAddition::new(id("extra"));
        extra.components.insert(component(), record(empty()));
        extra
            .bindings
            .insert(component(), vec![binding(nested_target("socket"))]);
        overlay.placements.push(placement(
            PrefabEntityTarget::addition(id("extra")),
            PrefabPlacementParent::Root,
        ));
        overlay.additions.push(extra);
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(42)),
                bindings: vec![],
            }],
        });
        // Physical movement does not change the mixed typed ownership path.
        overlay.placements.push(placement(
            nested_target("leaf"),
            PrefabPlacementParent::Entity(PrefabEntityTarget::source(id("host"))),
        ));
        let scene = SceneDocument::new(
            [id("A"), id("0_B"), id("prefab_0")],
            [
                anchor("A", "base", Some(overlay)).with_children([id("A_child"), id("skeleton_0")]),
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
        limits: PrefabExpansionLimits,
    ) -> (SelectedInstanceCompilation, Vec<AssetRef>) {
        let mut reads = Vec::new();
        let report = try_compile_selected_authoring_instance_with_loader(
            SelectedInstanceSource::Scene(&self.scene),
            &id("A"),
            basis,
            PrefabExpansionOptions::default().with_limits(limits),
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
            fact.reason == AuthoringPrefabOmissionReason::NativeBindingUnavailable
        }));
        assert_eq!(fixture.scene, before);
        let full = try_expand_authoring_prefabs_with_loader(
            &fixture.scene,
            &basis,
            Default::default(),
            None,
            |reference, _| Ok::<_, Infallible>(fixture.load(reference, &basis)),
        )
        .unwrap();
        assert!(full.candidate.is_none(), "{full:?}");
        assert_eq!(
            full.failure.unwrap().reason(),
            if conflicted {
                &PrefabExpansionFailureKind::SourceChanged
            } else {
                &PrefabExpansionFailureKind::OwnerAdmission
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
    let full = try_expand_authoring_prefabs_with_loader(
        &fixture.scene,
        &basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(fixture.load(reference, &basis)),
    )
    .unwrap();
    assert!(full.candidate.is_some(), "{full:?}");
    let candidate = full.candidate.as_ref().unwrap();
    let document = candidate.document();
    let provenance = candidate.provenance();
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
        .entities
        .iter()
        .find(|record| record.id == id("A"))
        .unwrap();
    assert_eq!(
        full_anchor.children[..fragment.roots.len()]
            .iter()
            .map(|child| remap[child].clone())
            .collect::<Vec<_>>(),
        fragment.roots,
    );
    assert_eq!(
        full_anchor.children[fragment.roots.len()..],
        [id("A_child"), id("skeleton_0")],
    );
    for selected_entity in &fragment.entities {
        let origin = &fragment.provenance[&selected_entity.id];
        let (full_id, _) = provenance
            .iter()
            .find(|(_, actual)| *actual == origin)
            .unwrap();
        let mut expected = document
            .entities
            .iter()
            .find(|entity| &entity.id == full_id)
            .unwrap()
            .clone();
        expected.id = selected_entity.id.clone();
        for child in &mut expected.children {
            *child = remap[child].clone();
        }
        for payload in expected.components.values_mut() {
            let plan = plan_declared_entity_references::<Infallible>(
                &schema(),
                &payload.value,
                Default::default(),
            )
            .unwrap();
            plan.rewrite_in_place(&mut payload.value, |_, reference| {
                Ok::<_, Infallible>(match reference {
                    EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                        entity: remap[entity].clone(),
                    },
                    EntityReference::Persistent { .. } => reference.clone(),
                })
            })
            .unwrap();
        }
        assert_eq!(&expected, selected_entity);
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
                .any(|entity| entity.id == selected_entity.id)
        );
    }
    let components = fragment
        .entities
        .iter()
        .map(|entity| entity.components.len())
        .sum::<usize>();
    assert_eq!(components, 10);
    for entity in &fragment.entities {
        for payload in entity.components.values() {
            let score = if fragment.provenance[&entity.id].local_entity_id() == &id("host") {
                42
            } else {
                7
            };
            assert_eq!(
                payload.value.get("score"),
                Some(&ComponentValue::U64(score))
            );
            assert_eq!(payload.value.get("persistent"), Some(&persistent()));
            assert_eq!(payload.value.get("image"), Some(&image()));
        }
    }
    assert_eq!(selected.asset_references.len(), components);
    assert!(selected.asset_references.iter().all(|reference| {
        reference.asset_ref() == &AssetRef::path("textures/selected.png").unwrap()
    }));
    assert_eq!(selected.omissions.len(), components);
    for fact in &selected.omissions {
        assert!(full.omissions.iter().any(|full_fact| {
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
    let (preview, _) = full.into_native_preview().unwrap();
    assert!(
        preview
            .document()
            .entities
            .iter()
            .all(|entity| entity.components.is_empty())
    );
}

#[test]
fn nominal_prefab_input_uses_its_actual_topology_without_instantiating_a_scene() {
    let fixture = Fixture::new();
    let basis = basis();
    let prefab = PrefabDocument::new(fixture.scene.roots.clone(), fixture.scene.entities.clone());
    let before = prefab.clone();
    let from_prefab = try_compile_selected_authoring_instance_with_loader(
        SelectedInstanceSource::Prefab(&prefab),
        &id("A"),
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
    {
        let (_, source) = fixture.sources.get_mut(&request("leaf")).unwrap();
        Arc::make_mut(source)
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("leaf"))
            .unwrap()
            .components
            .insert(
                unknown.clone(),
                record(ComponentValue::String(
                    "cannot localize without Schema".into(),
                )),
            );
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
            AuthoringPrefabOmissionReason::RecordUnavailable(
                AuthoringRecordReadiness::UnknownSchema
            ),
        );
        assert!(
            fragment.provenance[&fact.entity]
                .projection_path()
                .is_some()
        );
        assert!(fragment.entities.iter().any(|entity| {
            entity.id == fact.entity && !entity.components.contains_key(&unknown)
        }));
    }
    assert!(
        fixture.sources[&request("leaf")]
            .1
            .entities
            .iter()
            .any(|entity| { entity.id == id("leaf") && entity.components.contains_key(&unknown) })
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
            SelectedInstanceSource::Scene(&fixture.scene),
            &id("A"),
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
            &PrefabExpansionFailureKind::OwnerAdmission
        );
    }
}

#[test]
fn selection_rejects_non_instance_or_projected_locators_without_loading() {
    let fixture = Fixture::new();
    let basis = basis();
    for selected in ["absent", "prefab_0", "socket", "skeleton_0"] {
        let report = try_compile_selected_authoring_instance_with_loader(
            SelectedInstanceSource::Scene(&fixture.scene),
            &id(selected),
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
            &PrefabExpansionFailureKind::MissingTarget
        );
    }
}

#[test]
fn selected_work_reuses_compiler_caps_and_honors_loader_restrictions() {
    let fixture = Fixture::new();
    let basis = basis();
    for limits in [
        PrefabExpansionLimits::default().with_materialized_entities(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_materialized_components(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_materialized_value_nodes(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_materialized_value_bytes(ByteLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_resolved_instances(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_overlay_declarations(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default()
            .with_generated_identifier_bytes(ByteLimit::new(1).unwrap()),
        PrefabExpansionLimits::default()
            .with_single_generated_identifier_bytes(ByteLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_provenance_bytes(ByteLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_working_bytes(ByteLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_reference_work_nodes(ItemLimit::new(1).unwrap()),
        PrefabExpansionLimits::default().with_reference_work_bytes(ByteLimit::new(1).unwrap()),
    ] {
        let (report, _) = fixture.selected(&basis, limits);
        assert!(report.fragment.is_none(), "{report:?}");
        assert!(report.budget_failed);
        assert_eq!(
            report.failure.unwrap().reason(),
            &PrefabExpansionFailureKind::WorkLimit
        );
    }
    let report = try_compile_selected_authoring_instance_with_loader(
        SelectedInstanceSource::Scene(&fixture.scene),
        &id("A"),
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
        SelectedInstanceSource::Scene(&fixture.scene),
        &id("A"),
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
fn only_retained_selected_bindings_open_a_structurally_suppressed_dependency() {
    let basis = basis();
    for mode in ["missing", "discarded", "repaired"] {
        let mut fixture = Fixture::new();
        let mut inner = PrefabOverlay::new(key("branch"));
        inner.components.push(PrefabComponentIntent::Set {
            mode: crate::PrefabComponentSetMode::Add,
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            record: record(empty()),
            bindings: vec![binding(PrefabEntityTarget {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("dead")),
                    expected_base: key("needed-by-binding"),
                }],
                entity: PrefabEntityOrigin::SourceEntity(id("socket")),
            })],
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
            fixture.sources.insert(
                request("needed-by-binding"),
                (
                    key("needed-by-binding"),
                    Arc::new(PrefabDocument::from_ordered_roots([
                        SceneEntityRecord::new(id("socket"))
                            .with_component(component(), record(ComponentValue::Bool(false))),
                    ])),
                ),
            );
            outer.components.push(PrefabComponentIntent::Properties {
                target: inside("host"),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(local("socket")),
                    bindings: vec![PrefabReferenceBinding {
                        slot: ComponentFieldPath::empty(),
                        expected: id("socket"),
                        target: inside("host"),
                    }],
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
            mode != "discarded"
        );
        if mode == "discarded" {
            assert_success(&report);
            assert!(
                report
                    .fragment
                    .unwrap()
                    .entities
                    .iter()
                    .all(|entity| entity.components.is_empty())
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
                .find(|entity| fragment.provenance[&entity.id].local_entity_id() == &id("host"))
                .unwrap();
            assert_eq!(
                host.components[&component()].value.get("target"),
                Some(&local(host.id.as_str()))
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
                    PrefabPlacementParent::Entity(PrefabEntityTarget::source(id("socket"))),
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
fn containing_shells_count_against_the_existing_entity_cap_without_copying_raw_values() {
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
    let before = fixture.scene.clone();
    for allowed in [5, 6] {
        let limits = PrefabExpansionLimits::default()
            .with_materialized_entities(ItemLimit::new(allowed).unwrap())
            .with_materialized_components(ItemLimit::new(1).unwrap())
            .with_materialized_value_nodes(ItemLimit::new(1).unwrap())
            .with_materialized_value_bytes(ByteLimit::new(1).unwrap())
            .with_resolved_instances(ItemLimit::new(1).unwrap());
        let (report, reads) = fixture.selected(&basis, limits);
        assert_eq!(reads, [request("base")]);
        if allowed == 6 {
            assert_success(&report);
            assert_eq!(report.fragment.unwrap().entities.len(), 1);
            assert!(report.omissions.is_empty());
        } else {
            assert!(report.fragment.is_none());
            assert!(report.budget_failed);
        }
        assert_eq!(fixture.scene, before);
    }
}
