//! Regression cases through the production declaration-selection and expansion owner.

#[path = "compile/failure_tests.rs"]
mod failure_tests;

use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticValueRef;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};

use super::*;
use crate::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabDocument, PrefabExpansionLimits,
    PrefabExpansionOptions, PrefabExpansionReport, PrefabInstanceClaim, PrefabPlacement,
    PrefabSourceResolver, SceneComponentRecord, register_scene_components,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{value}.ron")).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "17148e7d-a75b-4f3d-9132-759252fbd872".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn registry() -> ComponentRegistry {
    #[derive(nara_ecs::Component)]
    struct C(ComponentValue);
    #[derive(nara_ecs::Component)]
    struct D(ComponentValue);
    let schema = |name| {
        ComponentSchema::new(
            ComponentTypeId::new(name),
            name,
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
    };
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry
        .register_persistent_component_with_codec::<C, _, _>(
            schema("test.CompilerC"),
            |value| Ok(C(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry
        .register_persistent_component_with_codec::<D, _, _>(
            schema("test.CompilerD"),
            |value| Ok(D(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn instance(source: &str, overrides: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: request(source),
        overrides,
    }
}

fn anchor(name: &str, source: &str, overlay: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    entity.prefab = Some(instance(source, overlay));
    entity
}

fn scene(source: &str, overlay: Option<PrefabOverlay>) -> SceneDocument {
    SceneDocument::from_ordered_roots([anchor("host", source, overlay)])
}

fn inside(anchor: &str, base: &str, terminal: PrefabEntityOrigin) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: key(base),
        }],
        entity: terminal,
    }
}

fn root(target: PrefabEntityTarget) -> PrefabPlacement {
    PrefabPlacement {
        target,
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    }
}

fn add_component(target: PrefabEntityTarget, component: &str) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target,
        component: ComponentTypeId::new(component),
        record: SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::U64(1))]),
        ),
        bindings: Vec::new(),
    }
}

fn projected(report: &PrefabExpansionReport, local: &str) -> SceneEntityId {
    report
        .provenance()
        .unwrap()
        .iter()
        .find(|(_, provenance)| {
            provenance.local_entity_id() == &id(local) && provenance.projection_path().is_some()
        })
        .unwrap()
        .0
        .clone()
}

fn budget_kind(report: &PrefabExpansionReport) -> Option<&str> {
    report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .and_then(|diagnostic| {
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == "budget-kind")
        })
        .and_then(|field| match field.value() {
            DiagnosticValueRef::Identifier(value) => Some(value),
            _ => None,
        })
}

#[test]
fn finite_same_source_addition_is_not_a_cycle() {
    let mut overlay = PrefabOverlay::new(key("p"));
    let mut addition = PrefabAddition::new(id("added_instance"));
    addition.prefab = Some(instance("p", None));
    overlay.additions.push(addition);
    overlay
        .placements
        .push(root(PrefabEntityTarget::addition(id("added_instance"))));
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("e"))]),
    );
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources);
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    assert_eq!(report.document().unwrap().entities.len(), 4);
}

#[test]
fn borrowed_nested_declarations_do_not_consume_selected_value_allowances() {
    let mut addition = PrefabAddition::new(id("added"));
    addition.components.insert(
        ComponentTypeId::new("test.CompilerC"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "value",
                ComponentValue::List((0..64).map(ComponentValue::U64).collect()),
            )]),
        ),
    );
    let mut nested = PrefabOverlay::new(key("leaf"));
    nested.additions.push(addition);
    nested
        .placements
        .push(root(PrefabEntityTarget::addition(id("added"))));
    let mut outer = PrefabOverlay::new(key("p"));
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("n")),
        value: Some(instance("leaf", Some(nested))),
    });
    outer.components.push(PrefabComponentIntent::Suppress {
        target: inside(
            "n",
            "leaf",
            PrefabEntityOrigin::InstanceAddition(id("added")),
        ),
        component: ComponentTypeId::new("test.CompilerC"),
    });
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("n"))]),
        )
        .with_prefab(request("leaf"), key("leaf"), PrefabDocument::default());
    let document = scene("p", Some(outer.clone()));
    let before = document.clone();
    let options = PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_materialized_value_nodes(ItemLimit::ONE),
    );
    let report = document.expand_prefabs_with_options(&registry(), &sources, options);
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    assert!(
        report
            .document()
            .unwrap()
            .entities
            .iter()
            .all(|entity| entity.components.is_empty())
    );
    assert_eq!(document, before);

    outer.components.clear();
    let rejected =
        scene("p", Some(outer)).expand_prefabs_with_options(&registry(), &sources, options);
    assert!(rejected.candidate.is_none());
    assert_eq!(budget_kind(&rejected), Some("materialized-value-nodes"));
}

#[test]
fn a_remaining_outer_claim_can_terminate_same_source_recursion() {
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.instances.push(PrefabInstanceClaim {
        target: inside("n", "p", PrefabEntityOrigin::SourceEntity(id("n"))),
        value: None,
    });
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([anchor("n", "p", None)]),
    );
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources);
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    assert_eq!(report.document().unwrap().entities.len(), 3);
}

#[test]
fn unchanged_cycle_uses_canonical_declarations_even_with_fresh_resolver_allocations() {
    let p = PrefabDocument::from_ordered_roots([anchor("n", "q", None)]);
    let q = PrefabDocument::from_ordered_roots([anchor("n", "alias_p", None)]);
    let mut calls = 0;
    let report = scene("p", None)
        .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |source, _| {
            calls += 1;
            let (document, source_key) = if source == &request("q") {
                (&q, key("q"))
            } else {
                (&p, key("p"))
            };
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound(
                Arc::new(document.clone()),
                source_key,
            )))
        })
        .unwrap();
    assert!(report.candidate.is_none());
    assert_eq!(calls, 3);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-cycle")
        .unwrap();
    for name in [
        "entity-id",
        "cycle-start-index",
        "cycle-depth",
        "cycle-from",
        "cycle-to",
    ] {
        assert!(
            diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == name),
            "{diagnostic:?}"
        );
    }
}

#[test]
fn rescued_descendants_keep_occurrence_bases_after_the_anchor_is_suppressed() {
    for addition_only in [false, true] {
        let mut q_overlay = PrefabOverlay::new(key("q"));
        let q = if addition_only {
            q_overlay.additions.push(PrefabAddition::new(id("saved")));
            q_overlay
                .placements
                .push(root(PrefabEntityTarget::addition(id("saved"))));
            PrefabDocument::default()
        } else {
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("saved"))])
        };
        let origin = if addition_only {
            PrefabEntityOrigin::InstanceAddition(id("saved"))
        } else {
            PrefabEntityOrigin::SourceEntity(id("saved"))
        };
        let mut r_overlay = PrefabOverlay::new(key("r"));
        r_overlay
            .placements
            .push(root(inside("n", "q", origin.clone())));
        r_overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("n")));
        let sources = InMemoryPrefabSourceResolver::new()
            .with_prefab(request("q"), key("q"), q)
            .with_prefab(
                request("r"),
                key("r"),
                PrefabDocument::from_ordered_roots([anchor(
                    "n",
                    "q",
                    addition_only.then_some(q_overlay),
                )]),
            )
            .with_prefab(
                request("p"),
                key("p"),
                PrefabDocument::from_ordered_roots([anchor("m", "r", Some(r_overlay))]),
            );
        for edit_saved in [false, true] {
            let mut outer = PrefabOverlay::new(key("p"));
            if edit_saved {
                let mut target = inside("m", "r", origin.clone());
                target.through.push(PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("n")),
                    expected_base: key("q"),
                });
                outer
                    .components
                    .push(add_component(target, "test.CompilerC"));
            }
            let report = scene("p", Some(outer)).expand_prefabs(&registry(), &sources);
            assert!(
                report.candidate.is_some(),
                "addition_only={addition_only}, edit={edit_saved}: {:?}",
                report.diagnostics
            );
            let saved = projected(&report, "saved");
            let record = report
                .document()
                .unwrap()
                .entities
                .iter()
                .find(|record| record.id == saved)
                .unwrap();
            assert_eq!(record.components.len(), usize::from(edit_saved));
            assert!(
                report
                    .provenance()
                    .unwrap()
                    .values()
                    .all(|origin| origin.local_entity_id() != &id("n"))
            );
        }
    }
}

#[test]
fn component_additions_across_layers_share_one_cumulative_count() {
    let mut inner = PrefabOverlay::new(key("q"));
    inner.components.push(add_component(
        PrefabEntityTarget::source(id("e")),
        "test.CompilerC",
    ));
    let mut outer = PrefabOverlay::new(key("p"));
    outer.components.push(add_component(
        inside("n", "q", PrefabEntityOrigin::SourceEntity(id("e"))),
        "test.CompilerD",
    ));
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("q"),
            key("q"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("e"))]),
        )
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([anchor("n", "q", Some(inner))]),
        );
    for limit in [1, 2] {
        let report = scene("p", Some(outer.clone())).expand_prefabs_with_options(
            &registry(),
            &sources,
            PrefabExpansionOptions::default().with_limits(
                PrefabExpansionLimits::default()
                    .with_materialized_components(ItemLimit::new(limit).unwrap()),
            ),
        );
        if limit == 1 {
            assert!(report.candidate.is_none());
            assert_eq!(budget_kind(&report), Some("materialized-components"));
        } else {
            assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        }
    }
}

#[test]
fn ancestor_suppression_skips_unused_sources_but_rescues_keep_demand() {
    let p = PrefabDocument::new(
        [id("parent")],
        [
            SceneEntityRecord::new(id("parent")).with_children([id("n")]),
            anchor("n", "q", None),
        ],
    );
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(request("p"), key("p"), p)
        .with_prefab(
            request("q"),
            key("q"),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("saved")),
                anchor("unused_bad", "missing", None),
            ]),
        );
    for rescue in [false, true] {
        let mut overlay = PrefabOverlay::new(key("p"));
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("parent")));
        if rescue {
            overlay.placements.push(root(inside(
                "n",
                "q",
                PrefabEntityOrigin::SourceEntity(id("saved")),
            )));
        }
        let mut requested = Vec::new();
        let report = scene("p", Some(overlay))
            .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |source, _| {
                requested.push(source.clone());
                // The skipped source is unavailable, not merely an empty valid Prefab.
                Ok::<_, Infallible>(if !rescue && source == &request("q") {
                    None
                } else {
                    sources.resolve_prefab(source)
                })
            })
            .unwrap();
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert_eq!(
            requested,
            if rescue {
                vec![request("p"), request("q")]
            } else {
                vec![request("p")]
            }
        );
    }
}

#[test]
fn outer_suppression_demand_is_applied_inside_the_reached_source() {
    let r = PrefabDocument::new(
        [id("parent")],
        [
            SceneEntityRecord::new(id("parent")).with_children([id("bad")]),
            anchor("bad", "missing", None),
        ],
    );
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(request("r"), key("r"), r)
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([anchor("m", "r", None)]),
        );
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.suppressions.push(inside(
        "m",
        "r",
        PrefabEntityOrigin::SourceEntity(id("parent")),
    ));
    let mut requested = Vec::new();
    let report = scene("p", Some(overlay))
        .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |source, _| {
            requested.push(source.clone());
            Ok::<_, Infallible>(sources.resolve_prefab(source))
        })
        .unwrap();
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    assert_eq!(requested, vec![request("p"), request("r")]);
}

#[test]
fn working_cap_is_enforced_and_raw_clones_do_not_consume_provenance() {
    let registry = registry();
    let mut overlay = PrefabOverlay::new(key("p"));
    for ordinal in 0..16 {
        let name = format!("addition_{ordinal}");
        overlay.additions.push(PrefabAddition::new(id(&name)));
        overlay
            .placements
            .push(root(PrefabEntityTarget::addition(id(&name))));
    }
    let document = scene("p", Some(overlay));
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::default(),
    );
    let mut loader = |source: &AssetRef, _: &mut crate::prefab::PrefabExpansionWorkingSet| {
        Ok::<_, Infallible>(sources.resolve_prefab(source))
    };
    let mut context =
        PrefabExpansionContext::new((&registry).into(), &mut loader, Default::default(), None);
    let report = context.expand_scene_document(&document);
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    let exact = context.working_set().checked_work_bytes().unwrap();
    assert!(context.usage.scratch_bytes > context.usage.provenance_bytes);
    let provenance = context.usage.provenance_bytes;
    for bytes in [exact, exact - 1] {
        let limits = PrefabExpansionLimits::default()
            .with_working_bytes(ByteLimit::new(bytes).unwrap())
            .with_provenance_bytes(ByteLimit::new(provenance).unwrap());
        assert_eq!(limits.checked_work_bytes(), Some(bytes));
        let report = document.expand_prefabs_with_options(
            &registry,
            &sources,
            PrefabExpansionOptions::default().with_limits(limits),
        );
        if bytes == exact {
            assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        } else {
            assert!(report.candidate.is_none());
            assert_eq!(budget_kind(&report), Some("working-bytes"));
        }
    }
}

#[test]
fn selected_programmatic_values_are_depth_admitted_before_their_first_clone() {
    let registry = registry();
    let maximum = nara_reflect::ComponentValueMaterializationLimits::default()
        .depth()
        .get();
    for depth in [maximum, maximum + 1, maximum + 32] {
        let mut value = ComponentValue::U64(1);
        for _ in 1..depth {
            value = ComponentValue::List(vec![value]);
        }
        let original_cost = value.cost();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("direct"))
            .with_component(
                ComponentTypeId::new("test.CompilerC"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            )]);
        let mut loader = |_: &AssetRef,
                          _: &mut crate::prefab::PrefabExpansionWorkingSet|
         -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("a direct value cannot request a Prefab")
        };
        let mut context =
            PrefabExpansionContext::new((&registry).into(), &mut loader, Default::default(), None);
        let report = context.expand_scene_document(&document);
        assert!(report.candidate.is_none());
        if depth > maximum {
            assert!(context.budget_failed);
            assert_eq!(budget_kind(&report), Some("materialized-value-depth"));
        } else {
            // Its shape is deliberately wrong, but the inclusive depth boundary is admitted.
            assert!(!context.budget_failed);
            assert_eq!(budget_kind(&report), None);
        }
        assert_eq!(
            document.entities[0].components[&ComponentTypeId::new("test.CompilerC")]
                .value
                .cost(),
            original_cost
        );
    }
}

#[test]
fn loading_snapshot_includes_the_suspended_source_clone_and_derived_indexes() {
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([
                anchor("n", "q", None),
                SceneEntityRecord::new(id("sibling")),
            ]),
        )
        .with_prefab(request("q"), key("q"), PrefabDocument::default());
    let mut snapshots = Vec::new();
    let limits =
        PrefabExpansionLimits::default().with_working_bytes(ByteLimit::new(1_000_000).unwrap());
    let report = scene("p", None)
        .try_expand_prefabs_with_loader(
            &registry(),
            PrefabExpansionOptions::default().with_limits(limits),
            None,
            |source, working| {
                snapshots.push(working.checked_work_bytes().unwrap());
                Ok::<_, Infallible>(sources.resolve_prefab(source))
            },
        )
        .unwrap();
    assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots[1] > snapshots[0] + 2 * 512);
    assert!(
        snapshots
            .iter()
            .all(|bytes| *bytes <= limits.checked_work_bytes().unwrap())
    );
}

#[test]
fn a_large_deferred_request_is_admitted_before_clone_and_loader_entry() {
    let oversized = AssetRef::path(format!("{}.ron", "x".repeat(1024 * 1024))).unwrap();
    let mut nested = SceneEntityRecord::new(id("n"));
    nested.prefab = Some(PrefabInstance {
        source: oversized,
        overrides: None,
    });
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([nested]),
    );
    let mut called = Vec::new();
    let report = scene("p", None)
        .try_expand_prefabs_with_loader(
            &registry(),
            PrefabExpansionOptions::default().with_limits(
                PrefabExpansionLimits::default()
                    .with_working_bytes(ByteLimit::new(64 * 1024).unwrap()),
            ),
            None,
            |source, _| {
                called.push(source.clone());
                Ok::<_, Infallible>(sources.resolve_prefab(source))
            },
        )
        .unwrap();
    assert!(report.candidate.is_none());
    assert_eq!(budget_kind(&report), Some("working-bytes"));
    assert_eq!(called, [request("p")]);
}

#[test]
fn complete_outer_repair_cannot_erase_inner_duplicate_property_declarations() {
    use crate::{PrefabPropertyClaim, PrefabPropertyIntent};
    use nara_reflect::{ComponentPropertyPath, ComponentPropertyStep};

    for descendant in [false, true] {
        for suppress in [false, true] {
            let path = ComponentPropertyPath::new(ComponentFieldId::new("value"));
            let second = if descendant {
                path.clone()
                    .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                        "child",
                    ))])
            } else {
                path.clone()
            };
            let mut inner = PrefabOverlay::new(key("leaf"));
            inner.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("e")),
                component: ComponentTypeId::new("test.CompilerC"),
                version: ComponentSchemaVersion::ONE,
                claims: [path, second]
                    .into_iter()
                    .map(|path| PrefabPropertyClaim {
                        path,
                        intent: PrefabPropertyIntent::Set(ComponentValue::U64(1)),
                        bindings: Vec::new(),
                    })
                    .collect(),
            });
            let sources = InMemoryPrefabSourceResolver::new()
                .with_prefab(
                    request("p"),
                    key("p"),
                    PrefabDocument::from_ordered_roots([anchor("m", "leaf", Some(inner))]),
                )
                .with_prefab(
                    request("leaf"),
                    key("leaf"),
                    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("e"))
                        .with_component(
                            ComponentTypeId::new("test.CompilerC"),
                            SceneComponentRecord::new(
                                ComponentSchemaVersion::ONE,
                                ComponentValue::map([("value", ComponentValue::U64(0))]),
                            ),
                        )]),
                );
            let target = inside("m", "leaf", PrefabEntityOrigin::SourceEntity(id("e")));
            let mut outer = PrefabOverlay::new(key("p"));
            outer.components.push(if suppress {
                PrefabComponentIntent::Suppress {
                    target,
                    component: ComponentTypeId::new("test.CompilerC"),
                }
            } else {
                let PrefabComponentIntent::Set {
                    component,
                    record,
                    bindings,
                    ..
                } = add_component(target.clone(), "test.CompilerC")
                else {
                    unreachable!()
                };
                PrefabComponentIntent::Set {
                    mode: PrefabComponentSetMode::Replace,
                    target,
                    component,
                    record,
                    bindings,
                }
            });
            let report = scene("p", Some(outer)).expand_prefabs(&registry(), &sources);
            assert!(report.candidate.is_none());
            let diagnostic = report
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-compilation-failed")
                .unwrap();
            assert!(
                diagnostic
                    .fields()
                    .iter()
                    .any(|field| field.key().as_str() == "failure-kind"
                        && field.value() == DiagnosticValueRef::Identifier("duplicate-intent")),
                "{diagnostic:?}"
            );
        }
    }
}

#[test]
fn loader_restriction_is_applied_before_source_copy_and_cannot_be_raised_again() {
    let document = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("e")),
    ]));
    for (below_live, try_raise) in [(false, false), (true, false), (false, true)] {
        let mut calls = 0;
        let report = scene("p", None)
            .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |_, working| {
                calls += 1;
                let live = working.checked_work_bytes().unwrap();
                working.restrict_working_bytes(
                    ByteLimit::new(live - usize::from(below_live)).unwrap(),
                );
                if try_raise {
                    working.restrict_working_bytes(ByteLimit::new(512 * 1024 * 1024).unwrap());
                }
                Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound(
                    Arc::clone(&document),
                    key("p"),
                )))
            })
            .unwrap();
        assert_eq!(calls, 1);
        assert!(report.candidate.is_none());
        assert_eq!(budget_kind(&report), Some("working-bytes"));
        assert_eq!(document.entities.len(), 1);
    }
}

#[test]
fn later_loading_callbacks_can_tighten_the_remaining_compiler_allowance() {
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([anchor("n", "q", None)]),
        )
        .with_prefab(
            request("q"),
            key("q"),
            PrefabDocument::from_ordered_roots([anchor("n", "r", None)]),
        );
    let mut requested = Vec::new();
    let report = scene("p", None)
        .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |source, working| {
            requested.push(source.clone());
            if source == &request("q") {
                working.restrict_working_bytes(
                    ByteLimit::new(working.checked_work_bytes().unwrap()).unwrap(),
                );
            }
            Ok::<_, Infallible>(sources.resolve_prefab(source))
        })
        .unwrap();
    assert!(report.candidate.is_none());
    assert_eq!(budget_kind(&report), Some("working-bytes"));
    assert_eq!(requested, vec![request("p"), request("q")]);
}

#[test]
fn loader_failure_is_preserved_even_if_it_also_tightened_the_allowance() {
    let failure = scene("p", None)
        .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |_, working| {
            working.restrict_working_bytes(ByteLimit::new(1).unwrap());
            Err::<Option<ResolvedPrefabSource>, _>("source-read-failed")
        })
        .unwrap_err();
    assert_eq!(failure, "source-read-failed");
}

#[test]
fn missing_source_and_depth_diagnostics_keep_their_locators_and_limits() {
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([anchor("n", "missing", None)]),
    );
    for depth in [1, 8] {
        let report = scene("p", None).expand_prefabs_with_options(
            &registry(),
            &sources,
            PrefabExpansionOptions::default().with_max_depth(depth),
        );
        let code = if depth == 1 {
            "scene.prefab-depth-exceeded"
        } else {
            "scene.prefab-source-missing"
        };
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == code)
            .unwrap();
        assert!(
            diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == "entity-id")
        );
        if depth == 1 {
            assert!(
                diagnostic
                    .fields()
                    .iter()
                    .any(|field| field.key().as_str() == "maximum-depth")
            );
        }
    }
}

#[test]
fn outer_suppression_after_a_cross_instance_move_does_not_request_the_discarded_instance() {
    let mut inner = PrefabOverlay::new(key("p"));
    inner.placements.push(PrefabPlacement {
        target: inside("a", "a", PrefabEntityOrigin::SourceEntity(id("q"))),
        parent: PrefabPlacementParent::Entity(inside(
            "b",
            "b",
            PrefabEntityOrigin::SourceEntity(id("r")),
        )),
        slot: PrefabPlacementSlot::Last,
    });
    let mut outer = PrefabOverlay::new(key("t"));
    outer
        .suppressions
        .push(inside("m", "p", PrefabEntityOrigin::SourceEntity(id("b"))));
    for discarded_source_exists in [true, false] {
        let mut sources = InMemoryPrefabSourceResolver::new()
            .with_prefab(
                request("t"),
                key("t"),
                PrefabDocument::from_ordered_roots([anchor("m", "p", Some(inner.clone()))]),
            )
            .with_prefab(
                request("p"),
                key("p"),
                PrefabDocument::from_ordered_roots([
                    anchor("a", "a", None),
                    anchor("b", "b", None),
                ]),
            )
            .with_prefab(
                request("a"),
                key("a"),
                PrefabDocument::from_ordered_roots([anchor("q", "discarded", None)]),
            )
            .with_prefab(
                request("b"),
                key("b"),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("r"))]),
            );
        if discarded_source_exists {
            sources.insert(
                request("discarded"),
                key("discarded"),
                PrefabDocument::default(),
            );
        }
        let mut requested = Vec::new();
        let report = scene("t", Some(outer.clone()))
            .try_expand_prefabs_with_loader(&registry(), Default::default(), None, |source, _| {
                requested.push(source.clone());
                Ok::<_, Infallible>(sources.resolve_prefab(source))
            })
            .unwrap();
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert!(!requested.contains(&request("discarded")), "{requested:?}");
        let provenance = report.provenance().unwrap();
        for removed in ["b", "r", "q"] {
            assert!(
                provenance
                    .values()
                    .all(|origin| origin.local_entity_id() != &id(removed))
            );
        }
        assert!(
            provenance
                .values()
                .any(|origin| origin.local_entity_id() == &id("a"))
        );
    }
}

#[test]
fn effective_reference_work_is_shared_across_components_even_when_no_assets_are_produced() {
    let registry = registry();
    let component = ComponentTypeId::new("test.CompilerC");
    let value = ComponentValue::map([("value", ComponentValue::U64(1))]);
    let mut unit_nodes = 0;
    let mut unit_bytes = 0;
    let references =
        nara_reflect::__private::collect_declared_effective_asset_references_with_admission(
            registry.schema(&component).unwrap(),
            ComponentSchemaVersion::ONE,
            &value,
            Default::default(),
            |nodes, bytes, _| {
                unit_nodes += nodes;
                unit_bytes += bytes;
                Ok::<_, Infallible>(())
            },
        )
        .unwrap();
    assert!(references.is_empty());
    assert!(unit_nodes > 0 && unit_bytes > 0);
    let document = SceneDocument::from_ordered_roots(["first", "second"].map(|name| {
        SceneEntityRecord::new(id(name)).with_component(
            component.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone()),
        )
    }));
    let original = document.clone();
    for (nodes, bytes, expected) in [
        (2 * unit_nodes, 2 * unit_bytes, None),
        (
            2 * unit_nodes - 1,
            2 * unit_bytes,
            Some("reference-work-nodes"),
        ),
        (
            2 * unit_nodes,
            2 * unit_bytes - 1,
            Some("reference-work-bytes"),
        ),
    ] {
        let limits = PrefabExpansionLimits::default()
            .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
            .with_reference_work_bytes(ByteLimit::new(bytes).unwrap());
        let report = document.expand_prefabs_with_options(
            &registry,
            &InMemoryPrefabSourceResolver::new(),
            PrefabExpansionOptions::default().with_limits(limits),
        );
        assert_eq!(
            report.candidate.is_none(),
            expected.is_some(),
            "{:?}",
            report.diagnostics
        );
        assert_eq!(budget_kind(&report), expected);
        assert_eq!(document, original);
    }
}
