use serde_json::json;

use super::*;

const ROOT_PACKAGE_ID: &str = "path+file:///project#sample-game@0.1.0";

#[test]
fn metadata_selects_exact_root_desktop_target_and_path_safe_graph() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let encoded = serde_json::to_vec(&metadata_value(&root)).unwrap();

    let facts = CargoMetadataFacts::parse(&encoded, &manifest).unwrap();
    let invocation = facts.desktop_invocation(test_host_target()).unwrap();

    assert_eq!(facts.root_package().name(), "sample-game");
    assert_eq!(facts.root_package().version(), "0.1.0");
    assert_eq!(facts.package_graph().len(), 2);
    assert!(
        facts
            .package_graph()
            .iter()
            .any(|package| package.source_kind() == CargoPackageSourceKind::Registry)
    );
    assert_eq!(invocation.package_name(), "sample-game");
    assert_eq!(invocation.manifest_path(), manifest);
    assert_eq!(invocation.target_name(), "desktop");
    assert_eq!(invocation.profile(), "dev");
    assert_eq!(invocation.features(), ["desktop"]);
    assert_eq!(invocation.host_target(), test_host_target());
}

#[test]
fn metadata_requires_the_real_cargo_version_field() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = metadata_value(&root);
    metadata.as_object_mut().unwrap().remove("version");
    metadata["format_version"] = json!(1);

    let error =
        CargoMetadataFacts::parse(&serde_json::to_vec(&metadata).unwrap(), &manifest).unwrap_err();

    assert!(matches!(
        error,
        CargoMetadataFactsError::InvalidField { field: "version" }
    ));
}

#[test]
fn metadata_rejects_a_wrong_desktop_target_contract() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["targets"][0]["required-features"] = json!([]);

    let error =
        CargoMetadataFacts::parse(&serde_json::to_vec(&metadata).unwrap(), &manifest).unwrap_err();

    assert!(matches!(
        error,
        CargoMetadataFactsError::DesktopTargetMismatch {
            field: "required-features"
        }
    ));
}

#[test]
fn metadata_without_a_resolve_graph_derives_the_root_from_the_manifest() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = metadata_value(&root);
    metadata["resolve"] = Value::Null;

    let facts =
        CargoMetadataFacts::parse(&serde_json::to_vec(&metadata).unwrap(), &manifest).unwrap();

    assert_eq!(facts.package_name(), "sample-game");
}

#[test]
fn a_present_resolve_root_must_match_the_manifest_owner() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = metadata_value(&root);
    metadata["resolve"]["root"] = json!("registry+https://example.invalid/index#dependency@1.2.3");

    let error =
        CargoMetadataFacts::parse(&serde_json::to_vec(&metadata).unwrap(), &manifest).unwrap_err();

    assert!(matches!(
        error,
        CargoMetadataFactsError::RootResolutionMismatch
    ));
}

#[test]
fn metadata_without_a_package_owning_the_manifest_fails_closed() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = metadata_value(&root);
    metadata["packages"].as_array_mut().unwrap().remove(0);

    let error =
        CargoMetadataFacts::parse(&serde_json::to_vec(&metadata).unwrap(), &manifest).unwrap_err();

    assert!(matches!(error, CargoMetadataFactsError::MissingRootPackage));
}

#[test]
fn metadata_input_is_bounded_before_json_allocation() {
    let oversized = vec![b' '; MAX_METADATA_BYTES + 1];

    let error = CargoMetadataFacts::parse(&oversized, &test_root().join("Cargo.toml")).unwrap_err();

    assert!(matches!(error, CargoMetadataFactsError::InputLimitExceeded));
}

#[test]
fn incremental_chunks_select_one_fresh_desktop_artifact() {
    let invocation = test_invocation();
    let expected_executable = expected_executable(&invocation);
    let stream = json_stream([
        artifact_message(
            ROOT_PACKAGE_ID,
            invocation.manifest_path(),
            &invocation.desktop_source_path,
            &expected_executable,
            true,
        ),
        diagnostic_message("warning", Some("W0001"), "bounded warning"),
        json!({"reason": "build-finished", "success": true}),
    ]);
    let mut decoder = CargoDesktopBuildDecoder::new(invocation);

    for chunk in stream.chunks(7) {
        decoder.push(chunk).unwrap();
    }
    let facts = decoder.finish().unwrap();

    assert_eq!(facts.artifact().executable(), expected_executable);
    assert!(facts.artifact().fresh());
    assert_eq!(facts.diagnostics().entries().len(), 1);
    assert_eq!(facts.diagnostics().entries()[0].code(), Some("W0001"));
}

#[test]
fn fresh_false_is_a_valid_current_build_artifact() {
    let invocation = test_invocation();
    let expected_executable = expected_executable(&invocation);
    let mut decoder = CargoDesktopBuildDecoder::new(invocation.clone());

    push_value(
        &mut decoder,
        artifact_message(
            ROOT_PACKAGE_ID,
            invocation.manifest_path(),
            &invocation.desktop_source_path,
            &expected_executable,
            false,
        ),
    )
    .unwrap();
    push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap();

    let facts = decoder.finish().unwrap();
    assert!(!facts.artifact().fresh());
}

#[test]
fn foreign_artifacts_are_ignored_before_exact_selection() {
    let invocation = test_invocation();
    let expected_executable = expected_executable(&invocation);
    let mut decoder = CargoDesktopBuildDecoder::new(invocation.clone());

    push_value(
        &mut decoder,
        artifact_message(
            "registry+https://example.invalid/index#dependency@1.0.0",
            &test_root().join("dependency/Cargo.toml"),
            &test_root().join("dependency/src/lib.rs"),
            &test_root().join("target/dependency"),
            true,
        ),
    )
    .unwrap();
    push_value(
        &mut decoder,
        artifact_message(
            ROOT_PACKAGE_ID,
            invocation.manifest_path(),
            &invocation.desktop_source_path,
            &expected_executable,
            true,
        ),
    )
    .unwrap();
    push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap();

    assert_eq!(
        decoder.finish().unwrap().artifact().executable(),
        expected_executable
    );
}

#[test]
fn duplicate_exact_artifact_is_rejected() {
    let invocation = test_invocation();
    let expected_executable = expected_executable(&invocation);
    let artifact = artifact_message(
        ROOT_PACKAGE_ID,
        invocation.manifest_path(),
        &invocation.desktop_source_path,
        &expected_executable,
        true,
    );
    let mut decoder = CargoDesktopBuildDecoder::new(invocation);

    push_value(&mut decoder, artifact.clone()).unwrap();
    let error = push_value(&mut decoder, artifact).unwrap_err();

    assert!(matches!(error, CargoDesktopBuildError::DuplicateArtifact));
}

#[test]
fn malformed_json_is_a_typed_build_failure() {
    let mut decoder = CargoDesktopBuildDecoder::new(test_invocation());

    let error = decoder.push(b"{not-json}\n").unwrap_err();

    assert!(matches!(
        error,
        CargoDesktopBuildError::Message(CargoJsonMessageError::MalformedJson(_))
    ));
}

#[test]
fn missing_artifact_is_rejected_after_successful_build_finished() {
    let mut decoder = CargoDesktopBuildDecoder::new(test_invocation());
    push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap();

    let error = decoder.finish().unwrap_err();

    assert!(matches!(error, CargoDesktopBuildError::MissingArtifact));
}

#[test]
fn a_different_root_binary_is_foreign_to_the_desktop_selector() {
    let invocation = test_invocation();
    let mut artifact = artifact_message(
        ROOT_PACKAGE_ID,
        invocation.manifest_path(),
        &invocation.desktop_source_path,
        &expected_executable(&invocation),
        true,
    );
    artifact["target"]["name"] = json!("tool");
    let mut decoder = CargoDesktopBuildDecoder::new(invocation);

    push_value(&mut decoder, artifact).unwrap();
    push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap();

    assert!(matches!(
        decoder.finish().unwrap_err(),
        CargoDesktopBuildError::MissingArtifact
    ));
}

#[test]
fn failed_build_finished_is_distinct_from_missing_build_finished() {
    let mut failed = CargoDesktopBuildDecoder::new(test_invocation());
    push_value(
        &mut failed,
        json!({"reason": "build-finished", "success": false}),
    )
    .unwrap();
    let failed_error = failed.finish().unwrap_err();

    let mut missing = CargoDesktopBuildDecoder::new(test_invocation());
    let missing_error = missing.finish().unwrap_err();

    assert!(matches!(failed_error, CargoDesktopBuildError::BuildFailed));
    assert!(matches!(
        missing_error,
        CargoDesktopBuildError::MissingBuildFinished
    ));
}

#[test]
fn duplicate_build_finished_is_rejected() {
    let mut decoder = CargoDesktopBuildDecoder::new(test_invocation());
    push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap();

    let error = push_value(
        &mut decoder,
        json!({"reason": "build-finished", "success": true}),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CargoDesktopBuildError::DuplicateBuildFinished
    ));
}

#[test]
fn non_absolute_executable_is_rejected() {
    let invocation = test_invocation();
    let mut decoder = CargoDesktopBuildDecoder::new(invocation.clone());
    let mut artifact = artifact_message(
        ROOT_PACKAGE_ID,
        invocation.manifest_path(),
        &invocation.desktop_source_path,
        &expected_executable(&invocation),
        true,
    );
    artifact["executable"] = json!("target/debug/desktop");

    let error = push_value(&mut decoder, artifact).unwrap_err();

    assert!(matches!(
        error,
        CargoDesktopBuildError::NonAbsoluteExecutable
    ));
}

#[test]
fn non_executable_desktop_artifact_is_rejected() {
    let invocation = test_invocation();
    let mut artifact = artifact_message(
        ROOT_PACKAGE_ID,
        invocation.manifest_path(),
        &invocation.desktop_source_path,
        &expected_executable(&invocation),
        true,
    );
    artifact["executable"] = Value::Null;
    let mut decoder = CargoDesktopBuildDecoder::new(invocation);

    let error = push_value(&mut decoder, artifact).unwrap_err();

    assert!(matches!(
        error,
        CargoDesktopBuildError::NonExecutableArtifact
    ));
}

#[test]
fn exact_target_with_wrong_host_profile_path_is_rejected() {
    let invocation = test_invocation();
    let mut decoder = CargoDesktopBuildDecoder::new(invocation.clone());
    let wrong_executable = test_root().join("target/release/desktop");

    let error = push_value(
        &mut decoder,
        artifact_message(
            ROOT_PACKAGE_ID,
            invocation.manifest_path(),
            &invocation.desktop_source_path,
            &wrong_executable,
            true,
        ),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CargoDesktopBuildError::ArtifactMismatch {
            field: CargoArtifactMismatchField::ExecutablePath
        }
    ));
}

#[test]
fn structured_diagnostics_are_truncated_and_count_bounded() {
    let mut decoder = CargoDesktopBuildDecoder::new(test_invocation());
    let long_message = "x".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES + 17);
    push_value(
        &mut decoder,
        diagnostic_message("error", None, &long_message),
    )
    .unwrap();
    for index in 0..300 {
        push_value(
            &mut decoder,
            diagnostic_message("warning", None, &format!("warning {index}")),
        )
        .unwrap();
    }

    let diagnostics = decoder.diagnostics();
    assert_eq!(diagnostics.entries().len(), MAX_DIAGNOSTIC_COUNT);
    assert_eq!(diagnostics.dropped_count(), 45);
    assert_eq!(diagnostics.truncated_count(), 1);
    assert_eq!(
        diagnostics.entries()[0].message().len(),
        MAX_DIAGNOSTIC_MESSAGE_BYTES
    );
}

fn test_invocation() -> CargoDesktopBuildInvocation {
    let root = test_root();
    let metadata = serde_json::to_vec(&metadata_value(&root)).unwrap();
    CargoMetadataFacts::parse(&metadata, &root.join("Cargo.toml"))
        .unwrap()
        .desktop_invocation(test_host_target())
        .unwrap()
}

fn metadata_value(root: &Path) -> Value {
    json!({
        "version": 1,
        "packages": [
            {
                "name": "sample-game",
                "version": "0.1.0",
                "id": ROOT_PACKAGE_ID,
                "source": null,
                "manifest_path": root.join("Cargo.toml"),
                "features": {"desktop": []},
                "targets": [{
                    "kind": ["bin"],
                    "crate_types": ["bin"],
                    "name": "desktop",
                    "src_path": root.join("src/bin/desktop.rs"),
                    "required-features": ["desktop"]
                }]
            },
            {
                "name": "dependency",
                "version": "1.2.3",
                "id": "registry+https://example.invalid/index#dependency@1.2.3",
                "source": "registry+https://example.invalid/index",
                "manifest_path": root.join("registry/dependency/Cargo.toml"),
                "features": {},
                "targets": []
            }
        ],
        "workspace_members": [ROOT_PACKAGE_ID],
        "workspace_root": root,
        "target_directory": root.join("target"),
        "resolve": {"root": ROOT_PACKAGE_ID}
    })
}

fn artifact_message(
    package_id: &str,
    manifest_path: &Path,
    source_path: &Path,
    executable: &Path,
    fresh: bool,
) -> Value {
    json!({
        "reason": "compiler-artifact",
        "package_id": package_id,
        "manifest_path": manifest_path,
        "target": {
            "kind": ["bin"],
            "crate_types": ["bin"],
            "name": "desktop",
            "src_path": source_path,
            "required-features": ["desktop"]
        },
        "profile": {"test": false},
        "features": ["desktop"],
        "executable": executable,
        "fresh": fresh
    })
}

fn diagnostic_message(level: &str, code: Option<&str>, message: &str) -> Value {
    json!({
        "reason": "compiler-message",
        "message": {
            "level": level,
            "code": code.map(|code| json!({"code": code})),
            "message": message
        }
    })
}

fn json_stream<const N: usize>(messages: [Value; N]) -> Vec<u8> {
    let mut stream = Vec::new();
    for message in messages {
        serde_json::to_writer(&mut stream, &message).unwrap();
        stream.push(b'\n');
    }
    stream
}

fn push_value(
    decoder: &mut CargoDesktopBuildDecoder,
    value: Value,
) -> Result<(), CargoDesktopBuildError> {
    let mut encoded = serde_json::to_vec(&value).unwrap();
    encoded.push(b'\n');
    decoder.push(&encoded)
}

fn expected_executable(invocation: &CargoDesktopBuildInvocation) -> PathBuf {
    invocation.expected_executable.clone()
}

fn test_host_target() -> &'static str {
    if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

fn test_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\project")
    } else {
        PathBuf::from("/project")
    }
}
