#![cfg(feature = "serde")]

use nara_asset::{AssetProductRef, ImportedProductId, RuntimeAssetCodec, StableAssetId};
use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, PersistentFileContractError, SerdeShapeError,
    SerdeShapeLimits,
};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionIdError, ActionMapAsset, ActionMapBuilder,
    ActionMapError, ActionMapFileCandidate, ActionMapFileError, ActionMapFileLimits,
    ActionMapLimitKind, ActionMapLimits, ActionMapRuntimeCodec, ActionTransform, ActionValueKind,
    AxisDeadZone, AxisDirection, InputBinding, KeyCode, MAX_ACTION_BINDINGS, MouseButton,
};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("fixtures/formats/v1/action-map.json");
const SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";

#[test]
fn nonempty_golden_preserves_product_contexts_and_ordered_physical_bindings() {
    let candidate = ActionMapFileCandidate::decode_json_bytes(GOLDEN.as_bytes()).unwrap();
    assert_eq!(candidate.product(), product());
    assert_eq!(
        candidate
            .definition()
            .contexts()
            .iter()
            .map(|entry| (entry.context.as_str(), entry.enabled))
            .collect::<Vec<_>>(),
        [("gameplay", true), ("menu", false)]
    );
    let bindings = candidate.definition().bindings();
    assert_eq!(bindings.len(), 3);
    assert_eq!(
        bindings[0],
        ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::Space)
    );
    assert_eq!(bindings[1].action.as_str(), "move-up");
    assert_eq!(
        bindings[1].input,
        InputBinding::Key(KeyCode::Character('w'))
    );
    assert_eq!(bindings[2].input, InputBinding::Mouse(MouseButton::Left));
    assert_eq!(bindings[2].context.as_str(), "menu");
    assert_eq!(candidate.to_json_string().unwrap(), canonical_golden());
}

#[test]
fn code_first_definition_writes_the_same_golden_and_moves_out_without_runtime_state() {
    let candidate = sample_candidate();
    let encoded = candidate.to_json_string().unwrap();
    assert_eq!(encoded, canonical_golden());
    let decoded = ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(decoded.definition().cost(), candidate.definition().cost());
    let (identity, definition) = decoded.into_parts();
    assert_eq!(identity, product());
    assert_eq!(definition.bindings(), candidate.definition().bindings());
    assert_eq!(definition.contexts(), candidate.definition().contexts());

    let value: Value = serde_json::from_str(&encoded).unwrap();
    let payload = value["payload"].as_object().unwrap();
    assert_eq!(payload.len(), 3);
    assert!(payload.contains_key("product"));
    assert!(payload.contains_key("contexts"));
    assert!(payload.contains_key("bindings"));
}

#[test]
fn typed_axis_definitions_round_trip_through_source_and_the_same_runtime_codec() {
    let action = ActionId::new("move").unwrap();
    let expected = [
        ActionBinding::key_axis(action.clone(), KeyCode::ArrowLeft, AxisDirection::Negative),
        ActionBinding::key_axis(
            action.clone(),
            KeyCode::Character('é'),
            AxisDirection::Positive,
        ),
        ActionBinding::gamepad_axis(action, u16::MAX, AxisDeadZone::new(0.25).unwrap(), true),
        ActionBinding::gamepad_button(ActionId::new("confirm").unwrap(), u16::MAX),
        ActionBinding {
            transform: ActionTransform::AxisButton {
                direction: AxisDirection::Negative,
            },
            ..ActionBinding::mouse(ActionId::new("aim").unwrap(), MouseButton::Other(u16::MAX))
        },
    ];
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for binding in &expected {
        builder.bind(binding.clone()).unwrap();
    }
    let candidate = ActionMapFileCandidate::new(product(), builder.compile().unwrap()).unwrap();
    let encoded = candidate.to_json_string().unwrap();
    let wire: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(wire["format_version"], json!(1));
    assert_eq!(
        wire["payload"]["bindings"][2]["input"],
        json!({ "GamepadAxis": 65535 })
    );
    assert_eq!(
        wire["payload"]["bindings"][2]["transform"],
        json!({ "Axis": { "dead_zone": 0.25, "inverted": true } })
    );
    let decoded = ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(decoded.product(), product());
    assert_eq!(decoded.definition().bindings(), expected);
    assert_eq!(decoded.definition().cost(), candidate.definition().cost());
    assert_eq!(
        decoded
            .definition()
            .value_kind(&ActionId::new("move").unwrap(), &ActionContext::gameplay()),
        Some(ActionValueKind::Axis)
    );
    assert_eq!(decoded.to_json_string().unwrap(), encoded);

    let (identity, definition) = decoded.into_parts();
    let runtime = ActionMapAsset::new(identity, definition).unwrap();
    let runtime_bytes = ActionMapRuntimeCodec::encode_runtime(&runtime).unwrap();
    let runtime_decoded = ActionMapRuntimeCodec::decode_runtime(&runtime_bytes).unwrap();
    assert_eq!(runtime_decoded.product(), identity);
    assert_eq!(runtime_decoded.definition().bindings(), expected);
    assert_eq!(
        ActionMapRuntimeCodec::runtime_retained_bytes(&runtime).unwrap(),
        ActionMapRuntimeCodec::runtime_retained_bytes(&runtime_decoded).unwrap()
    );
}

#[test]
fn json_member_order_does_not_change_context_or_binding_order() {
    let value = golden();
    let payload = format!(
        r#"{{"bindings":{},"product":{},"contexts":{}}}"#,
        serde_json::to_string(&value["payload"]["bindings"]).unwrap(),
        serde_json::to_string(&value["payload"]["product"]).unwrap(),
        serde_json::to_string(&value["payload"]["contexts"]).unwrap(),
    );
    let encoded = format!(
        concat!(
            r#"{{"payload":{},"generator":{},"engine_min_version":"0.1.0","#,
            r#""format_version":1,"kind":"action_map"}}"#,
        ),
        payload,
        serde_json::to_string(&value["generator"]).unwrap(),
    );
    let candidate = ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(candidate.to_json_string().unwrap(), canonical_golden());
}

#[test]
fn compatibility_rejection_precedes_domain_construction() {
    for (member, replacement) in [
        ("kind", json!("scene")),
        ("format_version", json!(2)),
        ("engine_min_version", json!("9999.0.0")),
    ] {
        let mut value = golden();
        value[member] = replacement;
        value["payload"] = Value::Null;
        let error = decode_value(&value).unwrap_err();
        assert!(matches!(
            (member, error),
            (
                "kind",
                ActionMapFileError::Contract(PersistentFileContractError::WrongKind { .. })
            ) | (
                "format_version",
                ActionMapFileError::Contract(
                    PersistentFileContractError::UnsupportedFormatVersion { .. }
                )
            ) | (
                "engine_min_version",
                ActionMapFileError::Contract(
                    PersistentFileContractError::EngineVersionTooOld { .. }
                )
            )
        ));
    }
}

#[test]
fn bare_live_payloads_ron_and_trailing_values_are_rejected() {
    let value = golden();
    let legacy = json!({
        "bindings": value["payload"]["bindings"],
        "disabled_contexts": ["menu"]
    });
    assert!(decode_value(&legacy).is_err());
    assert!(decode_value(&value["payload"]).is_err());
    for encoded in [
        "(bindings: [], disabled_contexts: [])".to_owned(),
        format!("{GOLDEN} {{}}"),
        format!("{GOLDEN} trailing"),
    ] {
        assert!(ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()).is_err());
    }
}

#[test]
fn every_object_record_rejects_unknown_members() {
    for path in [
        "",
        "/generator",
        "/payload",
        "/payload/product",
        "/payload/product/product",
        "/payload/contexts/0",
        "/payload/bindings/0",
        "/payload/bindings/0/input",
    ] {
        let mut value = golden();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(true));
        assert!(decode_value(&value).is_err(), "unknown member at {path}");
    }
    for field in ["disabled_contexts", "revision", "held", "mapping_lifetime"] {
        let mut value = golden();
        value["payload"][field] = json!([]);
        assert!(decode_value(&value).is_err(), "runtime member {field}");
    }
}

#[test]
fn duplicate_members_are_rejected_by_structural_preflight() {
    for (path, member) in [
        ("", "kind"),
        ("/generator", "name"),
        ("/payload", "contexts"),
        ("/payload/product", "source"),
        ("/payload/product/product", "kind"),
        ("/payload/contexts/0", "context"),
        ("/payload/bindings/0", "action"),
        ("/payload/bindings/0/input", "Key"),
    ] {
        let encoded = duplicate_member(&golden(), path, member);
        assert!(
            matches!(
                ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()),
                Err(ActionMapFileError::Shape(SerdeShapeError::DuplicateMapKey))
            ),
            "duplicate member at {path}/{member}"
        );
    }
}

#[test]
fn object_records_never_accept_positional_arrays() {
    for path in [
        "",
        "/generator",
        "/payload",
        "/payload/product",
        "/payload/product/product",
        "/payload/contexts/0",
        "/payload/bindings/0",
        "/payload/bindings/0/input",
    ] {
        let mut value = golden();
        let target = value.pointer_mut(path).unwrap();
        let members = target.as_object().unwrap().values().cloned().collect();
        *target = Value::Array(members);
        assert!(decode_value(&value).is_err(), "positional record at {path}");
    }
}

#[test]
fn all_payload_record_members_are_explicit_and_required() {
    for (path, member) in [
        ("", "kind"),
        ("", "format_version"),
        ("", "engine_min_version"),
        ("", "generator"),
        ("", "payload"),
        ("/generator", "name"),
        ("/generator", "version"),
        ("/payload", "product"),
        ("/payload", "contexts"),
        ("/payload", "bindings"),
        ("/payload/product", "source"),
        ("/payload/product", "product"),
        ("/payload/product/product", "kind"),
        ("/payload/contexts/0", "context"),
        ("/payload/contexts/0", "enabled"),
        ("/payload/bindings/0", "action"),
        ("/payload/bindings/0", "input"),
        ("/payload/bindings/0", "context"),
    ] {
        let mut value = golden();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(member)
            .unwrap();
        assert!(decode_value(&value).is_err(), "missing {path}/{member}");
    }
    let mut empty = golden();
    empty["payload"]["contexts"] = json!([]);
    empty["payload"]["bindings"] = json!([]);
    let candidate = decode_value(&empty).unwrap();
    assert!(candidate.definition().contexts().is_empty());
    assert!(candidate.definition().bindings().is_empty());
}

#[test]
fn encoded_limit_is_exact_for_reader_and_writer() {
    let candidate = sample_candidate();
    let encoded = candidate.to_json_string().unwrap();
    let exact =
        ActionMapFileLimits::default().with_encoded_bytes(ByteLimit::new(encoded.len()).unwrap());
    ActionMapFileCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), exact).unwrap();
    assert_eq!(
        candidate.to_json_string_with_limits(exact).unwrap(),
        encoded
    );
    let short = exact.with_encoded_bytes(ByteLimit::new(encoded.len() - 1).unwrap());
    assert!(matches!(
        ActionMapFileCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), short),
        Err(ActionMapFileError::EncodedBytesExceeded { observed, maximum })
            if observed == encoded.len() && maximum == encoded.len() - 1
    ));
    assert!(matches!(
        candidate.to_json_string_with_limits(short),
        Err(ActionMapFileError::EncodedBytesExceeded { maximum, .. })
            if maximum == encoded.len() - 1
    ));
}

#[test]
fn writer_and_reader_share_every_structural_limit() {
    let limits = ActionMapFileLimits::default();
    let shape = limits.shape();
    let restricted = [
        SerdeShapeLimits::new(
            DepthLimit::ONE,
            shape.nodes(),
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ),
        SerdeShapeLimits::new(
            shape.depth(),
            ItemLimit::ONE,
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ),
        SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            ItemLimit::ONE,
            shape.string_bytes(),
            shape.total_string_bytes(),
        ),
        SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            ByteLimit::new(35).unwrap(),
            shape.total_string_bytes(),
        ),
        SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            shape.string_bytes(),
            ByteLimit::ONE,
        ),
    ];
    let candidate = sample_candidate();
    for shape in restricted {
        let limited = limits.with_shape(shape);
        assert!(matches!(
            ActionMapFileCandidate::decode_json_bytes_with_limits(GOLDEN.as_bytes(), limited),
            Err(ActionMapFileError::Shape(_))
        ));
        assert!(matches!(
            candidate.to_json_string_with_limits(limited),
            Err(ActionMapFileError::Shape(_))
        ));
    }
}

#[test]
fn domain_counts_are_admitted_before_decoding_the_first_excess_record() {
    let exact_definition = ActionMapLimits::default()
        .with_contexts(ItemLimit::new(2).unwrap())
        .with_bindings(ItemLimit::new(3).unwrap());
    let exact = ActionMapFileLimits::default().with_definition(exact_definition);
    ActionMapFileCandidate::decode_json_bytes_with_limits(GOLDEN.as_bytes(), exact).unwrap();
    sample_candidate()
        .to_json_string_with_limits(exact)
        .unwrap();

    let mut excess_context = golden();
    excess_context["payload"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(json!(42));
    assert!(matches!(
        decode_value_with_limits(&excess_context, exact),
        Err(ActionMapFileError::ContextLimitExceeded { maximum: 2 })
    ));
    let mut excess_binding = golden();
    excess_binding["payload"]["bindings"]
        .as_array_mut()
        .unwrap()
        .push(json!(42));
    assert!(matches!(
        decode_value_with_limits(&excess_binding, exact),
        Err(ActionMapFileError::BindingLimitExceeded { maximum: 3 })
    ));
    for limited in [
        exact.with_definition(exact_definition.with_contexts(ItemLimit::ONE)),
        exact.with_definition(exact_definition.with_bindings(ItemLimit::new(2).unwrap())),
    ] {
        assert!(
            ActionMapFileCandidate::decode_json_bytes_with_limits(GOLDEN.as_bytes(), limited)
                .is_err()
        );
        assert!(
            sample_candidate()
                .to_json_string_with_limits(limited)
                .is_err()
        );
    }
}

#[test]
fn identifier_limits_use_decoded_utf8_bytes_and_survive_the_serde_boundary() {
    let mut value = golden();
    value["payload"]["bindings"][0]["action"] = json!("é".repeat(64));
    decode_value(&value).unwrap();
    value["payload"]["bindings"][0]["action"] = json!("é".repeat(4));
    let exact = ActionMapFileLimits::default().with_definition(
        ActionMapLimits::default().with_identifier_bytes(ByteLimit::new(8).unwrap()),
    );
    decode_value_with_limits(&value, exact).unwrap();
    value["payload"]["bindings"][0]["action"] = json!("é".repeat(4) + "x");
    assert!(matches!(
        decode_value_with_limits(&value, exact),
        Err(ActionMapFileError::IdentifierBytesExceeded {
            observed: 9,
            maximum: 8
        })
    ));

    for path in [
        "/payload/contexts/0/context",
        "/payload/bindings/0/action",
        "/payload/bindings/0/context",
    ] {
        let mut value = golden();
        *value.pointer_mut(path).unwrap() = json!("x".repeat(129));
        let defaults = ActionMapFileLimits::default();
        let shape = defaults.shape();
        let limits = defaults.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            ByteLimit::new(256).unwrap(),
            shape.total_string_bytes(),
        ));
        assert!(
            matches!(
                decode_value_with_limits(&value, limits),
                Err(ActionMapFileError::IdentifierBytesExceeded {
                    observed: 129,
                    maximum: 128,
                })
            ),
            "identifier at {path}"
        );
    }
    for (text, expected) in [
        ("", ActionIdError::Empty),
        ("bad\nid", ActionIdError::ContainsControl),
    ] {
        let mut value = golden();
        value["payload"]["bindings"][0]["action"] = json!(text);
        assert!(matches!(
            decode_value(&value),
            Err(ActionMapFileError::Identifier(error)) if error == expected
        ));
    }
}

#[test]
fn physical_input_grammar_rejects_unknown_variants_and_out_of_range_members() {
    for input in [
        json!({ "Gamepad": "South" }),
        json!({ "Key": "UnmappedKey" }),
        json!({ "Mouse": { "Other": 65536 } }),
        json!({ "Key": { "Character": "ab" } }),
        json!({ "Key": "Space", "Mouse": "Left" }),
        json!({ "GamepadButton": 65536 }),
        json!({ "GamepadAxis": -1 }),
        json!({ "GamepadAxis": 0.5 }),
    ] {
        let mut value = golden();
        value["payload"]["bindings"][0]["input"] = input;
        assert!(matches!(
            decode_value(&value),
            Err(ActionMapFileError::JsonParse(_))
        ));
    }
}

#[test]
fn malformed_transforms_and_invalid_dead_zones_are_rejected_during_decode() {
    for transform in [
        Value::Null,
        json!(true),
        json!("Unknown"),
        json!({ "Axis": {} }),
        json!({ "Axis": { "dead_zone": 0.25 } }),
        json!({ "Axis": { "inverted": false } }),
        json!({ "Axis": { "dead_zone": -0.01, "inverted": false } }),
        json!({ "Axis": { "dead_zone": 1.0, "inverted": false } }),
        json!({ "Axis": { "dead_zone": "NaN", "inverted": false } }),
        json!({ "Axis": { "dead_zone": 0.25, "inverted": 0 } }),
        json!({ "Axis": { "dead_zone": 0.25, "inverted": false, "unknown": true } }),
        json!({ "Axis": [0.25, false] }),
        json!({ "AxisButton": {} }),
        json!({ "AxisButton": { "direction": "Sideways" } }),
        json!({ "AxisButton": { "direction": "Positive", "unknown": true } }),
        json!({ "AxisButton": ["Positive"] }),
        json!({ "Axis": { "dead_zone": 0.25, "inverted": false }, "Button": null }),
    ] {
        let mut value = golden();
        value["payload"]["bindings"][0]["input"] = json!({ "GamepadAxis": 0 });
        value["payload"]["bindings"][0]["transform"] = transform.clone();
        assert!(
            matches!(decode_value(&value), Err(ActionMapFileError::JsonParse(_))),
            "malformed transform {transform}"
        );
    }
}

#[test]
fn transform_records_reject_duplicate_members_before_definition_construction() {
    let mut value = golden();
    value["payload"]["bindings"][0]["input"] = json!({ "GamepadAxis": 0 });
    value["payload"]["bindings"][0]["transform"] =
        json!({ "Axis": { "dead_zone": 0.25, "inverted": false } });
    for (path, member) in [
        ("/payload/bindings/0", "transform"),
        ("/payload/bindings/0/transform", "Axis"),
        ("/payload/bindings/0/transform/Axis", "dead_zone"),
        ("/payload/bindings/0/transform/Axis", "inverted"),
    ] {
        let encoded = duplicate_member(&value, path, member);
        assert!(matches!(
            ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()),
            Err(ActionMapFileError::Shape(SerdeShapeError::DuplicateMapKey))
        ));
    }
    value["payload"]["bindings"][0]["input"] = json!({ "Key": "Space" });
    value["payload"]["bindings"][0]["transform"] =
        json!({ "AxisButton": { "direction": "Negative" } });
    let encoded = duplicate_member(
        &value,
        "/payload/bindings/0/transform/AxisButton",
        "direction",
    );
    assert!(matches!(
        ActionMapFileCandidate::decode_json_bytes(encoded.as_bytes()),
        Err(ActionMapFileError::Shape(SerdeShapeError::DuplicateMapKey))
    ));
}

#[test]
fn typed_source_bindings_use_the_same_compiler_validation_as_code_first_bindings() {
    for transform in [
        None,
        Some(json!("Button")),
        Some(json!({ "AxisButton": { "direction": "Positive" } })),
    ] {
        let mut value = golden();
        value["payload"]["bindings"][0]["input"] = json!({ "GamepadAxis": 0 });
        if let Some(transform) = transform {
            value["payload"]["bindings"][0]["transform"] = transform;
        }
        assert!(matches!(
            decode_value(&value),
            Err(ActionMapFileError::Definition(
                ActionMapError::InvalidTransform {
                    input: InputBinding::GamepadAxis(0),
                    ..
                }
            ))
        ));
    }
    let mut mixed = golden();
    mixed["payload"]["bindings"][1]["action"] = json!("jump");
    mixed["payload"]["bindings"][1]["input"] = json!({ "GamepadAxis": 0 });
    mixed["payload"]["bindings"][1]["transform"] =
        json!({ "Axis": { "dead_zone": 0.25, "inverted": false } });
    assert!(matches!(
        decode_value(&mixed),
        Err(ActionMapFileError::Definition(
            ActionMapError::InconsistentValueKind {
                expected: ActionValueKind::Button,
                actual: ActionValueKind::Axis,
                ..
            }
        ))
    ));
    let mut duplicate = golden();
    duplicate["payload"]["bindings"][1] = duplicate["payload"]["bindings"][0].clone();
    duplicate["payload"]["bindings"][1]["transform"] =
        json!({ "AxisButton": { "direction": "Positive" } });
    assert!(matches!(
        decode_value(&duplicate),
        Err(ActionMapFileError::Definition(
            ActionMapError::DuplicateBinding {
                input: InputBinding::Key(KeyCode::Space),
                ..
            }
        ))
    ));
}

#[test]
fn transform_metadata_stays_within_the_original_finite_shape_budget() {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for axis in 0..MAX_ACTION_BINDINGS {
        builder
            .bind(ActionBinding::gamepad_axis(
                ActionId::new("x").unwrap(),
                u16::try_from(axis).unwrap(),
                AxisDeadZone::NONE,
                false,
            ))
            .unwrap();
    }
    let definition = builder.compile().unwrap();
    let candidate = ActionMapFileCandidate::new(product(), definition).unwrap();
    let maximum = ActionMapFileLimits::default().shape().nodes().get();
    assert!(matches!(
        candidate.to_json_string(),
        Err(ActionMapFileError::Shape(SerdeShapeError::NodeLimitExceeded { maximum: observed }))
            if observed == maximum
    ));
    let (identity, definition) = candidate.into_parts();
    let runtime = ActionMapAsset::new(identity, definition).unwrap();
    let cost_error = ActionMapRuntimeCodec::runtime_retained_bytes(&runtime).unwrap_err();
    let encode_error = ActionMapRuntimeCodec::encode_runtime(&runtime).unwrap_err();
    assert_eq!(
        cost_error.diagnostic().code().as_str(),
        "input.runtime-budget"
    );
    assert_eq!(cost_error.diagnostic(), encode_error.diagnostic());
}

#[test]
fn compiler_owns_unknown_context_duplicate_and_fanout_rejections() {
    let mut unknown = golden();
    unknown["payload"]["bindings"][0]["context"] = json!("undeclared");
    assert!(matches!(
        decode_value(&unknown),
        Err(ActionMapFileError::Definition(ActionMapError::UnknownContext(context)))
            if context.as_str() == "undeclared"
    ));
    let mut duplicate_context = golden();
    duplicate_context["payload"]["contexts"][1]["context"] = json!("gameplay");
    assert!(matches!(
        decode_value(&duplicate_context),
        Err(ActionMapFileError::Definition(
            ActionMapError::DuplicateContext(_)
        ))
    ));
    let mut duplicate_binding = golden();
    duplicate_binding["payload"]["bindings"][1] =
        duplicate_binding["payload"]["bindings"][0].clone();
    assert!(matches!(
        decode_value(&duplicate_binding),
        Err(ActionMapFileError::Definition(
            ActionMapError::DuplicateBinding { .. }
        ))
    ));
    let mut fanout = golden();
    fanout["payload"]["bindings"][1]["input"] = json!({ "Key": "Space" });
    let limited = ActionMapFileLimits::default()
        .with_definition(ActionMapLimits::default().with_bindings_per_input(ItemLimit::ONE));
    assert!(matches!(
        decode_value_with_limits(&fanout, limited),
        Err(ActionMapFileError::Definition(
            ActionMapError::LimitExceeded {
                kind: ActionMapLimitKind::BindingsPerInput,
                required: 2,
                limit: 1,
            }
        ))
    ));
}

#[test]
fn complete_primary_identity_is_required_at_file_and_code_first_boundaries() {
    for source in [
        "00000000-0000-0000-0000-000000000000",
        "919617EA-C89C-4F83-9CCE-B084DE435A83",
        "919617eac89c4f839cceb084de435a83",
    ] {
        let mut value = golden();
        value["payload"]["product"]["source"] = json!(source);
        assert!(decode_value(&value).is_err());
    }
    let mut bare_id = golden();
    bare_id["payload"]["product"] = json!(SOURCE);
    assert!(decode_value(&bare_id).is_err());
    let mut nil_auxiliary = golden();
    nil_auxiliary["payload"]["product"]["product"] = json!({
        "kind": "subproduct",
        "id": "00000000-0000-0000-0000-000000000000"
    });
    assert!(decode_value(&nil_auxiliary).is_err());

    let auxiliary =
        ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();
    let auxiliary = AssetProductRef::new(product().source(), auxiliary).unwrap();
    let mut value = golden();
    value["payload"]["product"] = serde_json::to_value(auxiliary).unwrap();
    assert!(matches!(
        decode_value(&value),
        Err(ActionMapFileError::PrimaryProductRequired { product }) if product == auxiliary
    ));
    let (_, definition) = sample_candidate().into_parts();
    assert!(matches!(
        ActionMapFileCandidate::new(auxiliary, definition),
        Err(ActionMapFileError::PrimaryProductRequired { product }) if product == auxiliary
    ));
}

#[test]
fn definition_work_and_retained_limits_are_exact_and_writer_self_validates_them() {
    let candidate = sample_candidate();
    let cost = candidate.definition().cost();
    let exact_definition = ActionMapLimits::default()
        .with_work_bytes(ByteLimit::new(cost.work_bytes()).unwrap())
        .with_retained_bytes(ByteLimit::new(cost.retained_bytes()).unwrap());
    let exact = ActionMapFileLimits::default().with_definition(exact_definition);
    ActionMapFileCandidate::decode_json_bytes_with_limits(GOLDEN.as_bytes(), exact).unwrap();
    candidate.to_json_string_with_limits(exact).unwrap();
    for (definition, kind) in [
        (
            exact_definition.with_work_bytes(ByteLimit::new(cost.work_bytes() - 1).unwrap()),
            ActionMapLimitKind::WorkBytes,
        ),
        (
            exact_definition
                .with_retained_bytes(ByteLimit::new(cost.retained_bytes() - 1).unwrap()),
            ActionMapLimitKind::RetainedBytes,
        ),
    ] {
        let limits = exact.with_definition(definition);
        assert!(matches!(
            ActionMapFileCandidate::decode_json_bytes_with_limits(GOLDEN.as_bytes(), limits),
            Err(ActionMapFileError::Definition(ActionMapError::LimitExceeded {
                kind: actual, ..
            })) if actual == kind
        ));
        assert!(matches!(
            candidate.to_json_string_with_limits(limits),
            Err(ActionMapFileError::Definition(ActionMapError::LimitExceeded {
                kind: actual, ..
            })) if actual == kind
        ));
    }
}

#[test]
fn host_reservation_is_finite_and_overflow_is_explicit() {
    let limits = ActionMapFileLimits::default();
    let required = limits.predecode_work_bytes().unwrap();
    assert!(
        required
            >= limits.encoded_bytes().get()
                + limits.definition().work_bytes().get()
                + limits.definition().retained_bytes().get()
    );
    assert!(
        limits
            .with_encoded_bytes(ByteLimit::new(usize::MAX).unwrap())
            .predecode_work_bytes()
            .is_none()
    );
}

fn product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(SOURCE).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn sample_candidate() -> ActionMapFileCandidate {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    let menu = ActionContext::new("menu").unwrap();
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder.declare_context(menu.clone(), false).unwrap();
    builder
        .bind(ActionBinding::key(
            ActionId::new("jump").unwrap(),
            KeyCode::Space,
        ))
        .unwrap();
    builder
        .bind(ActionBinding::key(
            ActionId::new("move-up").unwrap(),
            KeyCode::Character('w'),
        ))
        .unwrap();
    builder
        .bind(
            ActionBinding::mouse(ActionId::new("confirm").unwrap(), MouseButton::Left)
                .with_context(menu),
        )
        .unwrap();
    ActionMapFileCandidate::new(product(), builder.compile().unwrap()).unwrap()
}

fn golden() -> Value {
    serde_json::from_str(GOLDEN).unwrap()
}

fn canonical_golden() -> String {
    GOLDEN
        .replace("\r\n", "\n")
        .trim_end()
        .replace(
            "\"context\": \"gameplay\"\n",
            "\"context\": \"gameplay\",\n        \"transform\": \"Button\"\n",
        )
        .replace(
            "\"context\": \"menu\"\n",
            "\"context\": \"menu\",\n        \"transform\": \"Button\"\n",
        )
}

fn decode_value(value: &Value) -> Result<ActionMapFileCandidate, ActionMapFileError> {
    decode_value_with_limits(value, ActionMapFileLimits::default())
}

fn decode_value_with_limits(
    value: &Value,
    limits: ActionMapFileLimits,
) -> Result<ActionMapFileCandidate, ActionMapFileError> {
    ActionMapFileCandidate::decode_json_bytes_with_limits(
        &serde_json::to_vec(value).unwrap(),
        limits,
    )
}

fn duplicate_member(value: &Value, path: &str, member: &str) -> String {
    let object = value.pointer(path).unwrap();
    let original = serde_json::to_string(object).unwrap();
    let member_value = serde_json::to_string(&object[member]).unwrap();
    let repeated = format!("{{\"{member}\":{member_value},{}", &original[1..]);
    serde_json::to_string(value)
        .unwrap()
        .replacen(&original, &repeated, 1)
}
