//! Borrowed saved-source readiness, including physically dormant declarations.

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentValueKind, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt, PrefabAddition, PrefabComponentSetMode, PrefabDocument,
    PrefabInstance, PrefabInstanceClaim, PrefabOverlay, PrefabPropertyClaim, SceneDocument,
    SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.SavedReadiness")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.saved-readiness").unwrap()
}

fn version() -> ComponentSchemaVersion {
    ComponentSchemaVersion::new(2).unwrap()
}

fn basis(adapter: bool, scene_capability: bool) -> AuthoringSchemaBasis {
    let mut field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
        ComponentValue::U64(7),
    );
    if scene_capability {
        field = field.with_capabilities(ComponentCapability::SCENE_AUTHORING);
    }
    let mut schema = ComponentSchema::new(component(), "Readiness", version()).with_fields([field]);
    if scene_capability {
        schema = schema.with_capabilities(ComponentCapability::SCENE_AUTHORING);
    }
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapters = if adapter {
        vec![
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ]
    } else {
        Vec::new()
    };
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

fn key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "ddca8c29-eeda-4ea1-92bc-b6031e470e89".parse().unwrap(),
        "readiness",
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn instance(overlay: PrefabOverlay) -> PrefabInstance {
    let mut instance = PrefabInstance::forest(AssetRef::path("dormant.prefab.ron").unwrap());
    instance.overrides = Some(overlay);
    instance
}

fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        version(),
        ComponentValue::map([("value", ComponentValue::U64(7))]),
    )
    .with_owner(owner())
}

fn set(component: ComponentTypeId, record: SceneComponentRecord) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("missing")),
        component,
        record,
    }
}

fn document_at(
    site: usize,
    component: ComponentTypeId,
    record: SceneComponentRecord,
) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    if site == 0 {
        anchor.components.insert(component, record);
    } else {
        let mut overlay = PrefabOverlay::new(key());
        if site == 1 {
            let mut addition = PrefabAddition::new(id("addition"));
            addition.components.insert(component, record);
            overlay.additions.push(addition);
        } else {
            overlay.components.push(set(component, record));
        }
        if site == 3 {
            let mut parent = PrefabOverlay::new(key());
            parent.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("missing-claim-target")),
                value: Some(instance(overlay)),
            });
            let mut addition = PrefabAddition::new(id("outer-addition"));
            addition.prefab = Some(instance(parent));
            overlay = PrefabOverlay::new(key());
            overlay.additions.push(addition);
        }
        anchor.prefab = Some(instance(overlay));
    }
    SceneDocument::from_ordered_roots([anchor])
}

fn with_intents(intents: Vec<PrefabComponentIntent>) -> SceneDocument {
    let mut overlay = PrefabOverlay::new(key());
    overlay.components = intents;
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(instance(overlay));
    SceneDocument::from_ordered_roots([anchor])
}

fn properties(claims: Vec<PrefabPropertyClaim>) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("missing")),
        component: component(),
        version: version(),
        claims,
    }
}

fn claim() -> PrefabPropertyClaim {
    PrefabPropertyClaim {
        path: ComponentPropertyPath::new(ComponentFieldId::new("value")),
        intent: PrefabPropertyIntent::Set(ComponentValue::U64(9)),
    }
}

fn scan(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> bool {
    ExtractionWork::new(Default::default(), Some(0))
        .unwrap()
        .source_has_unavailable_records(SourceDocumentInput::Scene(document), basis)
        .unwrap()
}

#[test]
fn complete_records_use_shared_readiness_at_direct_addition_set_and_nested_claim_sites() {
    let available = basis(true, true);
    let unbound = basis(false, true);
    for site in 0..4 {
        let document = document_at(site, component(), record());
        assert!(!scan(&document, &available), "site {site}");
        assert!(scan(&document, &unbound), "site {site}");
        let prefab = PrefabDocument::new(document.placements.clone(), document.entities.clone());
        assert!(
            !ExtractionWork::new(Default::default(), Some(0))
                .unwrap()
                .source_has_unavailable_records(SourceDocumentInput::Prefab(&prefab), &available)
                .unwrap()
        );
        assert!(scan(
            &document_at(site, ComponentTypeId::new("test.Unknown"), record()),
            &available,
        ));
        for version in [
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(3).unwrap(),
        ] {
            let mut mismatch = record();
            mismatch.version = version;
            assert!(scan(&document_at(site, component(), mismatch), &available));
        }
        let mismatch = record()
            .with_owner(PersistentComponentSchemaOwnerId::try_new("test.other-owner").unwrap());
        assert!(scan(&document_at(site, component(), mismatch), &available));
    }
}

#[test]
fn suppress_and_empty_properties_require_actual_schema_adapter_capability_and_version() {
    let available = basis(true, true);
    let suppress = PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("missing")),
        component: component(),
    };
    for intent in [suppress, properties(Vec::new())] {
        let document = with_intents(vec![intent]);
        assert!(!scan(&document, &available));
        assert!(scan(&document, &basis(false, true)));
        assert!(scan(&document, &basis(false, false)));
        assert!(scan(
            &document,
            &AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap(),
        ));
    }
    let mut intent = properties(Vec::new());
    if let PrefabComponentIntent::Properties { version, .. } = &mut intent {
        *version = ComponentSchemaVersion::ONE;
    }
    assert!(scan(&with_intents(vec![intent]), &available));
}

#[test]
fn dormant_properties_reuse_reflect_selectors_values_and_original_claim_conflicts() {
    let basis = basis(true, true);
    assert!(!scan(
        &with_intents(vec![properties(vec![claim()])]),
        &basis
    ));
    let mut absent = claim();
    absent.intent = PrefabPropertyIntent::Absent;
    assert!(!scan(&with_intents(vec![properties(vec![absent])]), &basis));
    let mut unknown = claim();
    unknown.path = ComponentPropertyPath::new(ComponentFieldId::new("unknown"));
    let mut invalid_value = claim();
    invalid_value.intent = PrefabPropertyIntent::Set(ComponentValue::String("wrong shape".into()));
    let mut nested_absent = claim();
    nested_absent
        .path
        .steps
        .push(ComponentPropertyStep::MapEntry("missing".into()));
    nested_absent.intent = PrefabPropertyIntent::Absent;
    for claims in [
        vec![unknown],
        vec![invalid_value],
        vec![nested_absent],
        vec![claim(), claim()],
    ] {
        assert!(scan(&with_intents(vec![properties(claims)]), &basis));
    }
}

#[test]
fn readiness_shares_exact_residual_allowances_and_refusal_never_reports_safe() {
    let basis = basis(true, true);
    let document = with_intents(vec![properties(vec![claim()])]);
    let original = document.clone();
    let defaults = SceneCompilationLimits::default();
    let run = |limits| {
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        work.visit(3, 11, 17).unwrap();
        let result =
            work.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis);
        (work, result)
    };
    let (baseline, result) = run(defaults);
    assert!(!result.unwrap());
    let exact = defaults
        .with_reference_work_nodes(ItemLimit::new(baseline.nodes).unwrap())
        .with_reference_work_bytes(ByteLimit::new(baseline.reference_bytes).unwrap())
        .with_working_bytes(ByteLimit::new(baseline.bytes).unwrap());
    assert!(!run(exact).1.unwrap());
    for short in [
        exact.with_reference_work_nodes(ItemLimit::new(baseline.nodes - 1).unwrap()),
        exact.with_reference_work_bytes(ByteLimit::new(baseline.reference_bytes - 1).unwrap()),
        exact.with_working_bytes(ByteLimit::new(baseline.bytes - 1).unwrap()),
    ] {
        assert!(matches!(run(short).1, Err(ExtractionError::Capacity)));
    }
    let mut exhausted = ExtractionWork::new(defaults, Some(0)).unwrap();
    exhausted.property_reads = ComponentPropertyReadBudget::new(ItemLimit::ONE, ByteLimit::ONE);
    assert!(matches!(
        exhausted.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis),
        Err(ExtractionError::Capacity),
    ));
    assert_eq!(document, original);
}

#[test]
fn observed_raw_cost_preserves_cost_only_results_and_original_record_and_intent_borrows() {
    struct Observer {
        records: Vec<*const SceneComponentRecord>,
        intents: Vec<*const PrefabComponentIntent>,
        instances: Vec<*const PrefabInstance>,
        overlays: Vec<*const PrefabOverlay>,
        source_placements: Vec<(*const SceneEntityPlacement, usize)>,
        scratch: Vec<Option<usize>>,
        refuse_scratch: Option<usize>,
        raw_nodes: usize,
        raw_bytes: usize,
    }
    impl crate::patch::SourceCostObserver for Observer {
        fn admit_scratch(&mut self, bytes: Option<usize>) -> bool {
            let index = self.scratch.len();
            self.scratch.push(bytes);
            self.refuse_scratch != Some(index)
        }

        fn progress(&mut self, nodes: usize, bytes: usize) -> bool {
            self.raw_nodes += nodes;
            self.raw_bytes += bytes;
            true
        }

        fn record(&mut self, _component: &ComponentTypeId, record: &SceneComponentRecord) -> bool {
            self.records.push(record);
            true
        }

        fn instance(&mut self, instance: &PrefabInstance) -> bool {
            self.instances.push(instance);
            true
        }

        fn overlay(&mut self, overlay: &PrefabOverlay) -> bool {
            self.overlays.push(overlay);
            true
        }

        fn source_placements(&mut self, placements: &[SceneEntityPlacement]) -> bool {
            self.source_placements
                .push((placements.as_ptr(), placements.len()));
            true
        }

        fn component_intent(&mut self, intent: &PrefabComponentIntent) -> bool {
            self.intents.push(intent);
            true
        }
    }
    let mut document = with_intents(vec![
        set(component(), record()),
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("missing")),
            component: component(),
        },
        properties(vec![claim()]),
    ]);
    document.entities[0]
        .components
        .insert(component(), record());
    let overlay = document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let mut addition = PrefabAddition::new(id("addition"));
    addition.components.insert(component(), record());
    let mut nested = PrefabOverlay::new(key());
    nested.components.push(set(component(), record()));
    addition.prefab = Some(instance(nested));
    overlay.additions.push(addition);
    let overlay = document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Set {
        record: set_record, ..
    } = &overlay.components[0]
    else {
        unreachable!();
    };
    let nested = overlay.additions[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Set {
        record: nested_record,
        ..
    } = &nested.components[0]
    else {
        unreachable!();
    };
    let expected_records = [
        &document.entities[0].components[&component()] as *const _,
        &overlay.additions[0].components[&component()] as *const _,
        set_record as *const _,
        nested_record as *const _,
    ];
    let expected_intents = overlay
        .components
        .iter()
        .chain(&nested.components)
        .map(|intent| intent as *const _)
        .collect::<Vec<_>>();
    let limits = ScenePatchApplyLimits::default();
    let mut scratch = Vec::new();
    let expected_cost =
        crate::patch::bounded_scene_source_cost_with_admission(&document, limits, |bytes| {
            scratch.push(bytes);
            Ok::<_, ()>(())
        })
        .unwrap()
        .unwrap();
    let mut observer = Observer {
        records: Vec::new(),
        intents: Vec::new(),
        instances: Vec::new(),
        overlays: Vec::new(),
        source_placements: Vec::new(),
        scratch: Vec::new(),
        refuse_scratch: None,
        raw_nodes: 0,
        raw_bytes: 0,
    };
    let cost = crate::patch::bounded_source_cost_with_observer(
        SourceDocumentInput::Scene(&document),
        limits,
        &mut observer,
    )
    .unwrap();
    assert_eq!(cost, expected_cost);
    assert_eq!(observer.raw_bytes, cost.logical_bytes());
    // Nested overlay values already belong to operation_count; this direct record does not.
    assert_eq!(
        observer.raw_nodes,
        cost.operation_count() + record().value.cost().nodes()
    );
    assert_eq!(observer.scratch, scratch);
    assert_eq!(observer.records.len(), expected_records.len());
    assert!(
        expected_records
            .iter()
            .all(|record| observer.records.contains(record))
    );
    assert_eq!(observer.intents, expected_intents);
    assert_eq!(
        observer.instances,
        [
            document.entities[0].prefab.as_ref().unwrap() as *const _,
            overlay.additions[0].prefab.as_ref().unwrap() as *const _,
        ]
    );
    assert_eq!(observer.overlays, [overlay as *const _, nested as *const _]);
    assert_eq!(
        observer.source_placements,
        [(document.placements.as_ptr(), document.placements.len()),]
    );
    for denied in 0..scratch.len() {
        let mut observer = Observer {
            records: Vec::new(),
            intents: Vec::new(),
            instances: Vec::new(),
            overlays: Vec::new(),
            source_placements: Vec::new(),
            scratch: Vec::new(),
            refuse_scratch: Some(denied),
            raw_nodes: 0,
            raw_bytes: 0,
        };
        assert!(
            crate::patch::bounded_source_cost_with_observer(
                SourceDocumentInput::Scene(&document),
                limits,
                &mut observer,
            )
            .is_none()
        );
        assert_eq!(observer.scratch.len(), denied + 1);
    }
}

fn late_limit_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        version(),
        ComponentValue::List(vec![
            ComponentValue::List(vec![ComponentValue::String(
                "x".into()
            )]);
            8_192
        ]),
    )
}

#[test]
fn late_raw_value_failure_spends_its_prefix_before_retrying_the_same_work() {
    let basis = basis(true, true);
    let document = document_at(
        0,
        ComponentTypeId::new("missing.Payload"),
        late_limit_record(),
    );
    let limits = SceneCompilationLimits::default()
        .with_reference_work_nodes(ItemLimit::new(10_000).unwrap());
    let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
    assert!(matches!(
        work.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis),
        Err(ExtractionError::Capacity),
    ));
    let first = work.usage();
    assert!(first.0 >= 3_616 && first.0 < 10_000);
    assert!(work.reference_bytes >= 1_807);
    assert!(first.1 >= 1_807);

    assert!(matches!(
        work.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis),
        Err(ExtractionError::Capacity),
    ));
    let second = work.usage();
    assert!(second.0 > first.0);
    assert!(second.1 > first.1);
    assert!(
        second.0 - first.0 < first.0,
        "retry cannot repeat the large prefix for free"
    );
}

#[test]
fn nested_overlay_raw_failures_spend_work_and_never_report_a_readiness_result() {
    let basis = basis(true, true);
    let limits = SceneCompilationLimits::default()
        .with_reference_work_nodes(ItemLimit::new(10_000).unwrap());
    for site in 1..4 {
        let document = document_at(
            site,
            ComponentTypeId::new("missing.Payload"),
            late_limit_record(),
        );
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        assert!(matches!(
            work.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis),
            Err(ExtractionError::Capacity),
        ));
        let first = work.usage();
        assert!(
            first.0 > 9_000,
            "site {site} must retain the long visited prefix"
        );
        assert!(work.reference_bytes > 1_000);
        assert!(matches!(
            work.source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis),
            Err(ExtractionError::Capacity),
        ));
        assert!(work.usage().0 >= first.0);
        assert!(work.usage().1 >= first.1);
    }
}

#[test]
fn successful_raw_progress_is_settled_once_separately_from_scratch_and_schema_reads() {
    #[derive(Default)]
    struct Progress {
        nodes: usize,
        bytes: usize,
        scratch_calls: usize,
        scratch_bytes: usize,
    }
    impl crate::patch::SourceCostObserver for Progress {
        fn admit_scratch(&mut self, bytes: Option<usize>) -> bool {
            self.scratch_calls += 1;
            self.scratch_bytes += bytes.unwrap();
            true
        }

        fn progress(&mut self, nodes: usize, bytes: usize) -> bool {
            self.nodes += nodes;
            self.bytes += bytes;
            true
        }
    }
    let basis = basis(true, true);
    let document = document_at(0, component(), record());
    let mut progress = Progress::default();
    let cost = crate::patch::bounded_source_cost_with_observer(
        SourceDocumentInput::Scene(&document),
        ScenePatchApplyLimits::default(),
        &mut progress,
    )
    .unwrap();
    assert_eq!(
        progress.nodes,
        cost.operation_count() + cost.value_cost().nodes()
    );
    assert_eq!(progress.bytes, cost.logical_bytes());

    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    assert!(
        !work
            .source_has_unavailable_records(SourceDocumentInput::Scene(&document), &basis)
            .unwrap()
    );
    let schema_bytes = component().as_str().len() + owner().as_str().len();
    assert_eq!(
        work.usage(),
        (
            progress.nodes + progress.scratch_calls + 1,
            progress.bytes + progress.scratch_bytes + schema_bytes,
        )
    );
    assert_eq!(work.reference_bytes, progress.bytes + schema_bytes);
}

#[test]
fn raw_progress_keeps_spent_deltas_when_any_allowance_is_exceeded() {
    let limits = SceneCompilationLimits::default()
        .with_reference_work_nodes(ItemLimit::new(2).unwrap())
        .with_reference_work_bytes(ByteLimit::new(3).unwrap())
        .with_working_bytes(ByteLimit::new(4).unwrap());
    let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
    assert!(matches!(
        work.settle_raw_progress(3, 5),
        Err(ExtractionError::Capacity)
    ));
    assert_eq!(work.usage(), (3, 5));
    assert_eq!(work.reference_bytes, 5);
    assert!(matches!(work.traversal(), Err(ExtractionError::Capacity)));
}

#[test]
fn value_and_source_copy_attempts_retain_late_failed_prefixes() {
    let record = late_limit_record();
    let document = document_at(0, component(), record.clone());
    let limits = SceneCompilationLimits::default()
        .with_reference_work_nodes(ItemLimit::new(10_000).unwrap());
    for source_copy in [false, true] {
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        let mut first_nodes = 0;
        for attempt in 0..2 {
            let before = work.usage();
            let result = if source_copy {
                work.source_copy(SourceDocumentInput::Scene(&document))
            } else {
                work.value(&record.value).map(|_| ())
            };
            assert!(matches!(result, Err(ExtractionError::Capacity)));
            let after = work.usage();
            assert!(after.0 > before.0);
            assert!(after.1 >= before.1);
            if attempt == 0 {
                first_nodes = after.0;
                assert!(first_nodes > 3_000);
                assert!(after.1 > before.1);
            } else {
                assert!(after.0 - before.0 < first_nodes);
            }
        }
    }
}

#[test]
fn successful_value_and_source_copy_keep_raw_work_and_clone_storage_separate() {
    let record = record();
    let mut scratch = Vec::new();
    let value_cost = record
        .value
        .cost_up_to_with_admission(usize::MAX, usize::MAX, |bytes| {
            scratch.push(bytes.unwrap());
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap()
        .unwrap();
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    assert_eq!(work.value(&record.value).unwrap(), value_cost);
    assert_eq!(
        work.usage(),
        (
            value_cost.nodes() + scratch.len(),
            value_cost.logical_bytes() + scratch.iter().sum::<usize>(),
        )
    );
    assert_eq!(work.reference_bytes, value_cost.logical_bytes());

    let document = document_at(0, component(), record);
    scratch.clear();
    let cost = crate::patch::bounded_scene_source_cost_with_admission(
        &document,
        ScenePatchApplyLimits::default(),
        |bytes| {
            scratch.push(bytes.unwrap());
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap()
    .unwrap();
    let clone_bytes =
        cost.operation_count() * 512 + cost.value_cost().nodes() * 96 + cost.logical_bytes();
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    work.source_copy(SourceDocumentInput::Scene(&document))
        .unwrap();
    assert_eq!(
        work.usage(),
        (
            cost.operation_count() + cost.value_cost().nodes() + scratch.len() + 1,
            cost.logical_bytes() + scratch.iter().sum::<usize>() + clone_bytes,
        )
    );
    assert_eq!(work.reference_bytes, cost.logical_bytes());
}
