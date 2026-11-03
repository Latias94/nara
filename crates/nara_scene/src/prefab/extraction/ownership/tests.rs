//! Physical suffix continuity includes declarations with no effective occurrence.

use std::sync::Arc;

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::ByteLimit;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::{AuthoringAdapterReceipt, PrefabExtractionBinding, PrefabExtractionEnvironment},
    AuthoringSchemaBasis, PrefabAddition, PrefabComponentSetMode, PrefabDocument,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceClaim, PrefabInstanceHop,
    PrefabInstancePathSegment, PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent,
    ResolvedPrefabSource, SceneCompilationLimits, SceneDocument, ScenePlacement,
    ScenePlacementParent, SourceDocumentKind,
    prefab::extraction::corpus::{
        ExtractionDestination, ExtractionRequest, PreparedExtractionCorpus,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "6498e15b-8563-47e3-a3a6-a9a620a4b28e".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}
fn prefab(value: &str) -> SourceDocumentKey {
    key(value, SourceDocumentKind::Prefab)
}
fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/physical/{value}.ron")).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.PhysicalCount")
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.physical-count").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Physical count", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("score"),
            "Score",
            ComponentFieldPath::from_fields(["score"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
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

fn record(value: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("score", ComponentValue::U64(value))]),
    )
    .with_owner(owner())
}

fn deep_declaration(depth: usize, absent_leaf: bool) -> PrefabInstance {
    let mut instance = PrefabInstance::forest(request("leaf"));
    let mut overlay = PrefabOverlay::new(prefab("leaf"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("leaf")),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
            intent: if absent_leaf {
                PrefabPropertyIntent::Absent
            } else {
                PrefabPropertyIntent::Set(ComponentValue::U64(7))
            },
        }],
    });
    instance.overrides = Some(overlay);
    for level in 1..depth {
        let mut overlay = PrefabOverlay::new(prefab("leaf"));
        if level % 2 == 0 {
            let mut addition = PrefabAddition::new(id("child"));
            addition.prefab = Some(instance);
            overlay.additions.push(addition);
        } else {
            overlay.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("slot")),
                value: Some(instance),
            });
        }
        instance = PrefabInstance::forest(request("leaf"));
        instance.overrides = Some(overlay);
    }
    instance
}

#[test]
fn complete_topology_checks_the_deepest_declaration_without_native_recursion() {
    for depth in [MAX_PHYSICAL_DEPTH, MAX_PHYSICAL_DEPTH + 1] {
        let before = deep_declaration(depth, false);
        for changed in [false, true] {
            let after = deep_declaration(depth, changed);
            let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
            let result = instance_topology(Some(&before), Some(&after), &mut Vec::new(), &mut work);
            if depth == MAX_PHYSICAL_DEPTH && !changed {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(ExtractionError::InvalidSource)));
            }
        }
    }
}

#[test]
fn complete_topology_admits_its_explicit_ancestor_storage_before_allocation() {
    let before = deep_declaration(MAX_PHYSICAL_DEPTH, false);
    let after = deep_declaration(MAX_PHYSICAL_DEPTH, false);
    let limits = SceneCompilationLimits::default();
    let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
    instance_topology(Some(&before), Some(&after), &mut Vec::new(), &mut work).unwrap();
    let bytes = work.usage().1;
    for (limit, admitted) in [(bytes, true), (bytes - 1, false)] {
        let limits = limits.with_working_bytes(ByteLimit::new(limit).unwrap());
        let mut work = ExtractionWork::new(limits, Some(0)).unwrap();
        let result = instance_topology(Some(&before), Some(&after), &mut Vec::new(), &mut work);
        if admitted {
            assert!(result.is_ok());
            assert_eq!(work.usage().1, bytes);
        } else {
            assert!(matches!(result, Err(ExtractionError::Capacity)));
        }
    }
}

#[test]
fn the_last_physical_instance_still_maps_its_explicitly_absent_child_slot() {
    let make = || {
        let mut instance = deep_declaration(MAX_PHYSICAL_DEPTH, false);
        let mut path = PrefabInstancePath::new(id("host"));
        let mut leaf = &mut instance;
        for level in (1..MAX_PHYSICAL_DEPTH).rev() {
            let overlay = leaf.overrides.as_mut().unwrap();
            leaf = if level % 2 == 0 {
                path.steps
                    .push(PrefabInstancePathSegment::Addition { index: 0 });
                overlay.additions[0].prefab.as_mut().unwrap()
            } else {
                path.steps
                    .push(PrefabInstancePathSegment::InstanceClaim { index: 0 });
                overlay.instances[0].value.as_mut().unwrap()
            };
        }
        leaf.overrides
            .as_mut()
            .unwrap()
            .instances
            .push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("absent")),
                value: None,
            });
        path.steps
            .push(PrefabInstancePathSegment::InstanceClaim { index: 0 });
        let mut record = SceneEntityRecord::new(id("host"));
        record.prefab = Some(instance);
        (record, path)
    };
    let (before, path) = make();
    let (after, _) = make();
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    record_topology(&before, &after, &mut Vec::new(), &mut work).unwrap();
    assert_eq!(path.steps.len(), MAX_PHYSICAL_DEPTH);
    let mapped = MappedRecord {
        source: &prefab("physical"),
        before: &before,
        after: &after,
    };
    let slot = mapped.instance(&path).unwrap();
    assert!(slot.before.is_none() && slot.after.is_none());
}

fn source_inputs() -> ExtractionRequest {
    let basis = basis();
    let mut dormant = PrefabInstance::forest(request("never-load"));
    let mut old = PrefabOverlay::new(prefab("never-load"));
    old.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("unavailable-slot")),
        value: Some(PrefabInstance::forest(request("also-never-load"))),
    });
    old.components = vec![
        PrefabComponentIntent::Set {
            target: PrefabEntityTarget::source(id("unavailable-set")),
            component: component(),
            mode: PrefabComponentSetMode::Add,
            record: record(7),
        },
        PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("unavailable-pin")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(9)),
            }],
        },
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("unavailable-suppress")),
            component: component(),
        },
    ];
    old.placements
        .push(SceneEntityPlacement::root(id("old-layout")));
    old.suppressions
        .push(PrefabEntityTarget::source(id("old-hidden")));
    dormant.overrides = Some(old);
    let mut addition = PrefabAddition::new(id("a"));
    addition.components.insert(component(), record(3));
    addition.prefab = Some(dormant);
    let mut selected = PrefabInstance::forest(request("s"));
    let mut selected_overlay = PrefabOverlay::new(prefab("s"));
    selected_overlay.additions.push(addition);
    selected_overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("a")),
        ScenePlacement::last(ScenePlacementParent::Root),
    ));
    selected.overrides = Some(selected_overlay);
    let mut outer = PrefabOverlay::new(prefab("r"));
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("r-root")),
        value: Some(selected),
    });
    // This outer forwarded claim replaces a deeper original slot before opening its source.
    // Both declarations physically belong to P; the older complete body must move into Q.
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("r-root")),
                expected_base: prefab("s"),
            }],
            entity: PrefabEntityOrigin::InstanceAddition(id("a")),
        },
        value: None,
    });
    let mut instance = PrefabInstance::forest(request("r"));
    instance.overrides = Some(outer);
    let mut root = SceneEntityRecord::new(id("k")).with_component(component(), record(1));
    root.prefab = Some(instance);
    let p = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
            SceneEntityPlacement::root(id("x")),
        ],
        [
            SceneEntityRecord::new(id("x")),
            root,
            SceneEntityRecord::new(id("b")),
        ],
    );
    let r = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("r-root"))]);
    let mut anchor = SceneEntityRecord::new(id("host"));
    anchor.prefab = Some(PrefabInstance::forest(request("p")));
    let before: Vec<_> = ["p", "r", "s"]
        .into_iter()
        .map(|name| PrefabExtractionBinding {
            reference: request(name),
            source: prefab(name),
        })
        .collect();
    let mut after: Vec<_> = before
        .iter()
        .map(|binding| PrefabExtractionBinding {
            reference: binding.reference.clone(),
            source: binding.source.clone(),
        })
        .collect();
    after.push(PrefabExtractionBinding {
        reference: request("q"),
        source: prefab("q"),
    });
    ExtractionRequest {
        sources: vec![
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(p),
                    basis.clone(),
                    prefab("p"),
                ),
            },
            CapturedExtractionSource::Scene {
                key: key("scene", SourceDocumentKind::Scene),
                document: Arc::new(SceneDocument::from_ordered_roots([anchor])),
                basis: basis.clone(),
            },
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(r),
                    basis.clone(),
                    prefab("r"),
                ),
            },
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(PrefabDocument::from_ordered_roots([
                        SceneEntityRecord::new(id("s-root")),
                    ])),
                    basis.clone(),
                    prefab("s"),
                ),
            },
        ],
        environments: vec![PrefabExtractionEnvironment {
            name: "ownership-fixture".into(),
            sources: vec![0, 1, 2, 3],
            before,
            after,
            assets_before: None,
            assets_after: None,
        }],
        selected_environment: 0,
        basis,
        selected: prefab("p"),
        root: id("k"),
        max_depth: 32,
        destination: ExtractionDestination {
            key: prefab("q"),
            reference: request("q"),
            metadata: AssetRecord::new(
                "ab3bb1a0-b51c-46af-b5b8-ef948b5d5c1b".parse().unwrap(),
                AssetSourcePath::new("Prefabs/q.ron").unwrap(),
                AssetSourceKind::Prefab,
            ),
        },
    }
}

fn prepare(limits: SceneCompilationLimits) -> PreparedExtractionCorpus {
    source_inputs()
        .prepare(ExtractionWork::new(limits, Some(0)).unwrap())
        .unwrap_or_else(|failure| panic!("{:?}", failure.error))
}

fn root_index(prepared: &PreparedExtractionCorpus) -> usize {
    let SourceDocumentInput::Prefab(original) = prepared.original().sources[0].document() else {
        panic!("Prefab")
    };
    original
        .entities
        .iter()
        .position(|record| record.id == id("k"))
        .unwrap()
}

#[test]
fn original_root_facets_and_all_dormant_suffixes_follow_the_record_into_q() {
    let prepared = prepare(Default::default());
    let root = prepared.record(0, root_index(&prepared)).unwrap();
    assert_eq!(root.source, &prefab("q"));
    assert_eq!(root.after.prefab.as_ref().unwrap().source, request("r"));
    let slot = root.instance(&PrefabInstancePath::new(id("k"))).unwrap();
    assert_eq!(slot.after.unwrap().source, request("r"));
    let dormant = root
        .instance(&PrefabInstancePath {
            anchor: id("k"),
            steps: vec![
                PrefabInstancePathSegment::InstanceClaim { index: 0 },
                PrefabInstancePathSegment::Addition { index: 0 },
            ],
        })
        .unwrap();
    assert_eq!(dormant.source, &prefab("q"));
    assert_eq!(dormant.before.unwrap().source, request("never-load"));
    assert_eq!(dormant.after.unwrap().source, request("never-load"));
    for slot in [
        PrefabOverlaySlot::InstanceClaim { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
        PrefabOverlaySlot::Component { index: 1 },
        PrefabOverlaySlot::Component { index: 2 },
        PrefabOverlaySlot::Property {
            component_index: 1,
            claim_index: 0,
        },
        PrefabOverlaySlot::Placement { index: 0 },
        PrefabOverlaySlot::Suppression { index: 0 },
    ] {
        let mapped = dormant.intent(slot).unwrap();
        assert_eq!(mapped.source, &prefab("q"));
        assert_eq!(mapped.slot, slot);
        assert!(std::ptr::eq(
            mapped.before,
            dormant.before.unwrap().overrides.as_ref().unwrap()
        ));
        assert!(std::ptr::eq(
            mapped.after,
            dormant.after.unwrap().overrides.as_ref().unwrap()
        ));
    }
    let nested = root
        .instance(&PrefabInstancePath {
            anchor: id("k"),
            steps: vec![
                PrefabInstancePathSegment::InstanceClaim { index: 0 },
                PrefabInstancePathSegment::Addition { index: 0 },
                PrefabInstancePathSegment::InstanceClaim { index: 0 },
            ],
        })
        .unwrap();
    assert_eq!(nested.after.unwrap().source, request("also-never-load"));
    let removed_slot = root
        .instance(&PrefabInstancePath {
            anchor: id("k"),
            steps: vec![PrefabInstancePathSegment::InstanceClaim { index: 1 }],
        })
        .unwrap();
    assert!(removed_slot.before.is_none() && removed_slot.after.is_none());
    assert!(
        root.instance(&PrefabInstancePath::new(id("other")))
            .is_none()
    );
    assert!(
        dormant
            .intent(PrefabOverlaySlot::Property {
                component_index: 0,
                claim_index: 0
            })
            .is_none()
    );
    let CapturedExtractionSource::Prefab { source: p, .. } = &prepared.sources()[0] else {
        panic!("Prefab")
    };
    let anchor = p
        .document()
        .entities
        .iter()
        .find(|record| record.id == id("k"))
        .unwrap();
    assert_eq!(anchor.prefab.as_ref().unwrap().source, request("q"));
    assert!(anchor.components.is_empty());
}

fn copy_sources(sources: &[CapturedExtractionSource]) -> Vec<CapturedExtractionSource> {
    sources
        .iter()
        .map(|source| match source {
            CapturedExtractionSource::Scene {
                key,
                document,
                basis,
            } => CapturedExtractionSource::Scene {
                key: key.clone(),
                document: Arc::clone(document),
                basis: basis.clone(),
            },
            CapturedExtractionSource::Prefab { source } => CapturedExtractionSource::Prefab {
                source: source.clone(),
            },
        })
        .collect()
}

#[test]
fn losing_a_dormant_declaration_refuses_even_when_it_has_no_effective_occurrence() {
    let prepared = prepare(Default::default());
    let sources = copy_sources(prepared.sources());
    let original = &prepared.original().sources;
    let SourceDocumentInput::Prefab(p) = original[0].document() else {
        panic!("Prefab")
    };
    let destinations = p
        .entities
        .iter()
        .map(|record| {
            if record.id == id("x") {
                RecordDestination::Retained
            } else {
                RecordDestination::Extracted
            }
        })
        .collect::<Vec<_>>();
    let placements = (0..p.placements.len())
        .map(
            |index| match prepared.placement_destination(0, index).unwrap() {
                (OutputSource::Captured(0), output) => PlacementDestination::Retained(output),
                (OutputSource::Extracted, output) => PlacementDestination::Extracted(output),
                _ => panic!("selected source"),
            },
        )
        .collect::<Vec<_>>();
    for change in 0..4 {
        let mut broken = prepared.extracted().document().clone();
        let root = broken
            .entities
            .iter_mut()
            .find(|record| record.id == id("k"))
            .unwrap();
        let dormant = root
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap()
            .instances[0]
            .value
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap()
            .additions[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap();
        match change {
            0 => {
                dormant.instances.clear();
            }
            1 => {
                dormant.components.swap(0, 2);
            }
            2 => {
                dormant.suppressions.clear();
            }
            _ => {
                let PrefabComponentIntent::Properties { claims, .. } = &mut dormant.components[1]
                else {
                    panic!("Properties")
                };
                claims[0].intent = PrefabPropertyIntent::Absent;
            }
        }
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        assert!(matches!(
            map_declarations(
                original,
                &sources,
                0,
                &broken,
                &destinations,
                &placements,
                prepared.generated_placements().to_vec(),
                &id("k"),
                &mut work
            ),
            Err(ExtractionError::InvalidSource)
        ));
        assert!(work.usage().0 > 0);
    }
}

#[test]
fn original_and_generated_placements_have_disjoint_complete_output_ownership() {
    let prepared = prepare(Default::default());
    let originals = &prepared.original().sources;
    let SourceDocumentInput::Prefab(p) = originals[0].document() else {
        panic!("Prefab")
    };
    let records = p
        .entities
        .iter()
        .map(|record| {
            if record.id == id("x") {
                RecordDestination::Retained
            } else {
                RecordDestination::Extracted
            }
        })
        .collect::<Vec<_>>();
    let placements = (0..p.placements.len())
        .map(
            |index| match prepared.placement_destination(0, index).unwrap() {
                (OutputSource::Captured(0), output) => PlacementDestination::Retained(output),
                (OutputSource::Extracted, output) => PlacementDestination::Extracted(output),
                _ => panic!("selected source"),
            },
        )
        .collect::<Vec<_>>();
    assert!(!prepared.generated_placements().is_empty());
    for change in 0..4 {
        let mut destinations = placements.clone();
        let mut generated = prepared.generated_placements().to_vec();
        match change {
            0 => {
                destinations[2] = destinations[0];
            } // Last -> Last cannot prove unique ownership.
            1 => {
                destinations[1] = PlacementDestination::Extracted(generated[0]);
            }
            2 => {
                generated.clear();
            }
            _ => {
                generated.push(generated[0]);
            }
        }
        let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
        assert!(matches!(
            map_declarations(
                originals,
                prepared.sources(),
                0,
                prepared.extracted().document(),
                &records,
                &destinations,
                generated,
                &id("k"),
                &mut work
            ),
            Err(ExtractionError::InvalidSource)
        ));
    }
}
