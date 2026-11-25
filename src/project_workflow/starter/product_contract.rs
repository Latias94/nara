use nara_asset::{AssetMetaIdentity, AssetMetaIdentityCandidate};
use nara_fs::ContentDigest;
use nara_project::{
    NativeDocumentIdentity, ProductCapability, ProductCapabilitySet, ProjectExecutionContract,
    ProjectManifest, ProjectPresentMode, RuntimePreset, SourceUniverseId,
};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::SceneDocumentCandidate;
use toml::Value;

use super::{ProjectSlug, ProjectStarter, STARTER_ACTION_MAP_PATH};
use crate::{
    ProductRecipe,
    project_workflow::{
        STARTER_DESKTOP_CAPABILITIES,
        dependency::{NaraDependencyReceipt, NaraDependencySourceKind, validate_git_repository},
    },
};

const HEADLESS_NARA_FEATURES: [&str; 3] = ["runtime-2d", "serde", "project-tooling"];
const DESKTOP_NARA_FEATURES: [&str; 4] = [
    "nara/presentation-2d",
    "nara/desktop-winit",
    "nara/render-wgpu",
    "dep:nara_gilrs",
];

pub(super) struct ProductContractSources<'a> {
    pub(super) source_universe: SourceUniverseId,
    pub(super) expected_startup_metadata: &'a AssetMetaIdentity,
    pub(super) startup_metadata: &'a [u8],
    pub(super) cargo_manifest: &'a [u8],
    pub(super) project_manifest: &'a [u8],
    pub(super) authoring_schema_bundle: &'a [u8],
    pub(super) expected_authoring_schema_bundle: &'a AuthoringSchemaBundle,
    pub(super) startup_scene: &'a [u8],
}

pub(super) fn render_cargo_manifest(
    starter: ProjectStarter,
    source_universe: SourceUniverseId,
    dependency: &str,
    receipt: &NaraDependencyReceipt,
) -> String {
    let package_name = cargo_package_name(source_universe);
    let desktop = desktop_features(starter)
        .iter()
        .map(|feature| format!("\"{feature}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let gamepad_dependency = if starter.has_action_map() {
        let coordinate = gilrs_local_path(receipt).map_or_else(
            || dependency.to_owned(),
            |path| format!("path = {}", Value::String(path)),
        );
        format!(
            "\n[target.'cfg(windows)'.dependencies]\nnara_gilrs = {{ {coordinate}, optional = true }}\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
publish = false
build = "build.rs"

[package.metadata.nara]
project-tool-protocol = 1

[workspace]
resolver = "3"

[dependencies]
nara = {{ {}, default-features = false, features = ["{}", "{}", "{}"] }}

[features]
default = []
desktop = [{desktop}]

[lib]
name = "nara_game"
path = "src/lib.rs"

[[bin]]
name = "headless"
path = "src/bin/headless.rs"

[[bin]]
name = "desktop"
path = "src/bin/desktop.rs"
required-features = ["desktop"]

[[bin]]
name = "cook"
path = "src/bin/cook.rs"

[[bin]]
name = "player"
path = "src/bin/player.rs"
{gamepad_dependency}"#,
        dependency, HEADLESS_NARA_FEATURES[0], HEADLESS_NARA_FEATURES[1], HEADLESS_NARA_FEATURES[2]
    )
}

fn cargo_package_name(source_universe: SourceUniverseId) -> String {
    format!("nara-project-{source_universe}")
}

pub(super) fn render_project_manifest(
    starter: ProjectStarter,
    slug: &ProjectSlug,
    source_universe: SourceUniverseId,
) -> String {
    let input = if starter.has_action_map() {
        format!("\n[input]\naction_map = \"{STARTER_ACTION_MAP_PATH}\"\n")
    } else {
        String::new()
    };
    format!(
        r#"schema_version = 1

[project]
name = "{}"
source_universe = "{source_universe}"
native_document_identity = "stable_metadata"
stable_id = "{}"
version = "0.1.0"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"
fixed_timestep_seconds = 0.02

[window]
enabled = false

[capabilities]
requested = ["runtime-2d"]

[profiles.desktop.capabilities]
requested = ["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]

[profiles.desktop.execution]
contract = "desktop-supervised-play/v1"

[profiles.desktop.runtime]
preset = "local-headless"
fixed_timestep_seconds = 0.02

[profiles.desktop.window]
enabled = true
title = "{}"
width = 1280
height = 720
present_mode = "fifo"
resizable = true
{input}"#,
        slug.as_str(),
        slug.as_str(),
        slug.as_str()
    )
}

pub(crate) fn product_recipe(
    starter: ProjectStarter,
) -> Result<ProductRecipe, crate::ProductRecipeError> {
    use crate::runtime_content::RuntimeCodecDefinitionId;

    let recipe = ProductRecipe::new()
        .add_runtime_image_codec::<nara_render::image::ImageRuntimeCodec>(
            RuntimeCodecDefinitionId::new("nara.starter.desktop-2d.image", 1),
        )?;
    if starter.has_action_map() {
        recipe.add_runtime_asset_codec::<nara_input::ActionMapDomain, nara_input::ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("nara.starter.desktop-2d.actions", 1),
        )
    } else {
        Ok(recipe)
    }
}

pub(super) fn validate(
    starter: ProjectStarter,
    slug: &ProjectSlug,
    dependency: &NaraDependencyReceipt,
    sources: ProductContractSources<'_>,
) -> Result<(), &'static str> {
    validate_cargo_manifest(
        starter,
        sources.source_universe,
        dependency,
        sources.cargo_manifest,
    )?;
    let load = ProjectManifest::parse_toml_bytes(sources.project_manifest);
    if load.has_errors() {
        return Err("generated project manifest is invalid");
    }
    let Some(manifest) = load.manifest else {
        return Err("generated project manifest is absent");
    };
    let headless = manifest
        .resolve_profile(None)
        .map_err(|_| "generated headless settings are invalid")?;
    let desktop = manifest
        .resolve_profile(Some("desktop"))
        .map_err(|_| "generated desktop settings are invalid")?;
    let expected_headless = ProductCapabilitySet::singleton(ProductCapability::Runtime2d);
    let expected_desktop = expected_headless.union(STARTER_DESKTOP_CAPABILITIES);
    let expected_action_map = starter.has_action_map().then_some(STARTER_ACTION_MAP_PATH);
    if manifest.project.name != slug.as_str()
        || manifest.project.source_universe != sources.source_universe
        || manifest.project.native_document_identity != NativeDocumentIdentity::StableMetadata
        || manifest.project.stable_id.as_deref() != Some(slug.as_str())
        || manifest.project.version.as_deref() != Some("0.1.0")
        || manifest.paths.assets != "assets"
        || manifest.paths.scenes != "scenes"
        || manifest.paths.prefabs != "prefabs"
        || manifest.authoring.schema_bundle.as_deref() != Some("schema/authoring.bundle.json")
        || manifest.startup.default_scene.as_deref() != Some("startup.scene.json")
    {
        return Err("generated project identity or startup paths disagree with the starter");
    }
    if headless.requested_capabilities != expected_headless
        || headless.runtime_preset != RuntimePreset::LocalHeadless
        || headless.startup.default_scene.is_none()
        || headless.window.enabled
        || headless.input.action_map.as_ref().map(|path| path.as_str()) != expected_action_map
    {
        return Err("generated headless product contract disagrees with the starter");
    }
    if desktop.requested_capabilities != expected_desktop
        || desktop.execution.contract.as_ref()
            != Some(&ProjectExecutionContract::DesktopSupervisedPlayV1)
        || desktop.runtime_preset != RuntimePreset::LocalHeadless
        || !desktop.window.enabled
        || desktop.window.width != 1280
        || desktop.window.height != 720
        || desktop.window.title != slug.as_str()
        || !desktop.window.resizable
        || desktop.window.present_mode != ProjectPresentMode::Fifo
        || desktop.input.action_map.as_ref().map(|path| path.as_str()) != expected_action_map
    {
        return Err("generated desktop product contract disagrees with the starter");
    }
    validate_authoring_schema_bundle(
        sources.authoring_schema_bundle,
        sources.expected_authoring_schema_bundle,
    )?;
    SceneDocumentCandidate::decode_json_bytes(sources.startup_scene)
        .map_err(|_| "generated startup scene is invalid")?;
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(sources.startup_metadata)
        .map_err(|_| "generated startup Scene metadata is invalid")?;
    if metadata.identity() != sources.expected_startup_metadata {
        return Err("generated startup Scene identity disagrees with the creation plan");
    }
    Ok(())
}

fn validate_authoring_schema_bundle(
    encoded: &[u8],
    expected: &AuthoringSchemaBundle,
) -> Result<(), &'static str> {
    let bundle = AuthoringSchemaBundle::from_json_bytes(encoded)
        .map_err(|_| "generated authoring schema bundle is invalid")?;
    if &bundle != expected {
        return Err("generated authoring schema bundle disagrees with the starter");
    }
    Ok(())
}

fn validate_cargo_manifest(
    starter: ProjectStarter,
    source_universe: SourceUniverseId,
    dependency: &NaraDependencyReceipt,
    cargo_manifest: &[u8],
) -> Result<(), &'static str> {
    let source = std::str::from_utf8(cargo_manifest)
        .map_err(|_| "generated Cargo manifest is not valid UTF-8")?;
    let value =
        toml::from_str::<Value>(source).map_err(|_| "generated Cargo manifest is invalid TOML")?;
    let root = value
        .as_table()
        .ok_or("generated Cargo manifest root is not a table")?;
    if root.len() != 6 + usize::from(starter.has_action_map()) {
        return Err("generated Cargo manifest contains an unexpected top-level section");
    }
    let package = root
        .get("package")
        .and_then(Value::as_table)
        .ok_or("generated Cargo package table is absent")?;
    let package_name = cargo_package_name(source_universe);
    if package.get("name").and_then(Value::as_str) != Some(package_name.as_str())
        || package.get("version").and_then(Value::as_str) != Some("0.1.0")
        || package.get("edition").and_then(Value::as_str) != Some("2024")
        || package.get("rust-version").and_then(Value::as_str) != Some("1.95")
        || package.get("publish").and_then(Value::as_bool) != Some(false)
        || package.get("build").and_then(Value::as_str) != Some("build.rs")
        || package
            .get("metadata")
            .and_then(|value| value.get("nara"))
            .and_then(|value| value.get("project-tool-protocol"))
            .and_then(Value::as_integer)
            != Some(1)
    {
        return Err("generated Cargo package contract disagrees with the starter");
    }
    let workspace = root
        .get("workspace")
        .and_then(Value::as_table)
        .ok_or("generated Cargo workspace table is absent")?;
    if workspace.len() != 1 || workspace.get("resolver").and_then(Value::as_str) != Some("3") {
        return Err("generated Cargo workspace contract disagrees with the starter");
    }

    let dependencies = root
        .get("dependencies")
        .and_then(Value::as_table)
        .ok_or("generated Cargo dependencies table is absent")?;
    if dependencies.len() != 1 {
        return Err("generated Cargo manifest contains an unexpected dependency");
    }
    let dependency_table = dependencies
        .get("nara")
        .and_then(Value::as_table)
        .ok_or("generated Cargo Nara dependency is absent")?;
    if dependency_table
        .get("default-features")
        .and_then(Value::as_bool)
        != Some(false)
        || !string_array_matches(dependency_table.get("features"), &HEADLESS_NARA_FEATURES)
    {
        return Err("generated Cargo dependency features disagree with the starter");
    }
    match dependency.kind() {
        NaraDependencySourceKind::LocalCheckout => {
            if dependency_table.get("path").and_then(Value::as_str) != dependency.local_cargo_path()
                || dependency_table.contains_key("git")
                || dependency_table.contains_key("rev")
                || dependency_table.len() != 3
            {
                return Err("generated Cargo local dependency disagrees with the starter");
            }
        }
        NaraDependencySourceKind::ImmutableGit => {
            let repository = dependency_table
                .get("git")
                .and_then(Value::as_str)
                .ok_or("generated Cargo Git dependency is absent")?;
            let revision = dependency_table
                .get("rev")
                .and_then(Value::as_str)
                .ok_or("generated Cargo Git revision is absent")?;
            if validate_git_repository(repository).is_err()
                || ContentDigest::of_bytes(repository.as_bytes())
                    != dependency.immutable_git_repository_digest().unwrap()
                || Some(revision) != dependency.immutable_git_revision()
                || dependency_table.contains_key("path")
                || dependency_table.len() != 4
            {
                return Err("generated Cargo Git dependency disagrees with the starter");
            }
        }
    }
    if starter.has_action_map() {
        validate_gamepad_dependency(root.get("target"), dependency_table, dependency)?;
    }

    let features = root
        .get("features")
        .and_then(Value::as_table)
        .ok_or("generated Cargo features table is absent")?;
    if features.len() != 2
        || !string_array_matches(features.get("default"), &[])
        || !string_array_matches(features.get("desktop"), desktop_features(starter))
    {
        return Err("generated Cargo product features disagree with the starter");
    }
    let library = root
        .get("lib")
        .and_then(Value::as_table)
        .ok_or("generated Cargo library target is absent")?;
    if library.len() != 2
        || library.get("name").and_then(Value::as_str) != Some("nara_game")
        || library.get("path").and_then(Value::as_str) != Some("src/lib.rs")
    {
        return Err("generated Cargo library target disagrees with the starter");
    }
    let binaries = root
        .get("bin")
        .and_then(Value::as_array)
        .ok_or("generated Cargo binary targets are absent")?;
    if binaries.len() != 4
        || !binary_target_matches(&binaries[0], "headless", "src/bin/headless.rs", None)
        || !binary_target_matches(
            &binaries[1],
            "desktop",
            "src/bin/desktop.rs",
            Some("desktop"),
        )
        || !binary_target_matches(&binaries[2], "cook", "src/bin/cook.rs", None)
        || !binary_target_matches(&binaries[3], "player", "src/bin/player.rs", None)
    {
        return Err("generated Cargo binary targets disagree with the starter");
    }
    Ok(())
}

fn string_array_matches(value: Option<&Value>, expected: &[&str]) -> bool {
    value.and_then(Value::as_array).is_some_and(|values| {
        values.len() == expected.len()
            && values
                .iter()
                .zip(expected)
                .all(|(value, expected)| value.as_str() == Some(*expected))
    })
}

fn binary_target_matches(
    value: &Value,
    expected_name: &str,
    expected_path: &str,
    expected_feature: Option<&str>,
) -> bool {
    let Some(table) = value.as_table() else {
        return false;
    };
    let required_features = table.get("required-features").and_then(Value::as_array);
    table.len() == usize::from(expected_feature.is_some()) + 2
        && table.get("name").and_then(Value::as_str) == Some(expected_name)
        && table.get("path").and_then(Value::as_str) == Some(expected_path)
        && match expected_feature {
            Some(feature) => required_features.is_some_and(|features| {
                features.len() == 1 && features[0].as_str() == Some(feature)
            }),
            None => required_features.is_none(),
        }
}

fn desktop_features(starter: ProjectStarter) -> &'static [&'static str] {
    if starter.has_action_map() {
        &DESKTOP_NARA_FEATURES
    } else {
        &DESKTOP_NARA_FEATURES[..3]
    }
}

fn gilrs_local_path(receipt: &NaraDependencyReceipt) -> Option<String> {
    receipt
        .local_cargo_path()
        .map(|path| format!("{}/crates/nara_gilrs", path.trim_end_matches('/')))
}

fn validate_gamepad_dependency(
    target: Option<&Value>,
    engine: &toml::Table,
    receipt: &NaraDependencyReceipt,
) -> Result<(), &'static str> {
    let invalid = "generated Windows gamepad dependency disagrees with the starter";
    let targets = target.and_then(Value::as_table).ok_or(invalid)?;
    let windows = targets
        .get("cfg(windows)")
        .and_then(Value::as_table)
        .ok_or(invalid)?;
    let dependencies = windows
        .get("dependencies")
        .and_then(Value::as_table)
        .ok_or(invalid)?;
    let gilrs = dependencies
        .get("nara_gilrs")
        .and_then(Value::as_table)
        .ok_or(invalid)?;
    let coordinate_matches = match receipt.kind() {
        NaraDependencySourceKind::LocalCheckout => {
            gilrs.len() == 2
                && gilrs.get("path").and_then(Value::as_str) == gilrs_local_path(receipt).as_deref()
        }
        NaraDependencySourceKind::ImmutableGit => {
            gilrs.len() == 3
                && gilrs.get("git") == engine.get("git")
                && gilrs.get("rev") == engine.get("rev")
        }
    };
    if targets.len() != 1
        || windows.len() != 1
        || dependencies.len() != 1
        || gilrs.get("optional").and_then(Value::as_bool) != Some(true)
        || !coordinate_matches
    {
        return Err(invalid);
    }
    Ok(())
}
