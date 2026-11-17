use serde::de::{
    DeserializeSeed,
    value::{BorrowedBytesDeserializer, BytesDeserializer, CharDeserializer, Error},
};

use super::super::{PayloadSeed, decode_value};
use super::{AdjacentlyTagged, ComponentValue, InternallyTagged};

fn assert_same_outcome<E: std::fmt::Debug>(
    actual: Result<ComponentValue, E>,
    fallback: Result<ComponentValue, E>,
    wire: &str,
) {
    match (actual, fallback) {
        (Ok(ComponentValue::F64(actual)), Ok(ComponentValue::F64(expected))) => {
            assert_eq!(actual.get().to_bits(), expected.get().to_bits(), "{wire}");
        }
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected, "{wire}"),
        (Err(_), Err(_)) => {}
        (actual, fallback) => panic!("{wire}: actual={actual:?}, fallback={fallback:?}"),
    }
}

#[test]
fn numeric_scalar_dispatch_matches_value_first_fallback() {
    for (json_payload, ron_payload) in [
        ("127", "0x7f"),
        ("32768", "0x8000"),
        ("4294967296", "4_294_967_296"),
        ("9007199254740995", "9_007_199_254_740_995"),
        ("9223372036854775807", "0x7fff_ffff_ffff_ffff"),
        ("18446744073709551615", "0xffff_ffff_ffff_ffff"),
        ("-0", "-0"),
        ("-0.0", "-0.0"),
        ("5e-324", "5e-324"),
        ("2.2250738585072014e-308", "2.2250738585072014e-308"),
        ("1.0000000596046448", "1.0000000596046448"),
        ("-1e-999", "-1e-999"),
        ("1e999", "1e999"),
    ] {
        for kind in ["i64", "u64", "f64"] {
            let json = format!(r#"{{"type":"{kind}","value":{json_payload}}}"#);
            let json_fallback = format!(r#"{{"value":{json_payload},"type":"{kind}"}}"#);
            assert_same_outcome(
                serde_json::from_str(&json),
                serde_json::from_str(&json_fallback),
                &json,
            );
            let ron = format!(r#"(type:"{kind}",value:{ron_payload})"#);
            let ron_fallback = format!(r#"(value:{ron_payload},type:"{kind}")"#);
            assert_same_outcome(ron::from_str(&ron), ron::from_str(&ron_fallback), &ron);
        }
    }
}

#[test]
fn ron_scalar_suffixes_options_newtypes_and_chars_remain_rejected() {
    let cases: &[(&str, &[&str])] = &[
        ("bool", &["Some(true)", "None", "Wrapped(true)"]),
        (
            "i64",
            &[
                "Some(1)",
                "None",
                "Wrapped(1)",
                "1i8",
                "1i16",
                "1i32",
                "1i64",
                "1i128",
            ],
        ),
        (
            "u64",
            &[
                "Some(1)",
                "None",
                "Wrapped(1)",
                "1u8",
                "1u16",
                "1u32",
                "1u64",
                "1u128",
            ],
        ),
        (
            "f64",
            &["Some(1.0)", "None", "Wrapped(1.0)", "1.0f32", "1.0f64"],
        ),
        (
            "string",
            &[
                r#"Some("text")"#,
                "None",
                r#"Wrapped("text")"#,
                r#"b"text""#,
                "'a'",
                "'雪'",
            ],
        ),
    ];
    for &(kind, payloads) in cases {
        for payload in payloads {
            for wire in [
                format!(r#"(type:"{kind}",value:{payload})"#),
                format!(r#"(value:{payload},type:"{kind}")"#),
            ] {
                assert!(ron::from_str::<ComponentValue>(&wire).is_err(), "{wire}");
            }
        }
    }
}

fn string_payload<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    known_tag: bool,
) -> Result<ComponentValue, D::Error> {
    let payload = PayloadSeed(known_tag.then_some("string")).deserialize(deserializer)?;
    decode_value("string", Some(payload))
}

#[test]
fn string_scalar_bytes_keep_untagged_utf8_and_char_rules() {
    for bytes in [b"".as_slice(), "雪\0".as_bytes(), &[0xff], &[0xe2, 0x82]] {
        let expected = std::str::from_utf8(bytes)
            .ok()
            .map(|text| ComponentValue::String(text.to_owned()));
        let fallback = string_payload(BytesDeserializer::<Error>::new(bytes), false);
        assert_eq!(fallback.as_ref().ok(), expected.as_ref(), "{bytes:?}");
        assert_same_outcome(
            string_payload(BytesDeserializer::<Error>::new(bytes), true),
            fallback,
            &format!("bytes {bytes:?}"),
        );
        assert_same_outcome(
            string_payload(BorrowedBytesDeserializer::<Error>::new(bytes), true),
            string_payload(BorrowedBytesDeserializer::<Error>::new(bytes), false),
            &format!("borrowed bytes {bytes:?}"),
        );
    }
    for character in ['a', '雪', '\0'] {
        for known_tag in [false, true] {
            assert!(
                string_payload(CharDeserializer::<Error>::new(character), known_tag).is_err(),
                "{character:?}, known_tag={known_tag}"
            );
        }
    }
}

#[test]
fn parent_buffered_scalars_match_value_first_fallback_in_both_child_orders() {
    for (kind, json, ron, expected) in [
        ("bool", "false", "false", ComponentValue::Bool(false)),
        (
            "string",
            r#""雪\\u0000""#,
            r##"r#"雪\u0000"#"##,
            ComponentValue::String("雪\\u0000".into()),
        ),
    ] {
        super::assert_decodes_in_both_orders(kind, json, ron, &expected);
        super::assert_parent_buffered(
            &format!(r#"{{"type":"{kind}","value":{json}}}"#),
            &format!(r#"(type:"{kind}",value:{ron})"#),
            &expected,
        );
    }
    for (kind, json_payload, ron_payload) in [
        ("i64", "9223372036854775807", "0x7fff_ffff_ffff_ffff"),
        ("u64", "18446744073709551615", "0xffff_ffff_ffff_ffff"),
        ("f64", "-0", "-0"),
        ("f64", "-0.0", "-0.0"),
        ("f64", "5e-324", "5e-324"),
        ("f64", "true", "Some(1.0)"),
        ("string", "null", r#"Some("text")"#),
    ] {
        let json_fallback = format!(r#"{{"value":{json_payload},"type":"{kind}"}}"#);
        let ron_fallback = format!(r#"(value:{ron_payload},type:"{kind}")"#);
        for type_first in [true, false] {
            let json = if type_first {
                format!(r#"{{"type":"{kind}","value":{json_payload}}}"#)
            } else {
                json_fallback.clone()
            };
            let ron = if type_first {
                format!(r#"(type:"{kind}",value:{ron_payload})"#)
            } else {
                ron_fallback.clone()
            };
            for actual in [
                serde_json::from_str::<InternallyTagged>(&format!(
                    r#"{{"value":{json},"envelope":"component"}}"#
                ))
                .map(|InternallyTagged::Component { value }| value),
                serde_json::from_str::<AdjacentlyTagged>(&format!(
                    r#"{{"content":{json},"envelope":"component"}}"#
                ))
                .map(|AdjacentlyTagged::Component(value)| value),
            ] {
                assert_same_outcome(actual, serde_json::from_str(&json_fallback), &json);
            }
            for actual in [
                ron::from_str::<InternallyTagged>(&format!(
                    r#"(value:{ron},envelope:"component")"#
                ))
                .map(|InternallyTagged::Component { value }| value),
                ron::from_str::<AdjacentlyTagged>(&format!(
                    r#"(content:{ron},envelope:component)"#
                ))
                .map(|AdjacentlyTagged::Component(value)| value),
            ] {
                assert_same_outcome(actual, ron::from_str(&ron_fallback), &ron);
            }
        }
    }
}
