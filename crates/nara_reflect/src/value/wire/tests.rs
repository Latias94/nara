//! Compatibility cases for the existing tagged wire and untagged payload classification.
mod scalars;

use serde::Deserialize;

use crate::{ComponentFloat, ComponentValue};

#[test]
fn direct_ron_container_children_keep_buffered_tuple_and_brace_map_forms() {
    assert_decodes_in_both_orders(
        "list",
        r#"[["null",null]]"#,
        r#"[("null",())]"#,
        &ComponentValue::List(vec![ComponentValue::Null]),
    );
    assert_decodes_in_both_orders(
        "map",
        r#"{"child":{"type":"null"}}"#,
        r#"{"child":{"type":"null"}}"#,
        &ComponentValue::map([("child", ComponentValue::Null)]),
    );
    assert_parent_buffered(
        r#"{"type":"list","value":[["null",null]]}"#,
        r#"(type:"list",value:[("null",())])"#,
        &ComponentValue::List(vec![ComponentValue::Null]),
    );
    assert_parent_buffered(
        r#"{"type":"map","value":{"child":{"type":"null"}}}"#,
        r#"(type:"map",value:{"child":{"type":"null"}})"#,
        &ComponentValue::map([("child", ComponentValue::Null)]),
    );
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "envelope", rename_all = "snake_case")]
enum InternallyTagged {
    Component { value: ComponentValue },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "envelope", content = "content", rename_all = "snake_case")]
enum AdjacentlyTagged {
    Component(ComponentValue),
}

fn assert_decodes_in_both_orders(
    kind: &str,
    json_payload: &str,
    ron_payload: &str,
    expected: &ComponentValue,
) {
    for wire in [
        format!(r#"{{"type":"{kind}","value":{json_payload}}}"#),
        format!(r#"{{"value":{json_payload},"type":"{kind}"}}"#),
    ] {
        let actual = serde_json::from_str::<ComponentValue>(&wire)
            .unwrap_or_else(|error| panic!("{wire}: {error}"));
        assert_eq!(&actual, expected, "{wire}");
    }
    for wire in [
        format!(r#"(type:"{kind}",value:{ron_payload})"#),
        format!(r#"(value:{ron_payload},type:"{kind}")"#),
    ] {
        let actual = ron::from_str::<ComponentValue>(&wire)
            .unwrap_or_else(|error| panic!("{wire}: {error}"));
        assert_eq!(&actual, expected, "{wire}");
    }
}

fn assert_rejects_in_both_orders(kind: &str, json_payload: &str, ron_payload: &str) {
    for wire in [
        format!(r#"{{"type":"{kind}","value":{json_payload}}}"#),
        format!(r#"{{"value":{json_payload},"type":"{kind}"}}"#),
    ] {
        assert!(
            serde_json::from_str::<ComponentValue>(&wire).is_err(),
            "{wire}"
        );
    }
    for wire in [
        format!(r#"(type:"{kind}",value:{ron_payload})"#),
        format!(r#"(value:{ron_payload},type:"{kind}")"#),
    ] {
        assert!(ron::from_str::<ComponentValue>(&wire).is_err(), "{wire}");
    }
}

fn assert_parent_buffered(json: &str, ron: &str, expected: &ComponentValue) {
    for wire in [
        format!(r#"{{"envelope":"component","value":{json}}}"#),
        format!(r#"{{"value":{json},"envelope":"component"}}"#),
    ] {
        assert_eq!(
            serde_json::from_str::<InternallyTagged>(&wire).unwrap(),
            InternallyTagged::Component {
                value: expected.clone(),
            },
            "{wire}"
        );
    }
    for wire in [
        format!(r#"(envelope:"component",value:{ron})"#),
        format!(r#"(value:{ron},envelope:"component")"#),
    ] {
        assert_eq!(
            ron::from_str::<InternallyTagged>(&wire).unwrap(),
            InternallyTagged::Component {
                value: expected.clone(),
            },
            "{wire}"
        );
    }
    for wire in [
        format!(r#"{{"envelope":"component","content":{json}}}"#),
        format!(r#"{{"content":{json},"envelope":"component"}}"#),
    ] {
        assert_eq!(
            serde_json::from_str::<AdjacentlyTagged>(&wire).unwrap(),
            AdjacentlyTagged::Component(expected.clone()),
            "{wire}"
        );
    }
    for wire in [
        format!(r#"(envelope:component,content:{ron})"#),
        format!(r#"(content:{ron},envelope:component)"#),
    ] {
        assert_eq!(
            ron::from_str::<AdjacentlyTagged>(&wire).unwrap(),
            AdjacentlyTagged::Component(expected.clone()),
            "{wire}"
        );
    }
}

#[test]
fn canonical_tags_and_payload_shapes_are_stable() {
    for (expected, kind, json_payload, ron_payload) in [
        (ComponentValue::Bool(true), "bool", "true", "true"),
        (ComponentValue::Bool(false), "bool", "false", "false"),
        (ComponentValue::I64(-7), "i64", "-7", "-7"),
        (ComponentValue::U64(7), "u64", "7", "7"),
        (ComponentValue::f64(1.25).unwrap(), "f64", "1.25", "1.25"),
        (
            ComponentValue::String("hello".into()),
            "string",
            r#""hello""#,
            r#""hello""#,
        ),
        (
            ComponentValue::List(vec![ComponentValue::Bool(true), ComponentValue::Null]),
            "list",
            r#"[{"type":"bool","value":true},{"type":"null"}]"#,
            r#"[(type:"bool",value:true),(type:"null")]"#,
        ),
        (
            ComponentValue::map([("z", ComponentValue::Null), ("a", ComponentValue::I64(1))]),
            "map",
            r#"{"a":{"type":"i64","value":1},"z":{"type":"null"}}"#,
            r#"{"a":(type:"i64",value:1),"z":(type:"null")}"#,
        ),
        (
            ComponentValue::EntityReference(
                "11111111-1111-4111-8111-111111111111".parse().unwrap(),
            ),
            "entity_ref",
            r#""11111111-1111-4111-8111-111111111111""#,
            r#""11111111-1111-4111-8111-111111111111""#,
        ),
    ] {
        assert_eq!(
            serde_json::to_string(&expected).unwrap(),
            format!(r#"{{"type":"{kind}","value":{json_payload}}}"#)
        );
        assert_eq!(
            ron::to_string(&expected).unwrap(),
            format!(r#"(type:"{kind}",value:{ron_payload})"#)
        );
        assert_decodes_in_both_orders(kind, json_payload, ron_payload, &expected);
    }
}

#[test]
fn null_accepts_missing_or_explicit_unit_and_serializes_without_value() {
    assert_eq!(
        serde_json::to_string(&ComponentValue::Null).unwrap(),
        r#"{"type":"null"}"#
    );
    assert_eq!(
        ron::to_string(&ComponentValue::Null).unwrap(),
        r#"(type:"null")"#
    );
    assert_eq!(
        serde_json::from_str::<ComponentValue>(r#"{"type":"null"}"#).unwrap(),
        ComponentValue::Null
    );
    assert_eq!(
        ron::from_str::<ComponentValue>(r#"(type:"null")"#).unwrap(),
        ComponentValue::Null
    );
    assert_decodes_in_both_orders("null", "null", "()", &ComponentValue::Null);
    assert_parent_buffered(
        r#"{"value":null,"type":"null"}"#,
        r#"(value:(),type:"null")"#,
        &ComponentValue::Null,
    );
}

#[test]
fn other_tags_require_a_present_non_null_payload() {
    for kind in [
        "bool",
        "i64",
        "u64",
        "f64",
        "string",
        "list",
        "map",
        "entity_ref",
    ] {
        let json = format!(r#"{{"type":"{kind}"}}"#);
        let ron = format!(r#"(type:"{kind}")"#);
        assert!(
            serde_json::from_str::<ComponentValue>(&json).is_err(),
            "{json}"
        );
        assert!(ron::from_str::<ComponentValue>(&ron).is_err(), "{ron}");
        assert_rejects_in_both_orders(kind, "null", "()");
    }
    assert!(ron::from_str::<ComponentValue>(r#"(type:"null",value:None)"#).is_err());
}

#[test]
fn empty_containers_and_strings_remain_distinct_from_null() {
    for (kind, json, ron, expected) in [
        ("list", "[]", "[]", ComponentValue::List(Vec::new())),
        ("map", "{}", "{}", ComponentValue::Map(Default::default())),
        (
            "string",
            r#""""#,
            r#""""#,
            ComponentValue::String(String::new()),
        ),
    ] {
        assert_decodes_in_both_orders(kind, json, ron, &expected);
        assert_rejects_in_both_orders("null", json, ron);
    }
}

#[test]
fn escaped_unicode_strings_round_trip_in_both_field_orders() {
    let expected = ComponentValue::String("line\n\"\\雪\0".into());
    let ComponentValue::String(text) = &expected else {
        unreachable!();
    };
    assert_decodes_in_both_orders(
        "string",
        &serde_json::to_string(text).unwrap(),
        &ron::to_string(text).unwrap(),
        &expected,
    );
}

#[test]
fn nested_maps_and_lists_accept_reordered_fields_at_each_level() {
    let expected = ComponentValue::map([
        ("label", ComponentValue::String("root".into())),
        (
            "items",
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::map([
                    ("flag", ComponentValue::Bool(false)),
                    ("number", ComponentValue::U64(u64::MAX)),
                ]),
            ]),
        ),
    ]);
    let json_payload = r#"{
        "label":{"value":"root","type":"string"},
        "items":{"value":[
            {"value":null,"type":"null"},
            {"value":{
                "number":{"value":18446744073709551615,"type":"u64"},
                "flag":{"type":"bool","value":false}
            },"type":"map"}
        ],"type":"list"}
    }"#;
    let ron_payload = r#"{
        "label":(value:"root",type:"string"),
        "items":(value:[
            (value:(),type:"null"),
            (value:{
                "number":(value:18446744073709551615,type:"u64"),
                "flag":(type:"bool",value:false)
            },type:"map")
        ],type:"list")
    }"#;
    assert_decodes_in_both_orders("map", json_payload, ron_payload, &expected);
    assert_parent_buffered(
        &format!(r#"{{"value":{json_payload},"type":"map"}}"#),
        &format!(r#"(value:{ron_payload},type:"map")"#),
        &expected,
    );
}

#[test]
fn parent_buffers_preserve_canonical_nested_values_and_numeric_edges() {
    let expected = ComponentValue::List(vec![
        ComponentValue::I64(i64::MIN),
        ComponentValue::U64(u64::MAX),
        ComponentValue::f64(1.25).unwrap(),
        ComponentValue::map([("leaf", ComponentValue::String("nested".into()))]),
    ]);
    assert_parent_buffered(
        &serde_json::to_string(&expected).unwrap(),
        &ron::to_string(&expected).unwrap(),
        &expected,
    );
}

#[test]
fn ron_list_payloads_accept_sequences_and_tuples() {
    let expected = ComponentValue::List(vec![ComponentValue::Null, ComponentValue::Bool(true)]);
    let json_payload = r#"[{"type":"null"},{"type":"bool","value":true}]"#;
    for ron_payload in [
        r#"[(type:"null"),(type:"bool",value:true)]"#,
        r#"((type:"null"),(type:"bool",value:true))"#,
    ] {
        assert_decodes_in_both_orders("list", json_payload, ron_payload, &expected);
        assert_parent_buffered(
            &format!(r#"{{"type":"list","value":{json_payload}}}"#),
            &format!(r#"(type:"list",value:{ron_payload})"#),
            &expected,
        );
    }
    assert_rejects_in_both_orders("list", "null", "()");
}

#[test]
fn ron_map_payloads_accept_quoted_maps_and_struct_fields() {
    let expected = ComponentValue::map([("foo", ComponentValue::Null)]);
    let json_payload = r#"{"foo":{"type":"null"}}"#;
    for ron_payload in [r#"{"foo":(type:"null")}"#, r#"(foo:(type:"null"))"#] {
        assert_decodes_in_both_orders("map", json_payload, ron_payload, &expected);
        assert_parent_buffered(
            &format!(r#"{{"type":"map","value":{json_payload}}}"#),
            &format!(r#"(type:"map",value:{ron_payload})"#),
            &expected,
        );
    }
    assert_rejects_in_both_orders("map", "null", "()");
}

#[test]
fn outer_struct_sequences_accept_missing_null_and_nested_payloads_in_json() {
    for (wire, expected) in [
        (r#"["null"]"#, ComponentValue::Null),
        (r#"["null",null]"#, ComponentValue::Null),
        (r#"["bool",true]"#, ComponentValue::Bool(true)),
        (
            r#"["list",[["null"],["u64",18446744073709551615]]]"#,
            ComponentValue::List(vec![ComponentValue::Null, ComponentValue::U64(u64::MAX)]),
        ),
        (
            r#"["map",{"child":["i64",-1]}]"#,
            ComponentValue::map([("child", ComponentValue::I64(-1))]),
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<ComponentValue>(wire).unwrap(),
            expected,
            "{wire}"
        );
    }
    for wire in ["[]", r#"["bool"]"#, r#"["null",null,false]"#] {
        assert!(
            serde_json::from_str::<ComponentValue>(wire).is_err(),
            "{wire}"
        );
    }
}

/// RON tuple input reaches the outer struct's sequence visitor after a parent buffers it.
#[test]
fn buffered_ron_outer_struct_sequences_preserve_nested_payloads() {
    let expected = ComponentValue::List(vec![ComponentValue::Null, ComponentValue::Bool(true)]);
    let payload = r#"("list",[("null",()),("bool",true)])"#;
    for wire in [
        format!(r#"(envelope:"component",value:{payload})"#),
        format!(r#"(value:{payload},envelope:"component")"#),
    ] {
        assert_eq!(
            ron::from_str::<InternallyTagged>(&wire).unwrap(),
            InternallyTagged::Component {
                value: expected.clone(),
            },
            "{wire}"
        );
    }
    let wire = format!(r#"(content:{payload},envelope:component)"#);
    assert_eq!(
        ron::from_str::<AdjacentlyTagged>(&wire).unwrap(),
        AdjacentlyTagged::Component(expected)
    );
}

#[test]
fn signed_and_unsigned_integer_boundaries_are_exact() {
    for value in [i64::MIN, -1, 0, i64::MAX] {
        let payload = value.to_string();
        assert_decodes_in_both_orders("i64", &payload, &payload, &ComponentValue::I64(value));
    }
    for value in [0, 1, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
        let payload = value.to_string();
        assert_decodes_in_both_orders("u64", &payload, &payload, &ComponentValue::U64(value));
    }
}

/// Small unsigned tokens match Signed first; f64 tags also accept integer casts with rounding.
#[test]
fn untagged_numeric_order_keeps_integer_to_unsigned_and_float_coercions() {
    for value in [0, 7, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
        let payload = value.to_string();
        assert_decodes_in_both_orders("u64", &payload, &payload, &ComponentValue::U64(value));
        assert_decodes_in_both_orders(
            "f64",
            &payload,
            &payload,
            &ComponentValue::f64(value as f64).unwrap(),
        );
    }
    for value in [i64::MIN, -7, 9_007_199_254_740_993] {
        let payload = value.to_string();
        assert_decodes_in_both_orders(
            "f64",
            &payload,
            &payload,
            &ComponentValue::f64(value as f64).unwrap(),
        );
    }
}

#[test]
fn ron_numeric_tokens_keep_untagged_integer_classification_before_scalar_conversion() {
    for (kind, json, ron, expected) in [
        ("f64", "255", "0xff", ComponentValue::f64(255.0).unwrap()),
        (
            "f64",
            "1000",
            "1_000",
            ComponentValue::f64(1_000.0).unwrap(),
        ),
        ("u64", "0", "-0", ComponentValue::U64(0)),
        ("u64", "1", "+1", ComponentValue::U64(1)),
    ] {
        assert_decodes_in_both_orders(kind, json, ron, &expected);
    }
}

#[test]
fn integer_tags_reject_overflow_negative_unsigned_and_float_tokens() {
    for (kind, payload) in [
        ("i64", "9223372036854775808"),
        ("i64", "18446744073709551615"),
        ("i64", "-9223372036854775809"),
        ("i64", "18446744073709551616"),
        ("u64", "-1"),
        ("u64", "-9223372036854775808"),
        ("u64", "18446744073709551616"),
        ("i64", "1.0"),
        ("u64", "1.0"),
        ("i64", "1e0"),
        ("u64", "1e0"),
        ("i64", "-0.0"),
        ("u64", "-0.0"),
    ] {
        assert_rejects_in_both_orders(kind, payload, payload);
    }
}

/// Out-of-range integer tokens fall back to a finite float before the payload tag is checked.
#[test]
fn float_tag_accepts_finite_overflowing_integer_tokens() {
    for (payload, value) in [
        ("18446744073709551616", 18_446_744_073_709_551_616.0),
        ("-9223372036854775809", -9_223_372_036_854_775_808.0),
        ("1e3", 1_000.0),
        ("-2.5", -2.5),
        ("1.7976931348623157e308", f64::MAX),
    ] {
        assert_decodes_in_both_orders(
            "f64",
            payload,
            payload,
            &ComponentValue::f64(value).unwrap(),
        );
    }
}

#[test]
fn float_wire_preserves_signed_zero_bits() {
    for (payload, expected) in [("0", 0.0_f64), ("0.0", 0.0), ("-0.0", -0.0)] {
        for wire in [
            format!(r#"{{"type":"f64","value":{payload}}}"#),
            format!(r#"{{"value":{payload},"type":"f64"}}"#),
        ] {
            let ComponentValue::F64(actual) = serde_json::from_str(&wire).unwrap() else {
                panic!("expected f64: {wire}");
            };
            assert_eq!(actual.get().to_bits(), expected.to_bits(), "{wire}");
        }
        for wire in [
            format!(r#"(type:"f64",value:{payload})"#),
            format!(r#"(value:{payload},type:"f64")"#),
        ] {
            let ComponentValue::F64(actual) = ron::from_str(&wire).unwrap() else {
                panic!("expected f64: {wire}");
            };
            assert_eq!(actual.get().to_bits(), expected.to_bits(), "{wire}");
        }
    }
    let negative_zero = ComponentValue::F64(ComponentFloat::new(-0.0).unwrap());
    assert_eq!(
        serde_json::to_string(&negative_zero).unwrap(),
        r#"{"type":"f64","value":-0.0}"#
    );
    assert_eq!(
        ron::to_string(&negative_zero).unwrap(),
        r#"(type:"f64",value:-0.0)"#
    );
}

/// JSON parses bare -0 as a float; RON parses it as signed integer zero.
#[test]
fn bare_negative_zero_keeps_format_specific_payload_classification() {
    for kind in ["i64", "u64", "f64"] {
        for wire in [
            format!(r#"{{"type":"{kind}","value":-0}}"#),
            format!(r#"{{"value":-0,"type":"{kind}"}}"#),
        ] {
            let actual = serde_json::from_str::<ComponentValue>(&wire);
            if kind == "f64" {
                let ComponentValue::F64(value) = actual.unwrap() else {
                    panic!("expected f64: {wire}");
                };
                assert_eq!(value.get().to_bits(), (-0.0_f64).to_bits(), "{wire}");
            } else {
                assert!(actual.is_err(), "{wire}");
            }
        }
        for wire in [
            format!(r#"(type:"{kind}",value:-0)"#),
            format!(r#"(value:-0,type:"{kind}")"#),
        ] {
            let actual = ron::from_str::<ComponentValue>(&wire).unwrap();
            match kind {
                "i64" => assert_eq!(actual, ComponentValue::I64(0), "{wire}"),
                "u64" => assert_eq!(actual, ComponentValue::U64(0), "{wire}"),
                _ => {
                    let ComponentValue::F64(value) = actual else {
                        panic!("expected f64: {wire}");
                    };
                    assert_eq!(value.get().to_bits(), 0.0_f64.to_bits(), "{wire}");
                }
            }
        }
    }
}

#[test]
fn non_finite_or_overflowing_floats_are_rejected() {
    for payload in ["1e999", "-1e999"] {
        for kind in ["i64", "u64", "f64"] {
            assert_rejects_in_both_orders(kind, payload, payload);
        }
    }
    for payload in ["NaN", "inf", "-inf"] {
        assert_rejects_in_both_orders("f64", &format!(r#""{payload}""#), payload);
    }
}

#[test]
fn tags_reject_mismatched_payload_kinds() {
    for (kind, json, ron) in [
        ("null", "false", "false"),
        ("null", "0", "0"),
        ("bool", "1", "1"),
        ("i64", r#""1""#, r#""1""#),
        ("u64", "true", "true"),
        ("f64", r#""1.0""#, r#""1.0""#),
        ("string", "1", "1"),
        ("list", "{}", "{}"),
        ("map", "[]", "[]"),
        ("entity_ref", r#""target""#, r#""target""#),
        ("map", r#"{"untyped":true}"#, r#"{"untyped":true}"#),
    ] {
        assert_rejects_in_both_orders(kind, json, ron);
    }
}

#[test]
fn map_entries_keep_existing_last_duplicate_key_semantics() {
    assert_decodes_in_both_orders(
        "map",
        r#"{"key":{"type":"i64","value":1},"key":{"value":2,"type":"i64"}}"#,
        r#"{"key":(type:"i64",value:1),"key":(value:2,type:"i64")}"#,
        &ComponentValue::map([("key", ComponentValue::I64(2))]),
    );
}

#[test]
fn wire_rejects_duplicate_unknown_missing_or_invalid_fields() {
    for (json, ron) in [
        ("{}", "()"),
        (r#"{"value":true}"#, "(value:true)"),
        (r#"{"type":1,"value":true}"#, "(type:1,value:true)"),
        (r#"{"type":null}"#, "(type:())"),
        (
            r#"{"type":"unknown","value":true}"#,
            r#"(type:"unknown",value:true)"#,
        ),
        (
            r#"{"type":"Bool","value":true}"#,
            r#"(type:"Bool",value:true)"#,
        ),
        (
            r#"{"type":"bool","value":true,"extra":false}"#,
            r#"(type:"bool",value:true,extra:false)"#,
        ),
        (
            r#"{"extra":false,"value":true,"type":"bool"}"#,
            r#"(extra:false,value:true,type:"bool")"#,
        ),
        (
            r#"{"type":"bool","type":"bool","value":true}"#,
            r#"(type:"bool",type:"bool",value:true)"#,
        ),
        (
            r#"{"value":true,"type":"bool","type":"null"}"#,
            r#"(value:true,type:"bool",type:"null")"#,
        ),
        (
            r#"{"type":"bool","value":true,"value":false}"#,
            r#"(type:"bool",value:true,value:false)"#,
        ),
        (
            r#"{"value":true,"value":false,"type":"bool"}"#,
            r#"(value:true,value:false,type:"bool")"#,
        ),
        (
            r#"{"type":"null","value":null,"value":null}"#,
            r#"(type:"null",value:(),value:())"#,
        ),
    ] {
        assert!(
            serde_json::from_str::<ComponentValue>(json).is_err(),
            "{json}"
        );
        assert!(ron::from_str::<ComponentValue>(ron).is_err(), "{ron}");
    }
}

#[test]
fn malformed_nested_values_are_rejected_through_containers_and_parent_buffers() {
    for (json, ron) in [
        (
            r#"{"type":"list","value":[{"value":true}]}"#,
            r#"(type:"list",value:[(value:true)])"#,
        ),
        (
            r#"{"value":{"child":{"type":"null","extra":1}},"type":"map"}"#,
            r#"(value:{"child":(type:"null",extra:1)},type:"map")"#,
        ),
        (
            r#"{"type":"list","value":[{"type":"bool","value":true,"value":false}]}"#,
            r#"(type:"list",value:[(type:"bool",value:true,value:false)])"#,
        ),
    ] {
        assert!(
            serde_json::from_str::<ComponentValue>(json).is_err(),
            "{json}"
        );
        assert!(ron::from_str::<ComponentValue>(ron).is_err(), "{ron}");
        let internal_json = format!(r#"{{"value":{json},"envelope":"component"}}"#);
        let internal_ron = format!(r#"(value:{ron},envelope:"component")"#);
        let adjacent_json = format!(r#"{{"content":{json},"envelope":"component"}}"#);
        let adjacent_ron = format!(r#"(content:{ron},envelope:component)"#);
        assert!(serde_json::from_str::<InternallyTagged>(&internal_json).is_err());
        assert!(ron::from_str::<InternallyTagged>(&internal_ron).is_err());
        assert!(serde_json::from_str::<AdjacentlyTagged>(&adjacent_json).is_err());
        assert!(ron::from_str::<AdjacentlyTagged>(&adjacent_ron).is_err());
    }
}

#[test]
fn multiline_malformed_nested_children_report_data_errors_without_position_contracts() {
    let json = r#"{
        "type": "map",
        "value": {
            "children": {
                "type": "list",
                "value": [
                    {"type": "bool", "value": "not a bool"}
                ]
            }
        }
    }"#;
    let ron = r#"(
        type: "map",
        value: {
            "children": (
                type: "list",
                value: [
                    (type: "bool", value: "not a bool")
                ]
            )
        }
    )"#;
    let json_error = serde_json::from_str::<ComponentValue>(json).unwrap_err();
    assert_eq!(json_error.classify(), serde_json::error::Category::Data);
    let ron_error = ron::from_str::<ComponentValue>(ron).unwrap_err();
    assert!(matches!(ron_error.code, ron::error::Error::Message(_)));
}

#[test]
fn entity_uuid_survives_parent_buffering() {
    let uuid = "11111111-1111-4111-8111-111111111111";
    assert_parent_buffered(
        &format!(r#"{{"value":"{uuid}","type":"entity_ref"}}"#),
        &format!(r#"(value:"{uuid}",type:"entity_ref")"#),
        &ComponentValue::EntityReference(uuid.parse().unwrap()),
    );
}

#[test]
fn entity_reference_payloads_reject_invalid_and_nil_uuids() {
    for (json, ron) in [
        (r#""target""#, r#""target""#),
        (
            r#""00000000-0000-0000-0000-000000000000""#,
            r#""00000000-0000-0000-0000-000000000000""#,
        ),
        (
            r#"{"kind":"scene_local","entity":"target"}"#,
            r#"(kind:"scene_local",entity:"target")"#,
        ),
    ] {
        assert_rejects_in_both_orders("entity_ref", json, ron);
    }
}
