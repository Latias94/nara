//! Actual large value programs use their composition owner's explicit property budgets.

use super::*;
use crate::{SceneCompilationFailure, SceneCompilationFailureKind};
use nara_reflect::ComponentValueKind;

#[derive(nara_ecs::Component)]
struct LargePropertyValue(ComponentValue);

fn large_component() -> ComponentTypeId {
    ComponentTypeId::new("game.LargePropertyValue")
}

fn large_schema() -> ComponentSchema {
    ComponentSchema::new(
        large_component(),
        "Large property value",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("first"),
            "First",
            ComponentFieldPath::from_fields(["first"]),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        )
        .with_capabilities([ComponentCapability::Scene]),
        ComponentFieldSchema::required(
            ComponentFieldId::new("second"),
            "Second",
            ComponentFieldPath::from_fields(["second"]),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        )
        .with_capabilities([ComponentCapability::Scene]),
        ComponentFieldSchema::required(
            ComponentFieldId::new("x"),
            "X",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::U64,
        )
        .with_capabilities([ComponentCapability::Scene]),
    ])
}

fn large_value() -> ComponentValue {
    ComponentValue::map([
        (
            "first",
            ComponentValue::List(vec![ComponentValue::U64(0); 9_000]),
        ),
        (
            "second",
            ComponentValue::List(vec![ComponentValue::U64(1); 9_000]),
        ),
        ("x", ComponentValue::U64(2)),
    ])
}

fn large_registry() -> ComponentRegistrySnapshot {
    let mut registry = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry
        .register_persistent_component_with_codec::<LargePropertyValue, _, _>(
            large_schema(),
            |value| Ok(LargePropertyValue(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry.freeze().unwrap()
}

fn large_sources() -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
            .with_component(large_component(), effective_record(large_value()))]),
    )
}

fn large_scene(root: &str, value: ComponentValue) -> SceneDocument {
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("host")),
        component: large_component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new(root)),
            intent: PrefabPropertyIntent::Set(value),
        }],
    });
    scene(overlay)
}

fn generous_limits() -> SceneCompilationLimits {
    SceneCompilationLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(32_000).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(512 * 1024).unwrap())
}

fn assert_large_result(document: &CompiledSceneDocument) {
    let values = document
        .entities()
        .iter()
        .filter_map(|entity| entity.components().get(&large_component()))
        .collect::<Vec<_>>();
    let [record] = values.as_slice() else {
        panic!("expected exactly one retained large component")
    };
    assert_large_value(record.value(), record.schema_owner());
}

fn assert_large_value(
    value: &ComponentValue,
    selected_owner: Option<&PersistentComponentSchemaOwnerId>,
) {
    let mut expected = large_value();
    expected
        .set_path(
            &ComponentFieldPath::from_fields(["x"]),
            ComponentValue::U64(7),
        )
        .unwrap();
    assert_eq!(value, &expected);
    assert_eq!(selected_owner, Some(&owner()));
}

fn assert_budget_failure(failure: &SceneCompilationFailure) {
    assert_eq!(failure.reason(), &SceneCompilationFailureKind::WorkLimit);
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
}

fn assert_semantic_failure(failure: &SceneCompilationFailure) {
    assert_eq!(failure.reason(), &SceneCompilationFailureKind::Component);
    let location = failure.location().expect("actual declaring intent");
    assert_eq!(
        location.source(),
        &crate::PrefabDeclarationSource::Containing
    );
    assert_eq!(
        location.path(),
        &crate::PrefabInstancePath::new(id("instance"))
    );
    assert_eq!(
        failure.slot(),
        Some(crate::PrefabOverlaySlot::Component { index: 0 })
    );
}

#[test]
fn native_property_expansion_accepts_large_admitted_values_and_preserves_budget_taxonomy() {
    let registry = large_registry();
    let sources = large_sources();
    let document = large_scene("x", ComponentValue::U64(7));
    let report = expand(&registry, &document, &sources, generous_limits());
    assert!(report.failure.is_none(), "{:?}", report.diagnostics);
    assert_large_result(expanded(&report));

    let cost = large_value().cost();
    for limits in [
        generous_limits().with_materialized_value_nodes(ItemLimit::new(cost.nodes() - 1).unwrap()),
        generous_limits()
            .with_materialized_value_bytes(ByteLimit::new(cost.logical_bytes() - 1).unwrap()),
    ] {
        let report = expand(&registry, &document, &sources, limits);
        assert!(report.candidate.is_none());
        assert!(report.diagnostics.has_errors());
        assert_budget_failure(report.failure.as_ref().unwrap());
    }

    for document in [
        large_scene("missing", ComponentValue::U64(7)),
        large_scene("x", ComponentValue::Bool(false)),
    ] {
        let report = expand(&registry, &document, &sources, generous_limits());
        assert!(report.candidate.is_none());
        assert_semantic_failure(report.failure.as_ref().unwrap());
    }
}

#[test]
fn scalar_pins_admit_repeated_schema_storage_prefixes_before_retaining_paths() {
    let storage_name = "v".repeat(64 * 1024);
    let schema = ComponentSchema::new(
        large_component(),
        "Long storage path",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("values"),
        "Values",
        ComponentFieldPath::from_fields([storage_name.clone()]),
        ComponentValueShape::map(ComponentValueKind::U64.into()),
    )
    .with_capabilities([ComponentCapability::Scene])]);
    let mut registry = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry
        .register_persistent_component_with_codec::<LargePropertyValue, _, _>(
            schema,
            |value| Ok(LargePropertyValue(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let value = ComponentValue::map([(
        storage_name,
        ComponentValue::map((0..512).map(|index| (format!("key_{index}"), ComponentValue::U64(0)))),
    )]);
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
            .with_component(large_component(), effective_record(value))]),
    );
    let limits = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(40 * 1024 * 1024).unwrap());
    for pin_count in [1, 512] {
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: large_component(),
            version: ComponentSchemaVersion::ONE,
            claims: (0..pin_count)
                .map(|index| PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("values")).with_steps([
                        nara_reflect::ComponentPropertyStep::MapEntry(format!("key_{index}",)),
                    ]),
                    intent: PrefabPropertyIntent::Set(ComponentValue::U64(7)),
                })
                .collect(),
        });
        let report = expand(&registry, &scene(overlay), &sources, limits);
        if pin_count == 1 {
            assert!(report.failure.is_none(), "{:?}", report.diagnostics);
            let result = expanded(&report)
                .entities()
                .iter()
                .find_map(|entity| entity.components().get(&large_component()))
                .unwrap();
            let ComponentValue::Map(fields) = result.value() else {
                panic!("expected component fields")
            };
            assert_eq!(
                fields.values().next().unwrap().get("key_0"),
                Some(&ComponentValue::U64(7))
            );
        } else {
            let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
            assert_eq!(
                field(diagnostic, "budget-kind"),
                DiagnosticValueRef::Identifier("working-bytes"),
            );
            assert_budget_failure(report.failure.as_ref().unwrap());
        }
    }
}

#[cfg(feature = "serde")]
fn authoring_expand(
    document: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
    limits: SceneCompilationLimits,
) -> crate::__private::AuthoringSceneCompilationReport {
    use crate::{
        __private::{AuthoringAdapterReceipt, try_compile_authoring_scene_with_loader},
        AuthoringSchemaBasis, PrefabSourceResolver, ResolvedPrefabSource,
    };
    use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
    use std::{convert::Infallible, sync::Arc};

    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![large_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let before = document.clone();
    let sources_before = sources.clone();
    let report = try_compile_authoring_scene_with_loader(
        document,
        &basis,
        SceneCompilationOptions::default().with_limits(limits),
        None,
        |reference, _| {
            Ok::<_, Infallible>(sources.resolve_prefab(reference).map(|source| {
                ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(source.document().clone()),
                    basis.clone(),
                    source.key().clone(),
                )
            }))
        },
    )
    .unwrap();
    assert_eq!(document, &before);
    assert_eq!(sources, &sources_before);
    report
}

#[cfg(feature = "serde")]
#[test]
fn authoring_property_expansion_accepts_large_admitted_values_and_reports_real_budget_failure() {
    let sources = large_sources();
    let document = large_scene("x", ComponentValue::U64(7));
    let report = authoring_expand(&document, &sources, generous_limits());
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    assert!(!report.budget_exceeded());
    assert!(report.failure().is_none());
    let values = report
        .candidate()
        .unwrap()
        .entities()
        .iter()
        .filter_map(|entity| entity.components().get(&large_component()))
        .collect::<Vec<_>>();
    let [record] = values.as_slice() else {
        panic!("expected exactly one retained authoring component")
    };
    assert_large_value(record.value(), record.owner());

    let cost = large_value().cost();
    for limits in [
        generous_limits().with_materialized_value_nodes(ItemLimit::new(cost.nodes() - 1).unwrap()),
        generous_limits()
            .with_materialized_value_bytes(ByteLimit::new(cost.logical_bytes() - 1).unwrap()),
    ] {
        let report = authoring_expand(&document, &sources, limits);
        assert!(report.candidate().is_none());
        assert!(report.diagnostics().has_errors());
        assert!(report.budget_exceeded());
        assert_budget_failure(report.failure().unwrap());
    }

    for document in [
        large_scene("missing", ComponentValue::U64(7)),
        large_scene("x", ComponentValue::Bool(false)),
    ] {
        let report = authoring_expand(&document, &sources, generous_limits());
        assert!(report.candidate().is_none());
        assert!(!report.budget_exceeded());
        assert_semantic_failure(report.failure().unwrap());
    }
}

fn cost_list(items: usize) -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::String("cell".into()); items])
}

fn cost_payload(items: usize) -> ComponentValue {
    ComponentValue::map([("payload", cost_list(items))])
}

fn cost_registry(migrated: Option<ComponentValue>) -> ComponentRegistrySnapshot {
    let version = if migrated.is_some() {
        ComponentSchemaVersion::new(2).unwrap()
    } else {
        ComponentSchemaVersion::ONE
    };
    let schema = ComponentSchema::new(large_component(), "Value cost", version)
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::list(ComponentValueKind::String.into()),
            cost_list(16),
        )
        .with_capabilities([ComponentCapability::Scene])]);
    let mut registry = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry
        .register_persistent_component_with_codec::<LargePropertyValue, _, _>(
            schema,
            |value| Ok(LargePropertyValue(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    if let Some(migrated) = migrated {
        registry
            .register_component_migration(
                &large_component(),
                ComponentSchemaVersion::ONE,
                version,
                move |_| Ok(migrated.clone()),
            )
            .unwrap();
    }
    registry.freeze().unwrap()
}

struct CostFixture {
    registry: ComponentRegistrySnapshot,
    document: SceneDocument,
    sources: InMemoryPrefabSourceResolver,
}

#[derive(Debug)]
struct CostCompilation {
    result: Result<crate::draft::SceneDraft, SceneCompilationFailure>,
    usage: crate::prefab::SceneCompilationUsage,
    diagnostics: nara_diagnostic::DiagnosticReport,
    reference_work: (usize, usize),
    source_upgrade_required: bool,
}

impl CostFixture {
    fn layered(initial: usize, inner: usize, outer: PrefabPropertyIntent) -> Self {
        let properties = |base: &str, target: &str, intent| {
            let mut overlay = PrefabOverlay::new(key(base));
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id(target)),
                component: large_component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("payload")),
                    intent,
                }],
            });
            overlay
        };
        let mut middle = SceneEntityRecord::new(id("middle"));
        middle.prefab = Some(PrefabInstance {
            overrides: Some(properties(
                "base",
                "host",
                PrefabPropertyIntent::Set(cost_list(inner)),
            )),
            ..PrefabInstance::root(
                request("base"),
                key("base"),
                PrefabEntityTarget::source(id("host")),
            )
        });
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(PrefabInstance {
            overrides: Some(properties("middle", "middle", outer)),
            ..PrefabInstance::root(
                request("middle"),
                key("middle"),
                PrefabEntityTarget::source(id("middle")),
            )
        });
        Self {
            registry: cost_registry(None),
            document: SceneDocument::from_ordered_roots([anchor]),
            sources: InMemoryPrefabSourceResolver::new()
                .with_prefab(
                    request("base"),
                    key("base"),
                    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
                        .with_component(
                            large_component(),
                            effective_record(cost_payload(initial)),
                        )]),
                )
                .with_prefab(
                    request("middle"),
                    key("middle"),
                    PrefabDocument::from_ordered_roots([middle]),
                ),
        }
    }

    fn compile(&self, limits: SceneCompilationLimits) -> CostCompilation {
        let before = self.document.clone();
        let sources_before = self.sources.clone();
        let mut loader = |source: &AssetRef, _: &mut crate::prefab::SceneCompilationWorkingSet| {
            Ok::<_, std::convert::Infallible>(self.sources.resolve_prefab(source))
        };
        let mut context = crate::prefab::SceneCompilationContext::new(
            (&self.registry).into(),
            &mut loader,
            SceneCompilationOptions::default().with_limits(limits),
            None,
        );
        let result = crate::prefab::overlay::compile::expand_source(
            &mut context,
            &self.document.placements,
            &self.document.entities,
            None,
        );
        if let Err(failure) = &result {
            context.push_expansion_failure(failure);
        }
        assert_eq!(self.document, before);
        assert_eq!(self.sources, sources_before);
        CostCompilation {
            result,
            usage: context.usage,
            diagnostics: context.diagnostics,
            reference_work: context.reference_work,
            source_upgrade_required: context.source_upgrade_required,
        }
    }
}

fn exact_cost_limits(items: usize) -> SceneCompilationLimits {
    SceneCompilationLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(items + 2).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(7 + 4 * items).unwrap())
}

fn assert_cost_value(report: &CostCompilation, items: usize) {
    let document = report
        .result
        .as_ref()
        .expect("bounded compilation must fit");
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert_eq!(document.roots, [id("anchor")]);
    assert_eq!(document.entities.len(), 1);
    assert_eq!(document.entities[0].id, id("anchor"));
    let record = &document.entities[0].components[&large_component()];
    assert_eq!(record.value(), &cost_payload(items));
    assert_eq!(record.owner(), Some(&owner()));
    assert_eq!(report.usage.materialized_entities, 1);
    assert_eq!(report.usage.materialized_components, 1);
    assert_eq!(report.usage.materialized_value_nodes, items + 2);
    assert_eq!(report.usage.materialized_value_bytes, 7 + 4 * items);
}

fn assert_cost_budget(report: &CostCompilation, kind: &str, maximum: usize) {
    assert_budget_failure(report.result.as_ref().unwrap_err());
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .unwrap();
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier(kind),
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(maximum as u64),
    );
    assert!(matches!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(value) if value > maximum as u64
    ));
}

#[test]
fn property_cost_tracks_growth_shrink_and_absent_across_root_bindings() {
    for (initial, inner, outer, expected) in [
        (8, 256, PrefabPropertyIntent::Set(cost_list(4)), 4),
        (256, 4, PrefabPropertyIntent::Set(cost_list(256)), 256),
        (8, 256, PrefabPropertyIntent::Absent, 16),
    ] {
        let fixture = CostFixture::layered(initial, inner, outer);
        let limits = exact_cost_limits(expected);
        assert_cost_value(&fixture.compile(limits), expected);
        assert_cost_budget(
            &fixture.compile(
                limits.with_materialized_value_nodes(ItemLimit::new(expected + 1).unwrap()),
            ),
            "materialized-value-nodes",
            expected + 1,
        );
        assert_cost_budget(
            &fixture.compile(
                limits.with_materialized_value_bytes(ByteLimit::new(6 + 4 * expected).unwrap()),
            ),
            "materialized-value-bytes",
            6 + 4 * expected,
        );
    }
}

#[test]
fn migration_and_defaults_replace_raw_value_cost_before_publication() {
    for (raw, migrated, expected) in [
        (cost_payload(256), None, 256),
        (cost_payload(4), Some(cost_payload(256)), 256),
        (cost_payload(256), Some(cost_payload(4)), 4),
        (cost_payload(256), Some(empty()), 16),
        (empty(), None, 16),
        (cost_payload(0), None, 0),
    ] {
        let upgrades = migrated.is_some();
        let fixture = CostFixture {
            registry: cost_registry(migrated),
            document: SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("anchor"))
                .with_component(large_component(), effective_record(raw))]),
            sources: InMemoryPrefabSourceResolver::new(),
        };
        let limits = exact_cost_limits(expected);
        let report = fixture.compile(limits);
        assert_cost_value(&report, expected);
        assert_eq!(report.source_upgrade_required, upgrades);
        assert_eq!(
            report.result.as_ref().unwrap().entities[0].components[&large_component()].version,
            if upgrades {
                ComponentSchemaVersion::new(2).unwrap()
            } else {
                ComponentSchemaVersion::ONE
            },
        );
        assert_cost_budget(
            &fixture.compile(
                limits.with_materialized_value_nodes(ItemLimit::new(expected + 1).unwrap()),
            ),
            "materialized-value-nodes",
            expected + 1,
        );
        assert_cost_budget(
            &fixture.compile(
                limits.with_materialized_value_bytes(ByteLimit::new(6 + 4 * expected).unwrap()),
            ),
            "materialized-value-bytes",
            6 + 4 * expected,
        );
    }
}

#[test]
fn empty_reference_plans_keep_original_work_and_exact_budget_refusals() {
    let fixture = CostFixture::layered(8, 256, PrefabPropertyIntent::Set(cost_list(4)));
    let limits = exact_cost_limits(4);
    let observed = fixture.compile(limits);
    assert_cost_value(&observed, 4);
    let working = observed.usage.checked_work_bytes().unwrap();
    let (nodes, bytes) = observed.reference_work;
    assert!(nodes > 0);
    assert!(bytes > 0);
    let exact = limits
        .with_working_bytes(ByteLimit::new(working).unwrap())
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
        .with_reference_work_bytes(ByteLimit::new(bytes).unwrap());
    let fits = fixture.compile(exact);
    assert_cost_value(&fits, 4);
    assert_eq!(fits.result, observed.result);
    assert_eq!(fits.usage, observed.usage);
    assert_eq!(fits.reference_work, observed.reference_work);
    for (limits, kind, maximum) in [
        (
            exact.with_working_bytes(ByteLimit::new(working - 1).unwrap()),
            "working-bytes",
            working - 1,
        ),
        (
            exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
            "reference-work-nodes",
            nodes - 1,
        ),
        (
            exact.with_reference_work_bytes(ByteLimit::new(bytes - 1).unwrap()),
            "reference-work-bytes",
            bytes - 1,
        ),
    ] {
        assert_cost_budget(&fixture.compile(limits), kind, maximum);
    }
}
