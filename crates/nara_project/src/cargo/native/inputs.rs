use std::{
    cmp::Ordering,
    io,
    mem::size_of,
    path::{Path, PathBuf},
    sync::Arc,
};

use nara_core::vfs::capabilities::{
    DirectoryCapability, DirectoryEnumerationLimits, FileKind, FsError, FsOperation,
    RelativeComponent, RelativePath,
};

use super::{
    CargoBuildFailure, CargoBuildRequest, ObservedInput, capture_inputs, charge_input_bytes,
    check_input_work, revalidate_inputs, verify_input_root,
};
use crate::host_fs;

const MAX_RUST_FILES: usize = 4096;
const MAX_RUST_DEPTH: usize = 32;
const MAX_RUST_ENTRIES: usize = 16_384;
const MAX_RUST_PATH_BYTES: usize = 4096;
const MAX_RUST_METADATA_BYTES: usize = 64 * 1024 * 1024;
const ENUMERATION_BATCH: usize = 32;

/// Enumerates the finite default Rust inputs without resolving Cargo dependencies.
///
/// Includes root `Cargo.toml` and `Cargo.lock`, optional root `build.rs`, and regular Rust files
/// below an optional `src/`. Declare any additional compiler inputs explicitly in the request.
/// Traversal rejects linked or unportable children and bounds files, entries, depth and paths.
pub fn project_rust_input_inventory(
    project: &Path,
) -> Result<Vec<RelativePath>, CargoBuildFailure> {
    let project = std::fs::canonicalize(project).map_err(CargoBuildFailure::Io)?;
    let root = host_fs::readable_directory(&project)?.strict_read_only()?;
    rust_inventory(&root, None)
}

/// The default Rust file set and the original objects and bytes of the request's declared files.
///
/// The request and its consumer share this snapshot. Keeping the opened files live preserves
/// object identity across Cargo and a later publication check.
#[derive(Clone)]
pub struct CargoInputsSnapshot(Arc<CargoInputsSnapshotInner>);

struct CargoInputsSnapshotInner {
    workspace: PathBuf,
    root: DirectoryCapability,
    files: Vec<ObservedInput>,
    rust_paths: Vec<RelativePath>,
}

impl CargoBuildRequest {
    /// Bounds snapshot metadata for a consumer that holds its own retention lease.
    pub fn project_rust_input_retained_bytes(&self) -> Result<usize, CargoBuildFailure> {
        let fixed = size_of::<CargoInputsSnapshot>()
            + size_of::<CargoInputsSnapshotInner>()
            + 2 * size_of::<usize>();
        let workspace = multiply(self.workspace.as_os_str().as_encoded_bytes().len(), 2)?;
        self.inputs
            .iter()
            .try_fold(add(fixed, workspace)?, |bytes, path| {
                add(
                    bytes,
                    add(
                        multiply(relative_metadata(path)?, 3)?,
                        size_of::<ObservedInput>() + size_of::<&RelativePath>(),
                    )?,
                )
            })
    }

    /// Binds the build to the original project and all its declared filesystem inputs.
    ///
    /// The declared inputs must cover the default Rust inventory and may include other compiler
    /// inputs. Repeated calls for the same root return the original snapshot. Generated input
    /// remains a separate request-owned fact.
    pub fn capture_project_rust_inputs(
        &mut self,
        root: &DirectoryCapability,
    ) -> Result<CargoInputsSnapshot, CargoBuildFailure> {
        verify_input_root(root, &self.workspace)?;
        if let Some(snapshot) = &self.project_inputs {
            if !root.same_object(&snapshot.0.root)? {
                return Err(CargoBuildFailure::Stale);
            }
            return Ok(snapshot.clone());
        }
        let root = root.strict_read_only()?;
        let mut declared: Vec<_> = self.inputs.iter().collect();
        declared.sort_unstable_by(|left, right| compare_paths(left, right));
        let rust_paths = rust_inventory(&root, Some(&declared))?;
        let files = capture_inputs(&root, self)?;
        verify_input_root(&root, &self.workspace)?;
        let snapshot = CargoInputsSnapshot(Arc::new(CargoInputsSnapshotInner {
            workspace: self.workspace.clone(),
            root,
            files,
            rust_paths,
        }));
        self.project_inputs = Some(snapshot.clone());
        Ok(snapshot)
    }
}

impl CargoInputsSnapshot {
    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.0.workspace
    }

    /// Rechecks the workspace and default Rust file set without hashing the declared files.
    pub fn verify_file_set(&self) -> Result<(), CargoBuildFailure> {
        verify_input_root(&self.0.root, self.workspace())?;
        let expected: Vec<_> = self.0.rust_paths.iter().collect();
        let current = rust_inventory(&self.0.root, Some(&expected))?;
        if current != self.0.rust_paths {
            return Err(CargoBuildFailure::Stale);
        }
        Ok(())
    }

    /// Rechecks the default set and the identity and contents of every declared file.
    pub fn verify(&self) -> Result<(), CargoBuildFailure> {
        self.verify_file_set()?;
        revalidate_inputs(&self.0.root, &self.0.files)?;
        verify_input_root(&self.0.root, self.workspace())
    }

    pub(super) fn build_inputs(
        &self,
        request: &CargoBuildRequest,
    ) -> Result<(&DirectoryCapability, &[ObservedInput]), CargoBuildFailure> {
        check_input_work(request)?;
        let generated_bytes = request
            .generated_rust_input()
            .map_or(0, |input| input.digest.length());
        let limit = request.limits.input_bytes.get() as u64;
        self.0
            .files
            .iter()
            .try_fold(generated_bytes, |bytes, input| {
                charge_input_bytes(bytes, input.fact.digest, limit)
            })?;
        Ok((&self.0.root, &self.0.files))
    }
}

fn rust_inventory(
    root: &DirectoryCapability,
    declared: Option<&[&RelativePath]>,
) -> Result<Vec<RelativePath>, CargoBuildFailure> {
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
    fn push(&mut self, path: RelativePath) -> Result<(), CargoBuildFailure> {
        if self.declared.is_some_and(|declared| {
            declared
                .binary_search_by(|selected| compare_paths(selected, &path))
                .is_err()
        }) {
            return Err(CargoBuildFailure::MissingDeclaredInput);
        }
        if self.files.len() >= self.file_limit {
            return Err(CargoBuildFailure::Limit("Rust input file count"));
        }
        self.charge(relative_metadata(&path)?)?;
        self.files.push(path);
        Ok(())
    }

    fn visit(
        &mut self,
        directory: &DirectoryCapability,
        prefix: &RelativePath,
    ) -> Result<(), CargoBuildFailure> {
        if prefix.len() > MAX_RUST_DEPTH {
            return Err(CargoBuildFailure::Limit("Rust input depth"));
        }
        let remaining = MAX_RUST_ENTRIES.saturating_sub(self.entries);
        let mut session = directory
            .begin_child_enumeration(DirectoryEnumerationLimits::new(remaining, 1024, 1))?;
        loop {
            let batch = session.next_batch(ENUMERATION_BATCH)?;
            if !batch.rejected_entry_facts().is_empty() {
                return Err(CargoBuildFailure::InvalidRustInventory);
            }
            self.entries = add(self.entries, batch.entries().len())?;
            if self.entries > MAX_RUST_ENTRIES {
                return Err(CargoBuildFailure::Limit("Rust input entry count"));
            }
            for entry in batch.entries() {
                let name = entry
                    .name()
                    .to_str()
                    .ok_or(CargoBuildFailure::InvalidRustInventory)?;
                self.charge(add(
                    multiply(name.len(), 2)?,
                    size_of::<RelativeComponent>(),
                )?)?;
                if entry.kind() == FileKind::Regular && !name.ends_with(".rs") {
                    continue;
                }
                let path_bytes = add(add(relative_name_bytes(prefix)?, 1)?, name.len())?;
                if path_bytes > MAX_RUST_PATH_BYTES || prefix.len() >= MAX_RUST_DEPTH {
                    return Err(CargoBuildFailure::Limit("Rust input path or depth"));
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

    fn charge(&mut self, bytes: usize) -> Result<(), CargoBuildFailure> {
        self.metadata_bytes = add(self.metadata_bytes, bytes)?;
        if self.metadata_bytes > MAX_RUST_METADATA_BYTES {
            return Err(CargoBuildFailure::Limit("Rust input inventory metadata"));
        }
        Ok(())
    }
}

fn relative_metadata(path: &RelativePath) -> Result<usize, CargoBuildFailure> {
    add(
        size_of::<RelativePath>(),
        add(
            multiply(path.len(), size_of::<RelativeComponent>())?,
            multiply(relative_name_bytes(path)?, 2)?,
        )?,
    )
}

fn relative_name_bytes(path: &RelativePath) -> Result<usize, CargoBuildFailure> {
    path.components()
        .iter()
        .try_fold(path.len().saturating_sub(1), |bytes, component| {
            let name = component
                .to_str()
                .ok_or(CargoBuildFailure::InvalidRustInventory)?;
            add(bytes, name.len())
        })
}

fn compare_paths(left: &RelativePath, right: &RelativePath) -> Ordering {
    left.components()
        .iter()
        .map(RelativeComponent::to_str)
        .cmp(right.components().iter().map(RelativeComponent::to_str))
}

fn relative(name: &str) -> Result<RelativePath, CargoBuildFailure> {
    Ok(RelativePath::new(name).map_err(FsError::from)?)
}

fn missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
}

fn add(left: usize, right: usize) -> Result<usize, CargoBuildFailure> {
    left.checked_add(right)
        .ok_or(CargoBuildFailure::Limit("Rust input inventory metadata"))
}

fn multiply(left: usize, right: usize) -> Result<usize, CargoBuildFailure> {
    left.checked_mul(right)
        .ok_or(CargoBuildFailure::Limit("Rust input inventory metadata"))
}
