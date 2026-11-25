//! Retained source and declared-code fences around the original Player package owners.

use std::{
    cmp::Ordering,
    io,
    mem::size_of,
    path::{Path, PathBuf},
};

use crate::fs::{
    ContentDigest, DirectoryCapability, DirectoryEnumerationLimits, FileIdentity, FileKind,
    FsError, FsOperation, RelativeComponent, RelativePath,
};

use super::{PreparedProjectCook, ProjectCookError};
use crate::{
    asset_build::ArtifactBuildObservation,
    project_content::{CookedProductSources, ProjectContentLease},
    project_host::ProjectContentDeliveryReceipt,
    project_tooling::{
        cargo_build::{CargoBuildFailure, CargoBuildRequest},
        host_fs::{self, HostDirectoryError},
        player_package::{
            GeneratedPlayerBootstrap, PlayerPackageControlFailure, PlayerPackageLimits,
            PlayerPackageOutcome, PreparedPlayerPackage, build_player_package,
        },
    },
};

const BOOTSTRAP_INPUT: &str = "player-bootstrap.rs";
const MAX_RUST_FILES: usize = 4096;
const MAX_RUST_DEPTH: usize = 32;
const MAX_RUST_ENTRIES: usize = 16_384;
const MAX_RUST_PATH_BYTES: usize = 4096;
const MAX_RUST_METADATA_BYTES: usize = 64 * 1024 * 1024;
const ENUMERATION_BATCH: usize = 32;

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

/// Enumerates the finite default Rust inputs without resolving Cargo dependencies.
///
/// Includes root `Cargo.toml` and `Cargo.lock`, optional root `build.rs`, and regular Rust files
/// below an optional `src/`. Other compiler inputs belong in the caller's explicit Cargo inventory.
/// Traversal rejects linked or unportable children and bounds files, entries, depth and path work.
pub fn project_rust_input_inventory(project: &Path) -> Result<Vec<RelativePath>, ProjectCookError> {
    let root = open_project_root(project)?;
    rust_inventory(&root, None)
}

fn rust_inventory(
    root: &DirectoryCapability,
    declared: Option<&[&RelativePath]>,
) -> Result<Vec<RelativePath>, ProjectCookError> {
    let file_limit = declared.map_or(MAX_RUST_FILES, |paths| paths.len().min(MAX_RUST_FILES));
    let mut inventory = RustInventory {
        files: Vec::with_capacity(file_limit),
        declared,
        file_limit,
        entries: 0,
        metadata_bytes: 0,
    };
    for name in ["Cargo.toml", "Cargo.lock"] {
        let path = relative(name)?;
        root.open_file_exact(&path)?;
        inventory.push(path)?;
    }
    let build = relative("build.rs")?;
    match root.open_file_exact(&build) {
        Ok(_) => inventory.push(build)?,
        Err(error) if missing(&error) => {}
        Err(error) => return Err(error.into()),
    }
    let source = relative("src")?;
    match root.open_directory_exact(&source) {
        Ok(directory) => inventory.visit(&directory, &source)?,
        Err(error) if missing(&error) => {}
        Err(error) => return Err(error.into()),
    }
    inventory.files.sort_unstable_by(compare_paths);
    Ok(inventory.files)
}

struct RustInventory<'a> {
    files: Vec<RelativePath>,
    declared: Option<&'a [&'a RelativePath]>,
    file_limit: usize,
    entries: usize,
    metadata_bytes: usize,
}

impl RustInventory<'_> {
    fn push(&mut self, path: RelativePath) -> Result<(), ProjectCookError> {
        if self.declared.is_some_and(|declared| {
            declared
                .binary_search_by(|selected| compare_paths(selected, &path))
                .is_err()
        }) {
            return Err(CargoBuildFailure::MissingDeclaredInput.into());
        }
        if self.files.len() >= self.file_limit {
            return Err(ProjectCookError::Invalid(
                "Rust input inventory exceeded its finite file limit".into(),
            ));
        }
        self.charge(relative_metadata(&path)?)?;
        self.files.push(path);
        Ok(())
    }

    fn visit(
        &mut self,
        directory: &DirectoryCapability,
        prefix: &RelativePath,
    ) -> Result<(), ProjectCookError> {
        if prefix.len() > MAX_RUST_DEPTH {
            return Err(ProjectCookError::Invalid(
                "Rust input inventory exceeded its finite depth limit".into(),
            ));
        }
        let remaining = MAX_RUST_ENTRIES.saturating_sub(self.entries);
        let mut session = directory
            .begin_child_enumeration(DirectoryEnumerationLimits::new(remaining, 1024, 1))?;
        loop {
            let batch = session.next_batch(ENUMERATION_BATCH)?;
            if !batch.rejected_entry_facts().is_empty() {
                return Err(ProjectCookError::Invalid(
                    "Rust input inventory contains a linked, unavailable or unportable entry"
                        .into(),
                ));
            }
            self.entries = add(self.entries, batch.entries().len())?;
            if self.entries > MAX_RUST_ENTRIES {
                return Err(ProjectCookError::Invalid(
                    "Rust input inventory exceeded its finite entry limit".into(),
                ));
            }
            for entry in batch.entries() {
                let name = entry.name().to_str().ok_or_else(|| {
                    ProjectCookError::Invalid("Rust input names require portable UTF-8".into())
                })?;
                self.charge(add(
                    multiply(name.len(), 2)?,
                    size_of::<RelativeComponent>(),
                )?)?;
                if entry.kind() == FileKind::Regular && !name.ends_with(".rs") {
                    continue;
                }
                let path_bytes = add(add(relative_name_bytes(prefix)?, 1)?, name.len())?;
                if path_bytes > MAX_RUST_PATH_BYTES || prefix.len() >= MAX_RUST_DEPTH {
                    return Err(ProjectCookError::Invalid(
                        "Rust input inventory exceeded its finite path or depth limit".into(),
                    ));
                }
                self.charge(add(
                    multiply(path_bytes, 2)?,
                    multiply(prefix.len() + 1, size_of::<RelativeComponent>())?,
                )?)?;
                let path = RelativePath::from_components(
                    prefix
                        .components()
                        .iter()
                        .cloned()
                        .chain(std::iter::once(entry.name().clone())),
                )
                .map_err(FsError::from)?;
                match entry.kind() {
                    FileKind::Regular => {
                        let file = directory.open_child_file(entry.name())?;
                        if entry.identity() != Some(file.identity()) {
                            return Err(FsError::CapabilityIdentityDrift {
                                operation: FsOperation::OpenFile,
                            }
                            .into());
                        }
                        self.push(path)?;
                    }
                    FileKind::Directory => {
                        let child = directory.open_child_directory(entry.name())?;
                        if entry.identity() != Some(child.identity()) {
                            return Err(FsError::CapabilityIdentityDrift {
                                operation: FsOperation::OpenDirectory,
                            }
                            .into());
                        }
                        self.visit(&child, &path)?;
                    }
                    FileKind::Other => return Err(FsError::NotRegularFile.into()),
                }
            }
            if batch.is_complete() {
                break;
            }
        }
        Ok(())
    }

    fn charge(&mut self, bytes: usize) -> Result<(), ProjectCookError> {
        self.metadata_bytes = add(self.metadata_bytes, bytes)?;
        if self.metadata_bytes > MAX_RUST_METADATA_BYTES {
            return Err(ProjectCookError::Invalid(
                "Rust input inventory exceeded its finite name and metadata allowance".into(),
            ));
        }
        Ok(())
    }
}

struct ProjectCodeSelection {
    project: PathBuf,
    files: Vec<SelectedFile>,
    rust_inputs: Vec<RelativePath>,
    _lease: ProjectContentLease,
}

impl ProjectCodeSelection {
    fn capture(
        source: &ProjectCookSourceSelection,
        request: &CargoBuildRequest,
    ) -> Result<Self, ProjectCookError> {
        source.verify_project_root(request.workspace())?;
        let declared = request.declared_input_paths();
        let mut retained = add(
            size_of::<Self>(),
            multiply(request.workspace().as_os_str().len(), 2)?,
        )?;
        for path in declared {
            retained = add(
                retained,
                add(
                    multiply(relative_metadata(path)?, 3)?,
                    size_of::<SelectedFile>() + size_of::<&RelativePath>(),
                )?,
            )?;
        }
        let lease = source
            .receipt
            .reserve_consumer_retained(retained)
            .map_err(ProjectCookError::Source)?;
        let mut ordered: Vec<_> = declared.iter().collect();
        ordered.sort_unstable_by(|left, right| compare_paths(left, right));
        let rust_inputs = rust_inventory(&source.root, Some(&ordered))?;
        let mut files = Vec::with_capacity(declared.len());
        let mut total = 0_u64;
        let limit = request.limits().input_bytes.get() as u64;
        for path in declared {
            let file = source.root.open_file_exact(path)?;
            let digest = file.digest(limit.saturating_sub(total))?;
            total = total
                .checked_add(digest.length())
                .ok_or(CargoBuildFailure::Limit("declared input bytes"))?;
            files.push(SelectedFile {
                path: path.clone(),
                present: Some((file.identity(), digest)),
            });
        }
        Ok(Self {
            project: request.workspace().to_owned(),
            files,
            rust_inputs,
            _lease: lease,
        })
    }

    fn verify(&self, source: &ProjectCookSourceSelection) -> Result<(), ProjectCookError> {
        source.verify(&self.project)?;
        let expected: Vec<_> = self.rust_inputs.iter().collect();
        let current = rust_inventory(&source.root, Some(&expected))?;
        if current != self.rust_inputs {
            return Err(ProjectCookError::Invalid(
                "the project's Rust input set changed".into(),
            ));
        }
        for file in &self.files {
            file.verify(&source.root)?;
        }
        source.verify_project_root(&self.project)
    }
}

impl PreparedProjectCook {
    /// Builds one explicit Player with the original generated-input and Cargo owners.
    ///
    /// The declared inventory must cover the finite default Rust inputs and may explicitly name
    /// additional compiler inputs. Source and code facts remain retained through publication.
    pub fn build_player(
        self,
        request: CargoBuildRequest,
        limits: PlayerPackageLimits,
    ) -> Result<PreparedProjectPlayerPackage, ProjectCookError> {
        let Self { content, selection } = self;
        selection.verify(request.workspace())?;
        let code = ProjectCodeSelection::capture(&selection, &request)?;
        let (pack, expected) = content.into_pack();
        let bootstrap = GeneratedPlayerBootstrap::new(&pack, &expected, BOOTSTRAP_INPUT, limits)?;
        let request = request.with_generated_rust_input(bootstrap.name(), bootstrap.bytes())?;
        let package = build_player_package(pack, &expected, bootstrap, &request, limits, |_| {
            code.verify(&selection)
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

fn relative_metadata(path: &RelativePath) -> Result<usize, ProjectCookError> {
    add(
        size_of::<RelativePath>(),
        add(
            multiply(path.len(), size_of::<RelativeComponent>())?,
            multiply(relative_name_bytes(path)?, 2)?,
        )?,
    )
}

fn relative_name_bytes(path: &RelativePath) -> Result<usize, ProjectCookError> {
    path.components()
        .iter()
        .try_fold(path.len().saturating_sub(1), |bytes, component| {
            let name = component.to_str().ok_or_else(|| {
                ProjectCookError::Invalid("project inputs require portable UTF-8 paths".into())
            })?;
            add(bytes, name.len())
        })
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
