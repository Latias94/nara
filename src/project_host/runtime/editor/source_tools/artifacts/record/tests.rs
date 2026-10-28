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

fn reclamation(
    installation: u8,
    phase: ReclamationPhase,
    members: &[(&str, ReclamationMemberState)],
) -> Reclamation {
    Reclamation {
        id: [installation; 32],
        installation: [installation; 32],
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
        br#"{"format_version":1,"installations":[],"reclamations":[]}"#
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
    let canonical = inventory()
        .register(installation(2))
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
    reordered.installations.reverse();
    for entry in &mut reordered.installations {
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
        r#"{"format_version":1,"installations":[]}"#,
        r#"{"format_version":1,"installations":[],"reclamations":null}"#,
        r#"{"format_version":1,"installations":[],"reclamations":[],"extra":true}"#,
        r#"{"format_version":1,"format_\u0076ersion":1,"installations":[],"reclamations":[]}"#,
        r#"{"format_version":1,"installations":[],"reclamations":[],"reclamations":[]}"#,
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

#[test]
fn reclamation_roundtrips_intent_and_every_mixed_result_without_authority() {
    let inventory = inventory()
        .register(installation(2))
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
        .register(installation(2))
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
    let registered = previous.register(installation(3)).unwrap();
    assert_eq!(registered.reclamations, previous.reclamations);
    for id in [1, 2] {
        for observation in [
            Observation::Missing,
            Observation::Changed,
            Observation::Failed,
        ] {
            let mut refreshed = installation(id);
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
fn explicit_new_review_replaces_only_its_installations_recovery_record() {
    let previous = inventory()
        .register(installation(2))
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
fn recovery_rejects_zero_id_unregistered_installation_and_duplicate_records() {
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
    for installation in [[0; 32], [2; 32]] {
        record.installation = installation;
        assert_eq!(
            inventory().with_reclamation(&record),
            Err(Error::Unregistered)
        );
    }
    record.installation = [1; 32];
    let mut duplicate = inventory().with_reclamation(&record).unwrap();
    record.id = [3; 32];
    duplicate.reclamations.push(record);
    assert_eq!(duplicate.validate(), Err(Error::InvalidRecord));
    assert_eq!(encode(&duplicate), Err(Error::InvalidRecord));
    let mut input = value();
    input["reclamations"] = json!([
        {
            "id": vec![1; 32],
            "installation": vec![1; 32],
            "phase": "intent",
            "members": [],
        },
        {
            "id": vec![2; 32],
            "installation": vec![1; 32],
            "phase": "results",
            "members": [],
        },
    ]);
    assert_eq!(decode_value(&input), Err(Error::InvalidRecord));
}

#[test]
fn recovery_paths_are_a_unique_exact_subset_of_the_same_installation() {
    let mut other = installation(2);
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
        "installation": vec![2; 32],
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
fn recovery_capacity_roundtrips_all_installations_and_all_declared_members() {
    let mut inventory = Inventory::default();
    for id in 1..=MAX_INSTALLATIONS as u8 {
        let mut entry = installation(id);
        for index in entry.members.len()..MAX_MEMBERS {
            entry.members.push(member(
                &format!("data/item-{index:02}.bin"),
                Role::Content,
                ContentDigest::of_bytes(b"data"),
            ));
        }
        let record = Reclamation {
            id: [id; 32],
            installation: [id; 32],
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
