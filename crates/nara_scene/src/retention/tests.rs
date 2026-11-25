use super::*;
use crate::{
    InMemoryPrefabSourceResolver, PrefabDocument, SceneEntityProvenance, SourceDocumentKey,
    SourceDocumentKind,
};
use nara_reflect::ComponentRegistryBuilder;

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).expect("test scene entity id must be valid")
}

fn prefab_anchor(
    name: &str,
    source: AssetRef,
    base: SourceDocumentKey,
    additions: Vec<SceneEntityRecord>,
) -> SceneEntityRecord {
    let mut anchor = SceneEntityRecord::new(scene_id(name));
    let mut overlay = crate::PrefabOverlay::new(base);
    for entity in additions {
        let mut addition = crate::PrefabAddition::new(entity.id.clone());
        addition.components = entity.components;
        addition.prefab = entity.prefab;
        overlay.placements.push(crate::SceneEntityPlacement {
            target: crate::PrefabEntityTarget::addition(entity.id),
            placement: crate::ScenePlacement::last(crate::ScenePlacementParent::Root),
        });
        overlay.additions.push(addition);
    }
    anchor.prefab = Some(PrefabInstance {
        binding: PrefabInstanceBinding::Forest,
        arguments: None,
        source,
        overrides: Some(overlay),
    });
    anchor
}

fn string_bytes(value: &str) -> usize {
    value.len() + 2 * size_of::<usize>()
}

#[test]
fn source_and_compiled_records_each_charge_the_complete_payload_and_backing() {
    use crate::{CompiledSceneComponent, SceneComponentRecord};
    use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};

    let component = ComponentTypeId::new("nara.scene.Name");
    let text = "retained component";
    let empty = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root"))]);
    let mut source = empty.clone();
    source.entities[0].components.insert(
        component.clone(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(text.to_owned()),
        ),
    );
    let payload = text.len() + VALUE_NODE_ALLOCATION_BYTES;
    let backing = size_of::<ComponentValue>() + 4 * size_of::<usize>();
    let identity =
        size_of::<ComponentTypeId>() + BTREE_ENTRY_OVERHEAD + string_bytes(component.as_str());
    assert_eq!(
        scene_retained_bytes(&source).unwrap() - scene_retained_bytes(&empty).unwrap(),
        identity + size_of::<SceneComponentRecord>() + payload + backing,
    );
    let mut registry = ComponentRegistryBuilder::new();
    crate::register_scene_components(&mut registry).unwrap();
    let registry = registry.freeze().unwrap();
    let resolver = InMemoryPrefabSourceResolver::new();
    let compiled = source.compile(&registry, &resolver).candidate.unwrap();
    let empty_compiled = empty.compile(&registry, &resolver).candidate.unwrap();
    assert!(std::ptr::eq(
        source.entities[0].components[&component].value(),
        compiled.entities()[0].components()[&component]
            .value()
            .as_value()
            .unwrap()
    ));
    assert_eq!(
        compiled_scene_retained_bytes(&compiled).unwrap()
            - compiled_scene_retained_bytes(&empty_compiled).unwrap(),
        identity + size_of::<CompiledSceneComponent>() + payload + backing,
    );
}

#[test]
fn retention_charges_hidden_contextual_defaults_in_every_record_carrier() {
    use crate::{
        PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
        PrefabOverlay, SceneComponentRecord,
    };
    use nara_ecs::identity::{AuthoredEntityReference, EntityReference, ReferenceInputId};
    use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue};

    let component = ComponentTypeId::new("retention.Link");
    let field = ComponentFieldId::new("target");
    let input = ReferenceInputId::new("hidden-outward-input").unwrap();
    let default = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(input),
    });
    let plain = SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null);
    let contextual = plain
        .clone()
        .with_contextual_default(field.clone(), default.clone());
    let cost = default.cost();
    let expected = size_of::<ComponentFieldId>()
        + size_of::<ComponentValue>()
        + BTREE_ENTRY_OVERHEAD
        + string_bytes(field.as_str())
        + cost.logical_bytes()
        + cost.nodes() * VALUE_NODE_ALLOCATION_BYTES;
    let base = SourceDocumentKey::named(
        "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
        "retention.defaults",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    for carrier in 0..3 {
        let retained = |record: &SceneComponentRecord| match carrier {
            0 => scene_retained_bytes(&SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(scene_id("host"))
                    .with_component(component.clone(), record.clone()),
            ]))
            .unwrap(),
            _ => {
                let mut overlay = PrefabOverlay::new(base.clone());
                if carrier == 1 {
                    let mut addition = PrefabAddition::new(scene_id("host"));
                    addition
                        .components
                        .insert(component.clone(), record.clone());
                    overlay.additions.push(addition);
                } else {
                    overlay.components.push(PrefabComponentIntent::Set {
                        mode: PrefabComponentSetMode::Replace,
                        target: PrefabEntityTarget::source(scene_id("host")),
                        component: component.clone(),
                        record: record.clone(),
                    });
                }
                overlay_retained_bytes(&overlay).unwrap()
            }
        };
        assert_eq!(retained(&contextual) - retained(&plain), expected);
    }
}

#[test]
fn retention_charges_argument_base_and_complete_routes_in_nested_declarations() {
    use crate::{
        PrefabAddition, PrefabEntityTarget, PrefabInstanceClaim, PrefabOverlay,
        PrefabReferenceArguments,
    };
    use nara_ecs::identity::{
        AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, PrefabEntityOrigin,
        PrefabInstanceHop, ReferenceInputId,
    };

    let base = SourceDocumentKey::named(
        "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
        "retention.arguments",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let plain = PrefabInstance {
        binding: PrefabInstanceBinding::Forest,
        source: AssetRef::path("base.prefab.json").unwrap(),
        arguments: None,
        overrides: None,
    };
    let mut empty = plain.clone();
    empty.arguments = Some(PrefabReferenceArguments {
        base: base.clone(),
        values: Default::default(),
    });
    let input = ReferenceInputId::new("target").unwrap();
    let reference = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(scene_id("nested")),
                expected_base: base.clone(),
            }],
            target: AuthoredReferenceTarget::Input(input.clone()),
        },
    };
    let cost = reference.cost();
    let expected = size_of::<ReferenceInputId>()
        + size_of::<EntityReference>()
        + BTREE_ENTRY_OVERHEAD
        + string_bytes(input.as_str())
        + cost.logical_bytes()
        + cost.nodes() * VALUE_NODE_ALLOCATION_BYTES;
    let mut populated = empty.clone();
    populated
        .arguments
        .as_mut()
        .unwrap()
        .values
        .insert(input, reference);
    for carrier in 0..3 {
        let retained = |instance: &PrefabInstance| {
            if carrier == 0 {
                return prefab_instance_retained_bytes(instance).unwrap();
            }
            let mut overlay = PrefabOverlay::new(base.clone());
            if carrier == 1 {
                let mut addition = PrefabAddition::new(scene_id("nested"));
                addition.prefab = Some(instance.clone());
                overlay.additions.push(addition);
            } else {
                overlay.instances.push(PrefabInstanceClaim {
                    target: PrefabEntityTarget::source(scene_id("nested")),
                    value: Some(instance.clone()),
                });
            }
            overlay_retained_bytes(&overlay).unwrap()
        };
        assert_eq!(retained(&empty) - retained(&plain), base.retained_bytes());
        assert_eq!(retained(&populated) - retained(&empty), expected);
    }
}

#[cfg(feature = "serde")]
#[test]
fn prefab_retention_charges_unused_input_declarations() {
    use nara_ecs::identity::ReferenceInputId;

    let mut document = PrefabDocument::default();
    let empty = prefab_retained_bytes(&document).unwrap();
    let input = ReferenceInputId::new("unused-outward-input").unwrap();
    let expected =
        size_of::<ReferenceInputId>() + BTREE_ENTRY_OVERHEAD + string_bytes(input.as_str());
    document.inputs.insert(input);
    assert_eq!(prefab_retained_bytes(&document).unwrap() - empty, expected);
}

fn assert_provenance_retention(
    provenance: &SceneEntityProvenanceMap,
    expected_paths: &[(&str, usize)],
) {
    assert_eq!(provenance.len(), expected_paths.len());
    let mut expected = size_of::<SceneEntityProvenanceMap>();
    for (local_id, path_bytes) in expected_paths {
        let (expanded, source) = provenance
            .iter()
            .find(|(_, source)| source.local_entity_id().as_str() == *local_id)
            .expect("the expanded candidate must retain each entity's origin");
        match source.projection_path() {
            Some(path) => {
                assert_eq!(path.root_anchor(), &scene_id("anchor"));
                assert_eq!(projection_path_retained_bytes(path).unwrap(), *path_bytes);
            }
            None => assert_eq!(*path_bytes, 0),
        }
        expected += size_of::<SceneEntityId>()
            + size_of::<SceneEntityProvenance>()
            + 6 * size_of::<usize>()
            + string_bytes(expanded.as_str())
            + string_bytes(local_id)
            + *path_bytes;
    }
    assert_eq!(
        scene_provenance_retained_bytes(provenance).unwrap(),
        expected
    );
}

#[test]
fn provenance_retention_distinguishes_source_entities_from_instance_additions() {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let key = SourceDocumentKey::named(
        "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
        "retention.direct-prefab-source",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let path_request = AssetRef::path("prefabs/retained-source.prefab.ron").unwrap();
    let path_request_bytes = string_bytes("prefabs/retained-source.prefab.ron");
    let stable_request = AssetRef::stable_id("602e48c3-50ce-467b-8e0d-80ec1b4f1d2a").unwrap();
    for (request, request_bytes) in [(path_request, path_request_bytes), (stable_request, 16)] {
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("inherited"))]),
        );
        let document = SceneDocument::from_ordered_roots([prefab_anchor(
            "anchor",
            request.clone(),
            key.clone(),
            vec![SceneEntityRecord::new(scene_id("added-by-instance"))],
        )]);
        let expansion = document.compile(&registry, &resolver);
        assert!(
            !expansion.diagnostics.has_errors(),
            "{:?}",
            expansion.diagnostics
        );
        let candidate = expansion.candidate.unwrap();
        let provenance = candidate.canonical_provenance();
        for source in provenance.values() {
            let Some(path) = source.projection_path() else {
                continue;
            };
            let expected = match source.local_entity_id().as_str() {
                "inherited" => PrefabProjectionStep::SourceEntity {
                    source_document: request.clone(),
                    canonical_source: key.clone(),
                    entity: scene_id("inherited"),
                },
                "added-by-instance" => PrefabProjectionStep::InstanceAddition {
                    entity: scene_id("added-by-instance"),
                },
                _ => panic!("unexpected projected entity"),
            };
            assert_eq!(path.steps(), &[expected]);
        }

        let path_base = string_bytes("anchor") + size_of::<PrefabProjectionStep>();
        assert_provenance_retention(
            provenance,
            &[
                ("anchor", 0),
                (
                    "inherited",
                    path_base + request_bytes + key.retained_bytes() + string_bytes("inherited"),
                ),
                (
                    "added-by-instance",
                    path_base + string_bytes("added-by-instance"),
                ),
            ],
        );
    }
}

#[test]
fn provenance_retention_counts_both_origins_through_nested_prefab_anchors() {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let universe = "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap();
    let outer_request = AssetRef::stable_id("602e48c3-50ce-467b-8e0d-80ec1b4f1d2a").unwrap();
    let inner_request = AssetRef::path("prefabs/nested-retained.prefab.ron").unwrap();
    let outer_key = SourceDocumentKey::named(
        universe,
        "retention.outer-prefab-source",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let inner_key = SourceDocumentKey::named(
        universe,
        "retention.inner-prefab-source",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    for added_anchor in [false, true] {
        let nested = prefab_anchor(
            "nested-anchor",
            inner_request.clone(),
            inner_key.clone(),
            vec![SceneEntityRecord::new(scene_id("nested-addition"))],
        );
        let mut outer_entities = vec![SceneEntityRecord::new(scene_id("outer-source"))];
        let mut outer_additions = Vec::new();
        if added_anchor {
            outer_additions.push(nested);
        } else {
            outer_entities.push(nested);
        }
        let resolver = InMemoryPrefabSourceResolver::new()
            .with_prefab(
                outer_request.clone(),
                outer_key.clone(),
                PrefabDocument::from_ordered_roots(outer_entities),
            )
            .with_prefab(
                inner_request.clone(),
                inner_key.clone(),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id(
                    "inner-source",
                ))]),
            );
        let document = SceneDocument::from_ordered_roots([prefab_anchor(
            "anchor",
            outer_request.clone(),
            outer_key.clone(),
            outer_additions,
        )]);
        let expansion = document.compile(&registry, &resolver);
        assert!(
            !expansion.diagnostics.has_errors(),
            "{:?}",
            expansion.diagnostics
        );
        let candidate = expansion.candidate.unwrap();
        let parent_step = if added_anchor {
            PrefabProjectionStep::InstanceAddition {
                entity: scene_id("nested-anchor"),
            }
        } else {
            PrefabProjectionStep::SourceEntity {
                source_document: outer_request.clone(),
                canonical_source: outer_key.clone(),
                entity: scene_id("nested-anchor"),
            }
        };
        let nested_origin = |local_id: &str| {
            candidate
                .canonical_provenance()
                .values()
                .find(|source| source.local_entity_id().as_str() == local_id)
                .unwrap()
                .projection_path()
                .unwrap()
        };
        assert_eq!(
            nested_origin("nested-anchor").steps(),
            &[parent_step.clone()],
        );
        assert_eq!(
            nested_origin("inner-source").steps(),
            &[
                parent_step.clone(),
                PrefabProjectionStep::SourceEntity {
                    source_document: inner_request.clone(),
                    canonical_source: inner_key.clone(),
                    entity: scene_id("inner-source"),
                },
            ],
        );
        assert_eq!(
            nested_origin("nested-addition").steps(),
            &[
                parent_step,
                PrefabProjectionStep::InstanceAddition {
                    entity: scene_id("nested-addition"),
                },
            ],
        );

        let path_base = string_bytes("anchor");
        let source_base = size_of::<PrefabProjectionStep>() + 16 + outer_key.retained_bytes();
        let parent_bytes = size_of::<PrefabProjectionStep>()
            + string_bytes("nested-anchor")
            + if added_anchor {
                0
            } else {
                16 + outer_key.retained_bytes()
            };
        assert_provenance_retention(
            candidate.canonical_provenance(),
            &[
                ("anchor", 0),
                (
                    "outer-source",
                    path_base + source_base + string_bytes("outer-source"),
                ),
                ("nested-anchor", path_base + parent_bytes),
                (
                    "inner-source",
                    path_base
                        + parent_bytes
                        + size_of::<PrefabProjectionStep>()
                        + string_bytes("prefabs/nested-retained.prefab.ron")
                        + inner_key.retained_bytes()
                        + string_bytes("inner-source"),
                ),
                (
                    "nested-addition",
                    path_base
                        + parent_bytes
                        + size_of::<PrefabProjectionStep>()
                        + string_bytes("nested-addition"),
                ),
            ],
        );
    }
}

#[test]
fn scene_retention_counts_complete_placement_targets_and_spare_capacity() {
    use crate::{PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop};

    let root = scene_id("root");
    let child = scene_id("root/child");
    let base = SourceDocumentKey::named(
        "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
        "retention.placement-source",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let mut through = Vec::with_capacity(4);
    through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(root.clone()),
        expected_base: base.clone(),
    });
    let inherited = PrefabEntityTarget {
        through,
        entity: PrefabEntityOrigin::SourceEntity(scene_id("inherited")),
    };
    let mut placements = Vec::with_capacity(5);
    placements.push(SceneEntityPlacement::root(root.clone()));
    placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(child.clone()),
        ScenePlacement::before(
            ScenePlacementParent::Entity(inherited.clone()),
            inherited.clone(),
        ),
    ));
    placements.push(SceneEntityPlacement::new(
        inherited,
        ScenePlacement::last(ScenePlacementParent::Root),
    ));
    let mut entities = Vec::with_capacity(5);
    entities.push(SceneEntityRecord::new(root.clone()));
    entities.push(SceneEntityRecord::new(child.clone()));
    let document = SceneDocument {
        placements,
        entities,
    };
    let ScenePlacementParent::Entity(parent) = &document.placements[1].placement.parent else {
        unreachable!()
    };
    let ScenePlacementSlot::Before(sibling) = &document.placements[1].placement.slot else {
        unreachable!()
    };
    let inherited_bytes = |target: &PrefabEntityTarget| {
        string_bytes("inherited")
            + target.through.capacity() * size_of::<PrefabInstanceHop>()
            + string_bytes(root.as_str())
            + base.retained_bytes()
    };

    let expected = checked_sum([
        size_of::<SceneDocument>(),
        document.placements.capacity() * size_of::<SceneEntityPlacement>(),
        string_bytes(root.as_str()),
        string_bytes(child.as_str()),
        inherited_bytes(parent),
        inherited_bytes(sibling),
        inherited_bytes(&document.placements[2].target),
        document.entities.capacity() * size_of::<SceneEntityRecord>(),
        string_bytes(root.as_str()),
        string_bytes(child.as_str()),
    ])
    .unwrap();

    assert_eq!(scene_retained_bytes(&document).unwrap(), expected);
}

#[test]
fn root_binding_retention_charges_owned_pins_in_every_instance_carrier() {
    use crate::{
        PrefabAddition, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceClaim,
        PrefabInstanceHop, PrefabOverlay,
    };

    let base = SourceDocumentKey::named(
        "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
        "retention.bound-root-source",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let root = scene_id("selected-source-root");
    let forest = PrefabInstance::forest(AssetRef::path("base.prefab.json").unwrap());
    for routed in [false, true] {
        let through = if routed {
            vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(scene_id("selected-slot")),
                expected_base: base.clone(),
            }]
        } else {
            Vec::new()
        };
        let bound = PrefabInstance::root(
            forest.source.clone(),
            base.clone(),
            PrefabEntityTarget {
                through,
                entity: PrefabEntityOrigin::SourceEntity(root.clone()),
            },
        );
        let expected = base.retained_bytes()
            + string_bytes(root.as_str())
            + if routed {
                size_of::<PrefabInstanceHop>()
                    + string_bytes("selected-slot")
                    + base.retained_bytes()
            } else {
                0
            };
        for carrier in 0..3 {
            let retained = |instance: &PrefabInstance| {
                if carrier == 0 {
                    return prefab_instance_retained_bytes(instance).unwrap();
                }
                let mut overlay = PrefabOverlay::new(base.clone());
                if carrier == 1 {
                    let mut addition = PrefabAddition::new(scene_id("added"));
                    addition.prefab = Some(instance.clone());
                    overlay.additions.push(addition);
                } else {
                    overlay.instances.push(PrefabInstanceClaim {
                        target: PrefabEntityTarget::source(scene_id("nested")),
                        value: Some(instance.clone()),
                    });
                }
                overlay_retained_bytes(&overlay).unwrap()
            };
            assert_eq!(retained(&bound) - retained(&forest), expected);
        }
    }
}

#[test]
fn bound_candidate_retention_charges_the_containing_source_and_envelope() {
    let universe = "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap();
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let bindings = crate::SceneComponentBindings::new(registry, []).unwrap();
    let candidate = |name| {
        let report = bindings.compile_scene(
            &SceneDocument::default(),
            &crate::InMemoryPrefabSourceResolver::new(),
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let source = SourceDocumentKey::named(universe, name, SourceDocumentKind::Scene).unwrap();
        report.candidate.unwrap().bind_source(source)
    };
    let short = candidate("scene");
    let long = candidate("scene-with-a-longer-explicit-source-name");
    let short_bytes = startup_scene_retained_bytes(&short).unwrap();
    let long_bytes = startup_scene_retained_bytes(&long).unwrap();
    assert!(
        short_bytes >= size_of::<CompiledSceneCandidate>() + short.source_key().retained_bytes()
    );
    assert_eq!(
        long_bytes - short_bytes,
        long.source_key().retained_bytes() - short.source_key().retained_bytes(),
    );
    assert!(direct_startup_scene_retained_bytes(&short).unwrap() > short_bytes);
}
