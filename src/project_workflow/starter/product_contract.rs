use nara_fs::ContentDigest;
use nara_project::{ProductCapability, ProductCapabilitySet, ProjectManifest, RuntimePreset};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::SceneDocumentCandidate;
use toml::Value;

use super::ProjectSlug;
use crate::{
    ProductRecipe,
    project_workflow::dependency::{
        NaraDependencyReceipt, NaraDependencySourceKind, validate_git_repository,
    },
};

const HEADLESS_NARA_FEATURES: [&str; 2] = ["runtime-2d", "serde"];
const DESKTOP_NARA_FEATURES: [&str; 3] = [
    "nara/presentation-2d",
    "nara/desktop-winit",
    "nara/render-wgpu",
];
const LIBRARY_MARKERS: [&[u8]; 2] = [
    b"pub fn product_recipe() -> nara::ProductRecipe",
    b"nara::ProductRecipe::new()",
];
const BINARY_MARKER: &[u8] = b"nara_game::product_recipe()";

pub(super) struct ProductContractSources<'a> {
    pub(super) cargo_manifest: &'a [u8],
    pub(super) project_manifest: &'a [u8],
    pub(super) authoring_schema_bundle: &'a [u8],
    pub(super) expected_authoring_schema_bundle: &'a AuthoringSchemaBundle,
    pub(super) dependency_manifest_fragment: &'a str,
    pub(super) creation_command_arguments: &'a str,
    pub(super) author_guide: &'a str,
    pub(super) library: &'a [u8],
    pub(super) headless: &'a [u8],
    pub(super) desktop: &'a [u8],
    pub(super) startup_scene: &'a [u8],
}

pub(super) fn render_cargo_manifest(slug: &ProjectSlug, dependency: &str) -> String {
    format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
publish = false

[workspace]
resolver = "3"

[dependencies]
nara = {{ {}, default-features = false, features = ["{}", "{}"] }}

[features]
default = []
desktop = ["{}", "{}", "{}"]

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
"#,
        slug.as_str(),
        dependency,
        HEADLESS_NARA_FEATURES[0],
        HEADLESS_NARA_FEATURES[1],
        DESKTOP_NARA_FEATURES[0],
        DESKTOP_NARA_FEATURES[1],
        DESKTOP_NARA_FEATURES[2]
    )
}

pub(super) fn render_project_manifest(slug: &ProjectSlug) -> String {
    format!(
        r#"schema_version = 1

[project]
name = "{}"
stable_id = "{}"
version = "0.1.0"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[authoring]
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
"#,
        slug.as_str(),
        slug.as_str(),
        slug.as_str()
    )
}

pub(crate) fn product_recipe() -> ProductRecipe {
    ProductRecipe::new()
}

pub(super) fn validate(
    slug: &ProjectSlug,
    dependency: &NaraDependencyReceipt,
    sources: ProductContractSources<'_>,
) -> Result<(), &'static str> {
    validate_cargo_manifest(slug, dependency, sources.cargo_manifest)?;
    validate_author_guide(
        slug,
        sources.dependency_manifest_fragment,
        sources.creation_command_arguments,
        sources.author_guide,
    )?;
    if !product_recipe().is_empty() {
        return Err("generated ProductRecipe disagrees with the starter product contract");
    }

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
    let expected_headless = ProductCapabilitySet::from_capabilities([ProductCapability::Runtime2d]);
    let expected_desktop = ProductCapabilitySet::from_capabilities([
        ProductCapability::Runtime2d,
        ProductCapability::Presentation2d,
        ProductCapability::DesktopWinit,
        ProductCapability::RenderWgpu,
    ]);
    if manifest.project.name != slug.as_str()
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
    {
        return Err("generated headless product contract disagrees with the starter");
    }
    if desktop.requested_capabilities != expected_desktop
        || desktop.runtime_preset != RuntimePreset::LocalHeadless
        || !desktop.window.enabled
        || desktop.window.width != 1280
        || desktop.window.height != 720
        || desktop.window.title != slug.as_str()
        || !desktop.window.resizable
        || desktop.window.present_mode != nara_window::PresentMode::Fifo
    {
        return Err("generated desktop product contract disagrees with the starter");
    }
    if !LIBRARY_MARKERS
        .iter()
        .all(|marker| source_contains(sources.library, marker))
        || !source_contains(sources.headless, BINARY_MARKER)
        || !source_contains(sources.desktop, BINARY_MARKER)
    {
        return Err("generated Rust entry points disagree with the starter product contract");
    }
    validate_authoring_schema_bundle(
        sources.authoring_schema_bundle,
        sources.expected_authoring_schema_bundle,
    )?;
    SceneDocumentCandidate::decode_json_bytes(sources.startup_scene)
        .map_err(|_| "generated startup scene is invalid")?;
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

fn validate_author_guide(
    slug: &ProjectSlug,
    dependency_manifest_fragment: &str,
    creation_command_arguments: &str,
    guide: &str,
) -> Result<(), &'static str> {
    let dependency_marker = format!("Nara dependency: `{dependency_manifest_fragment}`");
    let creation_command = format!(
        "nara new desktop-2d/v1 {} --parent . {creation_command_arguments}",
        slug.as_str()
    );
    if !guide.starts_with(&format!("# {} author guide\n", slug.as_str()))
        || !guide.contains("Starter: `desktop-2d/v1`")
        || !guide.contains(&dependency_marker)
        || !guide.contains(&creation_command)
        || !guide.contains("nara check .")
        || !guide.contains("nara export .")
        || !guide.contains("nara repair startup-scene .")
        || !guide.contains("source-carrying startup-closure candidate")
    {
        return Err("generated author guide disagrees with the starter workflow");
    }
    Ok(())
}

fn validate_cargo_manifest(
    slug: &ProjectSlug,
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
    if root.len() != 6 {
        return Err("generated Cargo manifest contains an unexpected top-level section");
    }
    let package = root
        .get("package")
        .and_then(Value::as_table)
        .ok_or("generated Cargo package table is absent")?;
    if package.len() != 5
        || package.get("name").and_then(Value::as_str) != Some(slug.as_str())
        || package.get("version").and_then(Value::as_str) != Some("0.1.0")
        || package.get("edition").and_then(Value::as_str) != Some("2024")
        || package.get("rust-version").and_then(Value::as_str) != Some("1.95")
        || package.get("publish").and_then(Value::as_bool) != Some(false)
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
            let expected_path =
                std::iter::repeat_n("..", dependency.local_descendant_depth().unwrap() + 1)
                    .collect::<Vec<_>>()
                    .join("/");
            if dependency_table.get("path").and_then(Value::as_str) != Some(expected_path.as_str())
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

    let features = root
        .get("features")
        .and_then(Value::as_table)
        .ok_or("generated Cargo features table is absent")?;
    if features.len() != 2
        || !string_array_matches(features.get("default"), &[])
        || !string_array_matches(features.get("desktop"), &DESKTOP_NARA_FEATURES)
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
    if binaries.len() != 2
        || !binary_target_matches(&binaries[0], "headless", "src/bin/headless.rs", None)
        || !binary_target_matches(
            &binaries[1],
            "desktop",
            "src/bin/desktop.rs",
            Some("desktop"),
        )
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

fn source_contains(source: &[u8], marker: &[u8]) -> bool {
    source.windows(marker.len()).any(|window| window == marker)
}
