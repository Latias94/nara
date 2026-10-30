#![cfg(feature = "serde")]

use std::{mem::size_of, sync::Arc};

use nara_asset::{
    AssetDomain, AssetProductRef, AssetRuntimeError, ImportedProductId, RuntimeAssetCodec,
    StableAssetId,
};
use nara_core::PersistentFileContractError;
use nara_diagnostic::{Diagnostic, DiagnosticFieldClass, DiagnosticValueRef};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMapAsset, ActionMapBindings, ActionMapBuilder,
    ActionMapDefinition, ActionMapDomain, ActionMapFileCandidate, ActionMapFileError,
    ActionMapFileLimits, ActionMapLimits, ActionMapRuntimeCodec, InputBinding, KeyCode,
    MAX_ACTION_BINDINGS, MAX_ACTION_CONTEXTS, MAX_BINDINGS_PER_INPUT, MouseButton,
};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("fixtures/formats/v1/runtime-action-map.json");
const SOURCE_GOLDEN: &str = include_str!("fixtures/formats/v1/action-map.json");
const SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";

#[test]
fn nonempty_processed_golden_preserves_identity_order_and_canonical_bytes() {
    assert_eq!(ActionMapRuntimeCodec::RUNTIME_KIND, "runtime_action_map");
    assert_eq!(ActionMapRuntimeCodec::RUNTIME_VERSION, 1);
    let asset = ActionMapRuntimeCodec::decode_runtime(GOLDEN.as_bytes()).unwrap();
    assert_eq!(asset.product(), primary(SOURCE));
    assert_eq!(asset.definition().contexts().len(), 2);
    assert_eq!(
        asset.definition().contexts()[0].context.as_str(),
        "gameplay"
    );
    assert!(asset.definition().contexts()[0].enabled);
    assert_eq!(asset.definition().contexts()[1].context.as_str(), "menu");
    assert!(!asset.definition().contexts()[1].enabled);
    assert_eq!(asset.definition().bindings().len(), 3);
    assert_eq!(asset.definition().bindings()[0].action.as_str(), "jump");
    assert_eq!(
        asset.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Space),
    );
    assert_eq!(
        asset.definition().bindings()[1].input,
        InputBinding::Key(KeyCode::Character('w')),
    );
    assert_eq!(
        asset.definition().bindings()[2].input,
        InputBinding::Mouse(MouseButton::Left),
    );
    assert_eq!(asset.definition().bindings()[2].context.as_str(), "menu");
    assert!(std::ptr::eq(asset.as_ref(), asset.definition()));
    assert_eq!(encode(&asset).as_ref(), canonical_golden().as_bytes());
    let (product, definition) = asset.into_parts();
    assert_eq!(product, primary(SOURCE));
    assert_eq!(definition.bindings().len(), 3);
}

#[test]
fn source_preparation_and_code_first_cooking_consume_definitions_and_are_deterministic() {
    let candidate = ActionMapFileCandidate::decode_json_bytes(SOURCE_GOLDEN.as_bytes()).unwrap();
    // Cooking moves the admitted definition; it does not clone a borrowed allocation or patch
    // the source envelope into a processed input.
    let (product, definition) = candidate.into_parts();
    let cooked = ActionMapAsset::new(product, definition).unwrap();
    let code_first = sample_asset();
    assert_eq!(encode(&cooked), encode(&code_first));
    assert_eq!(encode(&code_first).as_ref(), canonical_golden().as_bytes());
    assert_eq!(encode(&code_first), encode(&code_first));
    assert_eq!(cooked.definition().cost(), code_first.definition().cost());

    let owner = Arc::new(cooked);
    let bindings = ActionMapBindings::from_owner(Arc::clone(&owner));
    let same_owner = ActionMapBindings::from_owner(Arc::clone(&owner));
    assert!(bindings.shares_owner_with(&same_owner));
    drop(owner);
    assert_eq!(
        bindings.definition().bindings(),
        code_first.definition().bindings()
    );
}

#[test]
fn source_and_processed_kinds_reject_each_other_before_payload_compilation() {
    assert_eq!(
        rejection_code(SOURCE_GOLDEN.as_bytes()),
        "input.runtime-kind"
    );
    assert!(matches!(
        ActionMapFileCandidate::decode_json_bytes(GOLDEN.as_bytes()),
        Err(ActionMapFileError::Contract(
            PersistentFileContractError::WrongKind { .. }
        )),
    ));
    for (kind, expected) in [
        ("action_map", "input.runtime-kind"),
        ("scene", "input.runtime-kind"),
    ] {
        let mut value = golden();
        value["kind"] = json!(kind);
        value["payload"] = Value::Null;
        assert_rejected(&value, expected);
    }
    for (field, replacement, expected) in [
        ("format_version", json!(2), "input.runtime-version"),
        (
            "engine_min_version",
            json!("9999.0.0"),
            "input.runtime-engine-version",
        ),
    ] {
        let mut value = golden();
        value[field] = replacement;
        value["payload"] = Value::Null;
        assert_rejected(&value, expected);
    }
    let mut zero = golden();
    zero["format_version"] = json!(0);
    assert_rejected(&zero, "input.runtime-metadata");
}

#[test]
fn every_record_is_strict_required_and_object_only() {
    for (path, members) in [
        (
            "",
            &[
                "kind",
                "format_version",
                "engine_min_version",
                "generator",
                "payload",
            ][..],
        ),
        ("/generator", &["name", "version"][..]),
        ("/payload", &["product", "contexts", "bindings"][..]),
        ("/payload/product", &["source", "product"][..]),
        ("/payload/product/product", &["kind"][..]),
        ("/payload/contexts/0", &["context", "enabled"][..]),
        ("/payload/bindings/0", &["action", "input", "context"][..]),
        ("/payload/bindings/0/input", &["Key"][..]),
    ] {
        let mut unknown = golden();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(true));
        assert_rejected(&unknown, "input.runtime-metadata");

        let mut positional = golden();
        let target = positional.pointer_mut(path).unwrap();
        *target = Value::Array(target.as_object().unwrap().values().cloned().collect());
        assert_rejected(&positional, "input.runtime-metadata");

        for member in members {
            let mut missing = golden();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(*member)
                .unwrap();
            assert_rejected(&missing, "input.runtime-metadata");
        }
    }
    for field in [
        "disabled_contexts",
        "revision",
        "held",
        "mapping_lifetime",
        "source_path",
    ] {
        let mut value = golden();
        value["payload"][field] = json!([]);
        assert_rejected(&value, "input.runtime-metadata");
    }
}

#[test]
fn duplicate_members_bare_payloads_and_trailing_values_are_rejected() {
    for (path, field) in [
        ("", "kind"),
        ("/generator", "name"),
        ("/payload", "contexts"),
        ("/payload/product", "source"),
        ("/payload/product/product", "kind"),
        ("/payload/contexts/0", "context"),
        ("/payload/bindings/0", "action"),
        ("/payload/bindings/0/input", "Key"),
    ] {
        let value = golden();
        let record = value.pointer(path).unwrap();
        let original = serde_json::to_string(record).unwrap();
        let repeated = format!("{{\"{field}\":{},{}", record[field], &original[1..]);
        let encoded = serde_json::to_string(&value)
            .unwrap()
            .replacen(&original, &repeated, 1);
        assert_eq!(rejection_code(encoded.as_bytes()), "input.runtime-metadata");
    }
    for bytes in [
        serde_json::to_vec(&golden()["payload"]).unwrap(),
        br#"{"bindings":[],"disabled_contexts":[]}"#.to_vec(),
        b"(bindings: [], disabled_contexts: [])".to_vec(),
        format!("{GOLDEN} {{}}").into_bytes(),
        format!("{GOLDEN} trailing").into_bytes(),
    ] {
        assert!(ActionMapRuntimeCodec::decode_runtime(&bytes).is_err());
    }
}

#[test]
fn member_order_uses_the_same_two_pass_context_compiler() {
    let value = golden();
    let encoded = format!(
        concat!(
            r#"{{"payload":{{"bindings":{},"product":{},"contexts":{}}},"generator":{},"#,
            r#""engine_min_version":"0.1.0","format_version":1,"kind":"runtime_action_map"}}"#,
        ),
        value["payload"]["bindings"],
        value["payload"]["product"],
        value["payload"]["contexts"],
        value["generator"],
    );
    let asset = ActionMapRuntimeCodec::decode_runtime(encoded.as_bytes()).unwrap();
    assert_eq!(encode(&asset).as_ref(), canonical_golden().as_bytes());
}

#[test]
fn complete_primary_identity_and_explicit_empty_required_inventory_are_independent() {
    let asset = sample_asset();
    assert!(ActionMapDomain::contains_product(
        &asset,
        ImportedProductId::PRIMARY
    ));
    assert!(!ActionMapDomain::contains_product(&asset, auxiliary()));
    assert!(ActionMapRuntimeCodec::contains_runtime_product(
        &asset,
        primary(SOURCE)
    ));
    assert!(!ActionMapRuntimeCodec::contains_runtime_product(
        &asset,
        primary("deee1b69-d190-4427-81ca-523051ca7ed1"),
    ));
    let auxiliary = AssetProductRef::new(primary(SOURCE).source(), auxiliary()).unwrap();
    assert!(!ActionMapRuntimeCodec::contains_runtime_product(
        &asset, auxiliary
    ));
    let mut visits = 0;
    let result = ActionMapRuntimeCodec::visit_runtime_dependencies(&asset, |_, _| {
        visits += 1;
        Err("a dependency must never be requested")
    });
    assert_eq!(result, Ok(()));
    assert_eq!(visits, 0);

    let (_, definition) = asset.into_parts();
    assert!(matches!(
        ActionMapAsset::new(auxiliary, definition),
        Err(ActionMapFileError::PrimaryProductRequired { product }) if product == auxiliary,
    ));
    let mut value = golden();
    value["payload"]["product"] = serde_json::to_value(auxiliary).unwrap();
    assert_rejected(&value, "input.runtime-product");
    for product in [
        json!(SOURCE),
        json!({"source": SOURCE}),
        json!({"source": "00000000-0000-0000-0000-000000000000", "product": {"kind": "primary"}}),
        json!({"source": SOURCE.to_uppercase(), "product": {"kind": "primary"}}),
        json!({"source": SOURCE, "product": {"kind": "subproduct", "id": "00000000-0000-0000-0000-000000000000"}}),
    ] {
        value["payload"]["product"] = product;
        assert_rejected(&value, "input.runtime-metadata");
    }
}

#[test]
fn promises_precede_decode_and_charge_inline_asset_and_compiled_storage_once() {
    let defaults = ActionMapFileLimits::default();
    let limits = ActionMapRuntimeCodec::runtime_limits();
    let inline_extra = size_of::<ActionMapAsset>() - size_of::<ActionMapDefinition>();
    assert_eq!(limits.encoded_bytes(), defaults.encoded_bytes());
    assert_eq!(
        limits.work_bytes().get(),
        defaults.predecode_work_bytes().unwrap()
    );
    assert_eq!(
        limits.retained_bytes().get(),
        inline_extra + defaults.definition().retained_bytes().get(),
    );
    let empty = ActionMapAsset::new(
        primary(SOURCE),
        ActionMapBuilder::new(ActionMapLimits::default())
            .compile()
            .unwrap(),
    )
    .unwrap();
    for asset in [empty, sample_asset(), dense_asset("x", 0)] {
        let expected = inline_extra + asset.definition().cost().retained_bytes();
        assert_eq!(
            ActionMapRuntimeCodec::runtime_retained_bytes(&asset).unwrap(),
            expected
        );
        let encoded = encode(&asset);
        let decoded = ActionMapRuntimeCodec::decode_runtime(&encoded).unwrap();
        assert_eq!(
            ActionMapRuntimeCodec::runtime_retained_bytes(&decoded).unwrap(),
            expected
        );
        assert!(expected >= size_of::<ActionMapAsset>());
        assert!(expected <= limits.retained_bytes().get());
    }
}

#[test]
fn encoded_input_cap_is_exact_and_checked_before_domain_decode() {
    let maximum = ActionMapRuntimeCodec::runtime_limits()
        .encoded_bytes()
        .get();
    let mut exact = canonical_golden().into_bytes();
    exact.resize(maximum, b' ');
    ActionMapRuntimeCodec::decode_runtime(&exact).unwrap();
    exact.push(b' ');
    for bytes in [exact, vec![b'!'; maximum + 1]] {
        assert_budget(
            &ActionMapRuntimeCodec::decode_runtime(&bytes).unwrap_err(),
            "encoded-bytes",
            Some(maximum + 1),
            maximum,
        );
    }
}

#[test]
fn maximum_context_and_binding_counts_are_admitted_and_excess_records_reject() {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for index in 1..MAX_ACTION_CONTEXTS {
        builder
            .declare_context(
                ActionContext::new(format!("context-{index}")).unwrap(),
                false,
            )
            .unwrap();
    }
    for index in 0..MAX_ACTION_BINDINGS {
        builder
            .bind(ActionBinding::mouse(
                ActionId::new(format!("action-{index}")).unwrap(),
                MouseButton::Other(u16::try_from(index).unwrap()),
            ))
            .unwrap();
    }
    let asset = ActionMapAsset::new(primary(SOURCE), builder.compile().unwrap()).unwrap();
    let encoded = encode(&asset);
    let decoded = ActionMapRuntimeCodec::decode_runtime(&encoded).unwrap();
    assert_eq!(decoded.definition().contexts().len(), MAX_ACTION_CONTEXTS);
    assert_eq!(decoded.definition().bindings().len(), MAX_ACTION_BINDINGS);
    assert!(
        ActionMapRuntimeCodec::runtime_retained_bytes(&decoded).unwrap()
            <= ActionMapRuntimeCodec::runtime_limits()
                .retained_bytes()
                .get()
    );
    let exact: Value = serde_json::from_slice(&encoded).unwrap();
    for (member, kind, maximum) in [
        ("contexts", "contexts", MAX_ACTION_CONTEXTS),
        (
            "bindings",
            "shape-container-items",
            ActionMapFileLimits::default()
                .shape()
                .container_items()
                .get(),
        ),
    ] {
        let mut excess = exact.clone();
        excess["payload"][member]
            .as_array_mut()
            .unwrap()
            .push(json!(42));
        assert_budget(
            &decode_value(&excess).unwrap_err(),
            kind,
            Some(maximum + 1),
            maximum,
        );
    }
}

#[test]
fn sixty_five_valid_context_records_preserve_the_streaming_domain_budget() {
    let mut value = golden();
    value["payload"]["bindings"] = json!([]);
    value["payload"]["contexts"] = json!(
        (0..=MAX_ACTION_CONTEXTS)
            .map(|index| json!({
                "context": format!("private-context-{index}"), "enabled": true,
            }))
            .collect::<Vec<_>>()
    );
    let error = decode_value(&value).unwrap_err();
    assert_budget(&error, "contexts", Some(65), 64);
    assert!(!format!("{error:?}").contains("private-context"));
}

#[test]
fn one_compiler_rejects_unknown_duplicate_and_excess_fanout_bindings() {
    let mut unknown = golden();
    unknown["payload"]["bindings"][0]["context"] = json!("undeclared");
    assert_rejected(&unknown, "input.runtime-definition");
    let mut context = golden();
    context["payload"]["contexts"][1]["context"] = json!("gameplay");
    assert_rejected(&context, "input.runtime-definition");
    let mut duplicate = golden();
    duplicate["payload"]["bindings"][1] = duplicate["payload"]["bindings"][0].clone();
    assert_rejected(&duplicate, "input.runtime-definition");
    let mut fanout = golden();
    fanout["payload"]["bindings"] = json!((0..MAX_BINDINGS_PER_INPUT).map(|index| json!({
        "action": format!("action-{index}"), "context": "gameplay", "input": {"Key": "Space"},
    })).collect::<Vec<_>>());
    decode_value(&fanout).unwrap();
    fanout["payload"]["bindings"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "action": "excess", "context": "gameplay", "input": {"Key": "Space"},
        }));
    assert_budget(
        &decode_value(&fanout).unwrap_err(),
        "bindings-per-input",
        Some(MAX_BINDINGS_PER_INPUT + 1),
        MAX_BINDINGS_PER_INPUT,
    );
}

#[test]
fn shape_and_identifier_limits_reject_before_returning_a_product() {
    for action in ["", "bad\nid"] {
        let mut value = golden();
        value["payload"]["bindings"][0]["action"] = json!(action);
        assert_rejected(&value, "input.runtime-metadata");
    }
    let maximum = ActionMapFileLimits::default().shape().string_bytes().get();
    let mut identifier = golden();
    identifier["payload"]["bindings"][0]["action"] = json!("x".repeat(maximum));
    decode_value(&identifier).unwrap();
    identifier["payload"]["bindings"][0]["action"] = json!("x".repeat(maximum + 1));
    // Structural preflight owns this refusal and does not carry the observed byte count.
    assert_budget(
        &decode_value(&identifier).unwrap_err(),
        "shape-string-bytes",
        None,
        maximum,
    );
    for input in [
        json!({"Gamepad": "South"}),
        json!({"Key": "UnmappedKey"}),
        json!({"Mouse": {"Other": 65536}}),
        json!({"Key": {"Character": "ab"}}),
        json!({"Key": "Space", "Mouse": "Left"}),
    ] {
        let mut value = golden();
        value["payload"]["bindings"][0]["input"] = input;
        assert_rejected(&value, "input.runtime-metadata");
    }
    let mut deep = golden();
    let mut nested = json!(true);
    for _ in 0..ActionMapFileLimits::default().shape().depth().get() {
        nested = json!([nested]);
    }
    deep["payload"] = nested;
    let maximum = ActionMapFileLimits::default().shape().depth().get();
    assert_budget(
        &decode_value(&deep).unwrap_err(),
        "shape-depth",
        Some(maximum + 1),
        maximum,
    );
}

#[test]
fn shape_node_limit_reports_the_first_excess_node_without_exposing_payload_data() {
    let limits = ActionMapFileLimits::default().shape();
    let mut value = golden();
    let width = limits.container_items().get();
    let rows = limits.nodes().get() / width + 1;
    value["payload"] = json!(vec![vec![false; width]; rows]);
    let maximum = limits.nodes().get();
    assert_budget(
        &decode_value(&value).unwrap_err(),
        "shape-nodes",
        Some(maximum + 1),
        maximum,
    );
}

#[test]
fn syntax_duplicate_keys_and_semantic_errors_keep_static_non_budget_diagnostics() {
    let syntax = br#"{"kind":"runtime_action_map","private-diagnostic-canary":[}"#;
    let error = ActionMapRuntimeCodec::decode_runtime(syntax).unwrap_err();
    assert_eq!(error.diagnostic().code().as_str(), "input.runtime-metadata");
    assert!(error.diagnostic().fields().is_empty());
    assert!(!format!("{error:?}").contains("private-diagnostic-canary"));

    let duplicate = GOLDEN.replacen(
        "\"kind\":",
        "\"private-diagnostic-canary\":true,\"private-diagnostic-canary\":false,\"kind\":",
        1,
    );
    let error = ActionMapRuntimeCodec::decode_runtime(duplicate.as_bytes()).unwrap_err();
    assert_eq!(error.diagnostic().code().as_str(), "input.runtime-metadata");
    assert!(error.diagnostic().fields().is_empty());
    assert!(!format!("{error:?}").contains("private-diagnostic-canary"));

    let mut unknown = golden();
    unknown["payload"]["bindings"][0]["context"] = json!("private-diagnostic-canary");
    let error = decode_value(&unknown).unwrap_err();
    assert_eq!(
        error.diagnostic().code().as_str(),
        "input.runtime-definition"
    );
    assert!(error.diagnostic().fields().is_empty());
    assert!(!format!("{error:?}").contains("private-diagnostic-canary"));
}

#[test]
fn code_first_cost_query_rejects_default_wire_caps_before_encoding() {
    let strings = dense_asset(&"x".repeat(124), 0);
    let baseline_wire: Value = serde_json::from_slice(&encode(&dense_asset("x", 0))).unwrap();
    let required = string_bytes(&baseline_wire) + MAX_ACTION_BINDINGS * 123;
    let maximum = ActionMapFileLimits::default()
        .shape()
        .total_string_bytes()
        .get();
    for error in [
        ActionMapRuntimeCodec::runtime_retained_bytes(&strings).unwrap_err(),
        ActionMapRuntimeCodec::encode_runtime(&strings).unwrap_err(),
    ] {
        assert_budget(&error, "shape-total-string-bytes", Some(required), maximum);
    }

    // The decoded string budget fits, but escaped JSON exceeds the encoded budget. Counting
    // uses the borrowed writer and refuses this value before allocating encoded output.
    let escaped = dense_asset(&"\"".repeat(82), 0);
    let cost = ActionMapRuntimeCodec::runtime_retained_bytes(&escaped).unwrap_err();
    let encoded = ActionMapRuntimeCodec::encode_runtime(&escaped).unwrap_err();
    assert_eq!(cost.diagnostic(), encoded.diagnostic());
    let Some(DiagnosticValueRef::Unsigned(required)) =
        diagnostic_field(cost.diagnostic(), "required")
    else {
        panic!("the borrowed counter must retain its known attempted byte count")
    };
    let maximum = ActionMapRuntimeCodec::runtime_limits()
        .encoded_bytes()
        .get();
    assert!(required > u64::try_from(maximum).unwrap());
    assert_budget(
        &cost,
        "encoded-bytes",
        Some(usize::try_from(required).unwrap()),
        maximum,
    );
    assert_eq!(
        encode(&sample_asset()).as_ref(),
        canonical_golden().as_bytes()
    );
}

#[test]
fn canonical_total_string_budget_is_exact_for_borrowed_cost_and_decode() {
    let baseline = dense_asset(&"x".repeat(87), 0);
    let baseline_wire: Value = serde_json::from_slice(&encode(&baseline)).unwrap();
    let maximum = ActionMapFileLimits::default()
        .shape()
        .total_string_bytes()
        .get();
    let extra = maximum - string_bytes(&baseline_wire);
    assert!(extra < MAX_ACTION_BINDINGS);
    let exact = dense_asset(&"x".repeat(87), extra);
    let encoded = encode(&exact);
    assert_eq!(
        string_bytes(&serde_json::from_slice(&encoded).unwrap()),
        maximum
    );
    let decoded = ActionMapRuntimeCodec::decode_runtime(&encoded).unwrap();
    assert_eq!(
        ActionMapRuntimeCodec::runtime_retained_bytes(&exact).unwrap(),
        ActionMapRuntimeCodec::runtime_retained_bytes(&decoded).unwrap(),
    );
    let excess = dense_asset(&"x".repeat(87), extra + 1);
    assert_budget(
        &ActionMapRuntimeCodec::runtime_retained_bytes(&excess).unwrap_err(),
        "shape-total-string-bytes",
        Some(maximum + 1),
        maximum,
    );
    let mut wire: Value = serde_json::from_slice(&encoded).unwrap();
    let action = format!(
        "{}x",
        wire["payload"]["bindings"][0]["action"].as_str().unwrap()
    );
    wire["payload"]["bindings"][0]["action"] = json!(action);
    assert_budget(
        &decode_value(&wire).unwrap_err(),
        "shape-total-string-bytes",
        None,
        maximum,
    );
}

fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap()
}

fn sample_asset() -> ActionMapAsset {
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
    ActionMapAsset::new(primary(SOURCE), builder.compile().unwrap()).unwrap()
}

fn dense_asset(padding: &str, extra: usize) -> ActionMapAsset {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for index in 0..MAX_ACTION_BINDINGS {
        let suffix = if index < extra { "x" } else { "" };
        builder
            .bind(ActionBinding::mouse(
                ActionId::new(format!("{index:04}{padding}{suffix}")).unwrap(),
                MouseButton::Other(u16::try_from(index).unwrap()),
            ))
            .unwrap();
    }
    ActionMapAsset::new(primary(SOURCE), builder.compile().unwrap()).unwrap()
}

fn golden() -> Value {
    serde_json::from_str(GOLDEN).unwrap()
}

fn canonical_golden() -> String {
    GOLDEN.replace("\r\n", "\n").trim_end().to_owned()
}

fn encode(asset: &ActionMapAsset) -> Box<[u8]> {
    ActionMapRuntimeCodec::encode_runtime(asset).unwrap()
}

fn decode_value(value: &Value) -> Result<ActionMapAsset, AssetRuntimeError> {
    ActionMapRuntimeCodec::decode_runtime(&serde_json::to_vec(value).unwrap())
}

fn error_code(error: AssetRuntimeError) -> String {
    error.diagnostic().code().as_str().to_owned()
}

fn rejection_code(bytes: &[u8]) -> String {
    error_code(ActionMapRuntimeCodec::decode_runtime(bytes).unwrap_err())
}

fn assert_rejected(value: &Value, code: &str) {
    let error = decode_value(value).unwrap_err();
    assert_eq!(error.diagnostic().code().as_str(), code);
    assert!(error.diagnostic().fields().is_empty());
}

fn diagnostic_field<'a>(diagnostic: &'a Diagnostic, key: &str) -> Option<DiagnosticValueRef<'a>> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .map(|field| field.value())
}

fn assert_budget(error: &AssetRuntimeError, kind: &str, required: Option<usize>, limit: usize) {
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code().as_str(), "input.runtime-budget");
    assert_eq!(
        diagnostic.summary().as_str(),
        "Runtime ActionMap exceeds its finite codec budget",
    );
    assert_eq!(
        diagnostic.fields().len(),
        if required.is_some() { 3 } else { 2 }
    );
    assert!(
        diagnostic
            .fields()
            .iter()
            .all(|field| field.class() == DiagnosticFieldClass::Public)
    );
    assert_eq!(
        diagnostic_field(diagnostic, "budget-kind"),
        Some(DiagnosticValueRef::Identifier(kind)),
    );
    assert_eq!(
        diagnostic_field(diagnostic, "limit"),
        Some(DiagnosticValueRef::Unsigned(u64::try_from(limit).unwrap())),
    );
    assert_eq!(
        diagnostic_field(diagnostic, "required"),
        required.map(|value| DiagnosticValueRef::Unsigned(u64::try_from(value).unwrap())),
    );
}

fn string_bytes(value: &Value) -> usize {
    match value {
        Value::String(value) => value.len(),
        Value::Array(values) => values.iter().map(string_bytes).sum(),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| key.len() + string_bytes(value))
            .sum(),
        _ => 0,
    }
}
