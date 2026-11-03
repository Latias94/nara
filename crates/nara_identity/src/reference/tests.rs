use nara_core::SourceUniverseId;

use crate::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, EntityReference,
    EntityReferenceCost, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop,
    ReferenceInputId, ReferenceInputIdError, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    StableAssetId,
};

const UNIVERSE: &str = "72011426-37e2-48ba-bc06-78841468889a";
const STABLE_ID: &str = "44f2caae-b658-4790-a8a7-928935839ab7";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn universe() -> SourceUniverseId {
    UNIVERSE.parse().unwrap()
}

fn named_key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(universe(), name, SourceDocumentKind::Prefab).unwrap()
}

#[test]
fn constant_time_node_count_agrees_with_full_cost_for_every_reference_role() {
    for count in [0, 1, 4_096] {
        let authored = AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("slot")),
                    expected_base: named_key("p"),
                };
                count
            ],
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new("u").unwrap()),
        };
        assert_eq!(authored.cost_nodes(), 2 + count * 2);
        assert_eq!(authored.cost_nodes(), authored.cost().nodes());
        let reference = EntityReference::Authored {
            reference: authored,
        };
        assert_eq!(reference.cost_nodes(), reference.cost().nodes());
    }
    let local = EntityReference::SceneLocal {
        entity: id("local"),
    };
    assert_eq!(local.cost_nodes(), local.cost().nodes());
    let persistent = EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str(STABLE_ID).unwrap(),
        ),
    };
    assert_eq!(persistent.cost_nodes(), persistent.cost().nodes());
}

#[test]
fn input_ids_validate_the_public_ascii_grammar_and_exact_byte_limit() {
    assert_eq!(ReferenceInputId::MAX_BYTES, 128);
    for value in ["A", "0", "_", ".", "..", "-", "camera.target_2-main"] {
        let input = ReferenceInputId::new(value).unwrap();
        assert_eq!(input.as_str(), value);
        assert_eq!(input.to_string(), value);
    }
    let maximum = "a".repeat(ReferenceInputId::MAX_BYTES);
    assert_eq!(ReferenceInputId::new(&maximum).unwrap().as_str(), maximum);
    assert_eq!(ReferenceInputId::new(""), Err(ReferenceInputIdError::Empty));
    for (value, character) in [
        ("with space", ' '),
        ("input/path", '/'),
        ("input\\path", '\\'),
        ("input:target", ':'),
        ("input\0", '\0'),
        ("input\n", '\n'),
        ("caf\u{e9}", '\u{e9}'),
    ] {
        assert_eq!(
            ReferenceInputId::new(value),
            Err(ReferenceInputIdError::InvalidCharacter(character))
        );
    }
}

#[test]
fn oversized_input_ids_return_bounded_diagnostics_instead_of_retaining_input() {
    for value in ["a".repeat(129), "a".repeat(128 * 1024), "\u{e9}".repeat(65)] {
        let error = ReferenceInputId::new(value.clone()).unwrap_err();
        assert_eq!(
            error,
            ReferenceInputIdError::TooLong {
                length: value.len(),
                maximum: 128,
            }
        );
        assert_eq!(
            error.to_string(),
            "reference input id exceeds its byte limit"
        );
    }
}

#[test]
fn authored_constructors_keep_entity_origins_and_inputs_distinct() {
    for (reference, target) in [
        (
            AuthoredEntityReference::source(id("actor")),
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("actor"))),
        ),
        (
            AuthoredEntityReference::addition(id("actor")),
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(id("actor"))),
        ),
        (
            AuthoredEntityReference::input(ReferenceInputId::new("actor").unwrap()),
            AuthoredReferenceTarget::Input(ReferenceInputId::new("actor").unwrap()),
        ),
    ] {
        assert!(reference.through.is_empty());
        assert_eq!(reference.target, target);
    }
}

#[test]
fn structural_target_conversion_consumes_the_route_and_origin_without_cloning() {
    for mut target in [
        PrefabEntityTarget::source(id("endpoint")),
        PrefabEntityTarget::addition(id("endpoint")),
    ] {
        target.through.push(PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(id("anchor")),
            expected_base: named_key("nested"),
        });
        let expected = target.clone();
        let route_pointer = target.through.as_ptr();
        let endpoint_pointer = target.entity.entity().as_str().as_ptr();
        let reference = AuthoredEntityReference::from(target);
        assert_eq!(reference.through, expected.through);
        assert_eq!(reference.through.as_ptr(), route_pointer);
        let AuthoredReferenceTarget::Entity(origin) = reference.target else {
            panic!("structural targets convert to entity endpoints");
        };
        assert_eq!(origin, expected.entity);
        assert_eq!(origin.entity().as_str().as_ptr(), endpoint_pointer);
    }
}

#[test]
fn native_reference_costs_preserve_the_existing_node_and_byte_contract() {
    for value in ["root/player".to_string(), "a".repeat(4096)] {
        let reference = EntityReference::SceneLocal { entity: id(&value) };
        assert_eq!(reference.cost().nodes(), 1);
        assert_eq!(reference.cost().logical_bytes(), value.len());
    }
    for namespace in ["save.main".to_string(), "n".repeat(256)] {
        let reference = EntityReference::Persistent {
            entity: PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new(&namespace).unwrap(),
                PersistentRuntimeId::parse_str(STABLE_ID).unwrap(),
            ),
        };
        assert_eq!(reference.cost().nodes(), 1);
        assert_eq!(reference.cost().logical_bytes(), namespace.len() + 16);
    }
}

#[test]
fn authored_costs_charge_the_root_and_every_typed_endpoint() {
    for (reference, bytes) in [
        (AuthoredEntityReference::source(id("source")), 6),
        (AuthoredEntityReference::addition(id("added")), 5),
        (
            AuthoredEntityReference::input(ReferenceInputId::new("camera.target").unwrap()),
            13,
        ),
    ] {
        let cost = reference.cost();
        assert_eq!(cost.nodes(), 2);
        assert_eq!(cost.logical_bytes(), bytes);
        assert_eq!(EntityReference::Authored { reference }.cost(), cost);
    }
}

#[test]
fn authored_costs_include_complete_named_location_and_stable_asset_keys() {
    let name = "characters/\u{e9}";
    let path = "prefabs/characters/actor.prefab.json";
    let mut reference = AuthoredEntityReference::source(id("leaf"));
    for (key, key_bytes) in [
        (named_key(name), 36 + name.len() + 1),
        (
            SourceDocumentKey::project_location(
                universe(),
                AssetSourcePath::new(path).unwrap(),
                SourceDocumentKind::Scene,
            ),
            36 + path.len() + 1,
        ),
        (
            SourceDocumentKey::stable_asset(
                StableAssetId::parse_str(STABLE_ID).unwrap(),
                SourceDocumentKind::Prefab,
            ),
            36 + 1,
        ),
    ] {
        let before = reference.cost();
        reference.through.push(PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(id("anchor")),
            expected_base: key,
        });
        let after = reference.cost();
        assert_eq!(after.nodes(), before.nodes() + 2);
        assert_eq!(
            after.logical_bytes(),
            before.logical_bytes() + 6 + key_bytes
        );
        assert_eq!(
            EntityReference::Authored {
                reference: reference.clone(),
            }
            .cost(),
            after
        );
    }
    assert_eq!(reference.cost().nodes(), 8);
}

#[test]
fn long_routes_charge_a_complete_shared_source_name_for_every_hop() {
    let name = "n".repeat(64 * 1024);
    let hop = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("branch/anchor")),
        expected_base: named_key(&name),
    };
    let reference = AuthoredEntityReference {
        through: vec![hop; 2048],
        target: AuthoredReferenceTarget::Input(ReferenceInputId::new("target").unwrap()),
    };
    assert_eq!(reference.cost().nodes(), 2 + 2 * 2048);
    assert_eq!(
        reference.cost().logical_bytes(),
        6 + 2048 * ("branch/anchor".len() + 36 + name.len() + 1)
    );
}

#[test]
fn logical_cost_accumulation_saturates_both_counters() {
    let cost = EntityReferenceCost {
        nodes: usize::MAX - 1,
        logical_bytes: usize::MAX - 4,
    }
    .saturating_add(2, 37);
    assert_eq!(cost.nodes(), usize::MAX);
    assert_eq!(cost.logical_bytes(), usize::MAX);
    assert_eq!(cost.saturating_add(2, 37), cost);
}

#[cfg(feature = "serde")]
mod wire {
    use super::*;
    use serde_json::json;

    #[test]
    fn input_serde_always_validates_through_the_constructor() {
        let valid = "a".repeat(ReferenceInputId::MAX_BYTES);
        let input = ReferenceInputId::new(&valid).unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap(), json!(valid));
        assert_eq!(
            serde_json::from_value::<ReferenceInputId>(json!(valid)).unwrap(),
            input
        );
        assert_eq!(
            ron::from_str::<ReferenceInputId>(&ron::to_string(&input).unwrap()).unwrap(),
            input
        );
        for invalid in [
            "".to_string(),
            "a".repeat(129),
            "x/y".to_string(),
            "\u{e9}".to_string(),
        ] {
            let encoded = serde_json::to_string(&invalid).unwrap();
            assert!(serde_json::from_str::<ReferenceInputId>(&encoded).is_err());
            assert!(ron::from_str::<ReferenceInputId>(&encoded).is_err());
            assert!(
                serde_json::from_value::<AuthoredReferenceTarget>(json!({
                    "kind": "input", "value": invalid
                }))
                .is_err()
            );
        }
    }

    #[test]
    fn structural_grammar_preserves_the_existing_data_and_serde_shape() {
        for (target, kind) in [
            (PrefabEntityTarget::source(id("actor")), "source_entity"),
            (
                PrefabEntityTarget::addition(id("actor")),
                "instance_addition",
            ),
        ] {
            let origin = json!({"kind": kind, "entity": "actor"});
            assert_eq!(serde_json::to_value(&target.entity).unwrap(), origin);
            assert_eq!(
                serde_json::to_value(&target).unwrap(),
                json!({"through": [], "entity": origin})
            );
            assert_eq!(
                serde_json::from_value::<PrefabEntityTarget>(json!({"entity": origin})).unwrap(),
                target
            );
            let encoded = ron::to_string(&target).unwrap();
            assert_eq!(
                ron::from_str::<PrefabEntityTarget>(&encoded).unwrap(),
                target
            );
        }
        let hop = PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
            expected_base: named_key("nested"),
        };
        assert_eq!(
            serde_json::to_value(&hop).unwrap(),
            json!({
                "anchor": {"kind": "source_entity", "entity": "anchor"},
                "expected_base": {
                    "kind": "prefab",
                    "origin": {"kind": "named", "universe": UNIVERSE, "name": "nested"}
                }
            })
        );
    }

    #[test]
    fn inline_reference_serde_preserves_every_endpoint_and_defaults_an_empty_route() {
        for (reference, target) in [
            (
                AuthoredEntityReference::source(id("actor")),
                json!({"kind": "entity", "value": {"kind": "source_entity", "entity": "actor"}}),
            ),
            (
                AuthoredEntityReference::addition(id("actor")),
                json!({
                    "kind": "entity", "value": {"kind": "instance_addition", "entity": "actor"}
                }),
            ),
            (
                AuthoredEntityReference::input(ReferenceInputId::new("actor").unwrap()),
                json!({"kind": "input", "value": "actor"}),
            ),
        ] {
            assert_eq!(
                serde_json::from_value::<AuthoredEntityReference>(json!({"target": target}))
                    .unwrap(),
                reference
            );
            let reference = EntityReference::Authored { reference };
            let expected = json!({
                "kind": "authored", "reference": {"through": [], "target": target}
            });
            assert_eq!(serde_json::to_value(&reference).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<EntityReference>(expected).unwrap(),
                reference
            );
            let encoded = ron::to_string(&reference).unwrap();
            assert_eq!(
                ron::from_str::<EntityReference>(&encoded).unwrap(),
                reference
            );
        }
    }

    #[test]
    fn routed_reference_serde_roundtrips_complete_keys_without_runtime_identity() {
        let through = vec![
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("named")),
                expected_base: named_key("nested"),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(id("located")),
                expected_base: SourceDocumentKey::project_location(
                    universe(),
                    AssetSourcePath::new("prefabs/nested.prefab.ron").unwrap(),
                    SourceDocumentKind::Prefab,
                ),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("stable")),
                expected_base: SourceDocumentKey::stable_asset(
                    StableAssetId::parse_str(STABLE_ID).unwrap(),
                    SourceDocumentKind::Prefab,
                ),
            },
        ];
        for target in [
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(id("leaf"))),
            AuthoredReferenceTarget::Input(ReferenceInputId::new("target").unwrap()),
        ] {
            let authored = AuthoredEntityReference {
                through: through.clone(),
                target,
            };
            let encoded = ron::to_string(&authored).unwrap();
            assert_eq!(
                ron::from_str::<AuthoredEntityReference>(&encoded).unwrap(),
                authored
            );
            let reference = EntityReference::Authored {
                reference: authored,
            };
            let encoded = serde_json::to_string(&reference).unwrap();
            assert!(!encoded.contains("incarnation"));
            assert!(!encoded.contains("instance_id"));
            assert_eq!(
                serde_json::from_str::<EntityReference>(&encoded).unwrap(),
                reference
            );
            let encoded = ron::to_string(&reference).unwrap();
            assert_eq!(
                ron::from_str::<EntityReference>(&encoded).unwrap(),
                reference
            );
        }
    }

    #[test]
    fn inline_reference_grammar_rejects_unknown_fields_at_every_owned_layer() {
        let reference = EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::InstanceAddition(id("anchor")),
                    expected_base: named_key("nested"),
                }],
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                    "leaf",
                ))),
            },
        };
        let valid = serde_json::to_value(&reference).unwrap();
        for pointer in [
            "",
            "/reference",
            "/reference/target",
            "/reference/target/value",
            "/reference/through/0",
            "/reference/through/0/anchor",
            "/reference/through/0/expected_base",
            "/reference/through/0/expected_base/origin",
        ] {
            let mut invalid = valid.clone();
            invalid
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert!(
                serde_json::from_value::<EntityReference>(invalid).is_err(),
                "{pointer}"
            );
        }
        for invalid in [
            json!({"kind": "input", "value": "target", "unexpected": true}),
            json!({"kind": "input"}),
            json!({"kind": "input", "value": 7}),
            json!({"kind": "entity", "value": "actor"}),
            json!({"kind": "unknown", "value": "actor"}),
        ] {
            assert!(serde_json::from_value::<AuthoredReferenceTarget>(invalid).is_err());
        }
        assert!(serde_json::from_value::<AuthoredEntityReference>(json!({"through": []})).is_err());
        assert!(
            serde_json::from_value::<PrefabEntityTarget>(json!({
                "entity": {"kind": "input", "value": "target"}
            }))
            .is_err()
        );
    }

    #[test]
    fn native_reference_serde_keeps_its_existing_wire_shapes() {
        let local = EntityReference::SceneLocal {
            entity: id("player"),
        };
        let persistent = EntityReference::Persistent {
            entity: PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new("save").unwrap(),
                PersistentRuntimeId::parse_str(STABLE_ID).unwrap(),
            ),
        };
        for (reference, expected) in [
            (local, json!({"kind": "scene_local", "entity": "player"})),
            (
                persistent,
                json!({"kind": "persistent", "entity": {"namespace": "save", "entity": STABLE_ID}}),
            ),
        ] {
            assert_eq!(serde_json::to_value(&reference).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<EntityReference>(expected).unwrap(),
                reference
            );
            let encoded = ron::to_string(&reference).unwrap();
            assert_eq!(
                ron::from_str::<EntityReference>(&encoded).unwrap(),
                reference
            );
        }
    }

    #[test]
    fn reference_fields_decode_in_any_order_without_buffering_source_key_origins() {
        let reference = AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                expected_base: named_key("nested"),
            }],
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new("target").unwrap()),
        };
        let json = format!(
            r#"{{"reference":{},"kind":"authored"}}"#,
            serde_json::to_string(&reference).unwrap()
        );
        let ron = format!(
            r#"(reference:{},kind:"authored")"#,
            ron::to_string(&reference).unwrap()
        );
        let expected = EntityReference::Authored { reference };
        assert_eq!(
            serde_json::from_str::<EntityReference>(&json).unwrap(),
            expected
        );
        assert_eq!(ron::from_str::<EntityReference>(&ron).unwrap(), expected);
    }

    #[test]
    fn reference_serde_rejects_cross_kind_null_missing_and_duplicate_payload_fields() {
        let entity = json!({"namespace": "save", "entity": STABLE_ID});
        let authored = json!({"target": {"kind": "input", "value": "target"}});
        for invalid in [
            json!({"kind": "scene_local", "entity": entity}),
            json!({"kind": "persistent", "entity": "player"}),
            json!({"kind": "authored", "entity": "player"}),
            json!({"kind": "scene_local", "reference": authored}),
            json!({"kind": "authored", "reference": authored, "entity": "player"}),
            json!({"kind": "scene_local", "entity": "player", "reference": authored}),
            json!({"kind": "authored", "reference": authored, "entity": null}),
            json!({"kind": "scene_local", "entity": "player", "reference": null}),
            json!({"kind": "persistent", "entity": entity, "reference": null}),
            json!({"kind": "scene_local", "entity": null}),
            json!({"kind": "authored", "reference": null}),
            json!({"kind": "authored"}),
            json!({"kind": "scene_local"}),
            json!({"kind": "unknown", "reference": authored}),
        ] {
            assert!(serde_json::from_value::<EntityReference>(invalid).is_err());
        }
        for invalid in [
            r#"{"kind":"scene_local","kind":"scene_local","entity":"player"}"#,
            r#"{"kind":"scene_local","entity":"player","entity":"player"}"#,
            concat!(
                r#"{"kind":"authored","reference":{"target":{"kind":"input","value":"target"}},"#,
                r#""reference":{"target":{"kind":"input","value":"target"}}}"#,
            ),
        ] {
            assert!(serde_json::from_str::<EntityReference>(invalid).is_err());
        }
        for invalid in [
            r#"(kind:"scene_local",entity:"player",unexpected:true)"#,
            r#"(kind:"scene_local",entity:"player",reference:None)"#,
            r#"(kind:"scene_local",entity:"player",entity:"player")"#,
            r#"(kind:"authored",reference:(target:(kind:input,value:"target")),extra:true)"#,
        ] {
            assert!(ron::from_str::<EntityReference>(invalid).is_err());
        }
    }
}
