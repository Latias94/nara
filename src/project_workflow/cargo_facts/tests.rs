use std::{cell::Cell, path::PathBuf};

use serde_json::{Value, json};

use super::*;

const PACKAGE_ID: &str = "path+file:///project#sample-game@0.1.0";
const DEPENDENCY_ID: &str = "registry+https://example.invalid/index#dependency@1.2.3";

#[test]
fn selection_canonicalizes_requested_features_and_accepts_cargo_feature_arguments() {
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "shipping",
        "thumbv8m.main-none-eabi",
        [
            "renderer?/trace",
            "dep:renderer",
            "sample-game/render",
            "render.fast+simd",
            "dep:renderer",
        ],
        false,
    )
    .unwrap();

    assert_eq!(selection.package(), "sample-game");
    assert_eq!(selection.target(), "game-player");
    assert_eq!(selection.profile(), "shipping");
    assert_eq!(selection.target_triple(), "thumbv8m.main-none-eabi");
    assert_eq!(
        selection.features(),
        [
            "dep:renderer",
            "render.fast+simd",
            "renderer?/trace",
            "sample-game/render",
        ]
    );
    assert!(!selection.default_features());
}

#[test]
fn selection_rejects_option_injection_paths_controls_and_empty_arguments() {
    for invalid in ["", "--release", "a/b", r"a\b", "a b", "a\n", "a=1", "a:bin"] {
        assert!(
            CargoBuildSelection::new(invalid, "player", "dev", test_target(), ["play"], true)
                .is_err()
        );
        assert!(
            CargoBuildSelection::new("game", invalid, "dev", test_target(), ["play"], true)
                .is_err()
        );
        assert!(
            CargoBuildSelection::new("game", "player", invalid, test_target(), ["play"], true)
                .is_err()
        );
        assert!(
            CargoBuildSelection::new("game", "player", "dev", invalid, ["play"], true).is_err()
        );
    }
    for feature in [
        "",
        "--all-features",
        "a,b",
        "a b",
        "a\0",
        r"a\b",
        "a/",
        "/b",
        "a//b",
        "a/../b",
        "dep:",
        "a?b/c",
    ] {
        assert!(matches!(
            CargoBuildSelection::new("game", "player", "dev", test_target(), [feature], false),
            Err(CargoBuildSelectionError::InvalidField { field: "feature" })
        ));
    }
    assert!(matches!(
        CargoBuildSelection::new("game", "player", "dev", "custom.json", ["play"], false),
        Err(CargoBuildSelectionError::InvalidField {
            field: "target triple"
        })
    ));
    assert!(
        CargoBuildSelection::new(
            "x".repeat(MAX_PACKAGE_NAME_BYTES + 1),
            "player",
            "dev",
            test_target(),
            ["play"],
            false,
        )
        .is_err()
    );
    assert!(
        CargoBuildSelection::new(
            "game",
            "player",
            "dev",
            test_target(),
            ["x".repeat(MAX_FEATURE_BYTES + 1)],
            false,
        )
        .is_err()
    );
}

#[test]
fn feature_input_count_is_bounded_before_collection_or_deduplication() {
    let consumed = Cell::new(0);
    let features = std::iter::repeat_with(|| {
        consumed.set(consumed.get() + 1);
        "play"
    });
    assert!(matches!(
        CargoBuildSelection::new("game", "player", "dev", test_target(), features, false),
        Err(CargoBuildSelectionError::TooManyFeatures)
    ));
    assert_eq!(consumed.get(), MAX_FEATURE_COUNT + 1);
}

#[test]
fn metadata_selects_the_manifest_owner_binary_and_path_safe_graph() {
    let root = test_root();
    let selection = test_selection();
    let facts =
        parse_metadata(&metadata_value(&root), &root.join("Cargo.toml"), &selection).unwrap();
    let invocation = facts.build_invocation(selection).unwrap();

    assert_eq!(facts.package_id(), PACKAGE_ID);
    assert_eq!(facts.package().name(), "sample-game");
    assert_eq!(facts.package_version(), "0.1.0");
    assert_eq!(facts.package_graph().len(), 2);
    assert!(facts.package_graph().iter().any(|package| {
        package.name() == "dependency" && package.source_kind() == CargoPackageSourceKind::Registry
    }));
    assert_eq!(facts.package_source(), None);
    assert_eq!(facts.project_tool_protocol(), None);
    assert_eq!(invocation.package_name(), "sample-game");
    assert_eq!(invocation.package().version(), "0.1.0");
    assert_eq!(invocation.manifest_path(), root.join("Cargo.toml"));
    assert_eq!(invocation.workspace_root(), root);
    assert_eq!(invocation.target_directory(), test_root().join("target"));
    assert_eq!(
        invocation.source_path(),
        test_root().join("src/bin/game_player.rs")
    );
    assert_eq!(invocation.target_name(), "game-player");
    assert_eq!(invocation.profile(), "dev");
    assert_eq!(invocation.features(), ["play"]);
    assert_eq!(
        invocation.metadata_features(),
        ["default", "play", "render"]
    );
    assert!(invocation.default_features());
    assert_eq!(invocation.target_triple(), test_target());
}

#[test]
fn workspace_member_custom_profile_and_foreign_wasm_are_one_complete_build_selection() {
    let root = test_root();
    let member = root.join("games/demo");
    let manifest = member.join("Cargo.toml");
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "shipping",
        "wasm32-unknown-unknown",
        ["play"],
        true,
    )
    .unwrap();
    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["manifest_path"] = json!(manifest);
    metadata["packages"][0]["targets"][0]["src_path"] = json!(member.join("src/main.rs"));
    metadata["target_directory"] = json!(root.join("shared-output"));
    // A virtual workspace has no resolve.root, and its selected package is not in workspace_root.
    metadata["resolve"]["root"] = Value::Null;
    let bytes = serde_json::to_vec(&metadata).unwrap();
    assert_eq!(
        CargoMetadataFacts::package_name_for_manifest(&bytes, &manifest).unwrap(),
        "sample-game"
    );
    let facts = CargoMetadataFacts::parse(&bytes, &manifest, &selection).unwrap();
    let invocation = facts.build_invocation(selection).unwrap();
    assert_eq!(invocation.workspace_root(), root);
    assert_ne!(invocation.workspace_root(), manifest.parent().unwrap());
    let executable = root.join("shared-output/wasm32-unknown-unknown/shipping/game-player.wasm");
    let mut artifact = artifact_message(&invocation, true);
    artifact["executable"] = json!(executable);
    let mut decoder = CargoBuildDecoder::new(invocation);
    push_value(&mut decoder, artifact).unwrap();
    push_value(&mut decoder, build_finished(true)).unwrap();

    let built = decoder.finish().unwrap();
    let artifact = built.artifact();
    assert_eq!(artifact.package().name(), "sample-game");
    assert_eq!(artifact.target_name(), "game-player");
    assert_eq!(artifact.profile(), "shipping");
    assert_eq!(artifact.target_triple(), "wasm32-unknown-unknown");
    assert_eq!(artifact.features(), ["default", "play", "render"]);
    assert_eq!(artifact.executable(), executable);
}

#[test]
fn builtin_profiles_and_custom_profiles_bind_their_actual_cargo_output_directories() {
    let root = test_root();
    for (profile, directory) in [
        ("dev", "debug"),
        ("test", "debug"),
        ("release", "release"),
        ("bench", "release"),
        ("shipping", "shipping"),
    ] {
        let selection = CargoBuildSelection::new(
            "sample-game",
            "game-player",
            profile,
            test_target(),
            ["play"],
            true,
        )
        .unwrap();
        let facts =
            parse_metadata(&metadata_value(&root), &root.join("Cargo.toml"), &selection).unwrap();
        let invocation = facts.build_invocation(selection).unwrap();
        let target_directory = root.join("target").join(test_target());
        let executable = target_directory
            .join(directory)
            .join("cargo-reported-player");
        assert_eq!(
            invocation.executable_directory,
            target_directory.join(directory)
        );

        let mut artifact = artifact_message(&invocation, false);
        artifact["executable"] = json!(executable);
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        push_value(&mut decoder, artifact.clone()).unwrap();
        push_value(&mut decoder, build_finished(true)).unwrap();
        let built = decoder.finish().unwrap();
        assert_eq!(built.artifact().profile(), profile);
        assert_eq!(built.artifact().executable(), executable);

        let wrong_directory = if directory == "debug" {
            "release"
        } else {
            "debug"
        };
        artifact["executable"] = json!(
            target_directory
                .join(wrong_directory)
                .join("cargo-reported-player")
        );
        let mut decoder = CargoBuildDecoder::new(invocation);
        assert!(matches!(
            push_value(&mut decoder, artifact),
            Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::ExecutablePath,
            })
        ));
    }
}

#[test]
fn workspace_metadata_can_include_features_not_compiled_for_the_selected_binary() {
    let root = test_root();
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "dev",
        test_target(),
        ["play"],
        false,
    )
    .unwrap();
    let mut metadata = metadata_value(&root);
    let helper_id = "path+file:///project/tools/helper#workspace-helper@0.1.0";
    metadata["packages"].as_array_mut().unwrap().push(json!({
        "name": "workspace-helper", "version": "0.1.0", "id": helper_id, "source": null,
        "manifest_path": root.join("tools/helper/Cargo.toml"), "features": {}, "targets": []
    }));
    metadata["workspace_members"]
        .as_array_mut()
        .unwrap()
        .push(json!(helper_id));
    metadata["resolve"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": helper_id, "features": []
        }));
    // Other workspace members can enable default/render while this command requests only play.
    metadata["packages"][0]["features"]["play"] = json!([]);
    metadata["packages"][0]["targets"][0]["required-features"] = json!(["play"]);
    let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &selection).unwrap();
    assert_eq!(facts.metadata_features(), ["default", "play", "render"]);
    let invocation = facts.build_invocation(selection).unwrap();
    assert_eq!(invocation.features(), ["play"]);
    assert!(!invocation.default_features());
    assert_eq!(
        invocation.metadata_features(),
        ["default", "play", "render"]
    );

    let mut artifact = artifact_message(&invocation, false);
    artifact["features"] = json!(["play"]);
    let mut decoder = CargoBuildDecoder::new(invocation);
    push_value(&mut decoder, artifact).unwrap();
    push_value(&mut decoder, build_finished(true)).unwrap();
    assert_eq!(decoder.finish().unwrap().artifact().features(), ["play"]);
}

#[test]
fn discovery_needs_no_fixed_binary_resolve_graph_or_starter_protocol() {
    let root = test_root();
    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["targets"] = json!([]);
    metadata["resolve"] = Value::Null;
    assert_eq!(
        CargoMetadataFacts::package_name_for_manifest(
            &serde_json::to_vec(&metadata).unwrap(),
            &root.join("Cargo.toml"),
        )
        .unwrap(),
        "sample-game"
    );
}

#[test]
fn starter_protocol_is_an_optional_fact_and_is_not_a_generic_admission_gate() {
    let root = test_root();
    let mut metadata = metadata_value(&root);
    for (declaration, expected) in [
        (Value::Null, None),
        (json!({"nara": {"project-tool-protocol": 1}}), Some(1)),
        (json!({"nara": {"project-tool-protocol": 999}}), Some(999)),
    ] {
        metadata["packages"][0]["metadata"] = declaration;
        let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &test_selection()).unwrap();
        assert_eq!(facts.project_tool_protocol(), expected);
    }
}

#[test]
fn required_features_preserve_declarations_and_leave_eligibility_to_cargo() {
    let root = test_root();
    let selection = test_selection();
    let mut metadata = metadata_value(&root);
    // Neither package feature definitions nor resolve.root establish target eligibility here.
    metadata["packages"][0]["features"] = json!({});
    metadata["resolve"]["root"] = json!(DEPENDENCY_ID);
    let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &selection).unwrap();
    assert_eq!(selection.features(), ["play"]);
    assert_eq!(facts.metadata_features(), ["default", "play", "render"]);

    metadata["resolve"]["nodes"][0]["features"] = json!(["play"]);
    let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &selection).unwrap();
    assert_eq!(facts.metadata_features(), ["play"]);
    assert_eq!(facts.required_features.as_ref(), ["render"]);
    let invocation = facts.build_invocation(selection).unwrap();
    assert_eq!(invocation.features(), ["play"]);
    assert_eq!(invocation.required_features.as_ref(), ["render"]);
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    push_value(
        &mut decoder,
        diagnostic_message("error", None, "target requires feature render"),
    )
    .unwrap();
    push_value(&mut decoder, build_finished(false)).unwrap();
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::BuildFailed)
    ));

    let mut decoder = CargoBuildDecoder::new(invocation);
    push_value(&mut decoder, build_finished(true)).unwrap();
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::MissingArtifact)
    ));
}

#[test]
fn dependency_required_features_are_retained_and_matched_without_resolving_eligibility() {
    let root = test_root();
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "dev",
        test_target(),
        ["dependency/windowed"],
        false,
    )
    .unwrap();
    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["features"] = json!({});
    metadata["packages"][0]["targets"][0]["required-features"] = json!(["dependency/windowed"]);
    metadata["packages"][1]["features"] = json!({"windowed": []});
    metadata["resolve"]["nodes"][0]["features"] = json!([]);
    metadata["resolve"]["nodes"][1]["features"] = json!(["windowed"]);
    let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &selection).unwrap();
    assert!(facts.metadata_features().is_empty());
    let invocation = facts.build_invocation(selection).unwrap();
    assert_eq!(invocation.features(), ["dependency/windowed"]);
    assert_eq!(
        invocation.required_features.as_ref(),
        ["dependency/windowed"]
    );

    let artifact = artifact_message(&invocation, false);
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    push_value(&mut decoder, artifact.clone()).unwrap();
    push_value(&mut decoder, build_finished(true)).unwrap();
    assert!(decoder.finish().unwrap().artifact().features().is_empty());

    let mut mismatched = artifact;
    mismatched["target"]["required-features"] = json!(["dependency/other"]);
    let mut decoder = CargoBuildDecoder::new(invocation);
    assert!(matches!(
        push_value(&mut decoder, mismatched),
        Err(CargoBuildError::ArtifactMismatch {
            field: CargoArtifactMismatchField::RequiredFeatures,
        })
    ));
}

#[test]
fn metadata_order_and_same_named_library_do_not_change_the_binary_facts() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let selection = test_selection();
    let mut metadata = metadata_value(&root);
    let expected = parse_metadata(&metadata, &manifest, &selection).unwrap();
    let mut library = metadata["packages"][0]["targets"][0].clone();
    library["kind"] = json!(["lib"]);
    library["crate_types"] = json!(["lib"]);
    library["src_path"] = json!(root.join("src/lib.rs"));
    metadata["packages"][0]["targets"]
        .as_array_mut()
        .unwrap()
        .insert(0, library);
    metadata["packages"].as_array_mut().unwrap().reverse();
    metadata["resolve"]["nodes"]
        .as_array_mut()
        .unwrap()
        .reverse();
    metadata["resolve"]["nodes"][1]["features"] = json!(["play", "render", "default"]);
    assert_eq!(
        parse_metadata(&metadata, &manifest, &selection).unwrap(),
        expected
    );
}

#[test]
fn full_metadata_requires_the_selected_resolve_node_including_empty_feature_sets() {
    let root = test_root();
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "release",
        "wasm32-unknown-unknown",
        std::iter::empty::<&str>(),
        false,
    )
    .unwrap();
    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["targets"][0]["required-features"] = json!([]);
    metadata["resolve"]["nodes"][0]["features"] = json!([]);
    let facts = parse_metadata(&metadata, &root.join("Cargo.toml"), &selection).unwrap();
    assert!(facts.metadata_features().is_empty());
    let invocation = facts.build_invocation(selection.clone()).unwrap();
    let mut artifact = artifact_message(&invocation, false);
    artifact["target"]
        .as_object_mut()
        .unwrap()
        .remove("required-features");
    let mut decoder = CargoBuildDecoder::new(invocation);
    push_value(&mut decoder, artifact).unwrap();
    push_value(&mut decoder, build_finished(true)).unwrap();
    assert!(decoder.finish().unwrap().artifact().features().is_empty());
    metadata["resolve"]["nodes"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(matches!(
        parse_metadata(&metadata, &root.join("Cargo.toml"), &selection),
        Err(CargoMetadataFactsError::MissingSelectedResolveNode)
    ));
    metadata["resolve"] = Value::Null;
    assert!(matches!(
        parse_metadata(&metadata, &root.join("Cargo.toml"), &selection),
        Err(CargoMetadataFactsError::InvalidField {
            field: "resolve.nodes"
        })
    ));
}

#[test]
fn metadata_requires_version_exact_owner_requested_package_and_workspace_membership() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let selection = test_selection();
    let mut metadata = metadata_value(&root);
    metadata.as_object_mut().unwrap().remove("version");
    metadata["format_version"] = json!(1);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::InvalidField { field: "version" })
    ));
    assert!(matches!(
        CargoMetadataFacts::package_name_for_manifest(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest
        ),
        Err(CargoMetadataFactsError::InvalidField { field: "version" })
    ));

    let mut metadata = metadata_value(&root);
    metadata["packages"][0]["name"] = json!("other-game");
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::SelectedPackageMismatch)
    ));
    assert_eq!(
        CargoMetadataFacts::package_name_for_manifest(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest
        )
        .unwrap(),
        "other-game"
    );

    metadata["workspace_members"] = json!([]);
    assert!(matches!(
        CargoMetadataFacts::package_name_for_manifest(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest
        ),
        Err(CargoMetadataFactsError::SelectedPackageNotWorkspaceMember)
    ));
    metadata["packages"].as_array_mut().unwrap().remove(0);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::MissingSelectedPackage)
    ));
}

#[test]
fn duplicate_packages_manifest_owners_binaries_resolve_nodes_and_features_reject() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let selection = test_selection();
    let mut metadata = metadata_value(&root);
    let duplicate = metadata["packages"][0].clone();
    metadata["packages"].as_array_mut().unwrap().push(duplicate);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::DuplicatePackageId)
    ));
    metadata["packages"][2]["id"] = json!("path+file:///other#sample-game@0.1.0");
    assert!(matches!(
        CargoMetadataFacts::package_name_for_manifest(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest
        ),
        Err(CargoMetadataFactsError::DuplicateSelectedPackage)
    ));

    let mut metadata = metadata_value(&root);
    let duplicate = metadata["packages"][0]["targets"][0].clone();
    metadata["packages"][0]["targets"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::DuplicateSelectedTarget)
    ));

    let mut metadata = metadata_value(&root);
    let duplicate = metadata["resolve"]["nodes"][0].clone();
    metadata["resolve"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::DuplicateResolveNode)
    ));
    metadata["resolve"]["nodes"].as_array_mut().unwrap().pop();
    metadata["resolve"]["nodes"][0]["features"] = json!(["render", "render"]);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &selection),
        Err(CargoMetadataFactsError::InvalidField {
            field: "resolve.nodes[].features"
        })
    ));
}

#[test]
fn metadata_input_collection_counts_and_path_safe_graph_fields_are_bounded() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let oversized = vec![b' '; MAX_METADATA_BYTES + 1];
    assert!(matches!(
        CargoMetadataFacts::parse(&oversized, &manifest, &test_selection()),
        Err(CargoMetadataFactsError::InputLimitExceeded)
    ));
    assert!(matches!(
        CargoMetadataFacts::package_name_for_manifest(&oversized, &manifest),
        Err(CargoMetadataFactsError::InputLimitExceeded)
    ));
    let mut metadata = metadata_value(&root);
    metadata["packages"] = json!(vec![metadata["packages"][0].clone(); MAX_PACKAGE_COUNT + 1]);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &test_selection()),
        Err(CargoMetadataFactsError::PackageLimitExceeded)
    ));
    let mut metadata = metadata_value(&root);
    metadata["resolve"]["nodes"][0]["features"] = json!(vec!["render"; MAX_FEATURE_COUNT + 1]);
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &test_selection()),
        Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "resolve.nodes[].features"
        })
    ));
    let mut metadata = metadata_value(&root);
    metadata["packages"][1]["name"] = json!("private/path");
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &test_selection()),
        Err(CargoMetadataFactsError::InvalidField {
            field: "packages[].name"
        })
    ));
}

#[test]
fn invocation_cannot_reuse_metadata_for_another_selection() {
    let root = test_root();
    let facts = parse_metadata(
        &metadata_value(&root),
        &root.join("Cargo.toml"),
        &test_selection(),
    )
    .unwrap();
    let other_package = CargoBuildSelection::new(
        "other-game",
        "game-player",
        "dev",
        test_target(),
        ["play"],
        true,
    )
    .unwrap();
    assert!(matches!(
        facts.build_invocation(other_package),
        Err(CargoInvocationError::PackageMismatch)
    ));
    for selection in [
        CargoBuildSelection::new(
            "sample-game",
            "other-bin",
            "dev",
            test_target(),
            ["play"],
            true,
        )
        .unwrap(),
        CargoBuildSelection::new(
            "sample-game",
            "game-player",
            "release",
            test_target(),
            ["play"],
            true,
        )
        .unwrap(),
        CargoBuildSelection::new(
            "sample-game",
            "game-player",
            "dev",
            "wasm32-unknown-unknown",
            ["play"],
            true,
        )
        .unwrap(),
        CargoBuildSelection::new(
            "sample-game",
            "game-player",
            "dev",
            test_target(),
            ["render"],
            true,
        )
        .unwrap(),
        CargoBuildSelection::new(
            "sample-game",
            "game-player",
            "dev",
            test_target(),
            ["play"],
            false,
        )
        .unwrap(),
    ] {
        assert!(matches!(
            facts.build_invocation(selection),
            Err(CargoInvocationError::SelectionMismatch)
        ));
    }
}

#[test]
fn incremental_chunks_select_the_exact_artifact_and_collect_diagnostics() {
    let invocation = test_invocation();
    let executable = expected_executable(&invocation);
    let stream = json_stream([
        artifact_message(&invocation, true),
        diagnostic_message("warning", Some("W0001"), "bounded warning"),
        build_finished(true),
    ]);
    let mut decoder = CargoBuildDecoder::new(invocation);
    for chunk in stream.chunks(7) {
        decoder.push(chunk).unwrap();
    }
    let facts = decoder.finish().unwrap();
    assert_eq!(facts.artifact().executable(), executable);
    assert!(facts.artifact().fresh());
    assert_eq!(facts.diagnostics().entries().len(), 1);
    assert_eq!(facts.diagnostics().entries()[0].code(), Some("W0001"));
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::AlreadyFinished)
    ));
}

#[test]
fn fresh_false_unterminated_final_record_and_crlf_are_valid() {
    let invocation = test_invocation();
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    decoder.push(b" \r\n").unwrap();
    decoder
        .push(&serde_json::to_vec(&artifact_message(&invocation, false)).unwrap())
        .unwrap();
    decoder.push(b"\r\n").unwrap();
    decoder
        .push(&serde_json::to_vec(&build_finished(true)).unwrap())
        .unwrap();
    assert!(!decoder.finish().unwrap().artifact().fresh());
}

#[test]
fn foreign_packages_other_binaries_and_same_named_libraries_are_ignored() {
    let invocation = test_invocation();
    let mut dependency = artifact_message(&invocation, true);
    dependency["package_id"] = json!(DEPENDENCY_ID);
    dependency["manifest_path"] = json!(test_root().join("registry/dependency/Cargo.toml"));
    let mut other_bin = artifact_message(&invocation, true);
    other_bin["target"]["name"] = json!("another-bin");
    let mut library = artifact_message(&invocation, true);
    library["target"]["kind"] = json!(["lib"]);
    library["target"]["crate_types"] = json!(["lib"]);
    library["executable"] = Value::Null;
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    decoder
        .push(&json_stream([
            dependency,
            other_bin,
            library,
            artifact_message(&invocation, true),
            build_finished(true),
        ]))
        .unwrap();
    assert_eq!(
        decoder.finish().unwrap().artifact().executable(),
        expected_executable(&invocation)
    );
}

#[test]
fn duplicate_exact_artifact_is_terminal_and_cannot_be_repaired() {
    let invocation = test_invocation();
    let artifact = artifact_message(&invocation, true);
    let mut decoder = CargoBuildDecoder::new(invocation);
    push_value(&mut decoder, artifact.clone()).unwrap();
    assert!(matches!(
        push_value(&mut decoder, artifact),
        Err(CargoBuildError::DuplicateArtifact)
    ));
    assert!(matches!(
        push_value(&mut decoder, build_finished(true)),
        Err(CargoBuildError::AlreadyFinished)
    ));
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::AlreadyFinished)
    ));
}

#[test]
fn malformed_json_and_unknown_message_reasons_are_typed_failures() {
    let mut decoder = CargoBuildDecoder::new(test_invocation());
    assert!(matches!(
        decoder.push(b"{not-json}\n"),
        Err(CargoBuildError::Message(
            CargoJsonMessageError::MalformedJson(_)
        ))
    ));
    let mut decoder = CargoBuildDecoder::new(test_invocation());
    assert!(matches!(
        push_value(&mut decoder, json!({"reason": "unexpected"})),
        Err(CargoBuildError::Message(
            CargoJsonMessageError::UnknownReason
        ))
    ));
}

#[test]
fn missing_artifact_failed_build_and_missing_build_finished_are_distinct() {
    let mut missing_artifact = CargoBuildDecoder::new(test_invocation());
    push_value(&mut missing_artifact, build_finished(true)).unwrap();
    assert!(matches!(
        missing_artifact.finish(),
        Err(CargoBuildError::MissingArtifact)
    ));
    let mut failed = CargoBuildDecoder::new(test_invocation());
    push_value(&mut failed, build_finished(false)).unwrap();
    assert!(matches!(failed.finish(), Err(CargoBuildError::BuildFailed)));
    let mut unfinished = CargoBuildDecoder::new(test_invocation());
    assert!(matches!(
        unfinished.finish(),
        Err(CargoBuildError::MissingBuildFinished)
    ));
}

#[test]
fn duplicate_build_finished_and_messages_after_build_finished_are_rejected() {
    let mut duplicate = CargoBuildDecoder::new(test_invocation());
    push_value(&mut duplicate, build_finished(true)).unwrap();
    assert!(matches!(
        push_value(&mut duplicate, build_finished(true)),
        Err(CargoBuildError::DuplicateBuildFinished)
    ));
    let mut after = CargoBuildDecoder::new(test_invocation());
    push_value(&mut after, build_finished(true)).unwrap();
    assert!(matches!(
        push_value(&mut after, diagnostic_message("warning", None, "late")),
        Err(CargoBuildError::MessageAfterBuildFinished)
    ));
}

#[test]
fn artifact_must_match_manifest_bin_source_required_features_and_profile() {
    let invocation = test_invocation();
    for (field, value, mismatch) in [
        (
            "manifest_path",
            json!(test_root().join("other/Cargo.toml")),
            CargoArtifactMismatchField::Manifest,
        ),
        (
            "target.kind",
            json!(["bin", "lib"]),
            CargoArtifactMismatchField::TargetKind,
        ),
        (
            "target.crate_types",
            json!(["lib"]),
            CargoArtifactMismatchField::CrateTypes,
        ),
        (
            "target.src_path",
            json!(test_root().join("other.rs")),
            CargoArtifactMismatchField::SourcePath,
        ),
        (
            "target.required-features",
            json!([]),
            CargoArtifactMismatchField::RequiredFeatures,
        ),
        (
            "profile.test",
            json!(true),
            CargoArtifactMismatchField::Profile,
        ),
    ] {
        let mut artifact = artifact_message(&invocation, true);
        if let Some((parent, child)) = field.split_once('.') {
            artifact[parent][child] = value;
        } else {
            artifact[field] = value;
        }
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        assert!(
            matches!(push_value(&mut decoder, artifact), Err(CargoBuildError::ArtifactMismatch { field }) if field == mismatch)
        );
    }
}

#[test]
fn feature_order_is_normalized_but_duplicate_artifact_features_reject() {
    let invocation = test_invocation();
    let mut artifact = artifact_message(&invocation, true);
    artifact["features"] = json!(["render", "play", "default", "unexpected"]);
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    push_value(&mut decoder, artifact).unwrap();
    push_value(&mut decoder, build_finished(true)).unwrap();
    assert_eq!(
        decoder.finish().unwrap().artifact().features(),
        ["default", "play", "render", "unexpected"]
    );
    let mut artifact = artifact_message(&invocation, true);
    artifact["features"] = json!(["default", "play", "render", "render"]);
    let mut decoder = CargoBuildDecoder::new(invocation);
    assert!(matches!(
        push_value(&mut decoder, artifact),
        Err(CargoBuildError::Message(
            CargoJsonMessageError::InvalidField { field: "features" }
        ))
    ));
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::AlreadyFinished)
    ));
}

#[test]
fn actual_artifact_feature_count_and_feature_bytes_remain_bounded() {
    let invocation = test_invocation();
    for features in [
        json!(vec!["play"; MAX_FEATURE_COUNT + 1]),
        json!(["x".repeat(MAX_FEATURE_BYTES + 1)]),
    ] {
        let mut artifact = artifact_message(&invocation, true);
        artifact["features"] = features;
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        assert!(matches!(
            push_value(&mut decoder, artifact),
            Err(CargoBuildError::Message(
                CargoJsonMessageError::FieldLimitExceeded { field: "features" }
            ))
        ));
        assert!(matches!(
            decoder.finish(),
            Err(CargoBuildError::AlreadyFinished)
        ));
    }
}

#[test]
fn executable_must_be_absolute_non_null_and_a_direct_child_of_the_selected_directory() {
    let invocation = test_invocation();
    for (executable, expected) in [
        (Value::Null, "missing"),
        (json!("target/debug/game-player"), "relative"),
        (
            json!(test_root().join("target/release/game-player")),
            "directory",
        ),
        (
            json!(invocation.executable_directory.join("deps/game-player")),
            "directory",
        ),
        (
            json!(invocation.executable_directory.join("../game-player")),
            "directory",
        ),
        (
            json!(
                invocation
                    .target_directory()
                    .join("other-target/debug/game-player")
            ),
            "directory",
        ),
    ] {
        let mut artifact = artifact_message(&invocation, true);
        artifact["executable"] = executable;
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        let error = push_value(&mut decoder, artifact).unwrap_err();
        assert!(matches!(
            (expected, error),
            ("missing", CargoBuildError::NonExecutableArtifact)
                | ("relative", CargoBuildError::NonAbsoluteExecutable)
                | (
                    "directory",
                    CargoBuildError::ArtifactMismatch {
                        field: CargoArtifactMismatchField::ExecutablePath
                    }
                )
        ));
    }
}

#[test]
fn stream_record_and_record_count_limits_fail_before_unbounded_retention() {
    let mut stream = CargoBuildDecoder::new(test_invocation());
    stream.total_bytes = MAX_JSON_STREAM_BYTES;
    assert!(matches!(
        stream.push(b" "),
        Err(CargoBuildError::InputLimitExceeded)
    ));
    let mut record = CargoBuildDecoder::new(test_invocation());
    assert!(matches!(
        record.push(&vec![b' '; MAX_JSON_RECORD_BYTES + 1]),
        Err(CargoBuildError::RecordLimitExceeded)
    ));
    let mut count = CargoBuildDecoder::new(test_invocation());
    count.record_count = MAX_JSON_RECORDS;
    assert!(matches!(
        push_value(&mut count, build_finished(true)),
        Err(CargoBuildError::RecordCountExceeded)
    ));
}

#[test]
fn structured_diagnostics_are_utf8_truncated_count_bounded_and_preserved_after_failure() {
    let mut decoder = CargoBuildDecoder::new(test_invocation());
    let long_message = "⚠".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES / 3 + 17);
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
    push_value(&mut decoder, build_finished(false)).unwrap();
    assert!(matches!(
        decoder.finish(),
        Err(CargoBuildError::BuildFailed)
    ));
    let diagnostics = decoder.diagnostics();
    assert_eq!(diagnostics.entries().len(), MAX_DIAGNOSTIC_COUNT);
    assert_eq!(diagnostics.dropped_count(), 45);
    assert_eq!(diagnostics.truncated_count(), 1);
    assert_eq!(
        diagnostics.entries()[0].message().len(),
        MAX_DIAGNOSTIC_MESSAGE_BYTES / 3 * 3
    );
    assert_eq!(
        diagnostics.entries()[0].level(),
        CargoDiagnosticLevel::Error
    );
}

#[test]
fn diagnostic_byte_budget_does_not_retain_raw_rendered_output() {
    let mut decoder = CargoBuildDecoder::new(test_invocation());
    let message = "x".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES);
    for _ in 0..20 {
        let mut diagnostic = diagnostic_message("warning", None, &message);
        diagnostic["message"]["rendered"] = json!("private/path or environment canary");
        push_value(&mut decoder, diagnostic).unwrap();
    }
    let diagnostics = decoder.diagnostics();
    assert_eq!(
        diagnostics.entries().len(),
        MAX_DIAGNOSTIC_BYTES / MAX_DIAGNOSTIC_MESSAGE_BYTES
    );
    assert_eq!(diagnostics.dropped_count(), 4);
    assert!(
        diagnostics
            .entries()
            .iter()
            .all(|diagnostic| diagnostic.message() == message)
    );
}

fn test_selection() -> CargoBuildSelection {
    CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "dev",
        test_target(),
        ["play"],
        true,
    )
    .unwrap()
}

fn test_invocation() -> CargoBuildInvocation {
    let root = test_root();
    let selection = test_selection();
    parse_metadata(&metadata_value(&root), &root.join("Cargo.toml"), &selection)
        .unwrap()
        .build_invocation(selection)
        .unwrap()
}

fn parse_metadata(
    metadata: &Value,
    manifest: &Path,
    selection: &CargoBuildSelection,
) -> Result<CargoMetadataFacts, CargoMetadataFactsError> {
    CargoMetadataFacts::parse(&serde_json::to_vec(metadata).unwrap(), manifest, selection)
}

fn metadata_value(root: &Path) -> Value {
    json!({
        "version": 1,
        "packages": [
            {
                "name": "sample-game", "version": "0.1.0", "id": PACKAGE_ID, "source": null,
                "manifest_path": root.join("Cargo.toml"),
                "features": {"default": ["render"], "play": ["render"], "render": []},
                "targets": [{
                    "kind": ["bin"], "crate_types": ["bin"], "name": "game-player",
                    "src_path": root.join("src/bin/game_player.rs"), "required-features": ["render"]
                }]
            },
            {
                "name": "dependency", "version": "1.2.3", "id": DEPENDENCY_ID,
                "source": "registry+https://example.invalid/index",
                "manifest_path": root.join("registry/dependency/Cargo.toml"), "features": {}, "targets": []
            }
        ],
        "workspace_members": [PACKAGE_ID], "workspace_root": root, "target_directory": root.join("target"),
        "resolve": {
            "root": PACKAGE_ID,
            "nodes": [{"id": PACKAGE_ID, "features": ["render", "default", "play"]}, {"id": DEPENDENCY_ID, "features": []}]
        }
    })
}

fn artifact_message(invocation: &CargoBuildInvocation, fresh: bool) -> Value {
    json!({
        "reason": "compiler-artifact", "package_id": invocation.package_id,
        "manifest_path": invocation.manifest_path(),
        "target": {
            "kind": ["bin"], "crate_types": ["bin"], "name": invocation.target_name(),
            "src_path": invocation.source_path(), "required-features": invocation.required_features
        },
        "profile": {"test": false}, "features": invocation.metadata_features(),
        "executable": expected_executable(invocation), "fresh": fresh
    })
}

fn diagnostic_message(level: &str, code: Option<&str>, message: &str) -> Value {
    json!({
        "reason": "compiler-message",
        "message": {"level": level, "code": code.map(|code| json!({"code": code})), "message": message}
    })
}

fn build_finished(success: bool) -> Value {
    json!({"reason": "build-finished", "success": success})
}

fn json_stream<const N: usize>(messages: [Value; N]) -> Vec<u8> {
    let mut stream = Vec::new();
    for message in messages {
        serde_json::to_writer(&mut stream, &message).unwrap();
        stream.push(b'\n');
    }
    stream
}

fn push_value(decoder: &mut CargoBuildDecoder, value: Value) -> Result<(), CargoBuildError> {
    let mut encoded = serde_json::to_vec(&value).unwrap();
    encoded.push(b'\n');
    decoder.push(&encoded)
}

fn expected_executable(invocation: &CargoBuildInvocation) -> PathBuf {
    // A fixture filename, not a production suffix policy. The decoder binds only its directory.
    invocation
        .executable_directory
        .join("cargo-reported-player")
}

fn test_target() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

fn test_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\project")
    } else {
        PathBuf::from("/project")
    }
}
