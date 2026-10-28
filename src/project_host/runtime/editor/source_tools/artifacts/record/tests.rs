use serde_json::{Value, json};

use super::*;

const RECEIPT_TEMPORARY: &str = ".nara-tool-stage-0123456789abcdef0123456789abcdef";

fn artifact(id: u8) -> Artifact {
    let receipt = ContentDigest::from_parts(64, [2; 32]);
    let manifest = ContentDigest::from_parts(128, [3; 32]);
    Artifact {
        id: [id; 32],
        package: "test.stats".into(),
        directory: format!(".nara/source-tools/artifacts/stats-{id}").into(),
        manifest,
        publication: Publication::Ready { receipt },
        members: vec![
            member(MANIFEST_NAME, Role::Metadata, manifest),
            member(RECEIPT_NAME, Role::Metadata, receipt),
            member("bin/tool.dll", Role::Tool, ContentDigest::of_bytes(b"tool")),
            member(
                "samples/readme.txt",
                Role::Content,
                ContentDigest::of_bytes(b"template"),
            ),
        ],
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

fn incomplete(id: u8, members: Vec<Member>) -> Artifact {
    Artifact {
        publication: Publication::Incomplete,
        members,
        ..artifact(id)
    }
}

fn inventory() -> Inventory {
    Inventory::default().register(artifact(1)).unwrap()
}

fn value() -> Value {
    serde_json::from_slice(&encode(&inventory()).unwrap()).unwrap()
}

fn decode_value(value: &Value) -> Result<Inventory, Error> {
    decode(&serde_json::to_vec(value).unwrap())
}

fn reclamation(
    artifact: u8,
    phase: ReclamationPhase,
    members: &[(&str, ReclamationMemberState)],
) -> Reclamation {
    Reclamation {
        id: [artifact; 32],
        artifact: [artifact; 32],
        phase,
        members: members
            .iter()
            .map(|(path, state)| ReclamationMember {
                path: (*path).into(),
                state: *state,
            })
            .collect(),
    }
}

#[test]
fn canonical_format_one_roundtrips_every_observation_without_live_authority() {
    assert_eq!(
        encode(&Inventory::default()).unwrap(),
        br#"{"format_version":1,"artifacts":[],"reclamations":[]}"#
    );
    for observation in [
        Observation::Present,
        Observation::Changed,
        Observation::Missing,
        Observation::Failed,
    ] {
        let mut inventory = inventory();
        inventory.artifacts[0].members[3].observation = observation;
        let bytes = encode(&inventory).unwrap();
        assert_eq!(decode(&bytes).unwrap(), inventory);
        assert_eq!(encode(&decode(&bytes).unwrap()).unwrap(), bytes);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["format_version"], json!(1));
        assert_eq!(value["artifacts"][0].as_object().unwrap().len(), 6);
        assert_eq!(
            value["artifacts"][0]["publication"],
            json!({
                "state": "ready",
                "receipt": {"length": 64, "hash": vec![2; 32]},
            })
        );
        assert!(value.get("installations").is_none());
        assert!(value["artifacts"][0].get("installed").is_none());
        assert!(value["artifacts"][0].get("receipt").is_none());
        assert!(value["artifacts"][0].get("creation_custody").is_none());
        assert!(
            value["artifacts"][0]["members"][0]
                .get("modified")
                .is_none()
        );
        assert!(value["artifacts"][0]["members"][0].get("adopted").is_none());
    }
}

#[test]
fn incomplete_roundtrips_only_actual_empty_subset_and_receipt_temporary_facts() {
    let subsets = [
        Vec::new(),
        vec![artifact(1).members[0].clone()],
        vec![artifact(1).members[2].clone()],
        vec![member(
            "samples/empty.txt",
            Role::Content,
            ContentDigest::of_bytes(b""),
        )],
        vec![member(
            RECEIPT_TEMPORARY,
            Role::ReceiptTemporary,
            ContentDigest::from_parts(MAX_RECEIPT_BYTES, [4; 32]),
        )],
        vec![
            artifact(1).members[0].clone(),
            artifact(1).members[3].clone(),
            member(
                RECEIPT_TEMPORARY,
                Role::ReceiptTemporary,
                ContentDigest::of_bytes(b"intended ready receipt"),
            ),
        ],
    ];
    for members in subsets {
        for observation in [
            Observation::Present,
            Observation::Changed,
            Observation::Missing,
            Observation::Failed,
        ] {
            let mut entry = incomplete(1, members.clone());
            for member in &mut entry.members {
                member.observation = observation;
            }
            let expected_len = entry.members.len();
            let inventory = Inventory::default().register(entry.clone()).unwrap();
            let bytes = encode(&inventory).unwrap();
            let decoded = decode(&bytes).unwrap();
            assert_eq!(decoded, inventory);
            assert_eq!(encode(&decoded).unwrap(), bytes);
            assert!(same_facts(&decoded.artifacts[0], &entry));
            assert_eq!(decoded.artifacts[0].members.len(), expected_len);
            assert!(
                decoded.artifacts[0]
                    .members
                    .iter()
                    .all(|member| member.path.as_ref() != RECEIPT_NAME)
            );
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["artifacts"][0]["publication"],
                json!({"state": "incomplete"})
            );
            assert_eq!(
                value["artifacts"][0]["members"].as_array().unwrap().len(),
                expected_len
            );
            if let Some(temporary) = value["artifacts"][0]["members"]
                .as_array()
                .unwrap()
                .iter()
                .find(|member| member["path"] == RECEIPT_TEMPORARY)
            {
                assert_eq!(temporary["role"], json!("receipt_temporary"));
            }
        }
    }
}

#[test]
fn order_is_canonical_without_mutating_input() {
    let canonical = inventory()
        .register(incomplete(
            2,
            vec![
                artifact(2).members[3].clone(),
                artifact(2).members[2].clone(),
                member(
                    RECEIPT_TEMPORARY,
                    Role::ReceiptTemporary,
                    ContentDigest::of_bytes(b"receipt"),
                ),
            ],
        ))
        .unwrap()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Intent,
            &[
                ("samples/readme.txt", ReclamationMemberState::Missing),
                ("bin/tool.dll", ReclamationMemberState::Pending),
            ],
        ))
        .unwrap()
        .with_reclamation(&reclamation(
            2,
            ReclamationPhase::Results,
            &[
                ("samples/readme.txt", ReclamationMemberState::Failed),
                ("bin/tool.dll", ReclamationMemberState::Removed),
            ],
        ))
        .unwrap();
    let mut reordered = canonical.clone();
    reordered.artifacts.reverse();
    for entry in &mut reordered.artifacts {
        entry.members.reverse();
    }
    reordered.reclamations.reverse();
    for entry in &mut reordered.reclamations {
        entry.members.reverse();
    }
    let before = reordered.clone();
    assert_eq!(encode(&reordered).unwrap(), encode(&canonical).unwrap());
    assert_eq!(reordered, before);
    assert_eq!(decode(&encode(&reordered).unwrap()).unwrap(), canonical);
    let mut value: Value = serde_json::from_slice(&encode(&canonical).unwrap()).unwrap();
    value["artifacts"].as_array_mut().unwrap().reverse();
    for entry in value["artifacts"].as_array_mut().unwrap() {
        entry["members"].as_array_mut().unwrap().reverse();
    }
    value["reclamations"].as_array_mut().unwrap().reverse();
    for entry in value["reclamations"].as_array_mut().unwrap() {
        entry["members"].as_array_mut().unwrap().reverse();
    }
    assert_eq!(decode_value(&value).unwrap(), canonical);
}

#[test]
fn registration_refreshes_finite_observations_and_never_rebinds_exact_facts() {
    let mut previous = inventory();
    previous.artifacts[0].members[3].observation = Observation::Changed;
    assert_eq!(previous.register(artifact(1)).unwrap(), inventory());
    for case in 0..9 {
        let mut changed = artifact(1);
        match case {
            0 => changed.package = "test.other".into(),
            1 => changed.directory = ".nara/source-tools/artifacts/other".into(),
            2 => {
                let receipt = ContentDigest::from_parts(64, [9; 32]);
                changed.publication = Publication::Ready { receipt };
                changed.members[1].baseline = receipt;
            }
            3 => changed.members[3].path = "samples/other.txt".into(),
            4 => changed.members[3].baseline = ContentDigest::of_bytes(b"different"),
            5 => changed.members[3].role = Role::Tool,
            6 => {
                changed.manifest = ContentDigest::from_parts(128, [9; 32]);
                changed.members[0].baseline = changed.manifest;
            }
            7 => {
                changed.publication = Publication::Incomplete;
                changed.members.remove(1);
            }
            _ => {
                changed.members.pop();
            }
        }
        assert_eq!(previous.register(changed), Err(Error::Changed));
    }
}

#[test]
fn incomplete_registration_refreshes_observations_without_extending_created_members() {
    let original = incomplete(
        1,
        vec![
            artifact(1).members[2].clone(),
            artifact(1).members[3].clone(),
        ],
    );
    let inventory = Inventory::default().register(original.clone()).unwrap();
    let mut observed = original.clone();
    observed.members[0].observation = Observation::Failed;
    let refreshed = inventory.register(observed.clone()).unwrap();
    assert!(same_facts(&original, &observed));
    assert_eq!(refreshed.artifacts[0], observed);
    let mut reordered = original.clone();
    reordered.members.reverse();
    assert!(same_facts(&original, &reordered));
    for case in 0..8 {
        let mut changed = original.clone();
        match case {
            0 => changed.package = "test.other".into(),
            1 => changed.directory = ".nara/source-tools/artifacts/other".into(),
            2 => changed.manifest = ContentDigest::from_parts(128, [9; 32]),
            3 => changed.members[0].path = "samples/other.txt".into(),
            4 => changed.members[0].role = Role::Content,
            5 => changed.members[0].baseline = ContentDigest::of_bytes(b"other"),
            6 => changed.members.clear(),
            _ => changed.members.push(artifact(1).members[0].clone()),
        }
        assert!(!same_facts(&original, &changed));
        assert_eq!(inventory.register(changed), Err(Error::Changed));
    }
    assert_eq!(inventory.register(artifact(1)), Err(Error::Changed));
    let mut different_id = original.clone();
    different_id.id = [2; 32];
    assert!(!same_facts(&original, &different_id));
    assert_eq!(inventory.register(different_id), Err(Error::InvalidRecord));
}

#[test]
fn only_complete_ready_artifacts_in_the_fixed_store_are_accepted() {
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
        let mut entry = artifact(1);
        entry.directory = directory.into();
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidPath),
            "{directory}"
        );
    }
    for index in 0..2 {
        let mut entry = artifact(1);
        entry.members.remove(index);
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidRecord)
        );
        let mut input = value();
        let path = if index == 0 {
            MANIFEST_NAME
        } else {
            RECEIPT_NAME
        };
        input["artifacts"][0]["members"]
            .as_array_mut()
            .unwrap()
            .retain(|member| member["path"] != path);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    let mut entry = artifact(1);
    entry.members[0].baseline = entry.members[1].baseline;
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
    let mut entry = artifact(1);
    entry.members[2].role = Role::Content;
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn incomplete_metadata_and_temporary_roles_cannot_claim_other_paths() {
    for role in [
        Role::Metadata,
        Role::Tool,
        Role::Content,
        Role::ReceiptTemporary,
    ] {
        for path in [RECEIPT_NAME, ".NARA-INSTALLATION.JSON"] {
            let entry = incomplete(1, vec![member(path, role, artifact(1).members[1].baseline)]);
            assert_eq!(
                Inventory::default().register(entry),
                Err(Error::InvalidRecord)
            );
        }
    }
    for path in [
        "other.json",
        "NARA-TOOL.JSON",
        "nested/nara-tool.json",
        RECEIPT_TEMPORARY,
    ] {
        let entry = incomplete(1, vec![member(path, Role::Metadata, artifact(1).manifest)]);
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidRecord)
        );
    }
    for role in [Role::Tool, Role::Content] {
        for path in [
            MANIFEST_NAME,
            "NARA-TOOL.JSON",
            "nara-tool.json/child",
            ".nara-installation.json/child",
        ] {
            let entry = incomplete(1, vec![member(path, role, ContentDigest::of_bytes(b"x"))]);
            assert_eq!(
                Inventory::default().register(entry),
                Err(Error::InvalidRecord)
            );
        }
    }
    let entry = incomplete(
        1,
        vec![member(
            MANIFEST_NAME,
            Role::Metadata,
            ContentDigest::of_bytes(b"wrong manifest"),
        )],
    );
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
    for path in [
        "bin/tool.dll",
        ".nara-tool-stage-",
        ".nara-tool-stage-0123456789abcdef0123456789abcde",
        ".nara-tool-stage-0123456789abcdef0123456789abcdef0",
        ".nara-tool-stage-0123456789ABCDEF0123456789ABCDEF",
        ".nara-tool-stage-0123456789abcdef0123456789abcdeg",
        ".NARA-tool-stage-0123456789abcdef0123456789abcdef",
        "nested/.nara-tool-stage-0123456789abcdef0123456789abcdef",
        ".nara-tool-stage-0123456789abcdef0123456789abcdef/child",
    ] {
        let entry = incomplete(
            1,
            vec![member(
                path,
                Role::ReceiptTemporary,
                ContentDigest::of_bytes(b"receipt"),
            )],
        );
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidRecord),
            "{path}"
        );
    }
    let temporary = member(
        RECEIPT_TEMPORARY,
        Role::ReceiptTemporary,
        ContentDigest::of_bytes(b"receipt"),
    );
    let mut ready = artifact(1);
    ready.members.push(temporary.clone());
    assert_eq!(
        Inventory::default().register(ready),
        Err(Error::InvalidRecord)
    );
    let mut another = temporary.clone();
    another.path = ".nara-tool-stage-fedcba9876543210fedcba9876543210".into();
    let entry = incomplete(1, vec![temporary, another]);
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn case_collisions_parent_leaf_collisions_and_duplicate_artifacts_refuse() {
    for path in [
        "BIN/other.dll",
        "bin",
        "bin/TOOL.dll",
        "../outside",
        "a//b",
        "a:b",
        "a./b",
    ] {
        let mut entry = artifact(1);
        entry.members.push(member(
            path,
            Role::Content,
            ContentDigest::of_bytes(b"extra"),
        ));
        assert!(Inventory::default().register(entry).is_err(), "{path}");
    }
    let mut inventory = inventory();
    let mut other = artifact(2);
    other.directory = inventory.artifacts[0].directory.clone();
    assert_eq!(inventory.register(other), Err(Error::InvalidRecord));
    inventory.artifacts.push(inventory.artifacts[0].clone());
    assert_eq!(inventory.validate(), Err(Error::InvalidRecord));
}

#[test]
fn mixed_publication_ids_directories_and_member_collisions_refuse() {
    let inventory = inventory();
    for directory in [
        ".nara/source-tools/artifacts/stats-1",
        ".nara/source-tools/artifacts/STATS-1",
    ] {
        let mut other = incomplete(2, Vec::new());
        other.directory = directory.into();
        assert_eq!(inventory.register(other), Err(Error::InvalidRecord));
    }
    let mut duplicate = inventory.clone();
    duplicate.artifacts.push(incomplete(1, Vec::new()));
    duplicate.artifacts[1].directory = ".nara/source-tools/artifacts/other".into();
    assert_eq!(duplicate.validate(), Err(Error::InvalidRecord));
    let mut input = value();
    let mut other = input["artifacts"][0].clone();
    other["directory"] = json!(".nara/source-tools/artifacts/other");
    input["artifacts"].as_array_mut().unwrap().push(other);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    for path in ["bin/TOOL.dll", "BIN/other.dll", "bin"] {
        let entry = incomplete(
            2,
            vec![
                artifact(2).members[2].clone(),
                member(path, Role::Content, ContentDigest::of_bytes(b"other")),
            ],
        );
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidPath)
        );
    }
    let mut duplicate_role = artifact(1).members[3].clone();
    duplicate_role.role = Role::Tool;
    let entry = incomplete(1, vec![artifact(1).members[3].clone(), duplicate_role]);
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidPath)
    );
}

#[test]
fn incomplete_retains_the_same_identity_manifest_payload_and_member_limits() {
    let mut full = incomplete(
        1,
        vec![
            artifact(1).members[0].clone(),
            member(
                RECEIPT_TEMPORARY,
                Role::ReceiptTemporary,
                ContentDigest::from_parts(MAX_RECEIPT_BYTES, [4; 32]),
            ),
        ],
    );
    for index in 0..MAX_PAYLOADS {
        full.members.push(member(
            &format!("data/item-{index:02}.bin"),
            Role::Content,
            ContentDigest::of_bytes(b""),
        ));
    }
    assert_eq!(full.members.len(), MAX_MEMBERS);
    let inventory = Inventory::default().register(full.clone()).unwrap();
    assert_eq!(decode(&encode(&inventory).unwrap()).unwrap(), inventory);
    full.members.push(member(
        "data/extra.bin",
        Role::Content,
        ContentDigest::of_bytes(b""),
    ));
    assert_eq!(Inventory::default().register(full), Err(Error::TooLarge));
    let mut too_many_payloads = incomplete(1, Vec::new());
    for index in 0..=MAX_PAYLOADS {
        too_many_payloads.members.push(member(
            &format!("data/item-{index:02}.bin"),
            Role::Content,
            ContentDigest::of_bytes(b""),
        ));
    }
    assert_eq!(
        Inventory::default().register(too_many_payloads),
        Err(Error::TooLarge)
    );
    for length in [MAX_PAYLOAD_BYTES + 1, u64::MAX] {
        let entry = incomplete(
            1,
            vec![member(
                "data/large.bin",
                Role::Content,
                ContentDigest::from_parts(length, [1; 32]),
            )],
        );
        assert_eq!(Inventory::default().register(entry), Err(Error::TooLarge));
    }
    let members = (0..4)
        .map(|index| {
            member(
                &format!("data/large-{index}.bin"),
                Role::Content,
                ContentDigest::from_parts(MAX_PAYLOAD_BYTES, [1; 32]),
            )
        })
        .collect();
    let at_limit = incomplete(1, members);
    assert!(Inventory::default().register(at_limit.clone()).is_ok());
    let mut over_limit = at_limit;
    over_limit.members.push(member(
        "data/extra.bin",
        Role::Content,
        ContentDigest::of_bytes(b"x"),
    ));
    assert_eq!(
        Inventory::default().register(over_limit),
        Err(Error::TooLarge)
    );
    let mut zero_id = incomplete(1, Vec::new());
    zero_id.id = [0; 32];
    assert_eq!(
        Inventory::default().register(zero_id),
        Err(Error::InvalidRecord)
    );
    for length in [0, MAX_METADATA_BYTES + 1] {
        let mut entry = incomplete(1, Vec::new());
        entry.manifest = ContentDigest::from_parts(length, [3; 32]);
        assert_eq!(
            Inventory::default().register(entry),
            Err(Error::InvalidRecord)
        );
    }
    let entry = incomplete(
        1,
        vec![member(
            RECEIPT_TEMPORARY,
            Role::ReceiptTemporary,
            ContentDigest::from_parts(MAX_RECEIPT_BYTES + 1, [4; 32]),
        )],
    );
    assert_eq!(
        Inventory::default().register(entry),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn full_inventory_never_evicts_and_still_admits_exact_repeat() {
    let mut inventory = Inventory::default();
    for id in 1..=MAX_ARTIFACTS as u8 {
        let entry = if id % 2 == 0 {
            incomplete(id, Vec::new())
        } else {
            artifact(id)
        };
        inventory = inventory.register(entry).unwrap();
    }
    assert_eq!(decode(&encode(&inventory).unwrap()).unwrap(), inventory);
    assert_eq!(inventory.register(artifact(33)), Err(Error::TooLarge));
    assert_eq!(inventory.register(artifact(1)).unwrap(), inventory);
    assert_eq!(
        inventory.register(incomplete(2, Vec::new())).unwrap(),
        inventory
    );
    let mut input: Value = serde_json::from_slice(&encode(&inventory).unwrap()).unwrap();
    let extra = input["artifacts"][0].clone();
    input["artifacts"].as_array_mut().unwrap().push(extra);
    assert_eq!(decode_value(&input), Err(Error::TooLarge));
    let mut entry = artifact(1);
    entry.members[3].baseline = ContentDigest::from_parts(MAX_PAYLOAD_BYTES + 1, [1; 32]);
    assert_eq!(Inventory::default().register(entry), Err(Error::TooLarge));
    let mut entry = artifact(1);
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
        input["artifacts"][0]["members"][0][field] = json!(true);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for observation in ["pending", "removed"] {
        let mut input = value();
        input["artifacts"][0]["members"][0]["observation"] = json!(observation);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    let mut input = value();
    input["artifacts"][0]["creation_custody"] = json!(true);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}

#[test]
fn old_installation_shapes_and_invalid_publication_fields_have_no_reader() {
    for input in [
        r#"{"format_version":1,"installations":[],"reclamations":[]}"#,
        r#"{"format_version":1,"artifacts":[],"installations":[],"reclamations":[]}"#,
    ] {
        assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    }
    let mut old = value();
    let entry = &mut old["artifacts"][0];
    let id = entry.as_object_mut().unwrap().remove("id").unwrap();
    let publication = entry
        .as_object_mut()
        .unwrap()
        .remove("publication")
        .unwrap();
    entry["installation"] = id;
    entry["receipt"] = publication["receipt"].clone();
    assert_eq!(decode_value(&old), Err(Error::InvalidRecord));
    for field in [
        "installed",
        "installation",
        "receipt",
        "file_identity",
        "delete_authority",
        "extra",
    ] {
        let mut input = value();
        input["artifacts"][0][field] = json!(true);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for publication in [
        Value::Null,
        json!("incomplete"),
        json!({}),
        json!({"state": "unknown"}),
        json!({"state": "ready"}),
        json!({"state": "ready", "receipt": {"length": 64, "hash": vec![2; 32]}, "extra": 1}),
        json!({"state": "ready", "receipt": {"length": 64, "hash": vec![2; 32]}, "file_identity": 1}),
        json!({"state": "ready", "receipt": {"length": 64, "hash": vec![2; 32]}, "delete_authority": true}),
        json!({"state": "incomplete", "receipt": {"length": 64, "hash": vec![2; 32]}}),
        json!({"state": "incomplete", "extra": 1}),
    ] {
        let mut input = value();
        if publication["state"] == "incomplete" {
            input["artifacts"][0]["members"] = json!([]);
        }
        input["artifacts"][0]["publication"] = publication;
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    let mut missing = value();
    missing["artifacts"][0]
        .as_object_mut()
        .unwrap()
        .remove("publication");
    assert_eq!(decode_value(&missing), Err(Error::InvalidRecord));
    let mut input = value();
    input["reclamations"] = json!([{
        "id": vec![1; 32],
        "installation": vec![1; 32],
        "phase": "intent",
        "members": [],
    }]);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}
#[test]
fn malformed_duplicate_unknown_deep_and_oversized_input_is_bounded() {
    for input in [
        "",
        "null",
        "[]",
        "{}",
        r#"{"format_version":1,"artifacts":[]}"#,
        r#"{"format_version":1,"artifacts":[],"reclamations":null}"#,
        r#"{"format_version":1,"artifacts":[],"reclamations":[],"extra":true}"#,
        r#"{"format_version":1,"format_\u0076ersion":1,"artifacts":[],"reclamations":[]}"#,
        r#"{"format_version":1,"artifacts":[],"reclamations":[],"reclamations":[]}"#,
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
    input["artifacts"][0]["members"][0]["path"] = json!("a".repeat(MAX_PATH_BYTES + 1));
    assert!(decode_value(&input).is_err());
}

#[test]
fn incomplete_receipt_members_and_recovery_cannot_invent_ready_or_missing_payloads() {
    let temporary = member(
        RECEIPT_TEMPORARY,
        Role::ReceiptTemporary,
        ContentDigest::of_bytes(b"receipt"),
    );
    for members in [Vec::new(), vec![temporary.clone()]] {
        let inventory = Inventory::default()
            .register(incomplete(1, members))
            .unwrap();
        let review = reclamation(1, ReclamationPhase::Intent, &[]);
        let accepted = inventory.with_reclamation(&review).unwrap();
        assert_eq!(decode(&encode(&accepted).unwrap()).unwrap(), accepted);
        for path in [RECEIPT_NAME, "bin/tool.dll", "samples/readme.txt"] {
            let review = reclamation(
                1,
                ReclamationPhase::Intent,
                &[(path, ReclamationMemberState::Missing)],
            );
            assert_eq!(
                inventory.with_reclamation(&review),
                Err(Error::InvalidRecord)
            );
        }
    }
    let accepted = Inventory::default()
        .register(incomplete(1, vec![temporary]))
        .unwrap()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Results,
            &[(RECEIPT_TEMPORARY, ReclamationMemberState::Removed)],
        ))
        .unwrap();
    assert_eq!(decode(&encode(&accepted).unwrap()).unwrap(), accepted);
    let mut input: Value = serde_json::from_slice(&encode(&accepted).unwrap()).unwrap();
    input["artifacts"][0]["members"][0]["path"] = json!(RECEIPT_NAME);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    let mut input = value();
    input["artifacts"][0]["publication"] = json!({"state": "incomplete"});
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    for length in [0, MAX_RECEIPT_BYTES + 1] {
        let mut input = value();
        input["artifacts"][0]["publication"]["receipt"]["length"] = json!(length);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
}

#[test]
fn reclamation_roundtrips_intent_and_every_mixed_result_without_authority() {
    let inventory = inventory()
        .register(artifact(2))
        .unwrap()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Intent,
            &[
                ("bin/tool.dll", ReclamationMemberState::Pending),
                ("samples/readme.txt", ReclamationMemberState::Missing),
            ],
        ))
        .unwrap()
        .with_reclamation(&reclamation(
            2,
            ReclamationPhase::Results,
            &[
                (RECEIPT_NAME, ReclamationMemberState::Pending),
                ("bin/tool.dll", ReclamationMemberState::Removed),
                (MANIFEST_NAME, ReclamationMemberState::Missing),
                ("samples/readme.txt", ReclamationMemberState::Failed),
            ],
        ))
        .unwrap();
    let bytes = encode(&inventory).unwrap();
    assert_eq!(decode(&bytes).unwrap(), inventory);
    assert_eq!(encode(&decode(&bytes).unwrap()).unwrap(), bytes);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert_eq!(value["reclamations"][0]["phase"], json!("intent"));
    assert_eq!(value["reclamations"][1]["phase"], json!("results"));
    assert_eq!(
        value["reclamations"][1]["members"],
        json!([
            {"path": RECEIPT_NAME, "state": "pending"},
            {"path": "bin/tool.dll", "state": "removed"},
            {"path": MANIFEST_NAME, "state": "missing"},
            {"path": "samples/readme.txt", "state": "failed"},
        ])
    );
    for record in value["reclamations"].as_array().unwrap() {
        assert_eq!(record.as_object().unwrap().len(), 4);
        for member in record["members"].as_array().unwrap() {
            assert_eq!(member.as_object().unwrap().len(), 2);
        }
    }
}

#[test]
fn registration_and_observation_refresh_preserve_intent_and_results() {
    let previous = inventory()
        .register(artifact(2))
        .unwrap()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Intent,
            &[("bin/tool.dll", ReclamationMemberState::Pending)],
        ))
        .unwrap()
        .with_reclamation(&reclamation(
            2,
            ReclamationPhase::Results,
            &[
                ("bin/tool.dll", ReclamationMemberState::Removed),
                ("samples/readme.txt", ReclamationMemberState::Failed),
            ],
        ))
        .unwrap();
    let registered = previous.register(artifact(3)).unwrap();
    assert_eq!(registered.reclamations, previous.reclamations);
    for id in [1, 2] {
        for observation in [
            Observation::Missing,
            Observation::Changed,
            Observation::Failed,
        ] {
            let mut refreshed = artifact(id);
            for member in &mut refreshed.members {
                member.observation = observation;
            }
            let next = registered.register(refreshed).unwrap();
            assert_eq!(next.reclamations, previous.reclamations);
            assert_eq!(decode(&encode(&next).unwrap()).unwrap(), next);
        }
    }
}

#[test]
fn explicit_new_review_replaces_only_its_artifacts_recovery_record() {
    let previous = inventory()
        .register(artifact(2))
        .unwrap()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Results,
            &[("bin/tool.dll", ReclamationMemberState::Removed)],
        ))
        .unwrap()
        .with_reclamation(&reclamation(
            2,
            ReclamationPhase::Results,
            &[("bin/tool.dll", ReclamationMemberState::Failed)],
        ))
        .unwrap();
    let before = previous.clone();
    let mut next_review = reclamation(
        1,
        ReclamationPhase::Intent,
        &[
            ("samples/readme.txt", ReclamationMemberState::Pending),
            ("bin/tool.dll", ReclamationMemberState::Missing),
        ],
    );
    next_review.id = [9; 32];
    let original_review = next_review.clone();
    let next = previous.with_reclamation(&next_review).unwrap();
    assert_eq!(previous, before);
    assert_eq!(next_review, original_review);
    assert_eq!(next.reclamations.len(), 2);
    assert_eq!(next.reclamations[0].id, [9; 32]);
    assert_eq!(next.reclamations[0].phase, ReclamationPhase::Intent);
    assert_eq!(
        next.reclamations[0].members[0].path.as_ref(),
        "bin/tool.dll"
    );
    assert_eq!(next.reclamations[1], before.reclamations[1]);
    assert_eq!(decode(&encode(&next).unwrap()).unwrap(), next);
}

#[test]
fn recovery_rejects_zero_id_unregistered_artifact_and_duplicate_records() {
    let mut record = reclamation(
        1,
        ReclamationPhase::Intent,
        &[("bin/tool.dll", ReclamationMemberState::Pending)],
    );
    record.id = [0; 32];
    assert_eq!(
        inventory().with_reclamation(&record),
        Err(Error::InvalidRecord)
    );
    record.id = [1; 32];
    for artifact in [[0; 32], [2; 32]] {
        record.artifact = artifact;
        assert_eq!(
            inventory().with_reclamation(&record),
            Err(Error::Unregistered)
        );
    }
    record.artifact = [1; 32];
    let mut duplicate = inventory().with_reclamation(&record).unwrap();
    record.id = [3; 32];
    duplicate.reclamations.push(record);
    assert_eq!(duplicate.validate(), Err(Error::InvalidRecord));
    assert_eq!(encode(&duplicate), Err(Error::InvalidRecord));
    let mut input = value();
    input["reclamations"] = json!([
        {
            "id": vec![1; 32],
            "artifact": vec![1; 32],
            "phase": "intent",
            "members": [],
        },
        {
            "id": vec![2; 32],
            "artifact": vec![1; 32],
            "phase": "results",
            "members": [],
        },
    ]);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    let inventory = inventory()
        .register(artifact(2))
        .unwrap()
        .with_reclamation(&reclamation(1, ReclamationPhase::Intent, &[]))
        .unwrap();
    let mut reused_id = reclamation(2, ReclamationPhase::Intent, &[]);
    reused_id.id = [1; 32];
    assert_eq!(
        inventory.with_reclamation(&reused_id),
        Err(Error::InvalidRecord)
    );
    let mut duplicate = inventory.clone();
    duplicate.reclamations.push(reused_id);
    assert_eq!(encode(&duplicate), Err(Error::InvalidRecord));
}

#[test]
fn recovery_paths_are_a_unique_exact_subset_of_the_same_artifact() {
    let mut other = artifact(2);
    other.members[2].path = "bin/other.dll".into();
    let inventory = inventory().register(other).unwrap();
    for path in ["bin/other.dll", "bin/TOOL.dll", "unknown.txt"] {
        let record = reclamation(
            1,
            ReclamationPhase::Intent,
            &[(path, ReclamationMemberState::Pending)],
        );
        assert_eq!(
            inventory.with_reclamation(&record),
            Err(Error::InvalidRecord)
        );
    }
    for path in ["../outside", "a//b", "a:b", "a./b"] {
        let record = reclamation(
            1,
            ReclamationPhase::Intent,
            &[(path, ReclamationMemberState::Pending)],
        );
        assert_eq!(inventory.with_reclamation(&record), Err(Error::InvalidPath));
    }
    let record = reclamation(
        1,
        ReclamationPhase::Results,
        &[
            ("bin/tool.dll", ReclamationMemberState::Removed),
            ("bin/tool.dll", ReclamationMemberState::Pending),
        ],
    );
    assert_eq!(
        inventory.with_reclamation(&record),
        Err(Error::InvalidRecord)
    );
    let mut input: Value = serde_json::from_slice(&encode(&inventory).unwrap()).unwrap();
    input["reclamations"] = json!([{
        "id": vec![1; 32],
        "artifact": vec![2; 32],
        "phase": "results",
        "members": [{"path": "bin/tool.dll", "state": "removed"}],
    }]);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}

#[test]
fn intent_never_claims_removal_or_failure_and_results_retain_every_state() {
    for state in [
        ReclamationMemberState::Removed,
        ReclamationMemberState::Failed,
    ] {
        let record = reclamation(1, ReclamationPhase::Intent, &[("bin/tool.dll", state)]);
        assert_eq!(
            inventory().with_reclamation(&record),
            Err(Error::InvalidRecord)
        );
    }
    let record = reclamation(
        1,
        ReclamationPhase::Results,
        &[("bin/tool.dll", ReclamationMemberState::Removed)],
    );
    let accepted = inventory().with_reclamation(&record).unwrap();
    let mut input: Value = serde_json::from_slice(&encode(&accepted).unwrap()).unwrap();
    input["reclamations"][0]["phase"] = json!("intent");
    for state in ["removed", "failed"] {
        input["reclamations"][0]["members"][0]["state"] = json!(state);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    for phase in ["idle", "removing", "terminal", "unknown"] {
        input["reclamations"][0]["phase"] = json!(phase);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
    input["reclamations"][0]["phase"] = json!("results");
    input["reclamations"][0]["members"][0]["state"] = json!("present");
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}

#[test]
fn recovery_has_no_reader_for_native_identity_attempts_or_removal_permission() {
    let accepted = inventory()
        .with_reclamation(&reclamation(
            1,
            ReclamationPhase::Intent,
            &[("bin/tool.dll", ReclamationMemberState::Pending)],
        ))
        .unwrap();
    let value: Value = serde_json::from_slice(&encode(&accepted).unwrap()).unwrap();
    for field in ["file_identity", "delete_authority", "attempt", "extra"] {
        let mut input = value.clone();
        input["reclamations"][0][field] = json!(true);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
        let mut input = value.clone();
        input["reclamations"][0]["members"][0][field] = json!(true);
        assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    }
}

#[test]
fn recovery_capacity_roundtrips_all_artifacts_and_all_declared_members() {
    let mut inventory = Inventory::default();
    for id in 1..=MAX_ARTIFACTS as u8 {
        let mut entry = artifact(id);
        for index in entry.members.len()..MAX_MEMBERS {
            entry.members.push(member(
                &format!("data/item-{index:02}.bin"),
                Role::Content,
                ContentDigest::of_bytes(b"data"),
            ));
        }
        let record = Reclamation {
            id: [id; 32],
            artifact: [id; 32],
            phase: ReclamationPhase::Intent,
            members: entry
                .members
                .iter()
                .map(|member| ReclamationMember {
                    path: member.path.clone(),
                    state: ReclamationMemberState::Pending,
                })
                .collect(),
        };
        inventory = inventory
            .register(entry)
            .unwrap()
            .with_reclamation(&record)
            .unwrap();
    }
    assert_eq!(inventory.reclamations.len(), MAX_RECLAMATIONS);
    assert_eq!(decode(&encode(&inventory).unwrap()).unwrap(), inventory);
    let mut too_many_members = inventory.reclamations[0].clone();
    too_many_members
        .members
        .push(too_many_members.members[0].clone());
    assert_eq!(
        inventory.with_reclamation(&too_many_members),
        Err(Error::TooLarge)
    );
    let mut invalid = inventory.clone();
    invalid.reclamations[0] = too_many_members;
    assert_eq!(encode(&invalid), Err(Error::TooLarge));
    let mut input: Value = serde_json::from_slice(&encode(&inventory).unwrap()).unwrap();
    let member = input["reclamations"][0]["members"][0].clone();
    input["reclamations"][0]["members"]
        .as_array_mut()
        .unwrap()
        .push(member);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
    let mut invalid = inventory.clone();
    invalid.reclamations.push(invalid.reclamations[0].clone());
    assert_eq!(invalid.validate(), Err(Error::TooLarge));
    assert_eq!(encode(&invalid), Err(Error::TooLarge));
    let mut input: Value = serde_json::from_slice(&encode(&inventory).unwrap()).unwrap();
    let record = input["reclamations"][0].clone();
    input["reclamations"].as_array_mut().unwrap().push(record);
    assert_eq!(decode_value(&input), Err(Error::TooLarge));
    assert_eq!(
        inventory
            .with_reclamation(&inventory.reclamations[0])
            .unwrap(),
        inventory
    );
}
