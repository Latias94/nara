//! Retained source and declared-code fences around the original Player package owners.

use std::{cmp::Ordering, io, mem::size_of, path::Path};

use crate::fs::{
    ContentDigest, DirectoryCapability, FileIdentity, FsError, FsOperation, RelativeComponent,
    RelativePath,
};

use super::{PreparedProjectCook, ProjectCookError};
use crate::{
    asset_build::ArtifactBuildObservation,
    project_content::{CookedProductSources, ProjectContentLease},
    project_host::ProjectContentDeliveryReceipt,
    project_tooling::{
        cargo_build::{CargoBuildRequest, CargoInputsSnapshot},
        host_fs::{self, HostDirectoryError},
        player_package::{
            GeneratedPlayerBootstrap, PlayerPackageControlFailure, PlayerPackageLimits,
            PlayerPackageOutcome, PlayerPackagePoint, PreparedPlayerPackage, build_player_package,
        },
    },
};

const BOOTSTRAP_INPUT: &str = "player-bootstrap.rs";

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// Original delivery facts, presence-aware build observations and selected Product sources.
///
/// Capture reopens each observed file through the original root and checks its expected bytes.
/// Verification also checks ownership of the selected Product sources in the original source roots.
/// The fence consists of separately observed file and catalogue facts from that root.
pub struct ProjectCookSourceSelection {
    root: DirectoryCapability,
    root_identity: FileIdentity,
    receipt: ProjectContentDeliveryReceipt,
    observations: Vec<ArtifactBuildObservation>,
    files: Vec<SelectedFile>,
    product_sources: Option<CookedProductSources>,
    _lease: ProjectContentLease,
}

struct SelectedFile {
    path: RelativePath,
    present: Option<(FileIdentity, ContentDigest)>,
}

impl ProjectCookSourceSelection {
    pub(super) fn capture(
        root: &DirectoryCapability,
        receipt: ProjectContentDeliveryReceipt,
        observations: Vec<ArtifactBuildObservation>,
        product_sources: Option<CookedProductSources>,
    ) -> Result<Self, ProjectCookError> {
        if root.identity() != receipt.project_root_identity() {
            return Err(FsError::IdentityMismatch {
                expected: receipt.project_root_identity(),
                actual: root.identity(),
            }
            .into());
        }
        let mut count = receipt.members().len();
        let mut retained = add(
            size_of::<Self>(),
            multiply(
                observations.capacity(),
                size_of::<ArtifactBuildObservation>(),
            )?,
        )?;
        for member in receipt.members() {
            retained = add(retained, source_fact_metadata(member.path().as_str())?)?;
        }
        for observation in &observations {
            retained = add(
                retained,
                match observation {
                    ArtifactBuildObservation::Raw { path, .. } => {
                        count = add(count, 1)?;
                        add(path.as_str().len(), source_fact_metadata(path.as_str())?)?
                    }
                    ArtifactBuildObservation::Processed { format, .. } => format.as_str().len(),
                },
            )?;
        }
        if let Some(sources) = &product_sources {
            retained = add(
                retained,
                sources
                    .retained_path_bytes()
                    .map_err(ProjectCookError::Source)?,
            )?;
        }
        let lease = receipt
            .reserve_consumer_retained(retained)
            .map_err(ProjectCookError::Source)?;
        let root = root.strict_read_only()?;
        let mut facts = Vec::with_capacity(count);
        for member in receipt.members() {
            facts.push((relative(member.path().as_str())?, Some(member.digest())));
        }
        for observation in &observations {
            if let ArtifactBuildObservation::Raw { path, digest } = observation {
                facts.push((
                    relative(path.as_str())?,
                    digest.map(|digest| digest.content()),
                ));
            }
        }
        normalize_source_facts(&mut facts)?;
        let mut files = Vec::with_capacity(facts.len());
        for (path, digest) in facts {
            files.push(SelectedFile::capture(&root, path, digest)?);
        }
        Ok(Self {
            root_identity: root.identity(),
            root,
            receipt,
            observations,
            files,
            product_sources,
            _lease: lease,
        })
    }

    #[must_use]
    pub fn delivery_receipt(&self) -> &ProjectContentDeliveryReceipt {
        &self.receipt
    }

    #[must_use]
    pub fn observations(&self) -> &[ArtifactBuildObservation] {
        &self.observations
    }

    /// Rechecks the canonical project root, original file facts and selected Product ownership.
    pub fn verify(&self, project: &Path) -> Result<(), ProjectCookError> {
        self.verify_project_root(project)?;
        for file in &self.files {
            file.verify(&self.root)?;
        }
        if let Some(sources) = &self.product_sources {
            sources
                .verify(&self.root)
                .map_err(ProjectCookError::Source)?;
        }
        self.verify_project_root(project)
    }

    fn verify_project_root(&self, project: &Path) -> Result<(), ProjectCookError> {
        let current = open_project_root(project)?;
        if self.root.identity() != self.root_identity || !self.root.same_object(&current)? {
            return Err(ProjectCookError::Invalid(
                "the original project root changed".into(),
            ));
        }
        Ok(())
    }
}

impl SelectedFile {
    fn capture(
        root: &DirectoryCapability,
        path: RelativePath,
        digest: Option<ContentDigest>,
    ) -> Result<Self, ProjectCookError> {
        let present = match digest {
            Some(digest) => {
                let file = root.open_file_exact(&path)?;
                file.verify_digest(digest, digest.length())?;
                Some((file.identity(), digest))
            }
            None => {
                require_absent(root, &path)?;
                None
            }
        };
        Ok(Self { path, present })
    }

    fn verify(&self, root: &DirectoryCapability) -> Result<(), ProjectCookError> {
        let Some((identity, digest)) = self.present else {
            return require_absent(root, &self.path);
        };
        let file = root.open_file_exact(&self.path)?;
        if file.identity() != identity {
            return Err(FsError::IdentityMismatch {
                expected: identity,
                actual: file.identity(),
            }
            .into());
        }
        file.verify_digest(digest, digest.length())?;
        Ok(())
    }
}

fn require_absent(root: &DirectoryCapability, path: &RelativePath) -> Result<(), ProjectCookError> {
    match root.open_file_exact(path) {
        Ok(_) => Err(FsError::TargetStateMismatch.into()),
        Err(error) if missing(&error) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn normalize_source_facts(
    facts: &mut Vec<(RelativePath, Option<ContentDigest>)>,
) -> Result<(), ProjectCookError> {
    facts.sort_unstable_by(|left, right| compare_paths(&left.0, &right.0));
    if facts
        .windows(2)
        .any(|pair| pair[0].0 == pair[1].0 && pair[0].1 != pair[1].1)
    {
        return Err(ProjectCookError::Invalid(
            "source observations disagree on a file's bytes or presence".into(),
        ));
    }
    facts.dedup_by(|left, right| left.0 == right.0);
    Ok(())
}

struct ProjectCodeSelection {
    inputs: CargoInputsSnapshot,
    _lease: ProjectContentLease,
}

impl ProjectCodeSelection {
    fn capture(
        source: &ProjectCookSourceSelection,
        request: &mut CargoBuildRequest,
    ) -> Result<Self, ProjectCookError> {
        source.verify_project_root(request.workspace())?;
        let retained = add(
            request.project_rust_input_retained_bytes()?,
            size_of::<ProjectContentLease>(),
        )?;
        let lease = source
            .receipt
            .reserve_consumer_retained(retained)
            .map_err(ProjectCookError::Source)?;
        Ok(Self {
            inputs: request.capture_project_rust_inputs(&source.root)?,
            _lease: lease,
        })
    }

    fn verify(&self, source: &ProjectCookSourceSelection) -> Result<(), ProjectCookError> {
        source.verify(self.inputs.workspace())?;
        Ok(self.inputs.verify()?)
    }
}

impl PreparedProjectCook {
    /// Builds one explicit Player with the original generated-input and Cargo owners.
    ///
    /// The declared inventory must cover the finite default Rust inputs and may explicitly name
    /// additional compiler inputs. Source and code facts remain retained through publication.
    pub fn build_player(
        self,
        mut request: CargoBuildRequest,
        limits: PlayerPackageLimits,
    ) -> Result<PreparedProjectPlayerPackage, ProjectCookError> {
        let Self { content, selection } = self;
        selection.verify(request.workspace())?;
        let code = ProjectCodeSelection::capture(&selection, &mut request)?;
        let (pack, expected) = content.into_pack();
        let bootstrap = GeneratedPlayerBootstrap::new(&pack, &expected, BOOTSTRAP_INPUT, limits)?;
        let request = request.with_generated_rust_input(bootstrap.name(), bootstrap.bytes())?;
        let package =
            build_player_package(pack, &expected, bootstrap, &request, limits, |point| {
                selection
                    .verify(request.workspace())
                    .and_then(|()| match point {
                        PlayerPackagePoint::BeforeBuild | PlayerPackagePoint::AfterBuild => code
                            .inputs
                            .verify_file_set()
                            .map_err(ProjectCookError::from),
                        _ => Ok(()),
                    })
                    .map_err(|_| PlayerPackageControlFailure::SelectionChanged)
            })?;
        code.verify(&selection)?;
        Ok(PreparedProjectPlayerPackage {
            package,
            selection,
            code,
        })
    }
}

/// Original unpublished Player package with its retained preparation source and code fences.
pub struct PreparedProjectPlayerPackage {
    package: PreparedPlayerPackage,
    selection: ProjectCookSourceSelection,
    code: ProjectCodeSelection,
}

impl PreparedProjectPlayerPackage {
    #[must_use]
    pub fn source(&self) -> &ProjectContentDeliveryReceipt {
        self.selection.delivery_receipt()
    }

    #[must_use]
    pub fn observations(&self) -> &[ArtifactBuildObservation] {
        self.selection.observations()
    }

    #[must_use]
    pub fn package(&self) -> &PreparedPlayerPackage {
        &self.package
    }

    /// Delegates complete-directory publication, retaining original rejection and cleanup custody.
    pub fn publish(
        self,
        parent: &DirectoryCapability,
        destination: &RelativeComponent,
    ) -> PlayerPackageOutcome {
        let Self {
            package,
            selection,
            code,
        } = self;
        package.publish(parent, destination, |_| {
            code.verify(&selection)
                .map_err(|_| PlayerPackageControlFailure::SelectionChanged)
        })
    }
}

fn open_project_root(project: &Path) -> Result<DirectoryCapability, ProjectCookError> {
    let canonical = std::fs::canonicalize(project).map_err(|source| FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    })?;
    let root = host_fs::readable_directory(&canonical).map_err(|error| match error {
        HostDirectoryError::Open(source) => FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        },
        HostDirectoryError::Capability(error) => error,
    })?;
    Ok(root.strict_read_only()?)
}

fn source_fact_metadata(name: &str) -> Result<usize, ProjectCookError> {
    let preflight = RelativePath::preflight(name).map_err(FsError::from)?;
    add(
        size_of::<(RelativePath, Option<ContentDigest>)>() + size_of::<SelectedFile>(),
        add(
            multiply(preflight.components(), size_of::<RelativeComponent>())?,
            multiply(name.len(), 2)?,
        )?,
    )
}

fn compare_paths(left: &RelativePath, right: &RelativePath) -> Ordering {
    left.components()
        .iter()
        .map(RelativeComponent::to_str)
        .cmp(right.components().iter().map(RelativeComponent::to_str))
}

fn relative(name: &str) -> Result<RelativePath, ProjectCookError> {
    Ok(RelativePath::new(name).map_err(FsError::from)?)
}

fn missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
}

fn add(left: usize, right: usize) -> Result<usize, ProjectCookError> {
    left.checked_add(right).ok_or_else(|| {
        ProjectCookError::Invalid("project cook selection metadata overflowed".into())
    })
}

fn multiply(left: usize, right: usize) -> Result<usize, ProjectCookError> {
    left.checked_mul(right).ok_or_else(|| {
        ProjectCookError::Invalid("project cook selection metadata overflowed".into())
    })
}
