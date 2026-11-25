use super::*;

#[test]
fn declarations_distinguish_unobserved_features_from_an_observed_empty_set() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let selection = test_selection();
    let mut metadata = metadata_value(&root);
    metadata["resolve"]["nodes"][0]["features"] = json!([]);
    let full = parse_metadata(&metadata, &manifest, &selection)
        .unwrap()
        .build_invocation(selection.clone())
        .unwrap();
    let observed_empty: &[String] = &[];
    assert_eq!(full.metadata_features(), Some(observed_empty));

    let declarations = parse_declarations(&declaration_value(&root), &manifest, selection).unwrap();
    assert_eq!(declarations.metadata_features(), None);
    assert_eq!(declarations.features(), ["play"]);
    assert!(declarations.default_features());
}

#[test]
fn declarations_bind_a_workspace_member_custom_profile_and_foreign_target() {
    let root = test_root();
    let member = root.join("games/demo");
    let manifest = member.join("Cargo.toml");
    let selection = CargoBuildSelection::new(
        "sample-game",
        "game-player",
        "shipping",
        "wasm32-unknown-unknown",
        ["dependency/windowed"],
        false,
    )
    .unwrap();
    let mut metadata = declaration_value(&root);
    metadata["packages"][0]["manifest_path"] = json!(manifest);
    metadata["packages"][0]["targets"][0]["src_path"] = json!(member.join("src/main.rs"));
    metadata["packages"][0]["targets"][0]["required-features"] = json!(["dependency/windowed"]);
    metadata["target_directory"] = json!(root.join("shared-output"));
    let invocation = parse_declarations(&metadata, &manifest, selection).unwrap();

    assert_eq!(invocation.package_id, PACKAGE_ID);
    assert_eq!(invocation.package_name(), "sample-game");
    assert_eq!(invocation.package_version(), "0.1.0");
    assert_eq!(
        invocation.package().source_kind(),
        CargoPackageSourceKind::Path
    );
    assert_eq!(invocation.manifest_path(), manifest);
    assert_eq!(invocation.workspace_root(), root);
    assert_eq!(invocation.target_directory(), root.join("shared-output"));
    assert_eq!(invocation.source_path(), member.join("src/main.rs"));
    assert_eq!(invocation.target_name(), "game-player");
    assert_eq!(invocation.profile(), "shipping");
    assert_eq!(invocation.target_triple(), "wasm32-unknown-unknown");
    assert_eq!(invocation.features(), ["dependency/windowed"]);
    assert!(!invocation.default_features());
    assert_eq!(
        invocation.required_features.as_ref(),
        ["dependency/windowed"]
    );
    assert_eq!(
        invocation.executable_directory,
        root.join("shared-output/wasm32-unknown-unknown/shipping")
    );
}

#[test]
fn declarations_require_metadata_v1_and_an_explicit_null_resolve() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    for resolve in [
        metadata_value(&root)["resolve"].clone(),
        json!({}),
        json!(false),
    ] {
        let mut metadata = declaration_value(&root);
        metadata["resolve"] = resolve;
        assert!(matches!(
            parse_declarations(&metadata, &manifest, test_selection()),
            Err(CargoTargetDeclarationError::Metadata(
                CargoMetadataFactsError::InvalidField { field: "resolve" }
            ))
        ));
    }
    let mut metadata = declaration_value(&root);
    metadata.as_object_mut().unwrap().remove("resolve");
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::InvalidField { field: "resolve" }
        ))
    ));

    let mut metadata = declaration_value(&root);
    metadata["version"] = json!(2);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::UnsupportedFormatVersion
        ))
    ));
    metadata.as_object_mut().unwrap().remove("version");
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::InvalidField { field: "version" }
        ))
    ));
}

#[test]
fn declaration_support_does_not_relax_full_metadata_resolution_requirements() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = declaration_value(&root);
    for remove_resolve in [false, true] {
        if remove_resolve {
            metadata.as_object_mut().unwrap().remove("resolve");
        }
        assert!(matches!(
            parse_metadata(&metadata, &manifest, &test_selection()),
            Err(CargoMetadataFactsError::InvalidField {
                field: "resolve.nodes"
            })
        ));
    }
    metadata["resolve"] = json!({"nodes": []});
    assert!(matches!(
        parse_metadata(&metadata, &manifest, &test_selection()),
        Err(CargoMetadataFactsError::MissingSelectedResolveNode)
    ));
}

#[test]
fn declarations_select_the_exact_manifest_and_require_its_workspace_package() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = declaration_value(&root);
    let decoy_id = "path+file:///project/decoy#sample-game@0.1.0";
    let mut decoy = metadata["packages"][0].clone();
    decoy["id"] = json!(decoy_id);
    decoy["manifest_path"] = json!(root.join("decoy/Cargo.toml"));
    decoy["targets"][0]["src_path"] = json!(root.join("decoy/src/main.rs"));
    metadata["packages"]
        .as_array_mut()
        .unwrap()
        .insert(0, decoy);
    metadata["workspace_members"]
        .as_array_mut()
        .unwrap()
        .push(json!(decoy_id));
    let invocation = parse_declarations(&metadata, &manifest, test_selection()).unwrap();
    assert_eq!(invocation.package_id, PACKAGE_ID);
    assert_eq!(
        invocation.source_path(),
        root.join("src/bin/game_player.rs")
    );

    assert!(matches!(
        parse_declarations(
            &metadata,
            &root.join("missing/Cargo.toml"),
            test_selection()
        ),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::MissingSelectedPackage
        ))
    ));
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
        parse_declarations(&metadata, &manifest, other_package),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::SelectedPackageMismatch
        ))
    ));
    metadata["workspace_members"] = json!([decoy_id]);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::SelectedPackageNotWorkspaceMember
        ))
    ));
}

#[test]
fn declarations_require_one_valid_selected_binary_and_required_feature_declaration() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    let mut metadata = declaration_value(&root);
    let target = metadata["packages"][0]["targets"][0].clone();
    metadata["packages"][0]["targets"] = json!([target, target]);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::DuplicateSelectedTarget
        ))
    ));
    metadata["packages"][0]["targets"] = json!([]);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::MissingSelectedTarget
        ))
    ));
    for (field, value) in [
        ("crate_types", json!(["lib"])),
        ("src_path", json!("src/main.rs")),
        ("required-features", json!("render")),
        ("required-features", json!(["render", "render"])),
    ] {
        let mut metadata = declaration_value(&root);
        metadata["packages"][0]["targets"][0][field] = value;
        assert!(
            parse_declarations(&metadata, &manifest, test_selection()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn declarations_keep_the_shared_metadata_and_target_bounds() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
    assert!(matches!(
        CargoBuildInvocation::from_metadata_declarations(
            &vec![b' '; MAX_METADATA_BYTES + 1],
            &manifest,
            test_selection(),
        ),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::InputLimitExceeded
        ))
    ));
    let mut metadata = declaration_value(&root);
    let target = metadata["packages"][0]["targets"][0].clone();
    metadata["packages"][0]["targets"] = json!(vec![target; MAX_TARGET_COUNT + 1]);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::FieldLimitExceeded {
                field: "packages[].targets"
            }
        ))
    ));
    let mut metadata = declaration_value(&root);
    metadata["packages"][0]["targets"][0]["required-features"] =
        json!(["x".repeat(MAX_FEATURE_BYTES + 1)]);
    assert!(matches!(
        parse_declarations(&metadata, &manifest, test_selection()),
        Err(CargoTargetDeclarationError::Metadata(
            CargoMetadataFactsError::FieldLimitExceeded {
                field: "packages[].targets[].required-features"
            }
        ))
    ));
}

#[test]
fn declarations_leave_feature_resolution_and_target_eligibility_to_cargo() {
    let root = test_root();
    let manifest = root.join("Cargo.toml");
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
    let full = parse_metadata(&metadata, &manifest, &selection)
        .unwrap()
        .build_invocation(selection.clone())
        .unwrap();
    metadata["resolve"] = Value::Null;
    metadata["packages"].as_array_mut().unwrap().truncate(1);
    let declarations = parse_declarations(&metadata, &manifest, selection).unwrap();
    assert!(full.metadata_features().is_some());
    assert_eq!(declarations.metadata_features(), None);

    for invocation in [full, declarations.clone()] {
        let mut artifact = artifact_message(&invocation, false);
        artifact["features"] = json!(["render", "compiled-only"]);
        let mut decoder = CargoBuildDecoder::new(invocation);
        push_value(&mut decoder, artifact).unwrap();
        push_value(&mut decoder, build_finished(true)).unwrap();
        assert_eq!(
            decoder.finish().unwrap().artifact().features(),
            ["compiled-only", "render"]
        );
    }
    let mut failed = CargoBuildDecoder::new(declarations.clone());
    push_value(&mut failed, build_finished(false)).unwrap();
    assert!(matches!(failed.finish(), Err(CargoBuildError::BuildFailed)));
    let mut missing = CargoBuildDecoder::new(declarations);
    push_value(&mut missing, build_finished(true)).unwrap();
    assert!(matches!(
        missing.finish(),
        Err(CargoBuildError::MissingArtifact)
    ));
}

#[test]
fn declaration_artifacts_still_bind_required_features_profile_and_output_identity() {
    let root = test_root();
    let invocation = parse_declarations(
        &declaration_value(&root),
        &root.join("Cargo.toml"),
        test_selection(),
    )
    .unwrap();
    for (field, value, expected) in [
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
        (
            "executable",
            json!(root.join("target/other-target/debug/game-player")),
            CargoArtifactMismatchField::ExecutablePath,
        ),
    ] {
        let mut artifact = artifact_message(&invocation, false);
        artifact["features"] = json!(["render"]);
        if let Some((parent, child)) = field.split_once('.') {
            artifact[parent][child] = value;
        } else {
            artifact[field] = value;
        }
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        assert!(matches!(
            push_value(&mut decoder, artifact),
            Err(CargoBuildError::ArtifactMismatch { field }) if field == expected
        ));
    }
}

fn declaration_value(root: &Path) -> Value {
    let mut metadata = metadata_value(root);
    metadata["packages"].as_array_mut().unwrap().truncate(1);
    metadata["resolve"] = Value::Null;
    metadata
}

fn parse_declarations(
    metadata: &Value,
    manifest: &Path,
    selection: CargoBuildSelection,
) -> Result<CargoBuildInvocation, CargoTargetDeclarationError> {
    CargoBuildInvocation::from_metadata_declarations(
        &serde_json::to_vec(metadata).unwrap(),
        manifest,
        selection,
    )
}
