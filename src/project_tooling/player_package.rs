//! Finite captured Player/content binding and whole-directory publication.
//!
//! A generated Rust input is an independently selected expectation, never a digest obtained from
//! the pack being opened. Trusted Player code must compile and use that input with its own recipe.
//! Declared Cargo provenance proves checked input facts, not Rust inclusion or executed branches.
//! Current content selection belongs to the compiled Host's fence; historical verified bytes alone
//! do not prove freshness. Publication is separate from Runtime Ready and native qualification.

use std::{
    fmt::{self, Write as _},
    io::{self, Write},
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_core::{ByteLimit, EngineVersion, FormatGenerator, FormatKind, PersistentFileEnvelope};
use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryEnumerationLimits, DirectoryPublicationBeginError,
    DirectoryPublicationOrphan, DirectoryPublicationOutcome, DirectoryPublicationReceipt,
    DirectoryPublicationRejection, FileKind, FsError, FsOperation, RelativeComponent, RelativePath,
};
use serde::Serialize;
use thiserror::Error;

use super::cargo_build::{
    CapturedCargoExecutable, CargoBuildFailure, CargoBuildPoint, CargoBuildRequest,
    CargoDeclaredInput, CargoToolchainObservation, build_executable,
};
use crate::{
    project_workflow::{cargo::CargoBuildSelection, cooked_content::PreparedRuntimeContent},
    runtime_content::{RuntimeContentError, RuntimeContentExpectation, RuntimeContentTarget},
};

pub const PLAYER_PACK_FILE: &str = "content.pack";
pub const PLAYER_PACKAGE_MANIFEST_FILE: &str = "package.json";

// FileCapability::verify_digest owns a 64 KiB read buffer plus the BLAKE3 hasher. The
// enumeration cursor is dropped before hashing; 4096 additional logical bytes cover the finite
// four-observation/name/relative-path metadata and the small streaming/publication records.
const PUBLICATION_WORK_BYTES: usize = 64 * 1024 + size_of::<blake3::Hasher>() + 4096;

#[cfg(test)]
mod tests;

/// Logical owner memory bounds and an independent limit on the complete staged disk payload.
///
/// Work includes still-retained content, generated input, executable/provenance and metadata, plus
/// the next owner's admitted work promise. This does not measure allocator capacity, subprocess
/// RSS or arbitrary allocations inside a trusted Host fence. Caller-owned source observations and
/// escaped acquisitions retain their original owners and allowances.
#[derive(Debug, Clone, Copy)]
pub struct PlayerPackageLimits {
    pub bootstrap_bytes: ByteLimit,
    pub manifest_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
    pub work_bytes: ByteLimit,
    pub disk_bytes: ByteLimit,
}

impl Default for PlayerPackageLimits {
    fn default() -> Self {
        let bytes = |value| ByteLimit::new(value).expect("nonzero Player package limit");
        Self {
            bootstrap_bytes: bytes(16 * 1024),
            manifest_bytes: bytes(1024 * 1024),
            retained_bytes: bytes(2 * 1024 * 1024 * 1024),
            work_bytes: bytes(3 * 1024 * 1024 * 1024),
            disk_bytes: bytes(2 * 1024 * 1024 * 1024),
        }
    }
}

/// Exact Rust input to place at one canonical declared Cargo input path before building.
///
/// The generated constants do not name a crate, so renamed engine dependencies work normally.
/// Neither command-line arguments nor the delivery manifest may override these expected facts in
/// the trusted Player implementation. This value does not write or authorize a workspace path.
pub struct GeneratedPlayerBootstrap {
    path: String,
    bytes: Box<[u8]>,
    digest: ContentDigest,
    expected: RuntimeContentExpectation,
    retained_bytes: usize,
}

impl fmt::Debug for GeneratedPlayerBootstrap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedPlayerBootstrap")
            .field("path", &self.path)
            .field("digest", &self.digest)
            .field("expected", &self.expected)
            .field("retained_bytes", &self.retained_bytes)
            .finish_non_exhaustive()
    }
}

impl GeneratedPlayerBootstrap {
    pub fn new(
        content: &PreparedRuntimeContent,
        input_path: &str,
        limits: PlayerPackageLimits,
    ) -> Result<Self, PlayerPackageError> {
        validate_input_path(input_path)?;
        let mut counter = TextCount(0);
        write_bootstrap(&mut counter, content.expectation())
            .map_err(|_| PlayerPackageError::Overflow)?;
        check(counter.0, limits.bootstrap_bytes, "bootstrap bytes")?;
        let expected = content.expectation();
        let retained_bytes = sum(&[
            size_of::<Self>(),
            input_path.len(),
            counter.0,
            expected.target().triple().len(),
            expected.target().profile().len(),
        ])?;
        let overlap = sum(&[content.retained_bytes(), retained_bytes])?;
        check(overlap, limits.retained_bytes, "bootstrap retention")?;
        check(overlap, limits.work_bytes, "bootstrap work")?;
        let mut text = String::with_capacity(counter.0);
        write_bootstrap(&mut text, expected).expect("writing admitted Rust constants to String");
        let bytes = text.into_bytes().into_boxed_slice();
        Ok(Self {
            path: input_path.to_owned(),
            digest: ContentDigest::of_bytes(&bytes),
            bytes,
            expected: expected.clone(),
            retained_bytes,
        })
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerPackagePoint {
    BeforeBuild,
    Cargo(CargoBuildPoint),
    AfterBuild,
    BeforeStaging,
    AfterPackWrite,
    BeforePublication,
    AfterPublication,
}

/// A finite Host fence result. The Host retains its real observation/selection envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerPackageControlFailure {
    Cancelled,
    SelectionChanged,
    Panicked,
}

#[derive(Debug, Error)]
pub enum PlayerPackageError {
    #[error("Player package {kind} requires {required} bytes, limit {limit}")]
    Limit {
        kind: &'static str,
        required: usize,
        limit: usize,
    },
    #[error("Player package ownership or encoded extent arithmetic overflowed")]
    Overflow,
    #[error("generated Player input requires one canonical portable relative path")]
    InputPath,
    #[error("generated Player input does not describe the prepared content")]
    BootstrapMismatch,
    #[error("captured Cargo provenance omits or disagrees with the exact generated input")]
    BuildInputMismatch,
    #[error("captured Cargo selection disagrees with the requested Player selection")]
    BuildSelectionMismatch,
    #[error("Player target triple disagrees with the content target")]
    TargetMismatch,
    #[error("the captured executable has no admissible distinct portable package filename")]
    ExecutableName,
    #[error("Player package control rejected {point:?}: {failure:?}")]
    Control {
        point: PlayerPackagePoint,
        failure: PlayerPackageControlFailure,
    },
    #[error("selected Player build failed")]
    Build {
        #[source]
        source: Box<CargoBuildFailure>,
        control: Option<(PlayerPackagePoint, PlayerPackageControlFailure)>,
    },
    #[error(transparent)]
    Runtime(#[from] RuntimeContentError),
    #[error(transparent)]
    Filesystem(#[from] FsError),
    #[error("native Player package I/O failed")]
    Io(#[from] io::Error),
    #[error("Player package manifest could not be encoded")]
    Manifest,
    #[error("staged Player package bytes disagree with the captured candidate")]
    StagedBytes,
    #[error("staged Player package does not contain exactly the three declared regular files")]
    StagedMembers,
    #[error("Player package staging name entropy was unavailable")]
    Entropy(#[from] getrandom::Error),
}

/// Builds the selected Player while keeping content and generated-input costs in the work bound.
///
/// Write `bootstrap.bytes()` at `bootstrap.path()` first, and include it in `request`'s inventory.
/// The control closure revalidates the actual source/metadata/auxiliary and processed selections;
/// it is not replaced by Cargo's separate code inventory. All callback panics become finite
/// failures. Cargo cleanup failures retain their original process/target-directory custody.
pub fn build_player_package(
    content: PreparedRuntimeContent,
    bootstrap: GeneratedPlayerBootstrap,
    request: &CargoBuildRequest,
    limits: PlayerPackageLimits,
    mut control: impl FnMut(PlayerPackagePoint) -> Result<(), PlayerPackageControlFailure>,
) -> Result<PreparedPlayerPackage, PlayerPackageError> {
    check_bootstrap(&content, &bootstrap)?;
    check_target(&content, request.selection())?;
    let resident = sum(&[content.retained_bytes(), bootstrap.retained_bytes()])?;
    check(
        sum(&[resident, request.limits().work_bytes.get()])?,
        limits.work_bytes,
        "Cargo overlap",
    )?;
    check(
        sum(&[resident, request.limits().retained_bytes.get()])?,
        limits.retained_bytes,
        "Cargo retention promise",
    )?;
    checkpoint(&mut control, PlayerPackagePoint::BeforeBuild)?;
    let mut rejected_control = None;
    let executable = build_executable(request, |point| {
        let point = PlayerPackagePoint::Cargo(point);
        checkpoint(&mut control, point).map_err(|error| {
            if let PlayerPackageError::Control { point, failure } = error {
                rejected_control = Some((point, failure));
            }
            CargoBuildFailure::Aborted
        })
    })
    .map_err(|source| PlayerPackageError::Build {
        source: Box::new(source),
        control: rejected_control,
    })?;
    checkpoint(&mut control, PlayerPackagePoint::AfterBuild)?;
    PreparedPlayerPackage::bind(content, bootstrap, request.selection(), executable, limits)
}

/// Move-only unpublished package retaining original captured content and executable bytes.
pub struct PreparedPlayerPackage {
    content: PreparedRuntimeContent,
    executable: CapturedCargoExecutable,
    executable_name: String,
    manifest: Box<[u8]>,
    pack_digest: ContentDigest,
    retained_bytes: usize,
}

impl fmt::Debug for PreparedPlayerPackage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedPlayerPackage")
            .field("content", &self.content)
            .field("executable", &self.executable)
            .field("executable_name", &self.executable_name)
            .field("pack_digest", &self.pack_digest)
            .field("manifest_bytes", &self.manifest.len())
            .field("retained_bytes", &self.retained_bytes)
            .finish_non_exhaustive()
    }
}

impl PreparedPlayerPackage {
    /// Binds already captured facts, without claiming that historical inputs remain current.
    ///
    /// Full requested selection and exact generated-input path/digest are checked. Actual compiled
    /// features and tool observations remain in the provenance and manifest. This does not scan
    /// executable code to prove use of the input and does not execute or qualify the Player.
    pub fn bind(
        content: PreparedRuntimeContent,
        bootstrap: GeneratedPlayerBootstrap,
        expected_selection: &CargoBuildSelection,
        executable: CapturedCargoExecutable,
        limits: PlayerPackageLimits,
    ) -> Result<Self, PlayerPackageError> {
        check_bootstrap(&content, &bootstrap)?;
        check_target(&content, expected_selection)?;
        let provenance = executable.provenance();
        if provenance.selection() != expected_selection {
            return Err(PlayerPackageError::BuildSelectionMismatch);
        }
        if !provenance
            .declared_inputs()
            .iter()
            .any(|input| input.path() == bootstrap.path() && input.digest() == bootstrap.digest())
        {
            return Err(PlayerPackageError::BuildInputMismatch);
        }
        let filename = executable
            .original_path()
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                ![PLAYER_PACK_FILE, PLAYER_PACKAGE_MANIFEST_FILE]
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
            })
            .ok_or(PlayerPackageError::ExecutableName)?;
        RelativeComponent::new(filename).map_err(|_| PlayerPackageError::ExecutableName)?;
        let base_retained = sum(&[
            size_of::<Self>(),
            content.retained_bytes(),
            executable.retained_bytes(),
            filename.len(),
        ])?;
        let overlap = sum(&[base_retained, bootstrap.retained_bytes()])?;
        check(overlap, limits.retained_bytes, "binding retention")?;
        check(
            sum(&[overlap, size_of::<DigestWriter<io::Sink>>()])?,
            limits.work_bytes,
            "pack hashing work",
        )?;
        let mut digest_writer = DigestWriter::new(io::sink(), content.base().pack_encoded_bytes()?);
        content.base().write_pack(&mut digest_writer)?;
        let pack_digest = digest_writer.finish()?;
        check(
            sum(&[overlap, 512])?,
            limits.work_bytes,
            "manifest envelope work",
        )?;
        let wire = manifest(&content, &bootstrap, &executable, filename, pack_digest)?;
        let mut count = ByteCount {
            bytes: 0,
            limit: limits.manifest_bytes.get(),
        };
        serde_json::to_writer(&mut count, &wire).map_err(|_| PlayerPackageError::Manifest)?;
        let retained_bytes = sum(&[base_retained, count.bytes])?;
        check(
            sum(&[retained_bytes, bootstrap.retained_bytes()])?,
            limits.retained_bytes,
            "manifest retention",
        )?;
        check(
            sum(&[
                retained_bytes,
                bootstrap.retained_bytes(),
                PUBLICATION_WORK_BYTES,
            ])?,
            limits.work_bytes,
            "publication work",
        )?;
        let disk_bytes = pack_digest
            .length()
            .checked_add(executable.digest().length())
            .and_then(|bytes| bytes.checked_add(count.bytes as u64))
            .ok_or(PlayerPackageError::Overflow)?;
        let disk_bytes = usize::try_from(disk_bytes).map_err(|_| PlayerPackageError::Overflow)?;
        check(disk_bytes, limits.disk_bytes, "disk payload")?;
        let mut bytes = Vec::with_capacity(count.bytes);
        serde_json::to_writer(&mut bytes, &wire).map_err(|_| PlayerPackageError::Manifest)?;
        let executable_name = filename.to_owned();
        Ok(Self {
            content,
            executable,
            executable_name,
            manifest: bytes.into_boxed_slice(),
            pack_digest,
            retained_bytes,
        })
    }

    #[must_use]
    pub fn executable_name(&self) -> &str {
        &self.executable_name
    }
    #[must_use]
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest
    }
    #[must_use]
    pub const fn pack_digest(&self) -> ContentDigest {
        self.pack_digest
    }
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Stages and verifies exactly three files, then uses the existing no-replace directory owner.
    ///
    /// Every writable file/directory handle is quiescent before publication. No output directory is
    /// recursively removed on failure. A postpublication fence error retains the physical receipt.
    pub fn publish(
        self,
        parent: &DirectoryCapability,
        destination: &RelativeComponent,
        mut control: impl FnMut(PlayerPackagePoint) -> Result<(), PlayerPackageControlFailure>,
    ) -> PlayerPackageOutcome {
        if let Err(error) = checkpoint(&mut control, PlayerPackagePoint::BeforeStaging) {
            return PlayerPackageOutcome::BeforeStaging(error);
        }
        let staging_name = match staging_name() {
            Ok(name) => name,
            Err(error) => return PlayerPackageOutcome::BeforeStaging(error),
        };
        let transaction = match parent.begin_directory_publication(destination, &staging_name) {
            Ok(transaction) => transaction,
            Err(DirectoryPublicationBeginError::BeforeStaging(error)) => {
                return PlayerPackageOutcome::BeforeStaging(error.into());
            }
            Err(DirectoryPublicationBeginError::RejectedClean(publication)) => {
                return PlayerPackageOutcome::Rejected {
                    error: None,
                    publication: *publication,
                };
            }
            Err(DirectoryPublicationBeginError::Orphaned(publication)) => {
                return PlayerPackageOutcome::Orphaned {
                    error: None,
                    publication: *publication,
                };
            }
            #[allow(unreachable_patterns)]
            Err(error) => return PlayerPackageOutcome::Unresolved(error),
        };
        if let Err(error) = self.stage(transaction.staging_directory(), &mut control) {
            return aborted(error, transaction.abort());
        }
        let facts = PlayerPackageFiles {
            executable_name: self.executable_name,
            executable: self.executable.digest(),
            catalog: self.content.base().catalog_digest(),
            pack: self.pack_digest,
            manifest: ContentDigest::of_bytes(&self.manifest),
            build_provenance: self.executable.provenance().digest(),
        };
        match transaction.publish() {
            DirectoryPublicationOutcome::Published(publication) => {
                let error = checkpoint(&mut control, PlayerPackagePoint::AfterPublication).err();
                PlayerPackageOutcome::Published {
                    receipt: PlayerPackageReceipt {
                        files: facts,
                        publication,
                    },
                    error,
                }
            }
            DirectoryPublicationOutcome::RejectedClean(publication) => {
                PlayerPackageOutcome::Rejected {
                    error: None,
                    publication,
                }
            }
            DirectoryPublicationOutcome::Orphaned(publication) => PlayerPackageOutcome::Orphaned {
                error: None,
                publication,
            },
        }
    }

    fn stage(
        &self,
        staging: DirectoryCapability,
        control: &mut impl FnMut(PlayerPackagePoint) -> Result<(), PlayerPackageControlFailure>,
    ) -> Result<(), PlayerPackageError> {
        {
            let name = RelativeComponent::new(PLAYER_PACK_FILE).expect("fixed pack filename");
            let mut file = staging.create_child_file(&name)?;
            {
                let writer = file.writer()?;
                let mut writer = DigestWriter::new(writer, self.pack_digest.length());
                self.content.base().write_pack(&mut writer)?;
                writer.flush()?;
                if writer.finish()? != self.pack_digest {
                    return Err(PlayerPackageError::StagedBytes);
                }
            }
            file.sync()?;
        }
        checkpoint(control, PlayerPackagePoint::AfterPackWrite)?;
        write_member(
            &staging,
            &self.executable_name,
            self.executable.bytes(),
            true,
        )?;
        write_member(
            &staging,
            PLAYER_PACKAGE_MANIFEST_FILE,
            &self.manifest,
            false,
        )?;
        checkpoint(control, PlayerPackagePoint::BeforePublication)?;
        validate_staged_members(&staging, &self.executable_name)?;
        for (name, digest) in [
            (PLAYER_PACK_FILE, self.pack_digest),
            (self.executable_name.as_str(), self.executable.digest()),
            (
                PLAYER_PACKAGE_MANIFEST_FILE,
                ContentDigest::of_bytes(&self.manifest),
            ),
        ] {
            let path = RelativePath::new(name).expect("admitted flat package filename");
            staging
                .open_file(&path)?
                .verify_digest(digest, digest.length())?;
        }
        match staging.sync() {
            Ok(_)
            | Err(FsError::Unsupported {
                operation: FsOperation::SyncDirectory,
                ..
            }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_staged_members(
    staging: &DirectoryCapability,
    executable_name: &str,
) -> Result<(), PlayerPackageError> {
    let expected = [
        PLAYER_PACK_FILE,
        executable_name,
        PLAYER_PACKAGE_MANIFEST_FILE,
    ];
    // A fourth entry is sufficient to reject. Enumeration's cumulative limits also bound hostile
    // names/rejections; no recursive scan or unbounded inventory is needed for this flat layout.
    let mut session =
        staging.begin_child_enumeration(DirectoryEnumerationLimits::new(4, 255, 1))?;
    let batch = session.next_batch(4)?;
    if !batch.is_complete()
        || !batch.rejected_entry_facts().is_empty()
        || batch.entries().len() != 3
    {
        return Err(PlayerPackageError::StagedMembers);
    }
    let mut seen = [false; 3];
    for entry in batch.entries() {
        let Some(index) = expected
            .iter()
            .position(|name| entry.name().to_str() == Some(*name))
        else {
            return Err(PlayerPackageError::StagedMembers);
        };
        if entry.kind() != FileKind::Regular || seen[index] {
            return Err(PlayerPackageError::StagedMembers);
        }
        seen[index] = true;
    }
    Ok(())
}

#[derive(Debug)]
pub struct PlayerPackageFiles {
    pub executable_name: String,
    pub executable: ContentDigest,
    pub catalog: ContentDigest,
    pub pack: ContentDigest,
    pub manifest: ContentDigest,
    pub build_provenance: ContentDigest,
}

/// Physical directory publication evidence, independent of native execution or last-good state.
#[derive(Debug)]
pub struct PlayerPackageReceipt {
    pub files: PlayerPackageFiles,
    pub publication: DirectoryPublicationReceipt,
}

#[must_use = "handle physical publication and any remaining cleanup custody"]
#[derive(Debug)]
pub enum PlayerPackageOutcome {
    BeforeStaging(PlayerPackageError),
    Rejected {
        error: Option<PlayerPackageError>,
        publication: DirectoryPublicationRejection,
    },
    Orphaned {
        error: Option<PlayerPackageError>,
        publication: DirectoryPublicationOrphan,
    },
    Unresolved(DirectoryPublicationBeginError),
    Published {
        receipt: PlayerPackageReceipt,
        error: Option<PlayerPackageError>,
    },
}

fn aborted(
    error: PlayerPackageError,
    outcome: DirectoryPublicationOutcome,
) -> PlayerPackageOutcome {
    match outcome {
        DirectoryPublicationOutcome::RejectedClean(publication) => PlayerPackageOutcome::Rejected {
            error: Some(error),
            publication,
        },
        DirectoryPublicationOutcome::Orphaned(publication) => PlayerPackageOutcome::Orphaned {
            error: Some(error),
            publication,
        },
        DirectoryPublicationOutcome::Published(_) => {
            unreachable!("aborting cannot publish a package")
        }
    }
}

fn checkpoint(
    control: &mut impl FnMut(PlayerPackagePoint) -> Result<(), PlayerPackageControlFailure>,
    point: PlayerPackagePoint,
) -> Result<(), PlayerPackageError> {
    catch_unwind(AssertUnwindSafe(|| control(point)))
        .unwrap_or(Err(PlayerPackageControlFailure::Panicked))
        .map_err(|failure| PlayerPackageError::Control { point, failure })
}

fn check_bootstrap(
    content: &PreparedRuntimeContent,
    input: &GeneratedPlayerBootstrap,
) -> Result<(), PlayerPackageError> {
    let expected = content.expectation();
    if expected.catalog_digest() != input.expected.catalog_digest()
        || expected.target() != input.expected.target()
    {
        return Err(PlayerPackageError::BootstrapMismatch);
    }
    Ok(())
}

fn check_target(
    content: &PreparedRuntimeContent,
    selection: &CargoBuildSelection,
) -> Result<(), PlayerPackageError> {
    if content.expectation().target().triple() != selection.target_triple() {
        return Err(PlayerPackageError::TargetMismatch);
    }
    Ok(())
}

fn validate_input_path(path: &str) -> Result<(), PlayerPackageError> {
    if path.len() > 4096
        || path.contains('\\')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(PlayerPackageError::InputPath);
    }
    RelativePath::new(path).map_err(|_| PlayerPackageError::InputPath)?;
    Ok(())
}

fn write_bootstrap(
    output: &mut impl fmt::Write,
    expected: &RuntimeContentExpectation,
) -> fmt::Result {
    writeln!(
        output,
        "// Generated by Nara's cooked Player owner. Compile and use with the Player recipe."
    )?;
    writeln!(
        output,
        "pub const CATALOG_HASH: [u8; 32] = {:?};",
        expected.catalog_digest().as_bytes()
    )?;
    writeln!(
        output,
        "pub const CATALOG_LENGTH: u64 = {};",
        expected.catalog_digest().length()
    )?;
    writeln!(
        output,
        "pub const CONTENT_TARGET: &str = {:?};",
        expected.target().triple()
    )?;
    writeln!(
        output,
        "pub const CONTENT_PROFILE: &str = {:?};",
        expected.target().profile()
    )?;
    writeln!(output, "pub const PACK_FILE: &str = {PLAYER_PACK_FILE:?};")
}

struct TextCount(usize);
impl fmt::Write for TextCount {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0 = self.0.checked_add(value.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

struct ByteCount {
    bytes: usize,
    limit: usize,
}
impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|bytes| *bytes <= self.limit)
            .ok_or_else(|| io::Error::other("bounded manifest limit exceeded"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct DigestWriter<W> {
    output: W,
    hasher: blake3::Hasher,
    bytes: u64,
    expected: u64,
}
impl<W: Write> DigestWriter<W> {
    fn new(output: W, expected: u64) -> Self {
        Self {
            output,
            hasher: blake3::Hasher::new(),
            bytes: 0,
            expected,
        }
    }
    fn finish(self) -> Result<ContentDigest, PlayerPackageError> {
        if self.bytes != self.expected {
            return Err(PlayerPackageError::StagedBytes);
        }
        Ok(ContentDigest::from_parts(
            self.bytes,
            *self.hasher.finalize().as_bytes(),
        ))
    }
}
impl<W: Write> Write for DigestWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self
            .expected
            .checked_sub(self.bytes)
            .ok_or_else(|| io::Error::other("pack extent overflow"))?;
        if bytes.len() as u64 > remaining {
            return Err(io::Error::other("pack exceeded its admitted extent"));
        }
        let written = self.output.write(bytes)?;
        self.hasher.update(&bytes[..written]);
        self.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

#[derive(Serialize)]
struct DigestWire {
    length: u64,
    hash: [u8; 32],
}
impl From<ContentDigest> for DigestWire {
    fn from(value: ContentDigest) -> Self {
        Self {
            length: value.length(),
            hash: *value.as_bytes(),
        }
    }
}

#[derive(Serialize)]
struct SelectionWire<'a> {
    package: &'a str,
    bin: &'a str,
    cargo_profile: &'a str,
    target_triple: &'a str,
    default_features: bool,
    requested_features: &'a [String],
}
#[derive(Serialize)]
struct BuildWire<'a> {
    selection: SelectionWire<'a>,
    resolved_features: &'a [String],
    toolchain: &'a CargoToolchainObservation,
    provenance_digest: DigestWire,
    metadata_digest: DigestWire,
    declared_inputs: &'a [CargoDeclaredInput],
}
#[derive(Serialize)]
struct ManifestWire<'a> {
    executable_name: &'a str,
    executable_digest: DigestWire,
    catalog_digest: DigestWire,
    content_target: &'a RuntimeContentTarget,
    pack_file: &'static str,
    pack_digest: DigestWire,
    generated_input_path: &'a str,
    generated_input_digest: DigestWire,
    build: BuildWire<'a>,
}

fn manifest<'a>(
    content: &'a PreparedRuntimeContent,
    input: &'a GeneratedPlayerBootstrap,
    executable: &'a CapturedCargoExecutable,
    executable_name: &'a str,
    pack: ContentDigest,
) -> Result<PersistentFileEnvelope<ManifestWire<'a>>, PlayerPackageError> {
    let provenance = executable.provenance();
    let selection = provenance.selection();
    let payload = ManifestWire {
        executable_name,
        executable_digest: executable.digest().into(),
        catalog_digest: content.base().catalog_digest().into(),
        content_target: content.expectation().target(),
        pack_file: PLAYER_PACK_FILE,
        pack_digest: pack.into(),
        generated_input_path: input.path(),
        generated_input_digest: input.digest().into(),
        build: BuildWire {
            selection: SelectionWire {
                package: selection.package(),
                bin: selection.target(),
                cargo_profile: selection.profile(),
                target_triple: selection.target_triple(),
                default_features: selection.default_features(),
                requested_features: selection.features(),
            },
            resolved_features: provenance.resolved_features(),
            toolchain: provenance.toolchain(),
            provenance_digest: provenance.digest().into(),
            metadata_digest: provenance.metadata_digest().into(),
            declared_inputs: provenance.declared_inputs(),
        },
    };
    let version = EngineVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| PlayerPackageError::Manifest)?;
    Ok(PersistentFileEnvelope::canonical_v1(
        FormatKind::new("player_package").map_err(|_| PlayerPackageError::Manifest)?,
        version.clone(),
        FormatGenerator::new("nara", version).map_err(|_| PlayerPackageError::Manifest)?,
        payload,
    ))
}

fn write_member(
    staging: &DirectoryCapability,
    name: &str,
    bytes: &[u8],
    executable: bool,
) -> Result<(), PlayerPackageError> {
    let name = RelativeComponent::new(name).expect("admitted flat package filename");
    let mut file = if executable {
        staging.create_child_executable_file(&name)?
    } else {
        staging.create_child_file(&name)?
    };
    {
        let mut writer = file.writer()?;
        writer.write_all(bytes)?;
        writer.flush()?;
    }
    file.sync()?;
    Ok(())
}

fn staging_name() -> Result<RelativeComponent, PlayerPackageError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)?;
    let mut text = String::with_capacity(45);
    text.push_str(".nara-player-");
    for byte in random {
        write!(text, "{byte:02x}").expect("writing staging name to String");
    }
    Ok(RelativeComponent::new(text).expect("generated staging name"))
}

fn sum(values: &[usize]) -> Result<usize, PlayerPackageError> {
    values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or(PlayerPackageError::Overflow)
    })
}

fn check(required: usize, limit: ByteLimit, kind: &'static str) -> Result<(), PlayerPackageError> {
    if required > limit.get() {
        return Err(PlayerPackageError::Limit {
            kind,
            required,
            limit: limit.get(),
        });
    }
    Ok(())
}
