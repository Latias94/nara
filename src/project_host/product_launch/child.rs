//! Child-local activation owner and bounded one-shot READY publication.

use std::{
    env,
    error::Error,
    fmt,
    fs::File,
    io::{self, Read},
    mem::size_of,
    thread,
    time::{Duration, Instant},
};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, FsError};

use super::platform::{
    InheritedBootstrapEndpoint, InheritedBootstrapRead, inherited_bootstrap_endpoint,
};
use super::source::{EditorExternalPlaySourceReceipt, EditorExternalPlayTarget};
use super::wire::{
    FRAME_LENGTH_BYTES, MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
    SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION, SUPERVISED_DESKTOP_LAUNCH_MAGIC,
    SupervisedDesktopLaunchContext, SupervisedDesktopProductIdentity,
    SupervisedDesktopReadyAttestation, SupervisedDesktopRuntimeClosureFacts,
    SupervisedDesktopWireError, frame_length,
};

const SUPERVISED_DESKTOP_LAUNCH_READ_TIMEOUT: Duration = Duration::from_secs(10);
const BOOTSTRAP_POLL_INTERVAL: Duration = Duration::from_millis(2);
const MAX_READY_WRITE_BYTES_PER_ADVANCE: usize = 16 * 1024;
const EXECUTABLE_DIGEST_BUFFER_BYTES: usize = 64 * 1024;

/// Fixed monotonic budget for publishing READY after the first playable presentation.
#[doc(hidden)]
pub const SUPERVISED_DESKTOP_READY_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Unverified child-side owner of one inherited bootstrap endpoint and decoded launch value.
///
/// Reading recognizes only the platform shape reserved by Nara's child-local bootstrap endpoint;
/// it never consults arguments, environment variables, or the current working directory.
/// This candidate exposes the explicit project root so product code can open one
/// [`DirectoryCapability`]. It cannot begin READY. Every relative source fact must first be
/// revalidated through that capability to produce [`ProductSupervisedDesktopSession`].
#[doc(hidden)]
pub struct ProductSupervisedDesktopLaunchCandidate {
    context: SupervisedDesktopLaunchContext,
    source_receipt: EditorExternalPlaySourceReceipt,
    measured_executable_digest: ContentDigest,
    endpoint: InheritedBootstrapEndpoint,
}

impl ProductSupervisedDesktopLaunchCandidate {
    pub fn read_from_inherited_bootstrap()
    -> Result<Option<Self>, ProductSupervisedDesktopBootstrapError> {
        let Some(mut endpoint) = inherited_bootstrap_endpoint().map_err(|source| {
            ProductSupervisedDesktopBootstrapError::Io {
                operation: "inspect inherited bootstrap endpoint",
                source,
            }
        })?
        else {
            return Ok(None);
        };
        let context = read_launch_context(&mut endpoint)?;
        let source_receipt = decode_source_receipt(&context)?;
        let measured_executable_digest =
            measure_current_executable_digest(context.executable_digest().length())?;
        endpoint.prepare_ready_write().map_err(|source| {
            ProductSupervisedDesktopBootstrapError::Io {
                operation: "prepare READY endpoint",
                source,
            }
        })?;
        Ok(Some(Self {
            context,
            source_receipt,
            measured_executable_digest,
            endpoint,
        }))
    }

    #[must_use]
    pub const fn context(&self) -> &SupervisedDesktopLaunchContext {
        &self.context
    }

    /// Revalidates every receipt path through the exact opened project-root capability.
    ///
    /// Success binds the resulting session to this capability identity. `DesktopRun` checks that
    /// the same shared authority is used for Runtime startup before it can arm READY publication.
    pub fn verify_project_source(
        self,
        project_root: &DirectoryCapability,
    ) -> Result<ProductSupervisedDesktopSession, ProductSupervisedDesktopBootstrapError> {
        verify_source_facts(project_root, &self.source_receipt)?;
        Ok(ProductSupervisedDesktopSession {
            context: self.context,
            verified_source_receipt: self.source_receipt,
            measured_executable_digest: self.measured_executable_digest,
            endpoint: self.endpoint,
            project_root_identity: project_root.identity(),
        })
    }
}

impl fmt::Debug for ProductSupervisedDesktopLaunchCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductSupervisedDesktopLaunchCandidate")
            .field("context", &self.context)
            .field("source_facts", &self.source_receipt.source_facts().len())
            .field(
                "measured_executable_digest",
                &self.measured_executable_digest,
            )
            .field("endpoint", &"inherited bootstrap")
            .finish()
    }
}

/// Capability-verified child activation owner that alone may begin one READY publication.
///
/// The source receipt was decoded from the launch context and every relative source fact was
/// re-proved through the bound project-root capability. Beginning READY consumes this owner. The
/// returned publication writes one bounded canonical record and closes the endpoint; there is no
/// post-READY command surface.
#[doc(hidden)]
pub struct ProductSupervisedDesktopSession {
    context: SupervisedDesktopLaunchContext,
    verified_source_receipt: EditorExternalPlaySourceReceipt,
    measured_executable_digest: ContentDigest,
    endpoint: InheritedBootstrapEndpoint,
    project_root_identity: FileIdentity,
}

impl ProductSupervisedDesktopSession {
    #[must_use]
    pub const fn context(&self) -> &SupervisedDesktopLaunchContext {
        &self.context
    }

    pub(crate) fn is_bound_to_project_root(&self, project_root: &DirectoryCapability) -> bool {
        self.project_root_identity == project_root.identity()
    }

    pub fn begin_ready(
        self,
        product_identity: SupervisedDesktopProductIdentity,
        runtime_closure: SupervisedDesktopRuntimeClosureFacts,
        now: Instant,
    ) -> Result<ProductSupervisedDesktopReadyPublication, ProductSupervisedDesktopBootstrapError>
    {
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &self.context,
            product_identity,
            self.measured_executable_digest,
            runtime_closure,
        )?;
        let encoded = ready.encode_canonical()?;
        let deadline = now
            .checked_add(SUPERVISED_DESKTOP_READY_WRITE_TIMEOUT)
            .ok_or(ProductSupervisedDesktopBootstrapError::DeadlineOverflow)?;
        Ok(ProductSupervisedDesktopReadyPublication {
            endpoint: Some(self.endpoint),
            ready,
            encoded,
            written: 0,
            deadline,
        })
    }
}

impl fmt::Debug for ProductSupervisedDesktopSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductSupervisedDesktopSession")
            .field("context", &self.context)
            .field(
                "verified_source_facts",
                &self.verified_source_receipt.source_facts().len(),
            )
            .field(
                "measured_executable_digest",
                &self.measured_executable_digest,
            )
            .field("project_root_identity", &self.project_root_identity)
            .field("endpoint", &"inherited bootstrap")
            .finish()
    }
}

/// Result of one nonblocking READY publication advance.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductSupervisedDesktopReadyAdvance {
    Pending,
    Published,
}

/// Fixed one-shot READY publication owner armed only after Runtime publication.
#[doc(hidden)]
pub struct ProductSupervisedDesktopReadyPublication {
    endpoint: Option<InheritedBootstrapEndpoint>,
    ready: SupervisedDesktopReadyAttestation,
    encoded: Vec<u8>,
    written: usize,
    deadline: Instant,
}

impl ProductSupervisedDesktopReadyPublication {
    #[must_use]
    pub const fn attestation(&self) -> &SupervisedDesktopReadyAttestation {
        &self.ready
    }

    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn advance(
        &mut self,
        now: Instant,
    ) -> Result<ProductSupervisedDesktopReadyAdvance, ProductSupervisedDesktopBootstrapError> {
        if self.endpoint.is_none() {
            return Ok(ProductSupervisedDesktopReadyAdvance::Published);
        }
        if now >= self.deadline {
            self.endpoint.take();
            return Err(ProductSupervisedDesktopBootstrapError::ReadyWriteTimedOut);
        }

        let end = self
            .written
            .checked_add(MAX_READY_WRITE_BYTES_PER_ADVANCE)
            .unwrap_or(usize::MAX)
            .min(self.encoded.len());
        let attempted = end.saturating_sub(self.written);
        let write_result = self
            .endpoint
            .as_mut()
            .expect("the READY endpoint was checked above")
            .try_write(&self.encoded[self.written..end]);
        let written = match write_result {
            Ok(written) if written <= attempted => written,
            Ok(_) => {
                self.endpoint.take();
                return Err(SupervisedDesktopWireError::InvalidLength.into());
            }
            Err(source) => {
                self.endpoint.take();
                return Err(ProductSupervisedDesktopBootstrapError::Io {
                    operation: "write READY",
                    source,
                });
            }
        };
        self.written = self
            .written
            .checked_add(written)
            .ok_or(SupervisedDesktopWireError::InvalidLength)?;
        if self.written == self.encoded.len() {
            self.endpoint.take();
            Ok(ProductSupervisedDesktopReadyAdvance::Published)
        } else {
            Ok(ProductSupervisedDesktopReadyAdvance::Pending)
        }
    }

    pub fn cancel(mut self) {
        self.endpoint.take();
    }
}

impl fmt::Debug for ProductSupervisedDesktopReadyPublication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductSupervisedDesktopReadyPublication")
            .field("ready", &self.ready)
            .field("encoded_bytes", &self.encoded.len())
            .field("written_bytes", &self.written)
            .field("deadline", &self.deadline)
            .field("published", &self.endpoint.is_none())
            .finish()
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub enum ProductSupervisedDesktopBootstrapError {
    DeadlineOverflow,
    LaunchReadTimedOut,
    ReadyWriteTimedOut,
    InvalidSourceReceipt,
    SourceFactVerification {
        index: usize,
        source: FsError,
    },
    Wire(SupervisedDesktopWireError),
    Io {
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for ProductSupervisedDesktopBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeadlineOverflow => {
                formatter.write_str("the supervised desktop bootstrap deadline overflowed")
            }
            Self::LaunchReadTimedOut => {
                formatter.write_str("the supervised desktop launch-context read timed out")
            }
            Self::ReadyWriteTimedOut => {
                formatter.write_str("the supervised desktop READY write timed out")
            }
            Self::InvalidSourceReceipt => formatter.write_str(
                "the supervised desktop source receipt is invalid or does not match the launch context",
            ),
            Self::SourceFactVerification { index, source } => write!(
                formatter,
                "supervised desktop source fact {index} could not be revalidated: {source}"
            ),
            Self::Wire(error) => write!(formatter, "supervised desktop bootstrap failed: {error}"),
            Self::Io { operation, source } => {
                write!(
                    formatter,
                    "supervised desktop bootstrap could not {operation}: {source}"
                )
            }
        }
    }
}

impl Error for ProductSupervisedDesktopBootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wire(error) => Some(error),
            Self::SourceFactVerification { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::DeadlineOverflow
            | Self::LaunchReadTimedOut
            | Self::ReadyWriteTimedOut
            | Self::InvalidSourceReceipt => None,
        }
    }
}

impl From<SupervisedDesktopWireError> for ProductSupervisedDesktopBootstrapError {
    fn from(error: SupervisedDesktopWireError) -> Self {
        Self::Wire(error)
    }
}

fn decode_source_receipt(
    context: &SupervisedDesktopLaunchContext,
) -> Result<EditorExternalPlaySourceReceipt, ProductSupervisedDesktopBootstrapError> {
    let receipt = EditorExternalPlaySourceReceipt::decode_canonical(context.source_receipt())
        .map_err(|_| ProductSupervisedDesktopBootstrapError::InvalidSourceReceipt)?;
    if receipt.target() != EditorExternalPlayTarget::Desktop
        || receipt.launch_context_version() != SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION
        || receipt.project_stable_id() != context.project_stable_id()
        || receipt.profile_name() != context.profile_name()
    {
        return Err(ProductSupervisedDesktopBootstrapError::InvalidSourceReceipt);
    }
    Ok(receipt)
}

fn verify_source_facts(
    project_root: &DirectoryCapability,
    receipt: &EditorExternalPlaySourceReceipt,
) -> Result<(), ProductSupervisedDesktopBootstrapError> {
    for (index, fact) in receipt.source_facts().iter().enumerate() {
        let file =
            project_root
                .open_file(fact.capability_path())
                .map_err(|source| {
                    ProductSupervisedDesktopBootstrapError::SourceFactVerification { index, source }
                })?;
        file.verify_digest(fact.digest(), fact.digest().length())
            .map_err(
                |source| ProductSupervisedDesktopBootstrapError::SourceFactVerification {
                    index,
                    source,
                },
            )?;
    }
    Ok(())
}

fn read_exact_bounded(
    endpoint: &mut InheritedBootstrapEndpoint,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the inherited bootstrap read timed out",
            ));
        }
        match endpoint.try_read(bytes)? {
            InheritedBootstrapRead::Pending => thread::sleep(BOOTSTRAP_POLL_INTERVAL),
            InheritedBootstrapRead::Read(read) => {
                let read = read.get();
                if read > bytes.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "the inherited bootstrap read exceeded its requested length",
                    ));
                }
                bytes = &mut bytes[read..];
            }
            InheritedBootstrapRead::Eof => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the inherited bootstrap endpoint closed before the launch context completed",
                ));
            }
        }
    }
    Ok(())
}

fn read_launch_context(
    endpoint: &mut InheritedBootstrapEndpoint,
) -> Result<SupervisedDesktopLaunchContext, ProductSupervisedDesktopBootstrapError> {
    let deadline = Instant::now()
        .checked_add(SUPERVISED_DESKTOP_LAUNCH_READ_TIMEOUT)
        .ok_or(ProductSupervisedDesktopBootstrapError::DeadlineOverflow)?;
    let header_length =
        SUPERVISED_DESKTOP_LAUNCH_MAGIC.len() + size_of::<u16>() + FRAME_LENGTH_BYTES;
    let mut frame = vec![0; header_length];
    read_launch_bytes(endpoint, &mut frame, deadline, "read launch-context header")?;
    let total = frame_length(
        &frame,
        SUPERVISED_DESKTOP_LAUNCH_MAGIC,
        SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
        MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
    )?
    .ok_or(SupervisedDesktopWireError::IncompleteFrame)?;
    let payload_length = total
        .checked_sub(header_length)
        .ok_or(SupervisedDesktopWireError::InvalidLength)?;
    frame
        .try_reserve_exact(payload_length)
        .map_err(|_| SupervisedDesktopWireError::AllocationFailed)?;
    frame.resize(total, 0);
    read_launch_bytes(
        endpoint,
        &mut frame[header_length..],
        deadline,
        "read launch context",
    )?;
    SupervisedDesktopLaunchContext::decode_canonical(&frame).map_err(Into::into)
}

fn measure_current_executable_digest(
    expected_length: u64,
) -> Result<ContentDigest, ProductSupervisedDesktopBootstrapError> {
    let executable =
        env::current_exe().map_err(|source| ProductSupervisedDesktopBootstrapError::Io {
            operation: "resolve the current executable for measurement",
            source,
        })?;
    let mut file =
        File::open(executable).map_err(|source| ProductSupervisedDesktopBootstrapError::Io {
            operation: "open the current executable for measurement",
            source,
        })?;
    let observed_length = file
        .metadata()
        .map_err(|source| ProductSupervisedDesktopBootstrapError::Io {
            operation: "inspect the current executable for measurement",
            source,
        })?
        .len();
    if observed_length != expected_length {
        return Err(SupervisedDesktopWireError::FactMismatch("measured executable length").into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut length = 0_u64;
    let mut buffer = [0_u8; EXECUTABLE_DIGEST_BUFFER_BYTES];
    while length < expected_length {
        let remaining = expected_length - length;
        let read_limit = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| SupervisedDesktopWireError::InvalidLength)?;
        let read = file.read(&mut buffer[..read_limit]).map_err(|source| {
            ProductSupervisedDesktopBootstrapError::Io {
                operation: "measure the current executable",
                source,
            }
        })?;
        if read == 0 {
            return Err(
                SupervisedDesktopWireError::FactMismatch("measured executable length").into(),
            );
        }
        length = length
            .checked_add(read as u64)
            .ok_or(SupervisedDesktopWireError::InvalidLength)?;
        hasher.update(&buffer[..read]);
    }
    let mut trailing = [0_u8; 1];
    let trailing =
        file.read(&mut trailing)
            .map_err(|source| ProductSupervisedDesktopBootstrapError::Io {
                operation: "verify the current executable measurement boundary",
                source,
            })?;
    if trailing != 0 {
        return Err(SupervisedDesktopWireError::FactMismatch("measured executable length").into());
    }
    Ok(ContentDigest::from_parts(
        length,
        *hasher.finalize().as_bytes(),
    ))
}

fn read_launch_bytes(
    endpoint: &mut InheritedBootstrapEndpoint,
    bytes: &mut [u8],
    deadline: Instant,
    operation: &'static str,
) -> Result<(), ProductSupervisedDesktopBootstrapError> {
    read_exact_bounded(endpoint, bytes, deadline).map_err(|source| {
        if source.kind() == io::ErrorKind::TimedOut {
            ProductSupervisedDesktopBootstrapError::LaunchReadTimedOut
        } else {
            ProductSupervisedDesktopBootstrapError::Io { operation, source }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
    use nara_project::ProjectPath;

    use super::*;
    use crate::project_host::product_launch::source::{
        EditorExternalPlaySourceFact, EditorExternalPlaySourceKind,
    };
    use crate::project_host::product_launch::wire::{
        SupervisedDesktopGenerationId, SupervisedDesktopSessionNonce,
    };

    static NEXT_TEST_PROJECT: AtomicU64 = AtomicU64::new(1);

    struct TestProject {
        root: PathBuf,
    }

    impl TestProject {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "nara-supervised-source-{}-{}",
                std::process::id(),
                NEXT_TEST_PROJECT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("scenes")).unwrap();
            fs::write(root.join("Cargo.toml"), b"cargo manifest").unwrap();
            fs::write(root.join("scenes/main.scene.json"), b"scene").unwrap();
            Self { root }
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::from_host_handle(
                host_directory(&self.root),
                HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
            )
            .unwrap()
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn source_fact(
        kind: EditorExternalPlaySourceKind,
        path: &str,
        bytes: &[u8],
    ) -> EditorExternalPlaySourceFact {
        EditorExternalPlaySourceFact::new(
            kind,
            ProjectPath::new(path).unwrap(),
            ContentDigest::of_bytes(bytes),
        )
        .unwrap()
    }

    fn source_receipt(
        project_stable_id: &str,
        profile_name: &str,
    ) -> EditorExternalPlaySourceReceipt {
        EditorExternalPlaySourceReceipt::new(
            project_stable_id.to_owned(),
            profile_name.to_owned(),
            EditorExternalPlayTarget::Desktop,
            SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"authoring bundle"),
            ContentDigest::of_bytes(b"content"),
            [7; 32],
            vec![
                source_fact(
                    EditorExternalPlaySourceKind::CargoManifest,
                    "Cargo.toml",
                    b"cargo manifest",
                ),
                source_fact(
                    EditorExternalPlaySourceKind::Scene,
                    "scenes/main.scene.json",
                    b"scene",
                ),
            ],
        )
        .unwrap()
    }

    fn launch_context(
        project_stable_id: &str,
        profile_name: &str,
        source_receipt: Vec<u8>,
    ) -> SupervisedDesktopLaunchContext {
        SupervisedDesktopLaunchContext::new(
            std::env::temp_dir().canonicalize().unwrap(),
            project_stable_id,
            profile_name,
            SupervisedDesktopProductIdentity::from_cargo_product("game", "0.1.0", "desktop")
                .unwrap(),
            SupervisedDesktopGenerationId::new([1; 32]).unwrap(),
            SupervisedDesktopSessionNonce::new([2; 32]).unwrap(),
            ContentDigest::of_bytes(b"executable"),
            ContentDigest::of_bytes(b"launch manifest"),
            source_receipt,
        )
        .unwrap()
    }

    #[test]
    fn decoded_source_receipt_must_match_launch_identity() {
        let encoded = source_receipt("project-id", "desktop")
            .encode_canonical()
            .unwrap();
        let context = launch_context("another-project", "desktop", encoded);

        assert!(matches!(
            decode_source_receipt(&context),
            Err(ProductSupervisedDesktopBootstrapError::InvalidSourceReceipt)
        ));
    }

    #[test]
    fn every_source_fact_is_reproved_through_the_project_capability() {
        let project = TestProject::new();
        let capability = project.capability();
        let receipt = source_receipt("project-id", "desktop");

        verify_source_facts(&capability, &receipt).unwrap();

        fs::write(
            project.root.join("scenes/main.scene.json"),
            b"changed scene",
        )
        .unwrap();
        assert!(matches!(
            verify_source_facts(&capability, &receipt),
            Err(ProductSupervisedDesktopBootstrapError::SourceFactVerification { index: 1, .. })
        ));
    }

    #[cfg(windows)]
    fn host_directory(path: &Path) -> File {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(not(windows))]
    fn host_directory(path: &Path) -> File {
        File::open(path).unwrap()
    }
}
