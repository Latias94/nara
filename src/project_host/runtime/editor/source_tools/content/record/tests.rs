use serde_json::{Value, json};

use super::*;

const SAMPLE: &str = "samples/readme.txt";

fn installation(id: u8) -> Installation {
    let installed = EditorSourceToolInstalledSelection {
        package: "test.stats".into(),
        directory: format!("tools/install-{id}").into(),
        installation: [id; 32],
        receipt: ContentDigest::from_parts(64, [2; 32]),
        manifest: ContentDigest::from_parts(128, [3; 32]),
    };
    Installation {
        members: vec![
            member(MANIFEST_NAME, Role::Metadata, installed.manifest),
            member(RECEIPT_NAME, Role::Metadata, installed.receipt),
            member("bin/tool.dll", Role::Tool, ContentDigest::of_bytes(b"tool")),
            member(SAMPLE, Role::Content, ContentDigest::of_bytes(b"sample")),
        ],
        installed,
    }
}

fn member(path: &str, role: Role, baseline: ContentDigest) -> Member {
    Member {
        path: path.into(),
        role,
        baseline,
        modified: false,
        adopted: false,
        observation: Observation::Present,
    }
}

fn ledger() -> Ledger {
    Ledger::default()
        .register_installation(installation(1))
        .unwrap()
}

fn value() -> Value {
    serde_json::from_slice(&encode(&ledger()).unwrap()).unwrap()
}

fn decode_value(value: &Value) -> Result<Ledger, Error> {
    decode(&serde_json::to_vec(value).unwrap())
}

#[test]
fn roundtrip_preserves_independent_protection_and_every_observation() {
    for observation in [
        Observation::Present,
        Observation::Pending,
        Observation::Removed,
        Observation::Missing,
        Observation::Failed,
    ] {
        for (modified, adopted) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut ledger = ledger();
            let sample = ledger.member_mut(&[1; 32], SAMPLE).unwrap();
            sample.modified = modified;
            sample.adopted = adopted;
            sample.observation = observation;
            let bytes = encode(&ledger).unwrap();
            assert_eq!(decode(&bytes).unwrap(), ledger);
            assert_eq!(encode(&decode(&bytes).unwrap()).unwrap(), bytes);
        }
    }
    assert_eq!(
        encode(&Ledger::default()).unwrap(),
        br#"{"format_version":1,"installations":[]}"#,
    );
}

#[test]
fn canonical_order_is_independent_of_input_order_without_mutating_the_input() {
    let mut ledger = ledger().register_installation(installation(2)).unwrap();
    ledger.installations.reverse();
    for entry in &mut ledger.installations {
        entry.members.reverse();
    }
    let before = ledger.clone();
    let bytes = encode(&ledger).unwrap();
    assert_eq!(ledger, before);
    let canonical = decode(&bytes).unwrap();
    assert_eq!(canonical.installations[0].installed.installation, [1; 32]);
    assert_eq!(
        canonical.installations[0].members[0].path.as_ref(),
        RECEIPT_NAME
    );
    let mut input: Value = serde_json::from_slice(&bytes).unwrap();
    input["installations"].as_array_mut().unwrap().reverse();
    input["installations"][0]["members"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(decode_value(&input).unwrap(), canonical);
}

#[test]
fn registration_preserves_existing_protection_and_status_and_rejects_rebinding() {
    let mut ledger = ledger();
    ledger.set_adopted(&[1; 32], SAMPLE).unwrap();
    ledger.member_mut(&[1; 32], SAMPLE).unwrap().modified = true;
    ledger.member_mut(&[1; 32], SAMPLE).unwrap().observation = Observation::Pending;
    let mut repeat = installation(1);
    repeat.members.reverse();
    assert_eq!(ledger.register_installation(repeat).unwrap(), ledger);
    ledger.member_mut(&[1; 32], SAMPLE).unwrap().observation = Observation::Removed;
    assert_eq!(
        ledger.register_installation(installation(1)).unwrap(),
        ledger
    );
    for case in 0..8 {
        let mut replacement = installation(1);
        match case {
            0 => replacement.installed.package = "test.other".into(),
            1 => replacement.installed.directory = "tools/other".into(),
            2 => {
                replacement.installed.receipt = ContentDigest::from_parts(64, [8; 32]);
                replacement.members[1].baseline = replacement.installed.receipt;
            }
            3 => {
                replacement.installed.manifest = ContentDigest::from_parts(128, [8; 32]);
                replacement.members[0].baseline = replacement.installed.manifest;
            }
            4 => replacement.members[3].baseline = ContentDigest::of_bytes(b"new baseline"),
            5 => replacement.members[3].path = "samples/other.txt".into(),
            6 => replacement.members[3].role = Role::Tool,
            _ => {
                replacement.members.pop();
            }
        }
        assert_eq!(
            ledger.register_installation(replacement),
            Err(Error::Changed),
            "{case}"
        );
    }
    assert_eq!(ledger.member_mut(&[2; 32], SAMPLE), Err(Error::Unmanaged));
    assert_eq!(
        ledger.member_mut(&[1; 32], "samples/unknown.txt"),
        Err(Error::Unmanaged)
    );
    assert_eq!(
        ledger.member_mut(&[1; 32], "../sample"),
        Err(Error::InvalidPath)
    );
}

#[test]
fn adoption_rejects_tool_and_metadata_members_without_mutating_the_ledger() {
    let mut ledger = ledger();
    ledger.set_adopted(&[1; 32], SAMPLE).unwrap();
    let before = ledger.clone();
    for path in ["bin/tool.dll", MANIFEST_NAME, RECEIPT_NAME] {
        assert_eq!(ledger.set_adopted(&[1; 32], path), Err(Error::InvalidPath));
        assert_eq!(ledger, before);
    }
    ledger.set_adopted(&[1; 32], SAMPLE).unwrap();
    assert_eq!(ledger, before);
}

#[test]
fn capacity_never_evicts_old_protection_and_still_allows_exact_repeats() {
    let mut ledger = Ledger::default();
    for id in 1..=MAX_INSTALLATIONS as u8 {
        ledger = ledger.register_installation(installation(id)).unwrap();
    }
    ledger.set_adopted(&[1; 32], SAMPLE).unwrap();
    let before = ledger.clone();
    assert_eq!(decode(&encode(&ledger).unwrap()).unwrap(), ledger);
    assert_eq!(
        ledger.register_installation(installation(33)),
        Err(Error::TooLarge)
    );
    assert_eq!(ledger, before);
    assert_eq!(
        ledger.register_installation(installation(1)).unwrap(),
        before
    );
}

#[test]
fn rejects_unknown_authority_fields_missing_fields_and_duplicate_keys() {
    let valid = String::from_utf8(encode(&ledger()).unwrap()).unwrap();
    for input in ["", "null", "{}", "[]", &valid[..valid.len() - 1]] {
        assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    }
    assert_eq!(
        decode(format!("{valid} {{}}").as_bytes()),
        Err(Error::InvalidRecord)
    );
    for pointer in [
        "",
        "/installations/0",
        "/installations/0/members/0",
        "/installations/0/receipt",
    ] {
        for field in [
            "unknown",
            "file_identity",
            "creation_proof",
            "approved",
            "delete_authority",
        ] {
            let mut input = value();
            input
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(field.to_owned(), json!(true));
            assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
        }
    }
    for field in [
        "format_version",
        "installations",
        "package",
        "directory",
        "installation",
        "receipt",
        "manifest",
        "members",
        "path",
        "role",
        "baseline",
        "modified",
        "adopted",
        "observation",
        "length",
        "hash",
    ] {
        let input = valid.replace(
            &format!("\"{field}\":"),
            &format!("\"{field}\":null,\"{field}\":"),
        );
        assert_ne!(input, valid);
        assert_eq!(
            decode(input.as_bytes()),
            Err(Error::InvalidRecord),
            "{field}"
        );
    }
    let escaped = valid.replace("\"modified\":", "\"mod\\u0069fied\":false,\"modified\":");
    assert_eq!(decode(escaped.as_bytes()), Err(Error::InvalidRecord));
    for pointer in [
        "/format_version",
        "/installations",
        "/installations/0/package",
        "/installations/0/members/0/adopted",
    ] {
        let mut input = value();
        let (parent, field) = pointer.rsplit_once('/').unwrap();
        input
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
}

#[test]
fn rejects_unsupported_versions_and_noninteger_or_unknown_enum_values() {
    for version in [0_u32, 2, u32::MAX] {
        let mut input = value();
        input["format_version"] = json!(version);
        assert_eq!(decode_value(&input), Err(Error::UnsupportedFormat));
    }
    for version in [json!(-1), json!(1.0), json!("1"), Value::Null] {
        let mut input = value();
        input["format_version"] = version;
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for (field, invalid) in [
        ("role", json!("executable")),
        ("observation", json!("approved")),
        ("modified", json!(0)),
    ] {
        let mut input = value();
        input["installations"][0]["members"][0][field] = invalid;
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
}

#[test]
fn validates_identity_package_receipt_and_reserved_or_overlapping_directories() {
    for package in [
        "",
        "Stats",
        ".stats",
        "test/stats",
        "test+stats",
        "test stats",
        "st\u{430}ts",
    ] {
        let mut input = value();
        input["installations"][0]["package"] = json!(package);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    let mut input = value();
    input["installations"][0]["installation"] = json!(vec![0; 32]);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    for (field, length) in [
        ("receipt", 0),
        ("receipt", 4097),
        ("manifest", 0),
        ("manifest", 65_537),
    ] {
        let mut input = value();
        input["installations"][0][field]["length"] = json!(length);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for directory in [".nara", ".NARA/source-tools", ".nara/SOURCE-TOOLS"] {
        let mut entry = installation(1);
        entry.installed.directory = directory.into();
        assert_eq!(
            Ledger::default().register_installation(entry),
            Err(Error::InvalidPath)
        );
    }
    let mut entry = installation(1);
    entry.installed.package = "0stats.tools_a-1".into();
    entry.installed.directory = ".nara/source-tools/install-1".into();
    assert!(Ledger::default().register_installation(entry).is_ok());
    for directory in [
        "tools/install-1",
        "TOOLS/INSTALL-1",
        "tools",
        "tools/install-1/child",
    ] {
        let mut entry = installation(2);
        entry.installed.directory = directory.into();
        assert_eq!(
            ledger().register_installation(entry),
            Err(Error::InvalidRecord)
        );
    }
    let mut duplicate = ledger();
    duplicate.installations.push(installation(1));
    assert_eq!(encode(&duplicate), Err(Error::InvalidRecord));
}

#[test]
fn portable_member_paths_reject_ads_devices_traversal_aliases_and_prefix_collisions() {
    for path in [
        "",
        "/absolute",
        "../escape",
        "a/./b",
        "a//b",
        "a/",
        "a\\b",
        "file:stream",
        "a./file",
        "a ",
        "CON.txt",
        "aux",
        "LPT1.bin",
        "\u{e9}.txt",
        "tool+data",
        "a/b/c/d/e/f/g/h/i",
    ] {
        let mut entry = installation(1);
        entry.members[3].path = path.into();
        assert_eq!(
            Ledger::default().register_installation(entry),
            Err(Error::InvalidPath),
            "{path}"
        );
    }
    for path in [
        format!("{}.txt", "a".repeat(256)),
        format!("{}/{}", "a".repeat(255), "b".repeat(257)),
    ] {
        let mut entry = installation(1);
        entry.members[3].path = path.into();
        assert_eq!(
            Ledger::default().register_installation(entry),
            Err(Error::InvalidPath)
        );
    }
    for path in [
        "BIN/TOOL.DLL",
        "BIN/another.dll",
        "bin",
        "bin/tool.dll/child",
    ] {
        let mut entry = installation(1);
        entry.members[3].path = path.into();
        assert_eq!(
            Ledger::default().register_installation(entry),
            Err(Error::InvalidPath),
            "{path}"
        );
    }
    let mut entry = installation(1);
    entry.members[3].path = format!("{}/{}/c", "a".repeat(255), "b".repeat(254)).into();
    assert_eq!(entry.members[3].path.len(), 512);
    assert!(Ledger::default().register_installation(entry).is_ok());
}

#[test]
fn metadata_is_restricted_to_the_exact_manifest_and_receipt_baselines() {
    for (path, role) in [
        ("metadata.json", Role::Metadata),
        ("NARA-TOOL.JSON", Role::Metadata),
        ("NARA-TOOL.JSON", Role::Tool),
        (RECEIPT_NAME, Role::Content),
    ] {
        let mut entry = installation(1);
        entry.members[0].path = path.into();
        entry.members[0].role = role;
        assert_eq!(
            Ledger::default().register_installation(entry),
            Err(Error::InvalidRecord)
        );
    }
    let mut entry = installation(1);
    entry.members[0].baseline = ContentDigest::from_parts(128, [9; 32]);
    assert_eq!(
        Ledger::default().register_installation(entry),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn payload_and_member_limits_admit_exact_bounds_and_reject_first_excess() {
    let mut entry = installation(1);
    entry.members.truncate(2);
    for index in 0..MAX_PAYLOADS {
        entry.members.push(member(
            &format!("payload-{index}.bin"),
            Role::Content,
            ContentDigest::from_parts(if index < 4 { MAX_PAYLOAD_BYTES } else { 0 }, [1; 32]),
        ));
    }
    assert_eq!(entry.members.len(), MAX_MEMBERS);
    let full = Ledger::default()
        .register_installation(entry.clone())
        .unwrap();
    assert_eq!(decode(&encode(&full).unwrap()).unwrap(), full);
    let mut all = Ledger::default();
    for id in 1..=MAX_INSTALLATIONS as u8 {
        let mut next = entry.clone();
        next.installed.installation = [id; 32];
        next.installed.directory = format!("tools/install-{id}").into();
        all = all.register_installation(next).unwrap();
    }
    assert_eq!(decode(&encode(&all).unwrap()).unwrap(), all);
    entry.members[6].baseline = ContentDigest::from_parts(1, [1; 32]);
    assert_eq!(
        Ledger::default().register_installation(entry),
        Err(Error::TooLarge)
    );
    let mut entry = installation(1);
    entry.members[3].baseline = ContentDigest::from_parts(MAX_PAYLOAD_BYTES + 1, [1; 32]);
    assert_eq!(
        Ledger::default().register_installation(entry),
        Err(Error::TooLarge)
    );
    let mut full = full;
    full.installations[0]
        .members
        .push(member("extra", Role::Tool, ContentDigest::of_bytes(b"")));
    assert_eq!(encode(&full), Err(Error::TooLarge));
}

#[test]
fn structural_preflight_and_bounded_writer_apply_before_dto_allocation_or_output_growth() {
    for length in [
        BYTE_LIMIT as usize - 1,
        BYTE_LIMIT as usize,
        BYTE_LIMIT as usize + 1,
    ] {
        let mut bytes = encode(&ledger()).unwrap();
        bytes.resize(length, b' ');
        let decoded = decode(&bytes);
        if length <= BYTE_LIMIT as usize {
            assert_eq!(decoded.unwrap(), ledger());
        } else {
            assert_eq!(decoded, Err(Error::TooLarge));
        }
    }
    for input in [
        format!("{}0{}", "[".repeat(9), "]".repeat(9)),
        format!("\"{}\"", "a".repeat(MAX_PATH_BYTES + 1)),
        format!("[{}]", vec!["null"; MAX_MEMBERS + 1].join(",")),
    ] {
        assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    }
    let mut output = RecordBytes {
        bytes: vec![0; BYTE_LIMIT as usize - 1],
        oversized: false,
    };
    assert!(output.write_all(&[1, 2]).is_err());
    assert!(output.oversized);
    assert_eq!(output.bytes.len(), BYTE_LIMIT as usize - 1);
    output.write_all(&[1]).unwrap();
    assert!(output.write_all(&[2]).is_err());
    assert_eq!(output.bytes.len(), BYTE_LIMIT as usize);
}
