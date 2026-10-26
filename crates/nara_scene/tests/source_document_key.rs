#![cfg(feature = "serde")]

use nara_asset::{AssetPath, StableAssetId};
use nara_core::SourceUniverseId;
use nara_scene::{SourceDocumentKey, SourceDocumentKind};

const UNIVERSE: &str = "72011426-37e2-48ba-bc06-78841468889a";
const STABLE_ID: &str = "44f2caae-b658-4790-a8a7-928935839ab7";

fn universe() -> SourceUniverseId {
    UNIVERSE.parse().unwrap()
}

fn stable_id() -> StableAssetId {
    STABLE_ID.parse().unwrap()
}

#[test]
fn json_and_ron_roundtrip_every_origin_and_document_kind() {
    for (kind, kind_name) in [
        (SourceDocumentKind::Scene, "scene"),
        (SourceDocumentKind::Prefab, "prefab"),
    ] {
        let fixtures = [
            (
                SourceDocumentKey::named(universe(), "base", kind).unwrap(),
                serde_json::json!({
                    "kind": kind_name,
                    "origin": {"named": {"universe": UNIVERSE, "name": "base"}}
                }),
                format!("(kind:{kind_name},origin:named(universe:\"{UNIVERSE}\",name:\"base\"))"),
            ),
            (
                SourceDocumentKey::project_location(
                    universe(),
                    AssetPath::new("prefabs/base.prefab.json").unwrap(),
                    kind,
                ),
                serde_json::json!({
                    "kind": kind_name,
                    "origin": {"project_location": {
                        "universe": UNIVERSE,
                        "path": "prefabs/base.prefab.json"
                    }}
                }),
                format!(
                    "(kind:{kind_name},origin:project_location(universe:\"{UNIVERSE}\",path:\"prefabs/base.prefab.json\"))"
                ),
            ),
            (
                SourceDocumentKey::stable_asset(stable_id(), kind),
                serde_json::json!({
                    "kind": kind_name,
                    "origin": {"stable_asset": {"stable_id": STABLE_ID}}
                }),
                format!("(kind:{kind_name},origin:stable_asset(stable_id:\"{STABLE_ID}\"))"),
            ),
        ];
        for (key, json, ron) in fixtures {
            assert_eq!(serde_json::to_value(&key).unwrap(), json);
            assert_eq!(
                serde_json::from_value::<SourceDocumentKey>(json).unwrap(),
                key
            );
            assert_eq!(
                serde_json::from_reader::<_, SourceDocumentKey>(
                    serde_json::to_vec(&key).unwrap().as_slice()
                )
                .unwrap(),
                key
            );
            assert_eq!(ron::from_str::<SourceDocumentKey>(&ron).unwrap(), key);
            let encoded = ron::to_string(&key).unwrap();
            assert_eq!(encoded, ron);
            assert_eq!(ron::from_str::<SourceDocumentKey>(&encoded).unwrap(), key);
        }
    }
}

#[test]
fn uuid_spelling_aliases_decode_to_the_same_canonical_identity() {
    let kind = SourceDocumentKind::Prefab;
    let cases = [
        (
            SourceDocumentKey::named(universe(), "base", kind).unwrap(),
            serde_json::json!({
                "kind": "prefab",
                "origin": {"named": {
                    "universe": UNIVERSE.to_uppercase(),
                    "name": "base"
                }}
            }),
            format!(
                "(kind:prefab,origin:named(universe:\"{}\",name:\"base\"))",
                UNIVERSE.to_uppercase()
            ),
        ),
        (
            SourceDocumentKey::project_location(
                universe(),
                AssetPath::new("base.prefab.json").unwrap(),
                kind,
            ),
            serde_json::json!({
                "kind": "prefab",
                "origin": {"project_location": {
                    "universe": UNIVERSE.replace('-', ""),
                    "path": "base.prefab.json"
                }}
            }),
            format!(
                "(kind:prefab,origin:project_location(universe:\"{}\",path:\"base.prefab.json\"))",
                UNIVERSE.replace('-', "")
            ),
        ),
        (
            SourceDocumentKey::stable_asset(stable_id(), kind),
            serde_json::json!({
                "kind": "prefab",
                "origin": {"stable_asset": {"stable_id": STABLE_ID.to_uppercase()}}
            }),
            format!(
                "(kind:prefab,origin:stable_asset(stable_id:\"{}\"))",
                STABLE_ID.to_uppercase()
            ),
        ),
    ];
    for (canonical, json, ron) in cases {
        for decoded in [
            serde_json::from_value::<SourceDocumentKey>(json).unwrap(),
            ron::from_str::<SourceDocumentKey>(&ron).unwrap(),
        ] {
            assert_eq!(decoded, canonical);
            assert_eq!(
                serde_json::to_string(&decoded).unwrap(),
                serde_json::to_string(&canonical).unwrap()
            );
            assert_eq!(
                ron::to_string(&decoded).unwrap(),
                ron::to_string(&canonical).unwrap()
            );
        }
    }
}

#[test]
fn codec_preserves_identity_axes_and_exact_name_and_path_spelling() {
    let other_universe = "8708dd60-e50c-43b0-a689-f3719f5b5701".parse().unwrap();
    let keys = [
        SourceDocumentKey::named(universe(), "base.prefab.json", SourceDocumentKind::Scene)
            .unwrap(),
        SourceDocumentKey::named(universe(), "base.prefab.json", SourceDocumentKind::Prefab)
            .unwrap(),
        SourceDocumentKey::named(
            other_universe,
            "base.prefab.json",
            SourceDocumentKind::Prefab,
        )
        .unwrap(),
        SourceDocumentKey::named(universe(), "Base.prefab.json", SourceDocumentKind::Prefab)
            .unwrap(),
        SourceDocumentKey::project_location(
            universe(),
            AssetPath::new("base.prefab.json").unwrap(),
            SourceDocumentKind::Prefab,
        ),
        SourceDocumentKey::project_location(
            universe(),
            AssetPath::new("Base.prefab.json").unwrap(),
            SourceDocumentKind::Prefab,
        ),
        SourceDocumentKey::stable_asset(stable_id(), SourceDocumentKind::Prefab),
    ];
    for (index, key) in keys.iter().enumerate() {
        let json = serde_json::to_string(key).unwrap();
        let ron = ron::to_string(key).unwrap();
        assert_eq!(
            &serde_json::from_str::<SourceDocumentKey>(&json).unwrap(),
            key
        );
        assert_eq!(&ron::from_str::<SourceDocumentKey>(&ron).unwrap(), key);
        for other in &keys[index + 1..] {
            assert_ne!(key, other);
            assert_ne!(json, serde_json::to_string(other).unwrap());
            assert_ne!(ron, ron::to_string(other).unwrap());
        }
    }
}

#[test]
fn invalid_names_paths_and_uuid_claims_are_rejected_in_both_formats() {
    for name in ["", "bad\0name"] {
        let json = serde_json::json!({
            "kind": "scene",
            "origin": {"named": {"universe": UNIVERSE, "name": name}}
        });
        let ron = format!(
            "(kind:scene,origin:named(universe:\"{UNIVERSE}\",name:{}))",
            ron::to_string(name).unwrap()
        );
        assert!(serde_json::from_value::<SourceDocumentKey>(json).is_err());
        assert!(ron::from_str::<SourceDocumentKey>(&ron).is_err());
    }
    for path in [
        "",
        "/absolute.prefab",
        "C:/drive.prefab",
        "bad\\separator.prefab",
        "./current.prefab",
        "../parent.prefab",
        "prefabs//empty.prefab",
        "prefabs/trailing/",
        "bad\0path.prefab",
    ] {
        let json = serde_json::json!({
            "kind": "prefab",
            "origin": {"project_location": {"universe": UNIVERSE, "path": path}}
        });
        let ron = format!(
            "(kind:prefab,origin:project_location(universe:\"{UNIVERSE}\",path:{}))",
            ron::to_string(path).unwrap()
        );
        assert!(
            serde_json::from_value::<SourceDocumentKey>(json).is_err(),
            "{path:?}"
        );
        assert!(
            ron::from_str::<SourceDocumentKey>(&ron).is_err(),
            "{path:?}"
        );
    }
    for invalid in ["", "not-a-uuid", "bad\0id"] {
        assert_invalid_uuid(invalid, true);
        assert_invalid_uuid(invalid, false);
    }
    // SourceUniverseId rejects nil. StableAssetId's existing constructor defines its own domain.
    assert_invalid_uuid("00000000-0000-0000-0000-000000000000", true);
}

fn assert_invalid_uuid(invalid: &str, is_universe: bool) {
    let (json, ron) = if is_universe {
        (
            serde_json::json!({
                "kind": "scene",
                "origin": {"named": {"universe": invalid, "name": "base"}}
            }),
            format!(
                "(kind:scene,origin:named(universe:{},name:\"base\"))",
                ron::to_string(invalid).unwrap()
            ),
        )
    } else {
        (
            serde_json::json!({
                "kind": "prefab",
                "origin": {"stable_asset": {"stable_id": invalid}}
            }),
            format!(
                "(kind:prefab,origin:stable_asset(stable_id:{}))",
                ron::to_string(invalid).unwrap()
            ),
        )
    };
    assert!(serde_json::from_value::<SourceDocumentKey>(json).is_err());
    assert!(ron::from_str::<SourceDocumentKey>(&ron).is_err());
}

#[test]
fn json_rejects_unknown_missing_duplicate_and_legacy_fields_without_fallback() {
    let valid = format!(
        r#"{{"kind":"prefab","origin":{{"named":{{"universe":"{UNIVERSE}","name":"base"}}}}}}"#
    );
    let invalid = [
        valid.replace(r#""kind":"prefab""#, r#""kind":"future""#),
        valid.replace(r#""kind":"prefab""#, r#""kind":"Prefab""#),
        valid.replace(r#""named":"#, r#""lookup":"#),
        valid.replace(
            r#""name":"base""#,
            r#""name":"base","lookup":"alias.prefab""#,
        ),
        valid.replace(r#""name":"base""#, r#""path":"base.prefab""#),
        valid.replace(r#""name":"base""#, r#""name":null"#),
        valid.replace(r#""name":"base""#, r#""name":"base","name":"other""#),
        valid.replace(r#""kind":"prefab""#, r#""kind":"prefab","kind":"scene""#),
        valid.replace(r#""kind":"prefab","#, ""),
        valid.replacen('{', r#"{"lookup":"alias.prefab","#, 1),
        format!(
            r#"{{"kind":"prefab","origin":{{"project_location":{{"universe":"{UNIVERSE}","path":"base.prefab","name":"base"}}}}}}"#
        ),
        format!(
            r#"{{"kind":"prefab","origin":{{"stable_asset":{{"stable_id":"{STABLE_ID}","universe":"{UNIVERSE}"}}}}}}"#
        ),
        format!(r#"{{"kind":"prefab","origin":"named","universe":"{UNIVERSE}","name":"base"}}"#),
        r#""base.prefab.json""#.to_owned(),
        format!(r#""{:?}""#, SourceDocumentKind::Prefab),
        "null".to_owned(),
        "{}".to_owned(),
    ];
    assert!(serde_json::from_str::<SourceDocumentKey>(&valid).is_ok());
    for input in invalid {
        assert!(
            serde_json::from_str::<SourceDocumentKey>(&input).is_err(),
            "unexpectedly accepted {input}"
        );
    }
}

#[test]
fn ron_rejects_unknown_missing_duplicate_and_legacy_fields_without_fallback() {
    let valid = format!("(kind:prefab,origin:named(universe:\"{UNIVERSE}\",name:\"base\"))");
    let invalid = [
        valid.replace("kind:prefab", "kind:future"),
        valid.replace("kind:prefab", "kind:Prefab"),
        valid.replace("origin:named", "origin:lookup"),
        valid.replace("name:\"base\"", "name:\"base\",lookup:\"alias.prefab\""),
        valid.replace("name:\"base\"", "path:\"base.prefab\""),
        valid.replace("name:\"base\"", "name:None"),
        valid.replace("name:\"base\"", "name:\"base\",name:\"other\""),
        valid.replace("kind:prefab", "kind:prefab,kind:scene"),
        valid.replace("kind:prefab,", ""),
        valid.replacen('(', "(lookup:\"alias.prefab\",", 1),
        format!(
            "(kind:prefab,origin:project_location(universe:\"{UNIVERSE}\",path:\"base.prefab\",name:\"base\"))"
        ),
        format!(
            "(kind:prefab,origin:stable_asset(stable_id:\"{STABLE_ID}\",universe:\"{UNIVERSE}\"))"
        ),
        format!("(kind:prefab,origin:\"named\",universe:\"{UNIVERSE}\",name:\"base\")"),
        "\"base.prefab.json\"".to_owned(),
        "None".to_owned(),
        "()".to_owned(),
    ];
    assert!(ron::from_str::<SourceDocumentKey>(&valid).is_ok());
    for input in invalid {
        assert!(
            ron::from_str::<SourceDocumentKey>(&input).is_err(),
            "unexpectedly accepted {input}"
        );
    }
}
