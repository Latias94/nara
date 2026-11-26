use std::num::NonZeroU32;

use nara_core::vfs::AssetPath;
use serde::{Deserialize, Deserializer, de::Error as _};

/// The tooling metadata in `nara.toml`. Game settings are configured in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub project: ProjectInfo,
    #[serde(default)]
    pub paths: ProjectPaths,
    pub window: Option<ProjectWindow>,
}

impl ProjectManifest {
    pub fn parse_toml(source: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    #[serde(deserialize_with = "nonempty")]
    pub name: String,
    #[serde(deserialize_with = "nonempty")]
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectPaths {
    /// Portable path relative to the project directory. Defaults to `assets`.
    #[serde(deserialize_with = "asset_path")]
    pub assets: String,
}

impl Default for ProjectPaths {
    fn default() -> Self {
        Self {
            assets: "assets".into(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWindow {
    #[serde(default, deserialize_with = "optional_nonempty")]
    pub title: Option<String>,
    pub width: Option<NonZeroU32>,
    pub height: Option<NonZeroU32>,
}

pub(crate) fn nonempty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.trim().is_empty() {
        return Err(D::Error::custom("expected a nonempty string"));
    }
    Ok(value)
}

pub(crate) fn optional_nonempty<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    nonempty(deserializer).map(Some)
}

fn asset_path<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    AssetPath::new(value.as_str()).map_err(D::Error::custom)?;
    Ok(value)
}
