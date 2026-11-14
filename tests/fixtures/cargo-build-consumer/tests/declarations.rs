//! Public declaration and Cargo-message decoding using synthetic metadata and artifact records.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use engine::project_workflow::cargo::{
    CargoArtifactMismatchField, CargoBuildDecoder, CargoBuildError, CargoBuildInvocation,
    CargoBuildSelection, CargoMetadataFacts, CargoTargetDeclarationError,
};
use serde_json::{Value, json};

const PACKAGE_ID: &str = "path+file:///workspace/games/demo#declaration-game@0.1.0";
const TARGET: &str = "wasm32-unknown-unknown";

#[test]
fn public_declarations_keep_requested_and_artifact_features_separate() {
    let root = workspace_root();
    let invocation = invocation_from(&metadata_document(&root)).unwrap();
    assert_eq!(invocation.metadata_features(), None);
    assert_eq!(invocation.package_name(), "declaration-game");
    assert_eq!(invocation.package_version(), "0.1.0");
    assert_eq!(
        invocation.manifest_path(),
        root.join("games/demo/Cargo.toml")
    );
    assert_eq!(invocation.workspace_root(), root);
    assert_eq!(invocation.target_directory(), root.join("output"));
    assert_eq!(
        invocation.source_path(),
        root.join("games/demo/src/main.rs")
    );
    assert_eq!(invocation.target_name(), "custom-player");
    assert_eq!(invocation.profile(), "shipping");
    assert_eq!(invocation.target_triple(), TARGET);
    assert_eq!(invocation.features(), ["trial"]);
    assert!(!invocation.default_features());

    let artifact = artifact_message(&root);
    let finished = json!({"reason": "build-finished", "success": true});
    let stream = format!("{artifact}\n{finished}\n");
    let mut decoder = CargoBuildDecoder::new(invocation);
    for chunk in stream.as_bytes().chunks(11) {
        decoder.push(chunk).unwrap();
    }
    let built = decoder.finish().unwrap();
    assert_eq!(built.artifact().package().name(), "declaration-game");
    assert_eq!(built.artifact().features(), ["expanded", "trial"]);
    assert_eq!(built.artifact().profile(), "shipping");
    assert_eq!(built.artifact().target_triple(), TARGET);
    assert_eq!(
        built.artifact().executable(),
        root.join("output/wasm32-unknown-unknown/shipping/cargo-player.wasm")
    );
}

#[test]
fn public_declarations_and_full_metadata_preserve_distinct_resolution_contracts() {
    let root = workspace_root();
    let manifest = root.join("games/demo/Cargo.toml");
    let mut metadata = metadata_document(&root);
    metadata["packages"][0]["features"] = json!({});
    metadata["packages"][0]["targets"][0]["required-features"] = json!([]);
    let selection = CargoBuildSelection::new(
        "declaration-game",
        "custom-player",
        "shipping",
        TARGET,
        std::iter::empty::<&str>(),
        false,
    )
    .unwrap();
    let declaration = CargoBuildInvocation::from_metadata_declarations(
        &serde_json::to_vec(&metadata).unwrap(),
        &manifest,
        selection.clone(),
    )
    .unwrap();
    assert!(declaration.features().is_empty());
    assert_eq!(declaration.metadata_features(), None);
    let mut artifact = artifact_message(&root);
    artifact["features"] = json!([]);
    artifact["target"]["required-features"] = json!([]);
    let mut decoder = CargoBuildDecoder::new(declaration);
    decoder.push(format!("{artifact}\n").as_bytes()).unwrap();
    decoder
        .push(b"{\"reason\":\"build-finished\",\"success\":true}\n")
        .unwrap();
    assert!(decoder.finish().unwrap().artifact().features().is_empty());
    assert!(
        CargoMetadataFacts::parse(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest,
            &selection,
        )
        .is_err()
    );

    metadata["resolve"] = json!({"root": null, "nodes": [{"id": PACKAGE_ID, "features": []}]});
    let full = CargoMetadataFacts::parse(
        &serde_json::to_vec(&metadata).unwrap(),
        &manifest,
        &selection,
    )
    .unwrap();
    assert!(full.metadata_features().is_empty());
    let full_invocation = full.build_invocation(selection.clone()).unwrap();
    let observed_empty: &[String] = &[];
    assert_eq!(full_invocation.metadata_features(), Some(observed_empty));
    assert!(invocation_from(&metadata).is_err());

    metadata.as_object_mut().unwrap().remove("resolve");
    assert!(invocation_from(&metadata).is_err());
    assert!(
        CargoMetadataFacts::parse(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest,
            &selection,
        )
        .is_err()
    );
}

#[test]
fn public_declaration_invocations_reject_artifact_identity_mismatches() {
    let root = workspace_root();
    let invocation = invocation_from(&metadata_document(&root)).unwrap();
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
            json!(root.join("output/wasm32-unknown-unknown/release/cargo-player.wasm")),
            CargoArtifactMismatchField::ExecutablePath,
        ),
    ] {
        let mut artifact = artifact_message(&root);
        if let Some((parent, child)) = field.split_once('.') {
            artifact[parent][child] = value;
        } else {
            artifact[field] = value;
        }
        let mut decoder = CargoBuildDecoder::new(invocation.clone());
        let record = format!("{artifact}\n");
        assert!(matches!(
            decoder.push(record.as_bytes()),
            Err(CargoBuildError::ArtifactMismatch { field }) if field == expected
        ));
    }
}

fn invocation_from(metadata: &Value) -> Result<CargoBuildInvocation, CargoTargetDeclarationError> {
    CargoBuildInvocation::from_metadata_declarations(
        &serde_json::to_vec(metadata).unwrap(),
        &workspace_root().join("games/demo/Cargo.toml"),
        selection(),
    )
}

fn selection() -> CargoBuildSelection {
    CargoBuildSelection::new(
        "declaration-game",
        "custom-player",
        "shipping",
        TARGET,
        ["trial"],
        false,
    )
    .unwrap()
}

fn metadata_document(root: &Path) -> Value {
    json!({
        "version": 1,
        "packages": [{
            "name": "declaration-game", "version": "0.1.0", "id": PACKAGE_ID, "source": null,
            "manifest_path": root.join("games/demo/Cargo.toml"),
            "features": {"trial": ["renderer/windowed"], "expanded": []},
            "targets": [{
                "kind": ["bin"], "crate_types": ["bin"], "name": "custom-player",
                "src_path": root.join("games/demo/src/main.rs"),
                "required-features": ["renderer/windowed"]
            }]
        }],
        "workspace_members": [PACKAGE_ID], "workspace_root": root,
        "target_directory": root.join("output"), "resolve": null
    })
}

fn artifact_message(root: &Path) -> Value {
    json!({
        "reason": "compiler-artifact", "package_id": PACKAGE_ID,
        "manifest_path": root.join("games/demo/Cargo.toml"),
        "target": {
            "kind": ["bin"], "crate_types": ["bin"], "name": "custom-player",
            "src_path": root.join("games/demo/src/main.rs"),
            "required-features": ["renderer/windowed"]
        },
        "profile": {"test": false}, "features": ["trial", "expanded"],
        "executable": root.join("output/wasm32-unknown-unknown/shipping/cargo-player.wasm"),
        "fresh": false
    })
}

fn workspace_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\declaration-workspace")
    } else {
        PathBuf::from("/declaration-workspace")
    }
}
