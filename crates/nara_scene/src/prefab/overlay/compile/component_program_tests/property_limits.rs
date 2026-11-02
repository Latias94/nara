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
    assert_large_value(&record.value, record.owner());

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
