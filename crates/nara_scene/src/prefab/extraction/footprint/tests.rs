//! Real captured sources and explicit environment bindings, without compiling component payloads.

use std::{collections::HashMap, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredEntityReference, EntityReference, ReferenceInputId};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};

use super::{SourceContextFootprint::*, *};
use crate::{
    __private::{PrefabExtractionBinding, PrefabExtractionEnvironment},
    AuthoringSchemaBasis, PrefabAddition, PrefabDocument, PrefabEntityOrigin, PrefabInstanceClaim,
    PrefabInstanceHop, PrefabOverlay, PrefabReferenceArguments, ResolvedPrefabSource,
    SceneCompilationLimits, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    ScenePlacement, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "bcf0a510-30cb-4a61-ab9f-3bc235ca1d50".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn alias(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{value}.ron")).unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn prefab(
    name: &str,
    document: PrefabDocument,
    basis: &AuthoringSchemaBasis,
) -> CapturedExtractionSource {
    CapturedExtractionSource::Prefab {
        source: ResolvedPrefabSource::bound_authoring_source(
            Arc::new(document),
            basis.clone(),
            key(name),
        ),
    }
}

fn record(name: &str, instance: PrefabInstance) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(instance);
    record
}

fn dependent(
    name: &str,
    dependency: &str,
    basis: &AuthoringSchemaBasis,
) -> CapturedExtractionSource {
    prefab(
        name,
        PrefabDocument::from_ordered_roots([record(
            "host",
            PrefabInstance::forest(alias(dependency)),
        )]),
        basis,
    )
}

fn scene(document: SceneDocument, basis: &AuthoringSchemaBasis) -> CapturedExtractionSource {
    CapturedExtractionSource::Scene {
        key: SourceDocumentKey::named(
            "bcf0a510-30cb-4a61-ab9f-3bc235ca1d50".parse().unwrap(),
            "scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        document: Arc::new(document),
        basis: basis.clone(),
    }
}

fn with_claim(base: &str, target: PrefabEntityTarget, nested: PrefabInstance) -> PrefabInstance {
    let mut instance = PrefabInstance::forest(alias(base));
    let mut overlay = PrefabOverlay::new(key(base));
    overlay.instances.push(PrefabInstanceClaim {
        target,
        value: Some(nested),
    });
    instance.overrides = Some(overlay);
    instance
}

fn dormant(base: &str, nested: PrefabInstance) -> PrefabInstance {
    with_claim(
        base,
        PrefabEntityTarget::source(id("never-materialized")),
        nested,
    )
}

fn environment(
    sources: &[CapturedExtractionSource],
    bindings: &[(&str, usize)],
) -> PrefabExtractionEnvironment {
    let before: Vec<_> = bindings
        .iter()
        .map(|(name, index)| PrefabExtractionBinding {
            reference: alias(name),
            source: sources[*index].key().clone(),
        })
        .collect();
    PrefabExtractionEnvironment {
        name: "footprint-fixture".into(),
        sources: (0..sources.len()).collect(),
        after: before
            .iter()
            .map(|binding| PrefabExtractionBinding {
                reference: binding.reference.clone(),
                source: binding.source.clone(),
            })
            .collect(),
        before,
        assets_before: None,
        assets_after: None,
    }
}

fn classify(
    sources: &[CapturedExtractionSource],
    environment: &PrefabExtractionEnvironment,
    work: &mut ExtractionWork,
) -> Result<Vec<SourceContextFootprint>, ExtractionError> {
    let mut aliases = HashMap::new();
    for binding in &environment.before {
        let index = environment
            .sources
            .iter()
            .copied()
            .find(|&index| sources[index].key() == &binding.source)
            .expect("each fixture binding must name an actual environment member");
        assert!(aliases.insert(&binding.reference, index).is_none());
    }
    classify_contexts(sources, &environment.sources, 0, work, |request, work| {
        let bytes = crate::provenance::provenance_asset_ref_bytes(request)
            .ok_or(ExtractionError::Capacity)?;
        work.visit(1, bytes, 0)?;
        Ok(aliases.get(request).copied())
    })
}

fn observed(
    sources: &[CapturedExtractionSource],
    bindings: &[(&str, usize)],
) -> Vec<SourceContextFootprint> {
    classify(
        sources,
        &environment(sources, bindings),
        &mut ExtractionWork::new(Default::default(), Some(0)).unwrap(),
    )
    .unwrap()
}

#[test]
fn leaves_and_independent_future_payloads_need_no_schema_or_value_walk() {
    let basis = basis();
    let opaque = SceneEntityRecord::new(id("opaque")).with_component(
        ComponentTypeId::new("future.Unregistered"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("future-payload", ComponentValue::U64(73))]),
        ),
    );
    let scene = Arc::new(SceneDocument::from_ordered_roots([record(
        "host",
        PrefabInstance::forest(alias("future")),
    )]));
    let scene_allocation = Arc::clone(&scene);
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "future",
            PrefabDocument::from_ordered_roots([opaque]),
            &basis,
        ),
        CapturedExtractionSource::Scene {
            key: SourceDocumentKey::named(
                "bcf0a510-30cb-4a61-ab9f-3bc235ca1d50".parse().unwrap(),
                "scene",
                SourceDocumentKind::Scene,
            )
            .unwrap(),
            document: scene,
            basis,
        },
    ];
    assert_eq!(
        observed(&sources, &[("p", 0), ("leaf", 1), ("future", 2)]),
        [Affected, Preserved, Preserved, Preserved]
    );
    let CapturedExtractionSource::Scene { document, .. } = &sources[3] else {
        panic!("captured Scene");
    };
    assert!(Arc::ptr_eq(document, &scene_allocation));
}

#[test]
fn selected_effect_crosses_chains_while_complete_disjoint_cycles_are_preserved() {
    let basis = basis();
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        dependent("direct", "p", &basis),
        dependent("parent", "direct", &basis),
        dependent("a", "b", &basis),
        dependent("b", "a", &basis),
        dependent("cycle-parent", "a", &basis),
    ];
    assert_eq!(
        observed(
            &sources,
            &[
                ("p", 0),
                ("direct", 1),
                ("parent", 2),
                ("a", 3),
                ("b", 4),
                ("cycle-parent", 5),
            ],
        ),
        [
            Affected, Affected, Affected, Preserved, Preserved, Preserved
        ]
    );
}

#[test]
fn dormant_claims_and_additions_recurse_even_when_their_targets_do_not_exist() {
    let basis = basis();
    let claim = dormant("leaf", dormant("leaf", PrefabInstance::forest(alias("p"))));
    let mut addition = PrefabAddition::new(id("hidden-addition"));
    addition.prefab = Some(claim.clone());
    let mut outer = PrefabInstance::forest(alias("leaf"));
    let mut overlay = PrefabOverlay::new(key("leaf"));
    overlay.additions.push(addition);
    outer.overrides = Some(overlay);
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "claim",
            PrefabDocument::from_ordered_roots([record("host", claim)]),
            &basis,
        ),
        prefab(
            "addition",
            PrefabDocument::from_ordered_roots([record("host", outer)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(
            &sources,
            &[("p", 0), ("leaf", 1), ("claim", 2), ("addition", 3)]
        ),
        [Affected, Affected, Affected, Affected]
    );
}

#[test]
fn a_missing_dormant_alias_makes_its_entire_reverse_closure_unproven() {
    let basis = basis();
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "incomplete",
            PrefabDocument::from_ordered_roots([record(
                "host",
                dormant("leaf", PrefabInstance::forest(alias("not-captured"))),
            )]),
            &basis,
        ),
        dependent("parent", "incomplete", &basis),
    ];
    assert_eq!(
        observed(
            &sources,
            &[("p", 0), ("leaf", 1), ("incomplete", 2), ("parent", 3)]
        ),
        [Affected, Unproven, Unproven, Unproven]
    );
}

#[test]
fn declared_inputs_and_nonempty_argument_environments_are_unproven() {
    let basis = basis();
    let mut parameters = PrefabDocument::default();
    parameters
        .inputs
        .insert(ReferenceInputId::new("unused").unwrap());
    let mut bound = PrefabInstance::forest(alias("leaf"));
    bound.arguments = Some(PrefabReferenceArguments {
        base: key("leaf"),
        values: [(
            ReferenceInputId::new("target").unwrap(),
            EntityReference::SceneLocal { entity: id("host") },
        )]
        .into(),
    });
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("parameters", parameters, &basis),
        dependent("parent", "parameters", &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "arguments",
            PrefabDocument::from_ordered_roots([record("host", bound)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(
            &sources,
            &[
                ("p", 0),
                ("parameters", 1),
                ("parent", 2),
                ("leaf", 3),
                ("arguments", 4)
            ],
        ),
        [Affected, Unproven, Unproven, Preserved, Unproven]
    );
}

#[test]
fn each_base_predicate_must_match_the_actual_alias_result() {
    let basis = basis();
    for wrong_base in [false, true] {
        for predicate in 0..3 {
            let expected = if wrong_base {
                key("different-canonical-source")
            } else {
                key("leaf")
            };
            let mut instance = PrefabInstance::forest(alias("request-alias"));
            match predicate {
                0 => {
                    instance.binding = PrefabInstanceBinding::Root {
                        base: expected,
                        root: PrefabEntityTarget::source(id("not-evaluated")),
                    }
                }
                1 => instance.overrides = Some(PrefabOverlay::new(expected)),
                _ => {
                    instance.arguments = Some(PrefabReferenceArguments {
                        base: expected,
                        values: Default::default(),
                    })
                }
            }
            let sources = [
                prefab("p", PrefabDocument::default(), &basis),
                prefab("leaf", PrefabDocument::default(), &basis),
                prefab(
                    "owner",
                    PrefabDocument::from_ordered_roots([record("host", instance)]),
                    &basis,
                ),
                dependent("parent", "owner", &basis),
            ];
            let expected = if wrong_base { Unproven } else { Preserved };
            let leaf = if wrong_base && predicate == 1 {
                Unproven
            } else {
                Preserved
            };
            assert_eq!(
                observed(
                    &sources,
                    &[
                        ("p", 0),
                        ("leaf", 1),
                        ("request-alias", 1),
                        ("owner", 2),
                        ("parent", 3)
                    ],
                ),
                [Affected, leaf, expected, expected]
            );
        }
    }
}

fn routed(base: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("unbound-anchor")),
            expected_base: key(base),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("target")),
    }
}

#[test]
fn outer_installations_affect_a_context_whose_own_slot_is_empty_even_when_dormant() {
    let basis = basis();
    for dormant_outer in [false, true] {
        let reference = PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("x")),
                expected_base: key("p"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("b")),
        };
        let unavailable = SceneEntityRecord::new(id("u")).with_component(
            ComponentTypeId::new("future.UnregisteredLink"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::EntityReference(EntityReference::Authored {
                    reference: AuthoredEntityReference::from(reference),
                }),
            ),
        );
        let u = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("u")),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(id("x")),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        id("u"),
                    ))),
                ),
            ],
            [unavailable, SceneEntityRecord::new(id("x"))],
        );
        assert!(u.entities.iter().all(|record| record.prefab.is_none()));
        let installed = with_claim(
            "u",
            PrefabEntityTarget::source(id("x")),
            PrefabInstance::forest(alias("p")),
        );
        let outer = if dormant_outer {
            dormant("leaf", installed)
        } else {
            installed
        };
        let sources = [
            prefab(
                "p",
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b"))]),
                &basis,
            ),
            prefab("u", u, &basis),
            prefab("leaf", PrefabDocument::default(), &basis),
            scene(
                SceneDocument::from_ordered_roots([record("outer", outer)]),
                &basis,
            ),
        ];
        let leaf = if dormant_outer { Affected } else { Preserved };
        assert_eq!(
            observed(&sources, &[("p", 0), ("u", 1), ("leaf", 2)]),
            [Affected, Affected, leaf, Affected]
        );
    }
}

#[test]
fn nested_additions_modify_their_own_overlay_base_context() {
    let basis = basis();
    let mut inner = PrefabAddition::new(id("inner"));
    inner.prefab = Some(PrefabInstance::forest(alias("p")));
    let mut installed = PrefabInstance::forest(alias("u"));
    let mut overlay = PrefabOverlay::new(key("u"));
    overlay.additions.push(inner);
    installed.overrides = Some(overlay);
    let mut outer = PrefabAddition::new(id("outer"));
    outer.prefab = Some(installed);
    let mut instance = PrefabInstance::forest(alias("leaf"));
    let mut overlay = PrefabOverlay::new(key("leaf"));
    overlay.additions.push(outer);
    instance.overrides = Some(overlay);
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("u", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        scene(
            SceneDocument::from_ordered_roots([record("host", instance)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(&sources, &[("p", 0), ("u", 1), ("leaf", 2)]),
        [Affected, Affected, Affected, Affected]
    );
}

#[test]
fn routed_claims_use_the_final_captured_canonical_key_without_a_matching_selector() {
    let basis = basis();
    let mut target = routed("leaf");
    target.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("another-unbound-anchor")),
        expected_base: key("u"),
    });
    let instance = with_claim("leaf", target, PrefabInstance::forest(alias("p")));
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("u", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        scene(
            SceneDocument::from_ordered_roots([record("host", instance)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(
            &sources,
            &[("p", 0), ("different-request-spelling", 1), ("leaf", 2)]
        ),
        [Affected, Affected, Preserved, Affected]
    );
}

#[test]
fn an_unknown_routed_context_marks_the_owner_and_identifiable_contexts_unproven() {
    let basis = basis();
    let mut target = routed("u");
    target.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("missing-anchor")),
        expected_base: key("not-captured"),
    });
    let instance = with_claim("leaf", target, PrefabInstance::forest(alias("installed")));
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("u", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab("installed", PrefabDocument::default(), &basis),
        scene(
            SceneDocument::from_ordered_roots([record("host", instance)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(
            &sources,
            &[("p", 0), ("u", 1), ("leaf", 2), ("installed", 3)]
        ),
        [Affected, Unproven, Unproven, Preserved, Unproven]
    );
}

#[test]
fn a_wrong_overlay_context_marks_actual_and_identifiable_requested_sources_unproven() {
    let basis = basis();
    for requested in ["claimed", "not-captured"] {
        for routed_claim in [false, true] {
            let target = if routed_claim {
                routed("claimed")
            } else {
                PrefabEntityTarget::source(id("never-materialized"))
            };
            let mut instance =
                with_claim("actual", target, PrefabInstance::forest(alias("installed")));
            instance.overrides.as_mut().unwrap().base = key(requested);
            let sources = [
                prefab("p", PrefabDocument::default(), &basis),
                prefab("actual", PrefabDocument::default(), &basis),
                prefab("claimed", PrefabDocument::default(), &basis),
                prefab("installed", PrefabDocument::default(), &basis),
                scene(
                    SceneDocument::from_ordered_roots([record("host", instance)]),
                    &basis,
                ),
            ];
            let claimed = if requested == "claimed" || routed_claim {
                Unproven
            } else {
                Preserved
            };
            assert_eq!(
                observed(
                    &sources,
                    &[("p", 0), ("actual", 1), ("claimed", 2), ("installed", 3)]
                ),
                [Affected, Unproven, claimed, Preserved, Unproven]
            );
        }
    }
}

#[test]
fn an_independent_root_bound_child_does_not_inherit_its_selected_ancestors_footprint() {
    let basis = basis();
    let unavailable = record("u", PrefabInstance::forest(alias("leaf"))).with_component(
        ComponentTypeId::new("future.Unregistered"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(73)),
    );
    let sources = [
        prefab(
            "p",
            PrefabDocument::from_ordered_roots([record(
                "host",
                PrefabInstance::root(alias("u"), key("u"), PrefabEntityTarget::source(id("u"))),
            )]),
            &basis,
        ),
        prefab(
            "u",
            PrefabDocument::from_ordered_roots([unavailable]),
            &basis,
        ),
        prefab(
            "leaf",
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
            &basis,
        ),
        dependent("parent", "p", &basis),
    ];
    assert_eq!(
        observed(&sources, &[("p", 0), ("u", 1), ("leaf", 2), ("parent", 3)]),
        [Affected, Preserved, Preserved, Affected]
    );
}

#[test]
fn structural_predicates_naming_p_are_affected_and_other_unbound_routes_are_unproven() {
    let basis = basis();
    let placement = |base| {
        PrefabDocument::new(
            [SceneEntityPlacement::new(
                routed(base),
                ScenePlacement::last(ScenePlacementParent::Root),
            )],
            [SceneEntityRecord::new(id("local"))],
        )
    };
    let mut stale =
        PrefabInstance::root(alias("leaf"), key("p"), PrefabEntityTarget::source(id("k")));
    stale.overrides = Some(PrefabOverlay::new(key("leaf")));
    let mut incomplete_and_affected = dormant("missing", PrefabInstance::forest(alias("p")));
    incomplete_and_affected.arguments = Some(PrefabReferenceArguments {
        base: key("missing"),
        values: Default::default(),
    });
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab("route-p", placement("p"), &basis),
        prefab("route-leaf", placement("leaf"), &basis),
        prefab(
            "stale-p",
            PrefabDocument::from_ordered_roots([record("host", stale)]),
            &basis,
        ),
        prefab(
            "both",
            PrefabDocument::from_ordered_roots([record("host", incomplete_and_affected)]),
            &basis,
        ),
    ];
    assert_eq!(
        observed(
            &sources,
            &[
                ("p", 0),
                ("leaf", 1),
                ("route-p", 2),
                ("route-leaf", 3),
                ("stale-p", 4),
                ("both", 5),
            ],
        ),
        [Affected, Preserved, Affected, Unproven, Unproven, Affected]
    );
}

#[test]
fn physical_depth_is_complete_through_64_and_refuses_an_unvisited_suffix() {
    let basis = basis();
    for depth in [64, 65] {
        let mut instance = PrefabInstance::forest(alias("leaf"));
        for _ in 1..depth {
            instance = dormant("leaf", instance);
        }
        let sources = [
            prefab("p", PrefabDocument::default(), &basis),
            prefab("leaf", PrefabDocument::default(), &basis),
            prefab(
                "deep",
                PrefabDocument::from_ordered_roots([record("host", instance)]),
                &basis,
            ),
            dependent("parent", "deep", &basis),
        ];
        let environment = environment(
            &sources,
            &[("p", 0), ("leaf", 1), ("deep", 2), ("parent", 3)],
        );
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        let result = classify(&sources, &environment, &mut work);
        if depth == MAX_PHYSICAL_DEPTH {
            assert_eq!(result.unwrap(), [Affected, Preserved, Preserved, Preserved]);
        } else {
            assert!(matches!(result, Err(ExtractionError::InvalidSource)));
        }
    }
}

#[test]
fn an_unvisited_suffix_cannot_certify_a_different_modified_context_as_preserved() {
    let basis = basis();
    let mut instance = with_claim(
        "u",
        PrefabEntityTarget::source(id("slot")),
        PrefabInstance::forest(alias("p")),
    );
    for _ in 0..MAX_PHYSICAL_DEPTH {
        instance = dormant("leaf", instance);
    }
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab("u", PrefabDocument::default(), &basis),
        scene(
            SceneDocument::from_ordered_roots([record("host", instance)]),
            &basis,
        ),
    ];
    let environment = environment(&sources, &[("p", 0), ("leaf", 1), ("u", 2)]);
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    assert!(matches!(
        classify(&sources, &environment, &mut work),
        Err(ExtractionError::InvalidSource)
    ));
}

#[test]
fn ancestor_cursor_storage_charges_full_growth_and_does_not_scale_with_sibling_width() {
    let basis = basis();
    let mut deep = PrefabInstance::forest(alias("leaf"));
    for _ in 0..9 {
        deep = dormant("leaf", deep);
    }
    let mut wide = PrefabInstance::forest(alias("leaf"));
    let mut overlay = PrefabOverlay::new(key("leaf"));
    for index in 0..9 {
        let mut addition = PrefabAddition::new(id(&format!("sibling-{index}")));
        addition.prefab = Some(PrefabInstance::forest(alias("leaf")));
        overlay.additions.push(addition);
    }
    wide.overrides = Some(overlay);
    let storage = |instance| {
        let sources = [
            prefab("p", PrefabDocument::default(), &basis),
            prefab("leaf", PrefabDocument::default(), &basis),
            prefab(
                "owner",
                PrefabDocument::from_ordered_roots([record("host", instance)]),
                &basis,
            ),
        ];
        let environment = environment(&sources, &[("p", 0), ("leaf", 1), ("owner", 2)]);
        let limits = SceneCompilationLimits::default();
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        assert_eq!(
            classify(&sources, &environment, &mut work).unwrap(),
            [Affected, Preserved, Preserved]
        );
        let reference_bytes = limits.reference_work_bytes().get()
            - work
                .compiler_options(0)
                .unwrap()
                .limits
                .reference_work_bytes()
                .get();
        work.usage().1 - reference_bytes
    };
    // Both inputs contain ten instances and produce nineteen reverse edges. Nine live
    // ancestor cursors request capacities 4, 8 and 16; nine siblings need only capacity 4.
    assert_eq!(
        storage(deep) - storage(wide),
        (8 + 16) * size_of::<PhysicalInstanceFrame<'_>>(),
    );
}

#[test]
fn reverse_edge_growth_charges_each_complete_allocation_request() {
    let basis = basis();
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "owner",
            PrefabDocument::from_ordered_roots((0..5).map(|index| {
                record(
                    &format!("host-{index}"),
                    PrefabInstance::forest(alias("leaf")),
                )
            })),
            &basis,
        ),
    ];
    let environment = environment(&sources, &[("p", 0), ("leaf", 1), ("owner", 2)]);
    let mut resolved = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    assert_eq!(
        classify(&sources, &environment, &mut resolved).unwrap(),
        [Affected, Preserved, Preserved]
    );

    // The same physical scan and lookup byte charges without any resolved reverse edges
    // isolate allocation debits; graph traversal itself consumes nodes, not bytes.
    let mut unresolved = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    classify_contexts(
        &sources,
        &environment.sources,
        0,
        &mut unresolved,
        |request, work| {
            let bytes = crate::provenance::provenance_asset_ref_bytes(request)
                .ok_or(ExtractionError::Capacity)?;
            work.visit(1, bytes, 0)?;
            Ok(None)
        },
    )
    .unwrap();
    assert_eq!(
        resolved.usage().1 - unresolved.usage().1,
        (4 + 8) * size_of::<ReverseEdge>(),
    );
}

#[test]
fn cumulative_limits_are_inclusive_and_all_original_declarations_remain_unchanged() {
    let basis = basis();
    let sources = [
        prefab("p", PrefabDocument::default(), &basis),
        prefab("leaf", PrefabDocument::default(), &basis),
        prefab(
            "owner",
            PrefabDocument::from_ordered_roots([record(
                "host",
                dormant("leaf", PrefabInstance::forest(alias("p"))),
            )]),
            &basis,
        ),
        dependent("parent", "owner", &basis),
        dependent("grandparent", "parent", &basis),
        dependent("great-grandparent", "grandparent", &basis),
    ];
    let environment = environment(
        &sources,
        &[
            ("p", 0),
            ("leaf", 1),
            ("owner", 2),
            ("parent", 3),
            ("grandparent", 4),
            ("great-grandparent", 5),
        ],
    );
    let original_bindings = [&environment.before, &environment.after].map(|bindings| {
        (
            bindings.as_ptr(),
            bindings
                .iter()
                .map(|binding| (binding.reference.clone(), binding.source.clone()))
                .collect::<Vec<_>>(),
        )
    });
    let originals: Vec<_> = sources
        .iter()
        .map(|source| {
            let CapturedExtractionSource::Prefab { source } = source else {
                panic!("captured Prefab");
            };
            (
                source.key().clone(),
                source.document().clone(),
                source.document().entities.as_ptr(),
            )
        })
        .collect();
    let defaults = SceneCompilationLimits::default();
    let mut work = ExtractionWork::new(defaults, Some(17)).unwrap();
    let expected = classify(&sources, &environment, &mut work).unwrap();
    let (nodes, bytes) = work.usage();
    let reference_bytes = defaults.reference_work_bytes().get()
        - work
            .compiler_options(0)
            .unwrap()
            .limits
            .reference_work_bytes()
            .get();
    for (node_limit, byte_limit, reference_limit, admitted) in [
        (nodes, bytes, reference_bytes, true),
        (nodes - 1, bytes, reference_bytes, false),
        (nodes, bytes - 1, reference_bytes, false),
        (nodes, bytes, reference_bytes - 1, false),
    ] {
        let limits = defaults
            .with_reference_work_nodes(ItemLimit::new(node_limit).unwrap())
            .with_working_bytes(ByteLimit::new(byte_limit).unwrap())
            .with_reference_work_bytes(ByteLimit::new(reference_limit).unwrap());
        let mut work = ExtractionWork::new(limits, Some(17)).unwrap();
        let result = classify(&sources, &environment, &mut work);
        if admitted {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(matches!(result, Err(ExtractionError::Capacity)));
        }
        for (captured, (key, document, entities)) in sources.iter().zip(&originals) {
            let CapturedExtractionSource::Prefab { source } = captured else {
                panic!("captured Prefab");
            };
            assert_eq!(source.key(), key);
            assert_eq!(source.document(), document);
            assert_eq!(source.document().entities.as_ptr(), *entities);
        }
        for (bindings, (allocation, original)) in [&environment.before, &environment.after]
            .into_iter()
            .zip(&original_bindings)
        {
            assert_eq!(bindings.as_ptr(), *allocation);
            assert_eq!(
                bindings
                    .iter()
                    .map(|binding| (binding.reference.clone(), binding.source.clone()))
                    .collect::<Vec<_>>(),
                *original,
            );
        }
    }
}
