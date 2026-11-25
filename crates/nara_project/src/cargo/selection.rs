use thiserror::Error;

use super::{MAX_FEATURE_BYTES, MAX_FEATURE_COUNT, MAX_PACKAGE_NAME_BYTES};

const MAX_PROFILE_BYTES: usize = 64;
const MAX_TRIPLE_BYTES: usize = 256;

/// Trusted selection for one Cargo package's binary build.
///
/// This admits a bounded ASCII subset of Cargo names and feature arguments, never paths or free
/// command-line arguments. Cargo owns their availability, dependency resolution and feature closure.
/// Requested features are canonicalized separately from Cargo's resolved feature set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBuildSelection {
    package: String,
    target: String,
    profile: String,
    target_triple: String,
    features: Box<[String]>,
    default_features: bool,
}

impl CargoBuildSelection {
    pub fn new(
        package: impl Into<String>,
        bin: impl Into<String>,
        profile: impl Into<String>,
        target_triple: impl Into<String>,
        features: impl IntoIterator<Item = impl Into<String>>,
        default_features: bool,
    ) -> Result<Self, CargoBuildSelectionError> {
        let package = validate_field(package.into(), "package", MAX_PACKAGE_NAME_BYTES, false)?;
        let target = validate_field(bin.into(), "bin", MAX_PACKAGE_NAME_BYTES, true)?;
        let profile = validate_field(profile.into(), "profile", MAX_PROFILE_BYTES, false)?;
        let target_triple = validate_field(
            target_triple.into(),
            "target triple",
            MAX_TRIPLE_BYTES,
            true,
        )?;
        if target_triple.ends_with(".json") {
            return Err(CargoBuildSelectionError::InvalidField {
                field: "target triple",
            });
        }
        let mut requested = Vec::new();
        for (index, feature) in features.into_iter().enumerate() {
            // Bound consumed items before conversion/allocation, including repeated requests.
            if index >= MAX_FEATURE_COUNT {
                return Err(CargoBuildSelectionError::TooManyFeatures);
            }
            let feature = feature.into();
            if feature.len() > MAX_FEATURE_BYTES || !is_feature_argument(&feature) {
                return Err(CargoBuildSelectionError::InvalidField { field: "feature" });
            }
            requested.push(feature);
        }
        requested.sort_unstable();
        requested.dedup();
        Ok(Self {
            package,
            target,
            profile,
            target_triple,
            features: requested.into_boxed_slice(),
            default_features,
        })
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }

    /// Returns the sorted requested feature arguments, not the resolved feature closure.
    pub fn features(&self) -> &[String] {
        &self.features
    }

    pub const fn default_features(&self) -> bool {
        self.default_features
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CargoBuildSelectionError {
    #[error("Cargo {field} is not one bounded admissible name or feature argument")]
    InvalidField { field: &'static str },
    #[error("Cargo feature selection contains too many features")]
    TooManyFeatures,
}

fn validate_field(
    value: String,
    field: &'static str,
    limit: usize,
    allow_dot: bool,
) -> Result<String, CargoBuildSelectionError> {
    if value.len() > limit || !is_name(&value, allow_dot) {
        return Err(CargoBuildSelectionError::InvalidField { field });
    }
    Ok(value)
}

pub(super) fn is_name(value: &str, allow_dot: bool) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_')
                || (allow_dot && byte == b'.')
        })
}

fn is_feature_name(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'+' | b'.'))
}

fn is_feature_argument(value: &str) -> bool {
    if let Some(dependency) = value.strip_prefix("dep:") {
        return is_name(dependency, false);
    }
    if let Some((dependency, feature)) = value.split_once('/') {
        return is_name(dependency.strip_suffix('?').unwrap_or(dependency), false)
            && is_feature_name(feature);
    }
    is_feature_name(value)
}
