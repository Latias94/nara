use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use thiserror::Error;

use super::build::{CargoBuildInvocation, CargoInvocationError};
use super::selection::{CargoBuildSelection, is_name};
use super::{
    MAX_FEATURE_BYTES, MAX_FEATURE_COUNT, MAX_METADATA_BYTES, MAX_PACKAGE_COUNT,
    MAX_PACKAGE_ID_BYTES, MAX_PACKAGE_NAME_BYTES, MAX_PATH_BYTES, MAX_TARGET_COUNT,
    MAX_VERSION_BYTES, paths_match,
};

/// Path-safe classification retained for one package in the bounded Cargo graph observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CargoPackageSourceKind {
    Path,
    Registry,
    Git,
    Other,
}

/// A package observation without package IDs, absolute paths or source URLs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CargoPackageGraphFact {
    name: String,
    version: String,
    source_kind: CargoPackageSourceKind,
}

impl CargoPackageGraphFact {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub const fn source_kind(&self) -> CargoPackageSourceKind {
        self.source_kind
    }
}

/// Bounded full Cargo metadata for one explicit package/bin selection.
///
/// The caller obtains metadata with the same feature/default-feature/target selection used for
/// the build. The selected resolve node observes workspace features, not the features compiled
/// for one binary. Cargo owns feature resolution and target eligibility; this parser only retains
/// bounded declarations and observations. Operational paths and IDs remain transient facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoMetadataFacts {
    pub(super) selection: CargoBuildSelection,
    pub(super) package_id: String,
    pub(super) package: CargoPackageGraphFact,
    package_graph: Box<[CargoPackageGraphFact]>,
    pub(super) manifest_path: PathBuf,
    pub(super) workspace_root: PathBuf,
    pub(super) target_directory: PathBuf,
    pub(super) source_path: PathBuf,
    pub(super) required_features: Box<[String]>,
    pub(super) metadata_features: Box<[String]>,
    package_source: Option<String>,
    project_tool_protocol: Option<u64>,
}

impl CargoMetadataFacts {
    pub fn parse(
        metadata: &[u8],
        expected_package_manifest: &Path,
        selection: &CargoBuildSelection,
    ) -> Result<Self, CargoMetadataFactsError> {
        let value = metadata_document(metadata, expected_package_manifest)?;
        let selected = manifest_owner(&value, expected_package_manifest)?;
        if selection.package() != selected.package.name() {
            return Err(CargoMetadataFactsError::SelectedPackageMismatch);
        }
        // resolve.root may identify a different workspace package or be null. The selected
        // package's node is a workspace observation, not an expected single-build feature set.
        let metadata_features =
            selected_metadata_features(value.get("resolve"), selected.package_id)?;
        let (source_path, required_features) =
            select_binary(selected.metadata, selection.target())?;
        let project_tool_protocol = selected
            .metadata
            .get("metadata")
            .and_then(|metadata| metadata.get("nara"))
            .and_then(|nara| nara.get("project-tool-protocol"))
            .and_then(Value::as_u64);
        Ok(Self {
            selection: selection.clone(),
            package_id: selected.package_id.to_owned(),
            package: selected.package,
            package_graph: selected.package_graph,
            manifest_path: selected.manifest_path,
            workspace_root: selected.workspace_root,
            target_directory: selected.target_directory,
            source_path,
            required_features,
            metadata_features,
            package_source: selected.source.map(str::to_owned),
            project_tool_protocol,
        })
    }

    /// Discovers the manifest's exact workspace package without assuming any binary convention.
    ///
    /// Starter adapters can then construct an explicit selection and parse the full resolved facts.
    pub fn package_name_for_manifest(
        metadata: &[u8],
        expected_package_manifest: &Path,
    ) -> Result<String, CargoMetadataFactsError> {
        let value = metadata_document(metadata, expected_package_manifest)?;
        Ok(manifest_owner(&value, expected_package_manifest)?
            .package
            .name)
    }

    pub fn package(&self) -> &CargoPackageGraphFact {
        &self.package
    }

    /// Returns Cargo's transient, potentially path-bearing package ID.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn package_name(&self) -> &str {
        self.package.name()
    }

    pub fn package_version(&self) -> &str {
        self.package.version()
    }

    pub fn package_graph(&self) -> &[CargoPackageGraphFact] {
        &self.package_graph
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn target_directory(&self) -> &Path {
        &self.target_directory
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Returns a transient source URL, separate from the path-safe package graph.
    pub fn package_source(&self) -> Option<&str> {
        self.package_source.as_deref()
    }

    /// Returns the selected package's features observed by workspace metadata.
    ///
    /// The selected binary's actual compiled features are reported by its compiler artifact.
    pub fn metadata_features(&self) -> &[String] {
        &self.metadata_features
    }

    /// Optional starter bootstrap declaration; generic Cargo facts do not enforce its version.
    pub const fn project_tool_protocol(&self) -> Option<u64> {
        self.project_tool_protocol
    }

    pub fn build_invocation(
        &self,
        selection: CargoBuildSelection,
    ) -> Result<CargoBuildInvocation, CargoInvocationError> {
        CargoBuildInvocation::new(self, selection)
    }
}

#[derive(Debug, Error)]
pub enum CargoMetadataFactsError {
    #[error("Cargo metadata exceeded its bounded input size")]
    InputLimitExceeded,
    #[error("Cargo metadata emitted malformed JSON")]
    MalformedJson(#[source] serde_json::Error),
    #[error("Cargo metadata field `{field}` was missing or invalid")]
    InvalidField { field: &'static str },
    #[error("Cargo metadata field `{field}` exceeded its bounded size")]
    FieldLimitExceeded { field: &'static str },
    #[error("Cargo metadata version is not supported")]
    UnsupportedFormatVersion,
    #[error("Cargo metadata contained too many packages")]
    PackageLimitExceeded,
    #[error("Cargo metadata repeated a package identity")]
    DuplicatePackageId,
    #[error("Cargo metadata did not identify the package owning the selected manifest")]
    MissingSelectedPackage,
    #[error("Cargo metadata repeated the package that owns the selected manifest")]
    DuplicateSelectedPackage,
    #[error("the selected Cargo package is not a workspace member")]
    SelectedPackageNotWorkspaceMember,
    #[error("the requested Cargo package does not own the selected manifest")]
    SelectedPackageMismatch,
    #[error("Cargo metadata did not include the selected package's resolve node")]
    MissingSelectedResolveNode,
    #[error("Cargo metadata repeated a resolve node identity")]
    DuplicateResolveNode,
    #[error("the selected Cargo package did not define the requested binary")]
    MissingSelectedTarget,
    #[error("the selected Cargo package defined the requested binary more than once")]
    DuplicateSelectedTarget,
    #[error("the selected Cargo binary field `{field}` was invalid")]
    SelectedTargetMismatch { field: &'static str },
    #[error("Cargo metadata path field `{field}` was not absolute")]
    NonAbsolutePath { field: &'static str },
}

fn metadata_document(
    metadata: &[u8],
    expected_package_manifest: &Path,
) -> Result<Value, CargoMetadataFactsError> {
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(CargoMetadataFactsError::InputLimitExceeded);
    }
    validate_absolute_metadata_path(expected_package_manifest, "expected_package_manifest")?;
    let value = serde_json::from_slice::<Value>(metadata)
        .map_err(CargoMetadataFactsError::MalformedJson)?;
    let object = value
        .as_object()
        .ok_or(CargoMetadataFactsError::InvalidField { field: "metadata" })?;
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(CargoMetadataFactsError::InvalidField { field: "version" })?;
    if version != 1 {
        return Err(CargoMetadataFactsError::UnsupportedFormatVersion);
    }
    Ok(value)
}

struct ManifestOwner<'a> {
    package_id: &'a str,
    package: CargoPackageGraphFact,
    package_graph: Box<[CargoPackageGraphFact]>,
    manifest_path: PathBuf,
    workspace_root: PathBuf,
    target_directory: PathBuf,
    metadata: &'a Map<String, Value>,
    source: Option<&'a str>,
}

fn manifest_owner<'a>(
    value: &'a Value,
    expected_package_manifest: &Path,
) -> Result<ManifestOwner<'a>, CargoMetadataFactsError> {
    let workspace_root = metadata_path_field(value.get("workspace_root"), "workspace_root")?;
    let target_directory = metadata_path_field(value.get("target_directory"), "target_directory")?;
    let workspace_members = metadata_string_array(
        value.get("workspace_members"),
        "workspace_members",
        MAX_PACKAGE_COUNT,
        MAX_PACKAGE_ID_BYTES,
    )?;
    if workspace_members.iter().collect::<BTreeSet<_>>().len() != workspace_members.len() {
        return Err(CargoMetadataFactsError::InvalidField {
            field: "workspace_members",
        });
    }
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or(CargoMetadataFactsError::InvalidField { field: "packages" })?;
    if packages.len() > MAX_PACKAGE_COUNT {
        return Err(CargoMetadataFactsError::PackageLimitExceeded);
    }

    let mut package_ids = BTreeSet::new();
    let mut package_graph = Vec::with_capacity(packages.len());
    let mut selected = None;
    for package in packages {
        let package = package
            .as_object()
            .ok_or(CargoMetadataFactsError::InvalidField {
                field: "packages[]",
            })?;
        let package_id =
            metadata_string_field(package.get("id"), "packages[].id", MAX_PACKAGE_ID_BYTES)?;
        if !package_ids.insert(package_id) {
            return Err(CargoMetadataFactsError::DuplicatePackageId);
        }
        let name = metadata_string_field(
            package.get("name"),
            "packages[].name",
            MAX_PACKAGE_NAME_BYTES,
        )?;
        let version = metadata_string_field(
            package.get("version"),
            "packages[].version",
            MAX_VERSION_BYTES,
        )?;
        if !is_name(name, false) {
            return Err(CargoMetadataFactsError::InvalidField {
                field: "packages[].name",
            });
        }
        if !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err(CargoMetadataFactsError::InvalidField {
                field: "packages[].version",
            });
        }
        let source = package_source(package.get("source"))?;
        let fact = CargoPackageGraphFact {
            name: name.to_owned(),
            version: version.to_owned(),
            source_kind: package_source_kind(source),
        };
        let manifest_path =
            metadata_path_field(package.get("manifest_path"), "packages[].manifest_path")?;
        if paths_match(&manifest_path, expected_package_manifest) {
            if selected.is_some() {
                return Err(CargoMetadataFactsError::DuplicateSelectedPackage);
            }
            selected = Some((package_id, fact.clone(), manifest_path, package, source));
        }
        package_graph.push(fact);
    }

    let (package_id, package, manifest_path, metadata, source) =
        selected.ok_or(CargoMetadataFactsError::MissingSelectedPackage)?;
    if !workspace_members.iter().any(|member| member == package_id) {
        return Err(CargoMetadataFactsError::SelectedPackageNotWorkspaceMember);
    }
    package_graph.sort_unstable();
    Ok(ManifestOwner {
        package_id,
        package,
        package_graph: package_graph.into_boxed_slice(),
        manifest_path,
        workspace_root,
        target_directory,
        metadata,
        source,
    })
}

fn select_binary(
    package: &Map<String, Value>,
    selected_name: &str,
) -> Result<(PathBuf, Box<[String]>), CargoMetadataFactsError> {
    let targets = package.get("targets").and_then(Value::as_array).ok_or(
        CargoMetadataFactsError::InvalidField {
            field: "packages[].targets",
        },
    )?;
    if targets.len() > MAX_TARGET_COUNT {
        return Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "packages[].targets",
        });
    }
    let mut selected = None;
    for target in targets {
        let target = target
            .as_object()
            .ok_or(CargoMetadataFactsError::InvalidField {
                field: "packages[].targets[]",
            })?;
        if metadata_string_field(
            target.get("name"),
            "packages[].targets[].name",
            MAX_PACKAGE_NAME_BYTES,
        )? != selected_name
        {
            continue;
        }
        let kind = normalized_metadata_features(target.get("kind"), "packages[].targets[].kind")?;
        if !kind.iter().any(|kind| kind == "bin") {
            continue;
        }
        if kind.as_ref() != ["bin"] {
            return Err(CargoMetadataFactsError::SelectedTargetMismatch { field: "kind" });
        }
        if selected.is_some() {
            return Err(CargoMetadataFactsError::DuplicateSelectedTarget);
        }
        if normalized_metadata_features(
            target.get("crate_types"),
            "packages[].targets[].crate_types",
        )?
        .as_ref()
            != ["bin"]
        {
            return Err(CargoMetadataFactsError::SelectedTargetMismatch {
                field: "crate_types",
            });
        }
        let required_features = target
            .get("required-features")
            .map(|features| {
                normalized_metadata_features(
                    Some(features),
                    "packages[].targets[].required-features",
                )
            })
            .transpose()?
            .unwrap_or_default();
        selected = Some((
            metadata_path_field(target.get("src_path"), "packages[].targets[].src_path")?,
            required_features,
        ));
    }
    selected.ok_or(CargoMetadataFactsError::MissingSelectedTarget)
}

fn selected_metadata_features(
    value: Option<&Value>,
    selected_id: &str,
) -> Result<Box<[String]>, CargoMetadataFactsError> {
    let nodes = value
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(Value::as_array)
        .ok_or(CargoMetadataFactsError::InvalidField {
            field: "resolve.nodes",
        })?;
    if nodes.len() > MAX_PACKAGE_COUNT {
        return Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "resolve.nodes",
        });
    }
    let mut ids = BTreeSet::new();
    let mut selected = None;
    for node in nodes {
        let id = metadata_string_field(node.get("id"), "resolve.nodes[].id", MAX_PACKAGE_ID_BYTES)?;
        if !ids.insert(id) {
            return Err(CargoMetadataFactsError::DuplicateResolveNode);
        }
        let features =
            normalized_metadata_features(node.get("features"), "resolve.nodes[].features")?;
        if id == selected_id {
            selected = Some(features);
        }
    }
    selected.ok_or(CargoMetadataFactsError::MissingSelectedResolveNode)
}

fn package_source(value: Option<&Value>) -> Result<Option<&str>, CargoMetadataFactsError> {
    match value {
        Some(Value::Null) | None => Ok(None),
        Some(value) => {
            metadata_string_field(Some(value), "packages[].source", MAX_PATH_BYTES).map(Some)
        }
    }
}

fn package_source_kind(source: Option<&str>) -> CargoPackageSourceKind {
    match source {
        None => CargoPackageSourceKind::Path,
        Some(source) if source.starts_with("registry+") => CargoPackageSourceKind::Registry,
        Some(source) if source.starts_with("git+") => CargoPackageSourceKind::Git,
        Some(source) if source.starts_with("path+") => CargoPackageSourceKind::Path,
        Some(_) => CargoPackageSourceKind::Other,
    }
}

fn validate_absolute_metadata_path(
    path: &Path,
    field: &'static str,
) -> Result<(), CargoMetadataFactsError> {
    let text = path
        .to_str()
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if text.len() > MAX_PATH_BYTES {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    if text.chars().any(char::is_control) {
        return Err(CargoMetadataFactsError::InvalidField { field });
    }
    if !path.is_absolute() {
        return Err(CargoMetadataFactsError::NonAbsolutePath { field });
    }
    Ok(())
}

fn metadata_path_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<PathBuf, CargoMetadataFactsError> {
    let path = PathBuf::from(metadata_string_field(value, field, MAX_PATH_BYTES)?);
    validate_absolute_metadata_path(&path, field)?;
    Ok(path)
}

fn metadata_string_field<'a>(
    value: Option<&'a Value>,
    field: &'static str,
    max_bytes: usize,
) -> Result<&'a str, CargoMetadataFactsError> {
    let value = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if value.len() > max_bytes {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    Ok(value)
}

fn metadata_string_array(
    value: Option<&Value>,
    field: &'static str,
    max_items: usize,
    max_item_bytes: usize,
) -> Result<Vec<String>, CargoMetadataFactsError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if values.len() > max_items {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    values
        .iter()
        .map(|value| metadata_string_field(Some(value), field, max_item_bytes).map(str::to_owned))
        .collect()
}

fn normalized_metadata_features(
    value: Option<&Value>,
    field: &'static str,
) -> Result<Box<[String]>, CargoMetadataFactsError> {
    let mut features = metadata_string_array(value, field, MAX_FEATURE_COUNT, MAX_FEATURE_BYTES)?;
    features.sort_unstable();
    if features.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CargoMetadataFactsError::InvalidField { field });
    }
    Ok(features.into_boxed_slice())
}
