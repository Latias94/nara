use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use thiserror::Error;

use super::{
    DESKTOP_FEATURE, DESKTOP_TARGET, MAX_FEATURE_BYTES, MAX_FEATURE_COUNT, MAX_METADATA_BYTES,
    MAX_PACKAGE_COUNT, MAX_PACKAGE_ID_BYTES, MAX_PACKAGE_NAME_BYTES, MAX_PATH_BYTES,
    MAX_TARGET_COUNT, MAX_VERSION_BYTES, paths_match,
};

/// Path-safe classification retained for one package in the bounded Cargo graph observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CargoPackageSourceKind {
    Path,
    Registry,
    Git,
    Other,
}

/// One bounded, path-safe package observation from Cargo metadata.
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

/// Bounded Cargo metadata facts admitted for the closed supervised desktop build convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoMetadataFacts {
    pub(super) root_package_id: String,
    pub(super) root_package: CargoPackageGraphFact,
    pub(super) package_graph: Box<[CargoPackageGraphFact]>,
    pub(super) manifest_path: PathBuf,
    pub(super) target_directory: PathBuf,
    pub(super) selected_source_path: PathBuf,
}

impl CargoMetadataFacts {
    pub fn parse(
        metadata: &[u8],
        expected_manifest_path: &Path,
    ) -> Result<Self, CargoMetadataFactsError> {
        Self::parse_selected(metadata, expected_manifest_path, true)
    }

    fn parse_selected(
        metadata: &[u8],
        expected_manifest_path: &Path,
        desktop: bool,
    ) -> Result<Self, CargoMetadataFactsError> {
        if metadata.len() > MAX_METADATA_BYTES {
            return Err(CargoMetadataFactsError::InputLimitExceeded);
        }
        validate_absolute_metadata_path(expected_manifest_path, "expected_manifest_path")?;

        let value = serde_json::from_slice::<Value>(metadata)
            .map_err(CargoMetadataFactsError::MalformedJson)?;
        let object = value
            .as_object()
            .ok_or(CargoMetadataFactsError::InvalidField { field: "metadata" })?;
        if u64_field(object.get("version"), "version")? != 1 {
            return Err(CargoMetadataFactsError::UnsupportedFormatVersion);
        }

        let workspace_root = metadata_path_field(object.get("workspace_root"), "workspace_root")?;
        let expected_workspace_root =
            expected_manifest_path
                .parent()
                .ok_or(CargoMetadataFactsError::InvalidField {
                    field: "expected_manifest_path",
                })?;
        if !paths_match(&workspace_root, expected_workspace_root) {
            return Err(CargoMetadataFactsError::WorkspaceRootMismatch);
        }

        let target_directory =
            metadata_path_field(object.get("target_directory"), "target_directory")?;
        let resolved_root_package_id = optional_resolved_root_package_id(object.get("resolve"))?;

        let workspace_members = metadata_string_array(
            object.get("workspace_members"),
            "workspace_members",
            MAX_PACKAGE_COUNT,
            MAX_PACKAGE_ID_BYTES,
        )?;
        let packages = object
            .get("packages")
            .and_then(Value::as_array)
            .ok_or(CargoMetadataFactsError::InvalidField { field: "packages" })?;
        if packages.len() > MAX_PACKAGE_COUNT {
            return Err(CargoMetadataFactsError::PackageLimitExceeded);
        }

        let mut package_ids = BTreeSet::new();
        let mut package_graph = Vec::with_capacity(packages.len());
        let mut root_package_id = None;
        let mut root_package = None;
        let mut selected_source_path = None;

        for package in packages {
            let package = package
                .as_object()
                .ok_or(CargoMetadataFactsError::InvalidField {
                    field: "packages[]",
                })?;
            let package_id =
                metadata_string_field(package.get("id"), "packages[].id", MAX_PACKAGE_ID_BYTES)?;
            if !package_ids.insert(package_id.to_owned()) {
                return Err(CargoMetadataFactsError::DuplicatePackageId);
            }
            let fact = CargoPackageGraphFact {
                name: metadata_string_field(
                    package.get("name"),
                    "packages[].name",
                    MAX_PACKAGE_NAME_BYTES,
                )?
                .to_owned(),
                version: metadata_string_field(
                    package.get("version"),
                    "packages[].version",
                    MAX_VERSION_BYTES,
                )?
                .to_owned(),
                source_kind: package_source_kind(package.get("source"))?,
            };
            let manifest_path =
                metadata_path_field(package.get("manifest_path"), "packages[].manifest_path")?;
            if paths_match(&manifest_path, expected_manifest_path) {
                if root_package.replace(fact.clone()).is_some() {
                    return Err(CargoMetadataFactsError::DuplicateRootPackage);
                }
                root_package_id = Some(package_id.to_owned());
                if desktop {
                    require_desktop_feature(package)?;
                } else if package
                    .get("metadata")
                    .and_then(|metadata| metadata.get("nara"))
                    .and_then(|metadata| metadata.get("project-tool-protocol"))
                    .and_then(Value::as_u64)
                    != Some(2)
                {
                    return Err(CargoMetadataFactsError::ProjectToolProtocol);
                }
                selected_source_path = Some(select_target(package, desktop)?);
            }
            package_graph.push(fact);
        }

        let root_package_id = root_package_id.ok_or(CargoMetadataFactsError::MissingRootPackage)?;
        if resolved_root_package_id
            .as_deref()
            .is_some_and(|resolved| resolved != root_package_id.as_str())
        {
            return Err(CargoMetadataFactsError::RootResolutionMismatch);
        }
        if !workspace_members
            .iter()
            .any(|member| member == &root_package_id)
        {
            return Err(CargoMetadataFactsError::RootPackageNotWorkspaceMember);
        }
        let root_package = root_package.ok_or(CargoMetadataFactsError::MissingRootPackage)?;
        let selected_source_path = selected_source_path.ok_or(if desktop {
            CargoMetadataFactsError::MissingDesktopTarget
        } else {
            CargoMetadataFactsError::MissingHeadlessTarget
        })?;
        package_graph.sort_unstable();

        Ok(Self {
            root_package_id,
            root_package,
            package_graph: package_graph.into_boxed_slice(),
            manifest_path: expected_manifest_path.to_path_buf(),
            target_directory,
            selected_source_path,
        })
    }

    pub fn root_package(&self) -> &CargoPackageGraphFact {
        &self.root_package
    }

    pub fn package_name(&self) -> &str {
        self.root_package.name()
    }

    pub fn package_version(&self) -> &str {
        self.root_package.version()
    }

    pub fn package_graph(&self) -> &[CargoPackageGraphFact] {
        &self.package_graph
    }
}

/// Bounded discovery facts for the existing project's headless tool entry.
#[derive(Debug)]
pub struct CargoHeadlessMetadataFacts(CargoMetadataFacts);

impl CargoHeadlessMetadataFacts {
    pub fn parse(metadata: &[u8], manifest_path: &Path) -> Result<Self, CargoMetadataFactsError> {
        CargoMetadataFacts::parse_selected(metadata, manifest_path, false).map(Self)
    }

    pub fn package_id(&self) -> &str {
        &self.0.root_package_id
    }
    pub fn source_path(&self) -> &Path {
        &self.0.selected_source_path
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
    #[error("Cargo metadata did not identify one root package")]
    MissingRootPackage,
    #[error("Cargo metadata repeated the package that owns the selected manifest")]
    DuplicateRootPackage,
    #[error("the resolved Cargo root package is not a workspace member")]
    RootPackageNotWorkspaceMember,
    #[error("the Cargo workspace root does not match the selected project manifest")]
    WorkspaceRootMismatch,
    #[error("Cargo resolve.root does not match the package that owns the selected manifest")]
    RootResolutionMismatch,
    #[error("the Cargo root package does not declare the `desktop` feature")]
    MissingDesktopFeature,
    #[error("the Cargo root package did not define the exact `desktop` binary target")]
    MissingDesktopTarget,
    #[error("the Cargo root package did not define the exact `headless` binary target")]
    MissingHeadlessTarget,
    #[error(
        "the project does not declare supported package.metadata.nara.project-tool-protocol = 2; update its headless tool entry before invoking project tools"
    )]
    ProjectToolProtocol,
    #[error("the Cargo root package defined the `desktop` target more than once")]
    DuplicateDesktopTarget,
    #[error("the Cargo `desktop` target field `{field}` did not match the closed convention")]
    DesktopTargetMismatch { field: &'static str },
    #[error("Cargo metadata path field `{field}` was not absolute")]
    NonAbsolutePath { field: &'static str },
}

fn require_desktop_feature(package: &Map<String, Value>) -> Result<(), CargoMetadataFactsError> {
    let features = package.get("features").and_then(Value::as_object).ok_or(
        CargoMetadataFactsError::InvalidField {
            field: "packages[].features",
        },
    )?;
    if features.len() > MAX_FEATURE_COUNT {
        return Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "packages[].features",
        });
    }
    if !features.contains_key(DESKTOP_FEATURE) {
        return Err(CargoMetadataFactsError::MissingDesktopFeature);
    }
    Ok(())
}

fn select_target(
    package: &Map<String, Value>,
    desktop: bool,
) -> Result<PathBuf, CargoMetadataFactsError> {
    let selected_name = if desktop { DESKTOP_TARGET } else { "headless" };
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
        let name = metadata_string_field(
            target.get("name"),
            "packages[].targets[].name",
            MAX_PACKAGE_NAME_BYTES,
        )?;
        if name != selected_name {
            continue;
        }
        if selected.is_some() {
            return Err(CargoMetadataFactsError::DuplicateDesktopTarget);
        }
        if metadata_string_array(
            target.get("kind"),
            "packages[].targets[].kind",
            MAX_FEATURE_COUNT,
            MAX_FEATURE_BYTES,
        )?
        .as_slice()
            != ["bin"]
        {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch { field: "kind" });
        }
        if metadata_string_array(
            target.get("crate_types"),
            "packages[].targets[].crate_types",
            MAX_FEATURE_COUNT,
            MAX_FEATURE_BYTES,
        )?
        .as_slice()
            != ["bin"]
        {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch {
                field: "crate_types",
            });
        }
        let required_features = if !desktop && target.get("required-features").is_none() {
            Box::default()
        } else {
            normalized_metadata_features(
                target.get("required-features"),
                "packages[].targets[].required-features",
            )?
        };
        let expected_features = if desktop {
            &[DESKTOP_FEATURE][..]
        } else {
            &[][..]
        };
        if required_features.as_ref() != expected_features {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch {
                field: "required-features",
            });
        }
        selected = Some(metadata_path_field(
            target.get("src_path"),
            "packages[].targets[].src_path",
        )?);
    }
    selected.ok_or(if desktop {
        CargoMetadataFactsError::MissingDesktopTarget
    } else {
        CargoMetadataFactsError::MissingHeadlessTarget
    })
}

fn optional_resolved_root_package_id(
    value: Option<&Value>,
) -> Result<Option<String>, CargoMetadataFactsError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Object(resolve) = value else {
        if value.is_null() {
            return Ok(None);
        }
        return Err(CargoMetadataFactsError::InvalidField { field: "resolve" });
    };
    match resolve.get("root") {
        Some(Value::Null) | None => Ok(None),
        Some(root) => metadata_string_field(Some(root), "resolve.root", MAX_PACKAGE_ID_BYTES)
            .map(|root| Some(root.to_owned())),
    }
}

fn package_source_kind(
    value: Option<&Value>,
) -> Result<CargoPackageSourceKind, CargoMetadataFactsError> {
    match value {
        Some(Value::Null) | None => Ok(CargoPackageSourceKind::Path),
        Some(Value::String(source)) => {
            if source.len() > MAX_PATH_BYTES {
                return Err(CargoMetadataFactsError::FieldLimitExceeded {
                    field: "packages[].source",
                });
            }
            Ok(if source.starts_with("registry+") {
                CargoPackageSourceKind::Registry
            } else if source.starts_with("git+") {
                CargoPackageSourceKind::Git
            } else if source.starts_with("path+") {
                CargoPackageSourceKind::Path
            } else {
                CargoPackageSourceKind::Other
            })
        }
        Some(_) => Err(CargoMetadataFactsError::InvalidField {
            field: "packages[].source",
        }),
    }
}

fn validate_absolute_metadata_path(
    path: &Path,
    field: &'static str,
) -> Result<(), CargoMetadataFactsError> {
    if path.to_string_lossy().len() > MAX_PATH_BYTES {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
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
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if value.is_empty() {
        return Err(CargoMetadataFactsError::InvalidField { field });
    }
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

fn u64_field(value: Option<&Value>, field: &'static str) -> Result<u64, CargoMetadataFactsError> {
    value
        .and_then(Value::as_u64)
        .ok_or(CargoMetadataFactsError::InvalidField { field })
}
