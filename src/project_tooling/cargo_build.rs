//! Explicit trusted Cargo builds returning exact target bytes, independently of execution.

use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Arc,
};

use nara_core::{ByteLimit, ItemLimit};
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FsError, LockMode, LockedFileCapability,
    RelativeComponent, RelativePath,
};
use serde::Serialize;
use thiserror::Error;

use crate::project_workflow::cargo::{
    CargoBuildDecoder, CargoBuildError, CargoBuildSelection, CargoDiagnostics,
    CargoInvocationError, CargoMetadataFacts, CargoMetadataFactsError,
};

use super::host_fs::{self, HostDirectoryError, HostFileError};

mod tools;
pub use tools::CargoBuildTools;

/// Finite logical bounds for one build and its escaped captured result.
#[derive(Debug, Clone, Copy)]
pub struct CargoBuildLimits {
    pub input_count: ItemLimit,
    pub input_bytes: ByteLimit,
    pub executable_bytes: ByteLimit,
    /// Maximum bytes hashed for each selected Cargo/rustc file, separate from target output.
    pub tool_bytes: ByteLimit,
    pub metadata_bytes: ByteLimit,
    pub cargo_output_bytes: ByteLimit,
    pub tool_identity_bytes: ByteLimit,
    pub work_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
}

impl Default for CargoBuildLimits {
    fn default() -> Self {
        Self {
            input_count: ItemLimit::new(1024).unwrap(),
            input_bytes: ByteLimit::new(64 * 1024 * 1024).unwrap(),
            executable_bytes: ByteLimit::new(512 * 1024 * 1024).unwrap(),
            tool_bytes: ByteLimit::new(512 * 1024 * 1024).unwrap(),
            metadata_bytes: ByteLimit::new(16 * 1024 * 1024).unwrap(),
            cargo_output_bytes: ByteLimit::new(64 * 1024 * 1024).unwrap(),
            tool_identity_bytes: ByteLimit::new(16 * 1024).unwrap(),
            work_bytes: ByteLimit::new(1024 * 1024 * 1024).unwrap(),
            retained_bytes: ByteLimit::new(1024 * 1024 * 1024).unwrap(),
        }
    }
}

/// A finite declared source inventory and explicit target selection, never project-file policy.
pub struct CargoBuildRequest {
    workspace: PathBuf,
    package_manifest: RelativePath,
    selection: CargoBuildSelection,
    tools: CargoBuildTools,
    inputs: Vec<RelativePath>,
    limits: CargoBuildLimits,
    target_directory: Option<PathBuf>,
}

impl CargoBuildRequest {
    pub fn new(
        workspace: impl Into<PathBuf>,
        package_manifest: RelativePath,
        selection: CargoBuildSelection,
        tools: CargoBuildTools,
        inputs: impl IntoIterator<Item = RelativePath>,
        limits: CargoBuildLimits,
    ) -> Result<Self, CargoBuildFailure> {
        let workspace = std::fs::canonicalize(workspace.into()).map_err(CargoBuildFailure::Io)?;
        let mut admitted = Vec::new();
        for input in inputs {
            if admitted.len() >= limits.input_count.get() {
                return Err(CargoBuildFailure::Limit("declared input count"));
            }
            admitted.push(input);
        }
        for required in [
            RelativePath::new("Cargo.toml").expect("fixed manifest"),
            RelativePath::new("Cargo.lock").expect("fixed lockfile"),
            package_manifest.clone(),
        ] {
            if !admitted.iter().any(|path| path == &required) {
                return Err(CargoBuildFailure::MissingDeclaredInput);
            }
        }
        let mut admitted = admitted
            .into_iter()
            .map(|path| Ok((input_spelling(&path)?, path)))
            .collect::<Result<Vec<_>, CargoBuildFailure>>()?;
        admitted.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        if admitted.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(CargoBuildFailure::DuplicateDeclaredInput);
        }
        let admitted = admitted.into_iter().map(|(_, path)| path).collect();
        Ok(Self {
            workspace,
            package_manifest,
            selection,
            tools,
            inputs: admitted,
            limits,
            target_directory: None,
        })
    }

    #[must_use]
    pub fn selection(&self) -> &CargoBuildSelection {
        &self.selection
    }

    pub fn with_target_directory(
        mut self,
        directory: impl Into<PathBuf>,
    ) -> Result<Self, CargoBuildFailure> {
        let directory = directory.into();
        if !directory.is_absolute() {
            return Err(CargoBuildFailure::InvalidTargetDirectory);
        }
        self.target_directory = Some(directory);
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoBuildPoint {
    BeforeMetadata,
    BeforeBuild,
    AfterBuild,
    BeforeCapture,
    BeforeReturn,
}

/// Actual tool identity, separate from MSRV, a channel alias or target-runner qualification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CargoToolchainObservation {
    #[serde(serialize_with = "serialize_digest")]
    cargo: ContentDigest,
    #[serde(serialize_with = "serialize_digest")]
    rustc: ContentDigest,
    #[serde(serialize_with = "serialize_digest")]
    cargo_version: ContentDigest,
    #[serde(serialize_with = "serialize_digest")]
    rustc_version: ContentDigest,
    host_target: String,
}

impl CargoToolchainObservation {
    #[must_use]
    pub const fn cargo_digest(&self) -> ContentDigest {
        self.cargo
    }
    #[must_use]
    pub const fn rustc_digest(&self) -> ContentDigest {
        self.rustc
    }
    #[must_use]
    pub fn host_target(&self) -> &str {
        &self.host_target
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CargoDeclaredInput {
    path: String,
    #[serde(serialize_with = "serialize_digest")]
    digest: ContentDigest,
}

impl CargoDeclaredInput {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Path-free observed build facts under a declared-input revalidation envelope.
///
/// This does not prove a simultaneous filesystem snapshot or intercept trusted Cargo build-script,
/// proc-macro, config, tool and environment inputs outside the declared inventory.
#[derive(Debug)]
pub struct CargoBuildProvenance {
    selection: CargoBuildSelection,
    resolved_features: Box<[String]>,
    toolchain: CargoToolchainObservation,
    inputs: Vec<CargoDeclaredInput>,
    metadata: ContentDigest,
    digest: ContentDigest,
}

impl CargoBuildProvenance {
    #[must_use]
    pub fn selection(&self) -> &CargoBuildSelection {
        &self.selection
    }
    #[must_use]
    pub fn resolved_features(&self) -> &[String] {
        &self.resolved_features
    }
    #[must_use]
    pub const fn toolchain(&self) -> &CargoToolchainObservation {
        &self.toolchain
    }
    #[must_use]
    pub fn declared_inputs(&self) -> &[CargoDeclaredInput] {
        &self.inputs
    }
    #[must_use]
    pub const fn metadata_digest(&self) -> ContentDigest {
        self.metadata
    }
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Exact captured target bytes. Construction neither executes them nor establishes Runtime Ready.
#[derive(Clone)]
pub struct CapturedCargoExecutable(Arc<CapturedExecutableInner>);

struct CapturedExecutableInner {
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    provenance: CargoBuildProvenance,
    diagnostics: CargoDiagnostics,
    original: FileCapability,
    original_path: PathBuf,
    retained_bytes: usize,
}

impl std::fmt::Debug for CapturedCargoExecutable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CapturedCargoExecutable")
            .field("digest", &self.0.digest)
            .field("byte_count", &self.0.bytes.len())
            .field("provenance_digest", &self.0.provenance.digest())
            .field("retained_bytes", &self.0.retained_bytes)
            .finish()
    }
}

impl CapturedCargoExecutable {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
    #[must_use]
    pub fn digest(&self) -> ContentDigest {
        self.0.digest
    }
    #[must_use]
    pub fn provenance(&self) -> &CargoBuildProvenance {
        &self.0.provenance
    }
    #[must_use]
    pub fn diagnostics(&self) -> &CargoDiagnostics {
        &self.0.diagnostics
    }
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes
    }

    pub(crate) fn original_path(&self) -> &Path {
        &self.0.original_path
    }

    pub(crate) fn shared_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.0.bytes)
    }

    pub(crate) fn verify_original(&self) -> Result<(), CargoBuildFailure> {
        let current = host_fs::readable_file(&self.0.original_path)?;
        if !current.same_object(&self.0.original)? {
            return Err(CargoBuildFailure::Stale);
        }
        current.verify_digest(self.0.digest, self.0.digest.length())?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CargoBuildFailure {
    #[error("a selected native build operation failed")]
    Io(#[source] io::Error),
    #[error(transparent)]
    Directory(#[from] HostDirectoryError),
    #[error(transparent)]
    File(#[from] HostFileError),
    #[error(transparent)]
    Filesystem(#[from] FsError),
    #[error("the selected build tool is not one absolute regular executable")]
    InvalidTool,
    #[error("the explicit Cargo target directory must be an absolute path")]
    InvalidTargetDirectory,
    #[error("the build inventory omits a required manifest, lockfile or selected source")]
    MissingDeclaredInput,
    #[error("the build inventory repeats a declared input")]
    DuplicateDeclaredInput,
    #[error("the build exceeded its {0} limit")]
    Limit(&'static str),
    #[error("the actual Cargo or rustc tool identity was invalid")]
    InvalidToolIdentity,
    #[error("the selected build process failed")]
    ProcessFailed,
    #[error("another cooperating build owns this Cargo output directory")]
    TargetBusy,
    #[error("the build child has not yet returned; cleanup ownership is retained")]
    CleanupIncomplete(#[source] Box<CargoBuildProcessCustody>),
    #[error("the declared build inputs or selected authority changed")]
    Stale,
    #[error(transparent)]
    Metadata(#[from] CargoMetadataFactsError),
    #[error(transparent)]
    Invocation(#[from] CargoInvocationError),
    #[error(transparent)]
    Messages(#[from] CargoBuildError),
    #[error("the captured executable was empty")]
    EmptyExecutable,
    #[error("the selected build was revoked by its Host")]
    Aborted,
    #[error("the build provenance could not be encoded")]
    Provenance,
}

struct ObservedInput {
    file: FileCapability,
    fact: CargoDeclaredInput,
}

#[derive(Debug)]
struct TargetBuildLease {
    path: PathBuf,
    root: DirectoryCapability,
    lock: LockedFileCapability,
}

impl TargetBuildLease {
    fn acquire(request: &CargoBuildRequest) -> Result<Self, CargoBuildFailure> {
        let path = request
            .target_directory
            .clone()
            .or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from))
            .unwrap_or_else(|| request.workspace.join("target"));
        let path = if path.is_absolute() {
            path
        } else {
            request.workspace.join(path)
        };
        Self::acquire_at(path)
    }

    fn acquire_at(path: PathBuf) -> Result<Self, CargoBuildFailure> {
        std::fs::create_dir_all(&path).map_err(CargoBuildFailure::Io)?;
        let path = std::fs::canonicalize(path).map_err(CargoBuildFailure::Io)?;
        let root = host_fs::writable_directory(&path)?;
        let name = RelativeComponent::new("nara-cargo-build.lock").expect("fixed lock name");
        let file = match root.open_child_file(&name) {
            Ok(file) => file,
            Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                match root.create_child_file(&name) {
                    Ok(file) => file,
                    Err(FsError::AlreadyExists { .. }) => root.open_child_file(&name)?,
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        };
        let lock = file.try_lock(LockMode::Exclusive).map_err(|error| {
            if matches!(error.error(), FsError::LockContended) {
                CargoBuildFailure::TargetBusy
            } else {
                let (source, _) = error.into_parts();
                CargoBuildFailure::Filesystem(source)
            }
        })?;
        let lease = Self { path, root, lock };
        lease.verify()?;
        Ok(lease)
    }

    fn verify(&self) -> Result<(), CargoBuildFailure> {
        let current_root = host_fs::readable_directory(&self.path)?;
        if !current_root.same_object(&self.root)? {
            return Err(CargoBuildFailure::Stale);
        }
        let current = self.root.open_child_file(
            &RelativeComponent::new("nara-cargo-build.lock").expect("fixed lock name"),
        )?;
        if current.identity() != self.lock.identity() {
            return Err(CargoBuildFailure::Stale);
        }
        Ok(())
    }
}

/// An unreaped direct build child and any output-directory lease it still requires.
#[derive(Debug)]
pub struct CargoBuildProcessCustody {
    child: Option<Child>,
    error: io::Error,
    leases: Vec<TargetBuildLease>,
}

impl std::fmt::Display for CargoBuildProcessCustody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("build child cleanup is incomplete")
    }
}

impl std::error::Error for CargoBuildProcessCustody {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl CargoBuildProcessCustody {
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Retries direct-child termination/reaping without surrendering the output lease on error.
    pub fn retry_cleanup(&mut self) -> Result<(), io::Error> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        let _ = child.kill();
        wait_retry(child)?;
        self.child = None;
        self.leases.clear();
        Ok(())
    }
}

/// Builds only the explicitly selected target and captures its exact bytes under declared bounds.
///
/// This synchronous native adapter is intended for finite CLI/build-tool work. The Editor keeps
/// its supervised process/cancellation owner and consumes the same pure selection/decoder facts.
pub fn build_executable(
    request: &CargoBuildRequest,
    mut control: impl FnMut(CargoBuildPoint) -> Result<(), CargoBuildFailure>,
) -> Result<CapturedCargoExecutable, CargoBuildFailure> {
    let mut lease = Some(TargetBuildLease::acquire(request)?);
    let result = build_executable_inner(
        request,
        &mut control,
        lease.as_ref().expect("live build lease"),
    );
    match result {
        Err(CargoBuildFailure::CleanupIncomplete(mut custody)) => {
            custody.leases.extend(lease.take());
            Err(CargoBuildFailure::CleanupIncomplete(custody))
        }
        result => result,
    }
}

fn build_executable_inner(
    request: &CargoBuildRequest,
    control: &mut impl FnMut(CargoBuildPoint) -> Result<(), CargoBuildFailure>,
    lease: &TargetBuildLease,
) -> Result<CapturedCargoExecutable, CargoBuildFailure> {
    let root = host_fs::readable_directory(&request.workspace)?;
    let inputs = capture_inputs(&root, request)?;
    let input_work = input_allocation(&inputs)?;
    let tool_bound = request.limits.tool_bytes.get() as u64;
    check(
        input_work.checked_add(request.limits.tool_identity_bytes.get().saturating_mul(4)),
        request.limits.work_bytes,
        "tool identity work",
    )?;
    let tools = tools::observe_tools(
        &request.tools,
        &request.workspace,
        request.limits.tool_identity_bytes.get(),
        tool_bound,
    )?;
    let toolchain = tools.facts().clone();
    control(CargoBuildPoint::BeforeMetadata)?;
    revalidate_inputs(&root, request, &inputs)?;
    tools.verify(&request.tools)?;
    let manifest = request
        .workspace
        .join(input_spelling(&request.package_manifest)?);
    let mut metadata_command = command(request, request.tools.cargo_path());
    metadata_command.env("CARGO_TARGET_DIR", &lease.path);
    metadata_command
        .args(["metadata", "--locked", "--format-version", "1"])
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--filter-platform")
        .arg(request.selection.target_triple());
    feature_arguments(&mut metadata_command, &request.selection);
    check(
        request
            .limits
            .metadata_bytes
            .get()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(input_work)),
        request.limits.work_bytes,
        "metadata input work",
    )?;
    let metadata = bounded_output(&mut metadata_command, request.limits.metadata_bytes.get())?;
    tools.verify(&request.tools)?;
    // JSON Value, bounded metadata indices and their selected owned facts can overlap. Charge
    // a conservative per-encoded-byte bound before semantic construction, not allocator RSS.
    check(
        metadata
            .len()
            .checked_mul(64)
            .and_then(|bytes| bytes.checked_add(input_work)),
        request.limits.work_bytes,
        "metadata interpretation work",
    )?;
    let metadata_digest = ContentDigest::of_bytes(&metadata);
    let facts = CargoMetadataFacts::parse(&metadata, &manifest, &request.selection)?;
    let invocation = facts.build_invocation(request.selection.clone())?;
    if !same_path(invocation.workspace_root(), &request.workspace) {
        return Err(CargoBuildFailure::Stale);
    }
    if !inputs.iter().any(|input| {
        same_path(
            &request.workspace.join(&input.fact.path),
            invocation.source_path(),
        )
    }) {
        return Err(CargoBuildFailure::MissingDeclaredInput);
    }
    let build_work = input_work
        .checked_add(invocation_allocation(&invocation)?)
        .and_then(|bytes| {
            bytes.checked_add(
                std::mem::size_of::<CargoToolchainObservation>()
                    + toolchain.host_target.len()
                    + 64 * 1024,
            )
        })
        .ok_or(CargoBuildFailure::Limit("build work"))?;
    drop(facts);
    drop(metadata);
    control(CargoBuildPoint::BeforeBuild)?;
    revalidate_inputs(&root, request, &inputs)?;
    lease.verify()?;
    tools.verify(&request.tools)?;
    let mut build = command(request, request.tools.cargo_path());
    build
        .args(["build", "--locked", "--message-format=json"])
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--package")
        .arg(request.selection.package())
        .arg("--bin")
        .arg(request.selection.target())
        .arg("--profile")
        .arg(request.selection.profile())
        .arg("--target")
        .arg(request.selection.target_triple())
        .env("CARGO_TARGET_DIR", invocation.target_directory());
    feature_arguments(&mut build, &request.selection);
    check(
        request
            .limits
            .cargo_output_bytes
            .get()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(build_work)),
        request.limits.work_bytes,
        "Cargo output work",
    )?;
    let output = bounded_output(&mut build, request.limits.cargo_output_bytes.get())?;
    check(
        output
            .len()
            .min(4 * 1024 * 1024)
            .checked_mul(64)
            .and_then(|bytes| {
                output
                    .capacity()
                    .checked_mul(2)
                    .and_then(|output| bytes.checked_add(output))
            })
            .and_then(|bytes| bytes.checked_add(build_work)),
        request.limits.work_bytes,
        "Cargo interpretation work",
    )?;
    let mut decoder = CargoBuildDecoder::new(invocation);
    decoder.push(&output)?;
    let observed = decoder.finish()?;
    drop(decoder);
    drop(output);
    tools.verify(&request.tools)?;
    let original_path = observed.artifact().executable().to_owned();
    let original = host_fs::readable_file(&original_path)?;
    let digest = original.digest(request.limits.executable_bytes.get() as u64)?;
    if digest.length() == 0 {
        return Err(CargoBuildFailure::EmptyExecutable);
    }
    control(CargoBuildPoint::AfterBuild)?;
    revalidate_inputs(&root, request, &inputs)?;
    lease.verify()?;
    tools.verify(&request.tools)?;
    let length = usize::try_from(digest.length())
        .map_err(|_| CargoBuildFailure::Limit("executable bytes"))?;
    let provenance_cost = input_work
        .checked_add(selection_allocation(&request.selection)?)
        .and_then(|bytes| {
            feature_allocation(observed.artifact().features())
                .ok()
                .and_then(|features| bytes.checked_add(features))
        })
        .and_then(|bytes| {
            diagnostics_allocation(observed.diagnostics())
                .ok()
                .and_then(|diagnostics| bytes.checked_add(diagnostics))
        })
        .and_then(|bytes| {
            bytes.checked_add(
                std::mem::size_of::<CapturedExecutableInner>()
                    + 2 * std::mem::size_of::<usize>()
                    + toolchain.host_target.len(),
            )
        })
        .and_then(|bytes| {
            original_path
                .as_os_str()
                .as_encoded_bytes()
                .len()
                .checked_mul(4)
                .and_then(|path| bytes.checked_add(path))
        })
        .ok_or(CargoBuildFailure::Limit("retained provenance"))?;
    check(
        length
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(build_work))
            .and_then(|bytes| {
                provenance_cost
                    .checked_mul(2)
                    .and_then(|metadata| bytes.checked_add(metadata))
            }),
        request.limits.work_bytes,
        "executable capture work",
    )?;
    let retained_bytes = length
        .checked_add(provenance_cost)
        .ok_or(CargoBuildFailure::Limit("retained executable"))?;
    check(
        Some(retained_bytes),
        request.limits.retained_bytes,
        "retained executable",
    )?;
    control(CargoBuildPoint::BeforeCapture)?;
    tools.verify(&request.tools)?;
    let bytes = original.read_to_end_bounded(digest.length())?;
    if ContentDigest::of_bytes(&bytes) != digest {
        return Err(CargoBuildFailure::Stale);
    }
    let declared = inputs.iter().map(|input| input.fact.clone()).collect();
    let mut provenance = CargoBuildProvenance {
        selection: request.selection.clone(),
        resolved_features: observed.artifact().features().to_vec().into_boxed_slice(),
        toolchain,
        inputs: declared,
        metadata: metadata_digest,
        digest: ContentDigest::of_bytes(&[]),
    };
    provenance.digest = provenance_digest(&provenance, digest)?;
    control(CargoBuildPoint::BeforeReturn)?;
    revalidate_inputs(&root, request, &inputs)?;
    let current = host_fs::readable_file(&original_path)?;
    if !current.same_object(&original)? {
        return Err(CargoBuildFailure::Stale);
    }
    current.verify_digest(digest, digest.length())?;
    lease.verify()?;
    tools.verify(&request.tools)?;
    Ok(CapturedCargoExecutable(Arc::new(CapturedExecutableInner {
        bytes: bytes.into(),
        digest,
        provenance,
        diagnostics: observed.diagnostics().clone(),
        original,
        original_path,
        retained_bytes,
    })))
}

fn command(request: &CargoBuildRequest, tool: &Path) -> Command {
    let mut command = Command::new(tool);
    command
        .current_dir(&request.workspace)
        .env("RUSTC", request.tools.rustc_path())
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null());
    command
}

fn feature_arguments(command: &mut Command, selection: &CargoBuildSelection) {
    if !selection.default_features() {
        command.arg("--no-default-features");
    }
    if !selection.features().is_empty() {
        command
            .arg("--features")
            .arg(selection.features().join(","));
    }
}

fn capture_inputs(
    root: &DirectoryCapability,
    request: &CargoBuildRequest,
) -> Result<Vec<ObservedInput>, CargoBuildFailure> {
    let work = request.inputs.iter().try_fold(0_usize, |bytes, path| {
        path.components()
            .iter()
            .try_fold(0_usize, |bytes, component| {
                component
                    .to_str()
                    .and_then(|component| bytes.checked_add(component.len() + 1))
            })
            .and_then(|bytes| bytes.checked_mul(4))
            .and_then(|path| bytes.checked_add(path + std::mem::size_of::<ObservedInput>()))
    });
    check(work, request.limits.work_bytes, "declared input work")?;
    let mut observed = Vec::with_capacity(request.inputs.len());
    let mut bytes = 0_u64;
    for path in &request.inputs {
        let file = root.open_file_exact(path)?;
        let digest = file.digest(request.limits.input_bytes.get() as u64)?;
        bytes = bytes
            .checked_add(digest.length())
            .ok_or(CargoBuildFailure::Limit("declared input bytes"))?;
        if bytes > request.limits.input_bytes.get() as u64 {
            return Err(CargoBuildFailure::Limit("declared input bytes"));
        }
        observed.push(ObservedInput {
            file,
            fact: CargoDeclaredInput {
                path: input_spelling(path)?,
                digest,
            },
        });
    }
    Ok(observed)
}

fn revalidate_inputs(
    root: &DirectoryCapability,
    request: &CargoBuildRequest,
    inputs: &[ObservedInput],
) -> Result<(), CargoBuildFailure> {
    let current_root = host_fs::readable_directory(&request.workspace)?;
    if !current_root.same_object(root)? {
        return Err(CargoBuildFailure::Stale);
    }
    for input in inputs {
        let path = RelativePath::new(&input.fact.path).map_err(|_| CargoBuildFailure::Stale)?;
        let current = current_root.open_file_exact(&path)?;
        if !current.same_object(&input.file)? {
            return Err(CargoBuildFailure::Stale);
        }
        current
            .verify_digest(input.fact.digest, input.fact.digest.length())
            .map_err(|error| match error {
                FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. } => {
                    CargoBuildFailure::Stale
                }
                error => CargoBuildFailure::Filesystem(error),
            })?;
    }
    Ok(())
}

pub(crate) fn bounded_output(
    command: &mut Command,
    limit: usize,
) -> Result<Vec<u8>, CargoBuildFailure> {
    let (status, output) = bounded_status_output(command, limit)?;
    if !status.success() {
        return Err(CargoBuildFailure::ProcessFailed);
    }
    Ok(output)
}

pub(crate) fn bounded_status_output(
    command: &mut Command,
    limit: usize,
) -> Result<(ExitStatus, Vec<u8>), CargoBuildFailure> {
    bounded_status_output_with(command, limit, finish_child)
}

fn bounded_status_output_with(
    command: &mut Command,
    limit: usize,
    finish: impl FnOnce(Child) -> Result<ExitStatus, CargoBuildFailure>,
) -> Result<(ExitStatus, Vec<u8>), CargoBuildFailure> {
    let mut child = command
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(CargoBuildFailure::Io)?;
    let mut output = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped child stdout")
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut output);
    if read.is_err() || output.len() > limit {
        let _ = child.kill();
    }
    let status = finish(child)?;
    read.map_err(CargoBuildFailure::Io)?;
    if output.len() > limit {
        return Err(CargoBuildFailure::Limit("tool output"));
    }
    Ok((status, output))
}

fn finish_child(child: Child) -> Result<ExitStatus, CargoBuildFailure> {
    finish_child_with(child, wait_retry)
}

fn finish_child_with(
    mut child: Child,
    mut wait: impl FnMut(&mut Child) -> io::Result<ExitStatus>,
) -> Result<ExitStatus, CargoBuildFailure> {
    match wait(&mut child) {
        Ok(status) => Ok(status),
        Err(first) => {
            let _ = child.kill();
            match wait(&mut child) {
                Ok(_) => Err(CargoBuildFailure::Io(first)),
                Err(error) => Err(CargoBuildFailure::CleanupIncomplete(Box::new(
                    CargoBuildProcessCustody {
                        child: Some(child),
                        error,
                        leases: Vec::new(),
                    },
                ))),
            }
        }
    }
}

fn wait_retry(child: &mut Child) -> io::Result<ExitStatus> {
    loop {
        match child.wait() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

pub(crate) fn parse_host_target(bytes: &[u8]) -> Result<String, CargoBuildFailure> {
    let text = std::str::from_utf8(bytes).map_err(|_| CargoBuildFailure::InvalidToolIdentity)?;
    let mut hosts = text.lines().filter_map(|line| line.strip_prefix("host: "));
    let host = hosts.next().ok_or(CargoBuildFailure::InvalidToolIdentity)?;
    if hosts.next().is_some()
        || host.is_empty()
        || host.len() > 256
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CargoBuildFailure::InvalidToolIdentity);
    }
    Ok(host.to_owned())
}

fn provenance_digest(
    provenance: &CargoBuildProvenance,
    executable: ContentDigest,
) -> Result<ContentDigest, CargoBuildFailure> {
    #[derive(Serialize)]
    struct Wire<'a> {
        domain: &'static str,
        package: &'a str,
        binary: &'a str,
        profile: &'a str,
        target: &'a str,
        default_features: bool,
        requested_features: &'a [String],
        resolved_features: &'a [String],
        toolchain: &'a CargoToolchainObservation,
        inputs: &'a [CargoDeclaredInput],
        #[serde(serialize_with = "serialize_digest")]
        metadata: ContentDigest,
        #[serde(serialize_with = "serialize_digest")]
        executable: ContentDigest,
        consistency: &'static str,
    }
    let selection = &provenance.selection;
    let wire = Wire {
        domain: "nara.cargo-build-observation.v1",
        package: selection.package(),
        binary: selection.target(),
        profile: selection.profile(),
        target: selection.target_triple(),
        default_features: selection.default_features(),
        requested_features: selection.features(),
        resolved_features: &provenance.resolved_features,
        toolchain: &provenance.toolchain,
        inputs: &provenance.inputs,
        metadata: provenance.metadata,
        executable,
        consistency: "revalidated-declared-inputs; ambient-trusted-cargo-and-native-inputs",
    };
    struct DigestWriter {
        hasher: blake3::Hasher,
        length: u64,
    }
    impl Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.length = self
                .length
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| io::Error::other("provenance length overflow"))?;
            self.hasher.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter {
        hasher: blake3::Hasher::new(),
        length: 0,
    };
    serde_json::to_writer(&mut writer, &wire).map_err(|_| CargoBuildFailure::Provenance)?;
    Ok(ContentDigest::from_parts(
        writer.length,
        *writer.hasher.finalize().as_bytes(),
    ))
}

fn serialize_digest<S: serde::Serializer>(
    digest: &ContentDigest,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    (digest.length(), digest.as_bytes()).serialize(serializer)
}

fn check(
    bytes: Option<usize>,
    limit: ByteLimit,
    name: &'static str,
) -> Result<(), CargoBuildFailure> {
    if bytes.is_none_or(|bytes| bytes > limit.get()) {
        return Err(CargoBuildFailure::Limit(name));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .eq_ignore_ascii_case(
                &right
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .replace('/', "\\"),
            )
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn input_spelling(path: &RelativePath) -> Result<String, CargoBuildFailure> {
    let mut spelling = String::new();
    for component in path.components() {
        let component = component
            .to_str()
            .ok_or(CargoBuildFailure::MissingDeclaredInput)?;
        if !spelling.is_empty() {
            spelling.push('/');
        }
        spelling.push_str(component);
    }
    Ok(spelling)
}

fn feature_allocation(features: &[String]) -> Result<usize, CargoBuildFailure> {
    features
        .iter()
        .try_fold(std::mem::size_of_val(features), |bytes, feature| {
            bytes
                .checked_add(feature.len())
                .ok_or(CargoBuildFailure::Limit("feature metadata"))
        })
}

fn selection_allocation(selection: &CargoBuildSelection) -> Result<usize, CargoBuildFailure> {
    feature_allocation(selection.features())?
        .checked_add(selection.package().len())
        .and_then(|bytes| bytes.checked_add(selection.target().len()))
        .and_then(|bytes| bytes.checked_add(selection.profile().len()))
        .and_then(|bytes| bytes.checked_add(selection.target_triple().len()))
        .ok_or(CargoBuildFailure::Limit("selection metadata"))
}

fn input_allocation(inputs: &[ObservedInput]) -> Result<usize, CargoBuildFailure> {
    inputs
        .iter()
        .try_fold(std::mem::size_of_val(inputs), |bytes, input| {
            bytes
                .checked_add(input.fact.path.len())
                .ok_or(CargoBuildFailure::Limit("input metadata"))
        })
}

fn invocation_allocation(
    invocation: &crate::project_workflow::cargo::CargoBuildInvocation,
) -> Result<usize, CargoBuildFailure> {
    let mut bytes = std::mem::size_of_val(invocation);
    for path in [
        invocation.manifest_path(),
        invocation.workspace_root(),
        invocation.target_directory(),
        invocation.source_path(),
    ] {
        bytes = path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_mul(4)
            .and_then(|path| bytes.checked_add(path))
            .ok_or(CargoBuildFailure::Limit("invocation metadata"))?;
    }
    bytes
        .checked_add(feature_allocation(invocation.features())?)
        .and_then(|bytes| {
            feature_allocation(invocation.metadata_features())
                .ok()
                .and_then(|features| bytes.checked_add(features))
        })
        .and_then(|bytes| {
            bytes.checked_add(
                invocation.package_name().len()
                    + invocation.package_version().len()
                    + invocation.target_name().len()
                    + invocation.target_triple().len()
                    + invocation.profile().len(),
            )
        })
        .ok_or(CargoBuildFailure::Limit("invocation metadata"))
}

fn diagnostics_allocation(diagnostics: &CargoDiagnostics) -> Result<usize, CargoBuildFailure> {
    diagnostics.entries().iter().try_fold(
        std::mem::size_of_val(diagnostics.entries()),
        |bytes, diagnostic| {
            bytes
                .checked_add(diagnostic.message().len())
                .and_then(|bytes| bytes.checked_add(diagnostic.code().map_or(0, str::len)))
                .ok_or(CargoBuildFailure::Limit("diagnostic metadata"))
        },
    )
}

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "cargo_build/tests.rs"]
mod tests;
