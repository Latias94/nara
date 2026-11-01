use super::*;
use crate::{ProjectAuthoringManifest, ProjectManifest};

const READY_MANIFEST: &str = r#"
schema_version = 1
[project]
name = "Required authoring state"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "project_location"
[authoring]
state = "ready"
"#;

const PENDING_MANIFEST: &str = r#"
schema_version = 1
[project]
name = "Pending authored upgrade"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "project_location"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[authoring.state.pending]
recovery_path = ".nara/upgrades/recovery.json"
recovery_digest = { length = 123, hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }
[profiles.dev.paths]
scenes = "dev-scenes"
[profiles.headless]
[profiles.server]
"#;

const HASH_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const HASH_BYTES: [u8; 32] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
    0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
];

#[test]
fn missing_authoring_section_or_state_is_a_parse_failure() {
    for source in [
        READY_MANIFEST.replace("[authoring]\nstate = \"ready\"\n", ""),
        READY_MANIFEST.replace("state = \"ready\"\n", ""),
        READY_MANIFEST.replace(
            "state = \"ready\"",
            "schema_bundle = \"schema/bundle.json\"",
        ),
    ] {
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(load.manifest.is_none(), "accepted missing state: {source}");
        assert!(load.has_errors());
        assert!(
            load.diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code().as_str() == "project.manifest.parse" })
        );
        assert!(toml::from_str::<ProjectManifest>(&source).is_err());
    }

    let ready: ProjectManifest = toml::from_str(READY_MANIFEST).unwrap();
    let mut missing_section = serde_json::to_value(&ready).unwrap();
    missing_section.as_object_mut().unwrap().remove("authoring");
    assert!(serde_json::from_value::<ProjectManifest>(missing_section).is_err());
    let mut missing_state = serde_json::to_value(&ready).unwrap();
    missing_state["authoring"]
        .as_object_mut()
        .unwrap()
        .remove("state");
    assert!(serde_json::from_value::<ProjectManifest>(missing_state).is_err());
}

#[test]
fn programmatic_default_and_all_serde_producers_write_explicit_ready() {
    let authoring = ProjectAuthoringManifest::default();
    assert_eq!(authoring.state, ProjectAuthoringState::Ready);
    let encoded = toml::to_string(&authoring).unwrap();
    assert!(encoded.contains("state = \"ready\""));
    assert_eq!(
        toml::from_str::<ProjectAuthoringManifest>(&encoded).unwrap(),
        authoring
    );
    assert!(toml::from_str::<ProjectAuthoringManifest>("").is_err());

    let load = ProjectManifest::parse_toml_str(READY_MANIFEST);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();
    assert_eq!(manifest.authoring, authoring);
    for encoded in [
        manifest.to_canonical_toml_bytes().unwrap(),
        toml::to_string(&manifest).unwrap().into_bytes(),
    ] {
        let value: toml::Value = toml::from_str(std::str::from_utf8(&encoded).unwrap()).unwrap();
        assert_eq!(value["authoring"]["state"].as_str(), Some("ready"));
        let reloaded = ProjectManifest::parse_toml_bytes(&encoded);
        assert!(!reloaded.has_errors(), "{:?}", reloaded.diagnostics);
        assert_eq!(reloaded.manifest.unwrap(), manifest);
    }
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["authoring"]["state"], "ready");
    assert_eq!(
        serde_json::from_value::<ProjectManifest>(json).unwrap(),
        manifest
    );
}

#[test]
fn pending_round_trips_the_exact_recovery_reference_without_file_admission() {
    let load = ProjectManifest::parse_toml_str(PENDING_MANIFEST);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let mut manifest = load.manifest.unwrap();
    let expected = ProjectAuthoringRecovery::new(
        ProjectPath::new(".nara/upgrades/recovery.json").unwrap(),
        ProjectAuthoringRecoveryDigest::from_parts(123, HASH_BYTES),
    )
    .unwrap();
    assert_eq!(
        expected.recovery_path().as_str(),
        ".nara/upgrades/recovery.json"
    );
    assert_eq!(expected.recovery_digest().length(), 123);
    assert_eq!(expected.recovery_digest().as_bytes(), &HASH_BYTES);
    assert_eq!(
        manifest.authoring.state,
        ProjectAuthoringState::Pending(expected)
    );

    let bytes = manifest.to_canonical_toml_bytes().unwrap();
    let reloaded = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!reloaded.has_errors(), "{:?}", reloaded.diagnostics);
    assert_eq!(reloaded.manifest.as_ref(), Some(&manifest));
    assert_eq!(
        reloaded
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap(),
        bytes
    );
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(
        json["authoring"]["state"]["pending"]["recovery_digest"]["hash"],
        HASH_HEX
    );
    assert_eq!(
        serde_json::from_value::<ProjectManifest>(json).unwrap(),
        manifest
    );

    let authoring = manifest.authoring.clone();
    manifest
        .replace_startup_scene(None, ProjectPath::new("repaired.scene.json").unwrap())
        .unwrap();
    let bytes = manifest.to_canonical_toml_bytes().unwrap();
    let repaired = ProjectManifest::parse_toml_bytes(&bytes).manifest.unwrap();
    assert_eq!(repaired.authoring, authoring);
}

#[test]
fn profiles_preserve_ready_and_pending_identically_and_cannot_override_them() {
    let mut manifest = ProjectManifest::parse_toml_str(PENDING_MANIFEST)
        .manifest
        .unwrap();
    for state in [
        manifest.authoring.state.clone(),
        ProjectAuthoringState::Ready,
    ] {
        manifest.authoring.state = state.clone();
        for profile in [None, Some("dev"), Some("headless"), Some("server")] {
            let settings = manifest.resolve_profile(profile).unwrap();
            assert_eq!(settings.authoring_state, state, "profile={profile:?}");
            assert_eq!(
                settings.authoring_schema_bundle.as_ref().unwrap().as_str(),
                "schema/authoring.bundle.json"
            );
        }
    }
    for source in [
        format!("{PENDING_MANIFEST}\n[profiles.dev.authoring]\nstate = 'ready'\n"),
        format!(
            "{READY_MANIFEST}\n[profiles.dev.authoring.state.pending]\nrecovery_path = '.nara/recovery.json'\nrecovery_digest = {{ length = 123, hash = '{HASH_HEX}' }}\n"
        ),
        format!("{READY_MANIFEST}\n[profiles.dev]\nauthoring_state = 'ready'\n"),
    ] {
        assert!(ProjectManifest::parse_toml_str(&source).manifest.is_none());
        assert!(toml::from_str::<ProjectManifest>(&source).is_err());
    }
}

#[test]
fn pending_paths_reject_escapes_unsupported_syntax_and_excess_bytes() {
    for path in [
        "",
        "../recovery.json",
        "private-recovery-canary/../../recovery.json",
        "/recovery.json",
        "C:/recovery.json",
        "C:\\recovery.json",
        "./recovery.json",
        "nested//recovery.json",
        "nested/recovery.json/",
        "recovery.json:stream",
        "recovery file.json",
        "recovery\n.json",
        "recovery\0.json",
        "recovery-\u{00e9}.json",
    ] {
        let encoded_path = toml::Value::String(path.to_owned()).to_string();
        let source = PENDING_MANIFEST.replace("\".nara/upgrades/recovery.json\"", &encoded_path);
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(load.manifest.is_none(), "accepted path: {path:?}");
        assert!(!format!("{load:?}").contains("private-recovery-canary"));
        assert!(toml::from_str::<ProjectManifest>(&source).is_err());
        if let Ok(path) = ProjectPath::new(path) {
            assert!(
                ProjectAuthoringRecovery::new(
                    path,
                    ProjectAuthoringRecoveryDigest::from_parts(123, HASH_BYTES)
                )
                .is_err()
            );
        }
    }

    let exact = format!("{}aa", "a/".repeat(511));
    assert_eq!(exact.len(), MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES);
    let digest = ProjectAuthoringRecoveryDigest::from_parts(123, HASH_BYTES);
    assert!(ProjectAuthoringRecovery::new(ProjectPath::new(&exact).unwrap(), digest).is_ok());
    let accepted = PENDING_MANIFEST.replace(".nara/upgrades/recovery.json", &exact);
    assert!(!ProjectManifest::parse_toml_str(&accepted).has_errors());
    let oversized = format!("{exact}a");
    assert!(matches!(
        ProjectAuthoringRecovery::new(ProjectPath::new(&oversized).unwrap(), digest),
        Err(ProjectAuthoringRecoveryPathError::TooLong { .. })
    ));
    let rejected = PENDING_MANIFEST.replace(".nara/upgrades/recovery.json", &oversized);
    assert!(
        ProjectManifest::parse_toml_str(&rejected)
            .manifest
            .is_none()
    );
}

#[test]
fn recovery_wire_requires_named_maps_in_json_and_toml() {
    let recovery = serde_json::json!({
        "recovery_path": ".nara/upgrades/recovery.json",
        "recovery_digest": { "length": 123, "hash": HASH_HEX },
    });
    assert!(serde_json::from_value::<ProjectAuthoringRecovery>(recovery.clone()).is_ok());
    for value in [
        serde_json::json!([".nara/upgrades/recovery.json", { "length": 123, "hash": HASH_HEX }]),
        serde_json::json!({ "recovery_path": ".nara/recovery.json", "recovery_digest": [123, HASH_HEX] }),
        serde_json::json!({ "recovery_path": ".nara/recovery.json", "recovery_digest": { "length": 123, "hash": HASH_HEX, "extra": true } }),
        serde_json::json!({ "recovery_path": ".nara/recovery.json", "recovery_digest": { "length": 123, "hash": HASH_HEX }, "extra": true }),
    ] {
        assert!(serde_json::from_value::<ProjectAuthoringRecovery>(value).is_err());
    }
    assert!(
        serde_json::from_value::<ProjectAuthoringRecoveryDigest>(serde_json::json!([
            123, HASH_HEX
        ]))
        .is_err()
    );
    let positional_recovery = format!(
        "[authoring]\nstate = {{ pending = ['.nara/recovery.json', {{ length = 123, hash = '{HASH_HEX}' }}] }}"
    );
    let source = READY_MANIFEST.replace("[authoring]\nstate = \"ready\"", &positional_recovery);
    assert!(ProjectManifest::parse_toml_str(&source).manifest.is_none());
    let source = PENDING_MANIFEST.replace(
        &format!("{{ length = 123, hash = \"{HASH_HEX}\" }}"),
        &format!("[123, '{HASH_HEX}']"),
    );
    assert!(ProjectManifest::parse_toml_str(&source).manifest.is_none());
}

#[test]
fn recovery_digest_preserves_length_bytes_and_canonical_hex_exactly() {
    for length in [0, 123, u64::from(u32::MAX)] {
        let digest = ProjectAuthoringRecoveryDigest::from_hex(length, HASH_HEX).unwrap();
        assert_eq!(
            digest,
            ProjectAuthoringRecoveryDigest::from_parts(length, HASH_BYTES)
        );
        assert_eq!(digest.length(), length);
        assert_eq!(digest.as_bytes(), &HASH_BYTES);
        let json = serde_json::to_value(digest).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "length": length, "hash": HASH_HEX })
        );
        assert_eq!(
            serde_json::from_value::<ProjectAuthoringRecoveryDigest>(json).unwrap(),
            digest
        );
        let encoded = toml::to_string(&digest).unwrap();
        assert_eq!(
            toml::from_str::<ProjectAuthoringRecoveryDigest>(&encoded).unwrap(),
            digest
        );
    }
    for hash in [
        String::new(),
        HASH_HEX[..63].to_owned(),
        format!("{HASH_HEX}0"),
        HASH_HEX.to_uppercase(),
        "g".repeat(64),
        format!("0x{HASH_HEX}"),
        format!(" {}", &HASH_HEX[..63]),
        "\u{00e9}".repeat(32),
    ] {
        assert!(ProjectAuthoringRecoveryDigest::from_hex(123, &hash).is_err());
        let source = PENDING_MANIFEST.replace(HASH_HEX, &hash);
        assert!(
            ProjectManifest::parse_toml_str(&source).manifest.is_none(),
            "accepted hash: {hash:?}"
        );
    }
}

#[test]
fn malformed_state_recovery_and_digest_records_reject_unknown_or_missing_data() {
    for source in [
        READY_MANIFEST.replace("state = \"ready\"", "state = \"pending\""),
        READY_MANIFEST.replace("state = \"ready\"", "state = \"Ready\""),
        READY_MANIFEST.replace("state = \"ready\"", "state = true"),
        READY_MANIFEST.replace(
            "state = \"ready\"",
            "state = 'ready'\nrecovery_path = '.nara/recovery.json'",
        ),
        READY_MANIFEST.replace("state = \"ready\"", "state = { pending = {} }"),
        PENDING_MANIFEST.replace("[authoring.state.pending]", "[authoring.state.upgrading]"),
        PENDING_MANIFEST.replace("recovery_path = \".nara/upgrades/recovery.json\"", ""),
        PENDING_MANIFEST.replace(
            "recovery_path = \".nara/upgrades/recovery.json\"",
            "recovery_path = 42",
        ),
        PENDING_MANIFEST.replace("recovery_digest = {", "extra = true\nrecovery_digest = {"),
        PENDING_MANIFEST.replace(
            &format!("recovery_digest = {{ length = 123, hash = \"{HASH_HEX}\" }}\n"),
            "",
        ),
        PENDING_MANIFEST.replace(&format!(", hash = \"{HASH_HEX}\""), ""),
        PENDING_MANIFEST.replace("length = 123, ", ""),
        PENDING_MANIFEST.replace("length = 123", "length = -1"),
        PENDING_MANIFEST.replace("length = 123", "length = 1.5"),
        PENDING_MANIFEST.replace("length = 123", "length = '123'"),
        PENDING_MANIFEST.replace("length = 123", "length = 123, algorithm = 'blake3'"),
        PENDING_MANIFEST.replace(&format!("hash = \"{HASH_HEX}\""), "hash = [0, 1, 2]"),
        PENDING_MANIFEST.replace(
            "[authoring.state.pending]",
            "[authoring.state]\nready = 'ready'\n[authoring.state.pending]",
        ),
    ] {
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(
            load.manifest.is_none(),
            "accepted malformed authoring state: {source}"
        );
        assert!(
            load.diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code().as_str() == "project.manifest.parse" })
        );
        assert!(toml::from_str::<ProjectManifest>(&source).is_err());
    }
}
