use std::{
    array, io,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::project_workflow::{
    __private::{
        CargoCompilerArtifactFacts, CargoJsonMessage, CargoJsonMessageError,
        ProjectExportBuildPreparation, ProjectExportCargoArtifact,
    },
    ProjectExportBinaryTarget, ProjectExportBuildError, ProjectExportBuildReceipt,
    StartupContentClosureReceipt,
};
use serde_json::Value;
use thiserror::Error;

use super::acceptance::{self, ExportAcceptanceError};
use crate::project_tooling::host_fs::{self, HostDirectoryError, HostFileError};

const MAX_TOOL_IDENTITY_BYTES: usize = 16 * 1024;
const MAX_CARGO_JSON_BYTES: u64 = 64 * 1024 * 1024;
const RELEASE_PROFILE: &str = "release";
const DESKTOP_FEATURE: &str = "desktop";

#[derive(Debug, Error)]
pub(crate) enum CargoBuildAdapterError {
    #[error("failed to inspect the Rust host target")]
    HostTargetIo(#[source] io::Error),
    #[error("Rust did not report one bounded host target")]
    HostTargetInvalid,
    #[error("failed to start the locked Cargo build")]
    Start(#[source] io::Error),
    #[error("failed while waiting for the locked Cargo build")]
    Wait(#[source] io::Error),
    #[error("the Cargo JSON stream exceeded its bounded size")]
    OutputLimitExceeded,
    #[error("Cargo emitted malformed JSON")]
    Json(#[source] serde_json::Error),
    #[error(transparent)]
    CargoMessage(#[from] CargoJsonMessageError),
    #[error("Cargo emitted an invalid compiler-artifact field: {field}")]
    InvalidArtifactField { field: &'static str },
    #[error("Cargo emitted a compiler artifact for an unexpected binary target")]
    UnexpectedBinaryTarget,
    #[error("Cargo emitted the {target:?} compiler artifact more than once")]
    DuplicateArtifact { target: ProjectExportBinaryTarget },
    #[error("Cargo did not emit the {target:?} compiler artifact")]
    MissingArtifact { target: ProjectExportBinaryTarget },
    #[error("Cargo did not emit exactly one successful build-finished message")]
    BuildFinishedMissing,
    #[error("the locked Cargo build failed")]
    BuildFailed,
    #[error("failed to reopen the project directory around the locked Cargo build")]
    ProjectRoot(#[source] HostDirectoryError),
    #[error(transparent)]
    HostFile(#[from] HostFileError),
    #[error("the built products did not prove the selected runtime schema and source closure")]
    ProductAgreement(#[from] ExportAcceptanceError),
    #[error(transparent)]
    BuildContract(#[from] ProjectExportBuildError),
}

pub(super) struct CargoBuildOutput {
    receipt: ProjectExportBuildReceipt,
}

impl CargoBuildOutput {
    pub(super) fn into_receipt(self) -> ProjectExportBuildReceipt {
        self.receipt
    }
}

pub(super) fn build(
    project_root: &Path,
    startup_content: &StartupContentClosureReceipt,
) -> Result<CargoBuildOutput, CargoBuildAdapterError> {
    build_with_target_directory(project_root, startup_content, None)
}

#[cfg(test)]
pub(super) fn build_in_target_directory(
    project_root: &Path,
    startup_content: &StartupContentClosureReceipt,
    target_directory: &Path,
) -> Result<CargoBuildOutput, CargoBuildAdapterError> {
    build_with_target_directory(project_root, startup_content, Some(target_directory))
}

fn build_with_target_directory(
    project_root: &Path,
    startup_content: &StartupContentClosureReceipt,
    target_directory: Option<&Path>,
) -> Result<CargoBuildOutput, CargoBuildAdapterError> {
    let preparation = ProjectExportBuildPreparation::capture(startup_content)?;
    verify_project_path_authority(project_root, &preparation)?;
    let target_triple = host_target()?;
    let manifest_path = project_root.join("Cargo.toml");
    let mut command = Command::new("cargo");
    command
        .current_dir(project_root)
        .arg("build")
        .arg("--locked")
        .arg("--release")
        .arg("--no-default-features")
        .arg("--features")
        .arg(DESKTOP_FEATURE)
        .args(["--bin", "headless", "--bin", "desktop"])
        .arg("--target")
        .arg(&target_triple)
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("--message-format=json")
        .stdout(Stdio::piped());
    if let Some(target_directory) = target_directory {
        command.env("CARGO_TARGET_DIR", target_directory);
    }
    let mut child = command.spawn().map_err(CargoBuildAdapterError::Start)?;
    let stdout = child
        .stdout
        .take()
        .expect("piped Cargo stdout is available");
    let mut reader = stdout.take(MAX_CARGO_JSON_BYTES.saturating_add(1));
    let mut selector = ArtifactSelector::new(project_root, &manifest_path);
    let parse_result = parse_messages(&mut reader, &mut selector);
    if parse_result.is_err() || reader.limit() == 0 {
        let _ = child.kill();
    }
    let status = child.wait().map_err(CargoBuildAdapterError::Wait)?;
    verify_project_path_authority(project_root, &preparation)?;
    if reader.limit() == 0 {
        return Err(CargoBuildAdapterError::OutputLimitExceeded);
    }
    parse_result?;
    if !status.success() {
        return Err(CargoBuildAdapterError::BuildFailed);
    }
    let selected = selector.finish()?;
    let [headless, desktop] = selected;
    let observations = acceptance::observe_built_products(
        "export-prepublication",
        startup_content,
        [
            (
                ProjectExportBinaryTarget::Headless,
                headless.executable.as_path(),
            ),
            (
                ProjectExportBinaryTarget::Desktop,
                desktop.executable.as_path(),
            ),
        ],
    )?;
    let [headless_observation, desktop_observation] = observations;
    let agreements = [
        headless_observation.into_product_agreement()?,
        desktop_observation.into_product_agreement()?,
    ];
    let headless = bind_artifact(headless, &target_triple)?;
    let desktop = bind_artifact(desktop, &target_triple)?;
    let receipt = preparation
        .bind([headless, desktop], agreements)
        .map_err(CargoBuildAdapterError::from)?;
    Ok(CargoBuildOutput { receipt })
}

fn verify_project_path_authority(
    project_root: &Path,
    preparation: &ProjectExportBuildPreparation,
) -> Result<(), CargoBuildAdapterError> {
    let current =
        host_fs::readable_directory(project_root).map_err(CargoBuildAdapterError::ProjectRoot)?;
    preparation
        .verify_build_inputs(&current)
        .map_err(CargoBuildAdapterError::from)
}

fn bind_artifact(
    artifact: SelectedArtifact,
    target_triple: &str,
) -> Result<ProjectExportCargoArtifact, CargoBuildAdapterError> {
    let file = host_fs::readable_file(&artifact.executable)?;
    ProjectExportCargoArtifact::new(
        artifact.target,
        artifact.package_id,
        target_triple,
        RELEASE_PROFILE,
        [DESKTOP_FEATURE.to_owned()],
        file,
    )
    .map_err(CargoBuildAdapterError::from)
}

fn host_target() -> Result<String, CargoBuildAdapterError> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(CargoBuildAdapterError::HostTargetIo)?;
    if !output.status.success()
        || output.stdout.len() > MAX_TOOL_IDENTITY_BYTES
        || output.stderr.len() > MAX_TOOL_IDENTITY_BYTES
    {
        return Err(CargoBuildAdapterError::HostTargetInvalid);
    }
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|_| CargoBuildAdapterError::HostTargetInvalid)?;
    let host = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .filter(|host| {
            !host.is_empty()
                && host.len() <= 256
                && host.is_ascii()
                && host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or(CargoBuildAdapterError::HostTargetInvalid)?;
    Ok(host.to_owned())
}

fn parse_messages(
    reader: &mut impl io::Read,
    selector: &mut ArtifactSelector,
) -> Result<(), CargoBuildAdapterError> {
    for value in serde_json::Deserializer::from_reader(reader).into_iter::<Value>() {
        let value = value.map_err(CargoBuildAdapterError::Json)?;
        selector.observe(CargoJsonMessage::parse(value)?)?;
    }
    Ok(())
}

struct ArtifactSelector {
    manifest_path: PathBuf,
    source_paths: [PathBuf; 2],
    selected: [Option<SelectedArtifact>; 2],
    build_finished: Option<bool>,
}

impl ArtifactSelector {
    fn new(project_root: &Path, manifest_path: &Path) -> Self {
        Self {
            manifest_path: manifest_path.to_path_buf(),
            source_paths: [
                project_root.join("src/bin/headless.rs"),
                project_root.join("src/bin/desktop.rs"),
            ],
            selected: array::from_fn(|_| None),
            build_finished: None,
        }
    }

    fn observe(&mut self, message: CargoJsonMessage) -> Result<(), CargoBuildAdapterError> {
        match message {
            CargoJsonMessage::CompilerArtifact(artifact) => self.observe_artifact(&artifact),
            CargoJsonMessage::BuildFinished { success } => {
                if self.build_finished.is_some() {
                    return Err(CargoBuildAdapterError::BuildFinishedMissing);
                }
                self.build_finished = Some(success);
                Ok(())
            }
            CargoJsonMessage::CompilerDiagnostic(_) | CargoJsonMessage::BuildScriptExecuted => {
                Ok(())
            }
        }
    }

    fn observe_artifact(
        &mut self,
        artifact: &CargoCompilerArtifactFacts,
    ) -> Result<(), CargoBuildAdapterError> {
        if !paths_match(artifact.manifest_path(), &self.manifest_path) {
            return Ok(());
        }
        if !strings_match(artifact.target_kind(), &["bin"]) {
            return Ok(());
        }
        if !strings_match(artifact.target_crate_types(), &["bin"]) {
            return Err(CargoBuildAdapterError::InvalidArtifactField {
                field: "target.crate_types",
            });
        }
        let target_role = match artifact.target_name() {
            "headless" => ProjectExportBinaryTarget::Headless,
            "desktop" => ProjectExportBinaryTarget::Desktop,
            _ => return Err(CargoBuildAdapterError::UnexpectedBinaryTarget),
        };
        if !paths_match(
            artifact.target_source_path(),
            &self.source_paths[target_index(target_role)],
        ) {
            return Err(CargoBuildAdapterError::InvalidArtifactField {
                field: "target.src_path",
            });
        }
        if !strings_match(artifact.features(), &[DESKTOP_FEATURE]) {
            return Err(CargoBuildAdapterError::InvalidArtifactField { field: "features" });
        }
        if let Some(required_features) = artifact.target_required_features() {
            let expected_required_features = match target_role {
                ProjectExportBinaryTarget::Headless => &[][..],
                ProjectExportBinaryTarget::Desktop => &[DESKTOP_FEATURE][..],
            };
            if !strings_match(required_features, expected_required_features) {
                return Err(CargoBuildAdapterError::InvalidArtifactField {
                    field: "target.required-features",
                });
            }
        }
        if artifact.profile_test() {
            return Err(CargoBuildAdapterError::InvalidArtifactField {
                field: "profile.test",
            });
        }
        let executable = artifact
            .executable()
            .ok_or(CargoBuildAdapterError::InvalidArtifactField {
                field: "executable",
            })?
            .to_path_buf();
        if !executable.is_absolute() {
            return Err(CargoBuildAdapterError::InvalidArtifactField {
                field: "executable",
            });
        }
        let selected = SelectedArtifact {
            target: target_role,
            package_id: artifact.package_id().to_owned(),
            executable,
        };
        let slot = &mut self.selected[target_index(target_role)];
        if slot.replace(selected).is_some() {
            return Err(CargoBuildAdapterError::DuplicateArtifact {
                target: target_role,
            });
        }
        Ok(())
    }

    fn finish(self) -> Result<[SelectedArtifact; 2], CargoBuildAdapterError> {
        if self.build_finished != Some(true) {
            return Err(CargoBuildAdapterError::BuildFinishedMissing);
        }
        let [headless, desktop] = self.selected;
        Ok([
            headless.ok_or(CargoBuildAdapterError::MissingArtifact {
                target: ProjectExportBinaryTarget::Headless,
            })?,
            desktop.ok_or(CargoBuildAdapterError::MissingArtifact {
                target: ProjectExportBinaryTarget::Desktop,
            })?,
        ])
    }
}

struct SelectedArtifact {
    target: ProjectExportBinaryTarget,
    package_id: String,
    executable: PathBuf,
}

fn strings_match(actual: &[String], expected: &[&str]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual == expected)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        fn normalized(path: &Path) -> String {
            let path = path.to_string_lossy();
            path.strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('/', "\\")
                .to_ascii_lowercase()
        }

        normalized(left) == normalized(right)
    }

    #[cfg(not(windows))]
    {
        left == right
    }
}

const fn target_index(target: ProjectExportBinaryTarget) -> usize {
    match target {
        ProjectExportBinaryTarget::Headless => 0,
        ProjectExportBinaryTarget::Desktop => 1,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn exact_headless_and_desktop_artifacts_form_one_selection() {
        let root = test_root();
        let manifest = root.join("Cargo.toml");
        let mut selector = ArtifactSelector::new(&root, &manifest);

        selector
            .observe(artifact_message(&root, "headless"))
            .unwrap();
        selector
            .observe(artifact_message(&root, "desktop"))
            .unwrap();
        selector
            .observe(
                CargoJsonMessage::parse(json!({
                    "reason": "build-finished",
                    "success": true
                }))
                .unwrap(),
            )
            .unwrap();

        let [headless, desktop] = selector.finish().unwrap();
        assert_eq!(headless.target, ProjectExportBinaryTarget::Headless);
        assert_eq!(desktop.target, ProjectExportBinaryTarget::Desktop);
    }

    #[test]
    fn an_additional_project_binary_is_rejected() {
        let root = test_root();
        let manifest = root.join("Cargo.toml");
        let mut selector = ArtifactSelector::new(&root, &manifest);

        let error = selector
            .observe(artifact_message(&root, "editor"))
            .unwrap_err();

        assert!(matches!(
            error,
            CargoBuildAdapterError::UnexpectedBinaryTarget
        ));
    }

    fn artifact_message(root: &Path, target: &str) -> CargoJsonMessage {
        CargoJsonMessage::parse(json!({
            "reason": "compiler-artifact",
            "package_id": "path+file:///project#nara-game@0.1.0",
            "manifest_path": root.join("Cargo.toml"),
            "target": {
                "kind": ["bin"],
                "crate_types": ["bin"],
                "name": target,
                "src_path": root.join(format!("src/bin/{target}.rs")),
                "required-features": if target == "desktop" { json!(["desktop"]) } else { json!([]) },
            },
            "profile": {"test": false},
            "features": ["desktop"],
            "executable": root.join("target/release").join(target),
            "fresh": true,
        }))
        .unwrap()
    }

    fn test_root() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\project")
        } else {
            PathBuf::from("/project")
        }
    }
}
