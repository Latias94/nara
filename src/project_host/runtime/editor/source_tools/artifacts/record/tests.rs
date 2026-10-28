use serde_json::{Value, json};

use super::*;

fn installation(id: u8) -> Installation {
    let installed = EditorSourceToolInstalledSelection {
        package: "test.stats".into(),
        directory: format!(".nara/source-tools/artifacts/stats-{id}").into(),
        installation: [id; 32],
        receipt: ContentDigest::from_parts(64, [2; 32]),
        manifest: ContentDigest::from_parts(128, [3; 32]),
    };
    Installation {
        members: vec![
            member(MANIFEST_NAME, Role::Metadata, installed.manifest),
            member(RECEIPT_NAME, Role::Metadata, installed.receipt),
            member("bin/tool.dll", Role::Tool, ContentDigest::of_bytes(b"tool")),
            member(
                "samples/readme.txt",
                Role::Content,
                ContentDigest::of_bytes(b"template"),
            ),
        ],
        installed,
    }
}

fn member(path: &str, role: Role, baseline: ContentDigest) -> Member {
    Member {
        path: path.into(),
        role,
        baseline,
        observation: Observation::Present,
    }
}

fn inventory() -> Inventory {
    Inventory::default().register(installation(1)).unwrap()
}

fn value() -> Value {
    serde_json::from_slice(&encode(&inventory()).unwrap()).unwrap()
}

fn decode_value(value: &Value) -> Result<Inventory, Error> {
    decode(&serde_json::to_vec(value).unwrap())
}

#[test]
fn canonical_format_one_roundtrips_every_observation_without_live_authority() {
    assert_eq!(
        encode(&Inventory::default()).unwrap(),
        br#"{"format_version":1,"installations":[]}"#
    );
    for observation in [
        Observation::Present,
        Observation::Changed,
        Observation::Missing,
        Observation::Failed,
    ] {
        let mut inventory = inventory();
        inventory.installations[0].members[3].observation = observation;
        let bytes = encode(&inventory).unwrap();
        assert_eq!(decode(&bytes).unwrap(), inventory);
        assert_eq!(encode(&decode(&bytes).unwrap()).unwrap(), bytes);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value["installations"][0].get("creation_custody").is_none());
        assert!(
            value["installations"][0]["members"][0]
                .get("modified")
                .is_none()
        );
        assert!(
            value["installations"][0]["members"][0]
                .get("adopted")
                .is_none()
        );
    }
}

#[test]
fn order_is_canonical_without_mutating_input() {
    let canonical = inventory().register(installation(2)).unwrap();
    let mut reordered = canonical.clone();
    reordered.installations.reverse();
    for entry in &mut reordered.installations {
        entry.members.reverse();
    }
    let before = reordered.clone();
    assert_eq!(encode(&reordered).unwrap(), encode(&canonical).unwrap());
    assert_eq!(reordered, before);
    assert_eq!(decode(&encode(&reordered).unwrap()).unwrap(), canonical);
}

#[test]
fn registration_refreshes_finite_observations_and_never_rebinds_exact_facts() {
    let mut previous = inventory();
    previous.installations[0].members[3].observation = Observation::Changed;
    assert_eq!(previous.register(installation(1)).unwrap(), inventory());
    for case in 0..6 {
        let mut changed = installation(1);
        match case {
            0 => changed.installed.package = "test.other".into(),
            1 => changed.installed.directory = ".nara/source-tools/artifacts/other".into(),
            2 => {
                changed.installed.receipt = ContentDigest::from_parts(64, [9; 32]);
                changed.members[1].baseline = changed.installed.receipt;
            }
            3 => changed.members[3].path = "samples/other.txt".into(),
            4 => changed.members[3].baseline = ContentDigest::of_bytes(b"different"),
            _ => changed.members[3].role = Role::Tool,
        }
        assert_eq!(previous.register(changed), Err(Error::Changed));
    }
}

#[test]
fn only_complete_installations_in_the_fixed_store_are_accepted() {
    for directory in [
        "tools/stats",
        ".nara",
        ".nara/source-tools",
        ".nara/source-tools/artifacts",
        ".nara/source-tools/artifacts/",
        ".NARA/source-tools/artifacts/stats",
        ".nara/source-tools/artifacts/stats/nested",
        ".nara/source-tools/artifacts/../stats",
    ] {
        let mut entry = installation(1);
        entry.installed.directory = directory.into();
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidPath),
            "{directory}"
        );
    }
    for index in 0..2 {
        let mut entry = installation(1);
        entry.members.remove(index);
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidRecord)
        );
    }
    let mut entry = installation(1);
    entry.members[0].baseline = entry.installed.receipt;
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
    let mut entry = installation(1);
    entry.members[2].role = Role::Content;
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn case_collisions_parent_leaf_collisions_and_duplicate_installations_refuse() {
    for path in [
        "BIN/other.dll",
        "bin",
        "bin/TOOL.dll",
        "../outside",
        "a//b",
        "a:b",
        "a./b",
    ] {
        let mut entry = installation(1);
        entry.members.push(member(
            path,
            Role::Content,
            ContentDigest::of_bytes(b"extra"),
        ));
        assert!(Inventory::default().register(entry).is_err(), "{path}");
    }
    let mut inventory = inventory();
    let mut other = installation(2);
    other.installed.directory = inventory.installations[0].installed.directory.clone();
    assert_eq!(inventory.register(other), Err(Error::InvalidRecord));
    inventory
        .installations
        .push(inventory.installations[0].clone());
    assert_eq!(inventory.validate(), Err(Error::InvalidRecord));
}

#[test]
fn full_inventory_never_evicts_and_still_admits_exact_repeat() {
    let mut inventory = Inventory::default();
    for id in 1..=MAX_INSTALLATIONS as u8 {
        inventory = inventory.register(installation(id)).unwrap();
    }
    assert_eq!(decode(&encode(&inventory).unwrap()).unwrap(), inventory);
    assert_eq!(inventory.register(installation(33)), Err(Error::TooLarge));
    assert_eq!(inventory.register(installation(1)).unwrap(), inventory);
    let mut entry = installation(1);
    entry.members[3].baseline = ContentDigest::from_parts(MAX_PAYLOAD_BYTES + 1, [1; 32]);
    assert_eq!(Inventory::default().register(entry), Err(Error::TooLarge));
    let mut entry = installation(1);
    for id in 0..5 {
        entry.members.push(member(
            &format!("large-{id}"),
            Role::Content,
            ContentDigest::from_parts(MAX_PAYLOAD_BYTES, [1; 32]),
        ));
    }
    assert_eq!(Inventory::default().register(entry), Err(Error::TooLarge));
}

#[test]
fn old_fields_and_removal_states_have_no_reader() {
    for field in ["modified", "adopted", "file_identity", "delete_authority"] {
        let mut input = value();
        input["installations"][0]["members"][0][field] = json!(true);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for observation in ["pending", "removed"] {
        let mut input = value();
        input["installations"][0]["members"][0]["observation"] = json!(observation);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    let mut input = value();
    input["installations"][0]["creation_custody"] = json!(true);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}

#[test]
fn malformed_duplicate_unknown_deep_and_oversized_input_is_bounded() {
    for input in [
        "",
        "null",
        "[]",
        "{}",
        r#"{"format_version":1,"installations":[],"extra":true}"#,
        r#"{"format_version":1,"format_\u0076ersion":1,"installations":[]}"#,
    ] {
        assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    }
    let mut input = value();
    input["format_version"] = json!(2);
    assert_eq!(decode_value(&input), Err(Error::UnsupportedFormat));
    let duplicate = String::from_utf8(encode(&inventory()).unwrap())
        .unwrap()
        .replace("\"path\":", "\"path\":null,\"path\":");
    assert_eq!(decode(duplicate.as_bytes()), Err(Error::InvalidRecord));
    assert_eq!(
        decode(&vec![b' '; BYTE_LIMIT as usize + 1]),
        Err(Error::TooLarge)
    );
    let deep = format!("{}0{}", "[".repeat(64), "]".repeat(64));
    assert_eq!(decode(deep.as_bytes()), Err(Error::InvalidRecord));
    let mut input = value();
    input["installations"][0]["members"][0]["path"] = json!("a".repeat(MAX_PATH_BYTES + 1));
    assert!(decode_value(&input).is_err());
}
