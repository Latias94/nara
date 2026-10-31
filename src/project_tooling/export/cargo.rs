//! The closed desktop-2D starter preset consumes the general captured Cargo build contract.

use std::{path::Path, process::Command};

use thiserror::Error;

use crate::project_tooling::{
    cargo_build::{
        CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools, bounded_output,
        build_executable, parse_host_target,
    },
    host_fs,
};
use crate::project_workflow::{
    __private::{ProjectExportBuildPreparation, ProjectExportCargoArtifact},
    ProjectExportBinaryTarget, ProjectExportBuildError, ProjectExportBuildReceipt,
    StartupContentClosureReceipt,
    cargo::{CargoBuildSelection, CargoMetadataFacts},
};
use nara_fs::RelativePath;

use super::acceptance::{self, ExportAcceptanceError};

const STARTER_BUILD_INPUTS: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "src/lib.rs",
    "src/bin/headless.rs",
    "src/bin/desktop.rs",
];

#[derive(Debug, Error)]
pub(crate) enum CargoBuildAdapterError {
    #[error(transparent)]
    Build(#[from] CargoBuildFailure),
    #[error("the closed starter build selection could not be admitted")]
    Selection,
    #[error("the project directory changed around the build")]
    ProjectRoot,
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

fn input_paths() -> impl Iterator<Item = RelativePath> {
    STARTER_BUILD_INPUTS
        .into_iter()
        .map(|path| RelativePath::new(path).expect("fixed starter input path"))
}

fn build_with_target_directory(
    project_root: &Path,
    startup_content: &StartupContentClosureReceipt,
    target_directory: Option<&Path>,
) -> Result<CargoBuildOutput, CargoBuildAdapterError> {
    let preparation = ProjectExportBuildPreparation::capture(startup_content, input_paths())?;
    verify_project_path_authority(project_root, &preparation)?;
    let tools = CargoBuildTools::discover_in(project_root)?;
    let mut rustc = Command::new(tools.rustc_path());
    rustc.current_dir(project_root).arg("-vV");
    let target_triple = parse_host_target(&bounded_output(&mut rustc, 16 * 1024)?)?;
    let manifest = project_root.join("Cargo.toml");
    let mut metadata = Command::new(tools.cargo_path());
    metadata
        .current_dir(project_root)
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--no-default-features",
            "--features",
            "desktop",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--filter-platform")
        .arg(&target_triple)
        .env("RUSTC", tools.rustc_path());
    if let Some(directory) = target_directory {
        metadata.env("CARGO_TARGET_DIR", directory);
    }
    let metadata = bounded_output(&mut metadata, 16 * 1024 * 1024)?;
    let package = CargoMetadataFacts::package_name_for_manifest(&metadata, &manifest)
        .map_err(|_| CargoBuildAdapterError::Selection)?;
    let mut captured = Vec::with_capacity(2);
    for role in [
        ProjectExportBinaryTarget::Headless,
        ProjectExportBinaryTarget::Desktop,
    ] {
        let selection = CargoBuildSelection::new(
            &package,
            role.cargo_target_name(),
            "release",
            &target_triple,
            ["desktop"],
            false,
        )
        .map_err(|_| CargoBuildAdapterError::Selection)?;
        let mut request = CargoBuildRequest::new(
            project_root,
            RelativePath::new("Cargo.toml").expect("fixed manifest path"),
            selection,
            tools.clone(),
            input_paths(),
            CargoBuildLimits::default(),
        )?;
        if let Some(directory) = target_directory {
            request = request.with_target_directory(directory)?;
        }
        captured.push(build_executable(&request, |_| {
            verify_project_path_authority(project_root, &preparation)
                .map_err(|_| CargoBuildFailure::Stale)
        })?);
    }
    let mut captured = captured.into_iter();
    let headless = captured.next().expect("two selected starter products");
    let desktop = captured.next().expect("two selected starter products");
    // Native product qualification is deliberately after target build. This adapter selects its
    // current Host target; the general builder never executes a foreign or native artifact.
    headless.verify_original()?;
    desktop.verify_original()?;
    let [headless_observation, desktop_observation] = acceptance::observe_built_products(
        "export-prepublication",
        startup_content,
        [
            (
                ProjectExportBinaryTarget::Headless,
                headless.original_path(),
            ),
            (ProjectExportBinaryTarget::Desktop, desktop.original_path()),
        ],
    )?;
    headless.verify_original()?;
    desktop.verify_original()?;
    let agreements = [
        headless_observation.into_product_agreement()?,
        desktop_observation.into_product_agreement()?,
    ];
    let artifacts = [
        ProjectExportCargoArtifact::from_captured(ProjectExportBinaryTarget::Headless, headless)?,
        ProjectExportCargoArtifact::from_captured(ProjectExportBinaryTarget::Desktop, desktop)?,
    ];
    let receipt = preparation.bind(artifacts, agreements)?;
    Ok(CargoBuildOutput { receipt })
}

fn verify_project_path_authority(
    project_root: &Path,
    preparation: &ProjectExportBuildPreparation,
) -> Result<(), CargoBuildAdapterError> {
    let current = host_fs::readable_directory(project_root)
        .map_err(|_| CargoBuildAdapterError::ProjectRoot)?;
    preparation.verify_build_inputs(&current)?;
    Ok(())
}
