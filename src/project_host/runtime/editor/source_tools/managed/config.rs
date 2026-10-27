//! Admitted framework-dependent metadata; actual assembly identities are checked before load.

use std::collections::{BTreeMap, BTreeSet};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use serde::{Deserialize, de::DeserializeOwned};

use super::super::{EditorSourceToolReleaseMemberKind, PreparedEditorSourceToolRelease};
use super::EditorSourceToolManagedError as Error;

const TARGET: &str = ".NETCoreApp,Version=v10.0";

pub(super) fn prepare_images(release: &PreparedEditorSourceToolRelease) -> Result<Vec<u8>, Error> {
    if !release.requires().is_empty() {
        return Err(Error::Contract(
            "installed requirements need graph admission",
        ));
    }
    let stem = release
        .entry()
        .strip_suffix(".dll")
        .ok_or(Error::Contract("managed DLL entry required"))?;
    let deps_path = format!("{stem}.deps.json");
    let config_path = format!("{stem}.runtimeconfig.json");
    let members = release.members();
    let member = |path: &str| {
        members
            .iter()
            .find(|member| {
                member.path() == path && member.kind() == EditorSourceToolReleaseMemberKind::Tool
            })
            .ok_or(Error::Contract("missing managed configuration"))
    };
    let config: RuntimeConfig = decode(member(&config_path)?.bytes())?;
    if config.runtime_options.tfm != "net10.0"
        || config.runtime_options.framework.name != "Microsoft.NETCore.App"
        || config.runtime_options.framework.version != "10.0.0"
        || config
            .runtime_options
            .config_properties
            .iter()
            .any(|(name, value)| {
                name != "System.Runtime.Serialization.EnableUnsafeBinaryFormatterSerialization"
                    || *value
            })
    {
        return Err(Error::Contract("unsupported managed runtime configuration"));
    }
    let deps: Deps = decode(member(&deps_path)?.bytes())?;
    if deps.runtime_target.name != TARGET
        || !deps.runtime_target.signature.is_empty()
        || !deps.compilation_options.is_empty()
        || deps.targets.len() != 1
    {
        return Err(Error::Contract("unsupported managed dependency target"));
    }
    let libraries = deps
        .targets
        .get(TARGET)
        .ok_or(Error::Contract("missing managed dependency target"))?;
    if libraries.is_empty() || libraries.len() != deps.libraries.len() {
        return Err(Error::Contract(
            "incomplete managed dependency declarations",
        ));
    }
    let mut paths = BTreeSet::new();
    for (identity, target) in libraries {
        let library = deps
            .libraries
            .get(identity)
            .ok_or(Error::Contract("missing managed library declaration"))?;
        if library.kind != "project"
            || library.serviceable
            || !library.sha512.is_empty()
            || target.runtime.is_empty()
        {
            return Err(Error::Contract(
                "only captured managed project libraries supported",
            ));
        }
        for (name, version) in &target.dependencies {
            if !libraries.contains_key(&format!("{name}/{version}")) {
                return Err(Error::Contract("missing captured managed dependency"));
            }
        }
        for (path, metadata) in &target.runtime {
            if !path.ends_with(".dll") || !metadata.is_empty() || !paths.insert(path.as_str()) {
                return Err(Error::Contract("unsupported managed runtime asset"));
            }
            member(path)?;
        }
    }
    if !paths.contains(release.entry()) {
        return Err(Error::Contract(
            "entry is absent from managed dependency declarations",
        ));
    }
    for value in members
        .iter()
        .filter(|member| member.kind() == EditorSourceToolReleaseMemberKind::Tool)
    {
        if !paths.contains(value.path())
            && value.path() != deps_path
            && value.path() != config_path
            && value.path() != release.catalog()
        {
            return Err(Error::Contract(
                "unsupported or undeclared managed tool member",
            ));
        }
    }
    let images: Vec<_> = members
        .iter()
        .filter(|member| paths.contains(member.path()))
        .collect();
    let entry = images
        .iter()
        .position(|member| member.path() == release.entry())
        .ok_or(Error::Contract("missing captured managed entry"))?;
    let total = images
        .iter()
        .map(|member| member.bytes().len() + 8)
        .sum::<usize>()
        + 20;
    let mut prefix = Vec::with_capacity(total);
    prefix.extend_from_slice(b"NARATOOL");
    prefix.extend_from_slice(&1_u32.to_le_bytes());
    prefix.extend_from_slice(&(images.len() as u32).to_le_bytes());
    prefix.extend_from_slice(&(entry as u32).to_le_bytes());
    for image in images {
        prefix.extend_from_slice(&(image.bytes().len() as u64).to_le_bytes());
        prefix.extend_from_slice(image.bytes());
    }
    Ok(prefix)
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > 64 * 1024 {
        return Err(Error::Contract("managed configuration byte limit"));
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("nonzero depth"),
        ItemLimit::new(4096).expect("nonzero nodes"),
        ItemLimit::new(512).expect("nonzero container"),
        ByteLimit::new(4096).expect("nonzero string"),
        ByteLimit::new(64 * 1024).expect("nonzero string aggregate"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits)
        .map_err(|_| Error::Contract("managed configuration shape"))?;
    serde_json::from_slice(bytes).map_err(|_| Error::Contract("unsupported managed configuration"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeConfig {
    runtime_options: RuntimeOptions,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeOptions {
    tfm: String,
    framework: Framework,
    #[serde(default)]
    config_properties: BTreeMap<String, bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Framework {
    name: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Deps {
    runtime_target: RuntimeTarget,
    compilation_options: BTreeMap<String, serde_json::Value>,
    targets: BTreeMap<String, BTreeMap<String, TargetLibrary>>,
    libraries: BTreeMap<String, Library>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeTarget {
    name: String,
    signature: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetLibrary {
    runtime: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    #[serde(rename = "type")]
    kind: String,
    serviceable: bool,
    sha512: String,
}
