use std::{
    io,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    project_host::{
        MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES, SupervisedDesktopLaunchContext,
        SupervisedDesktopReadyAttestation, SupervisedDesktopRuntimeClosureFacts,
        SupervisedDesktopSourceAuthority, SupervisedDesktopWireError,
        supervised_desktop_ready_frame_length,
    },
};
use thiserror::Error;

use super::{
    process::{
        SupervisedBootstrapCancellation, SupervisedBootstrapChannel, SupervisedProcessError,
    },
    wake::SupervisedPlayWake,
};

const READY_READ_CHUNK_BYTES: usize = 8 * 1024;
const READY_READ_BUDGET_PER_ADVANCE: usize = 8;
const BOOTSTRAP_WRITE_RETRY_DELAY: Duration = Duration::from_millis(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SupervisedDesktopRuntimeExpectation {
    manifest_digest: ContentDigest,
    authoring_bundle_digest: ContentDigest,
    schema_fingerprint: String,
    content_digest: ContentDigest,
}

impl SupervisedDesktopRuntimeExpectation {
    pub(super) fn from_source_authority(
        source: &SupervisedDesktopSourceAuthority,
    ) -> Result<Self, SupervisedDesktopWireError> {
        Self::new(
            source.manifest_digest(),
            source.authoring_bundle_digest(),
            hex_bytes(&source.schema_fingerprint()),
            source.content_digest(),
        )
    }

    fn new(
        manifest_digest: ContentDigest,
        authoring_bundle_digest: ContentDigest,
        schema_fingerprint: String,
        content_digest: ContentDigest,
    ) -> Result<Self, SupervisedDesktopWireError> {
        if manifest_digest.length() == 0 {
            return Err(SupervisedDesktopWireError::InvalidField(
                "runtime manifest digest",
            ));
        }
        if authoring_bundle_digest.length() == 0 {
            return Err(SupervisedDesktopWireError::InvalidField(
                "runtime authoring bundle digest",
            ));
        }
        if schema_fingerprint.len() != 64
            || !schema_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(SupervisedDesktopWireError::InvalidField(
                "runtime schema fingerprint",
            ));
        }
        Ok(Self {
            manifest_digest,
            authoring_bundle_digest,
            schema_fingerprint,
            content_digest,
        })
    }

    fn matches(&self, facts: &SupervisedDesktopRuntimeClosureFacts) -> bool {
        facts.manifest_digest() == self.manifest_digest
            && facts.authoring_bundle_digest() == self.authoring_bundle_digest
            && facts.schema_fingerprint() == self.schema_fingerprint.as_str()
            && facts.content_digest() == self.content_digest
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupervisedDesktopHandshakeStatus {
    WritingLaunch,
    AwaitingReady,
    Ready,
    Failed,
}

pub(super) struct SupervisedDesktopHandshakeOwner {
    launch: SupervisedDesktopLaunchContext,
    runtime_expectation: SupervisedDesktopRuntimeExpectation,
    deadline: Instant,
    writer: Option<BootstrapWriter>,
    channel: Option<SupervisedBootstrapChannel>,
    admission: ReadyAdmission,
    admitted: bool,
    ready: Option<SupervisedDesktopReadyAttestation>,
    failure: Option<SupervisedDesktopHandshakeFailure>,
}

impl SupervisedDesktopHandshakeOwner {
    pub(super) fn start(
        channel: SupervisedBootstrapChannel,
        launch: SupervisedDesktopLaunchContext,
        runtime_expectation: SupervisedDesktopRuntimeExpectation,
        deadline: Instant,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedDesktopHandshakeStartError> {
        let encoded = launch
            .encode_canonical()
            .map_err(SupervisedDesktopHandshakeStartError::Wire)?;
        let writer = BootstrapWriter::start(channel, encoded, wake)
            .map_err(SupervisedDesktopHandshakeStartError::WriterSpawn)?;
        Ok(Self {
            launch,
            runtime_expectation,
            deadline,
            writer: Some(writer),
            channel: None,
            admission: ReadyAdmission::new(),
            admitted: false,
            ready: None,
            failure: None,
        })
    }

    pub(super) fn status(&self) -> SupervisedDesktopHandshakeStatus {
        if self.admitted {
            SupervisedDesktopHandshakeStatus::Ready
        } else if self.failure.is_some() {
            SupervisedDesktopHandshakeStatus::Failed
        } else if self.writer.is_some() {
            SupervisedDesktopHandshakeStatus::WritingLaunch
        } else {
            SupervisedDesktopHandshakeStatus::AwaitingReady
        }
    }

    pub(super) fn failure(&self) -> Option<&SupervisedDesktopHandshakeFailure> {
        self.failure.as_ref()
    }

    pub(super) fn take_ready(&mut self) -> Option<SupervisedDesktopReadyAttestation> {
        self.ready.take()
    }

    pub(super) fn request_cancel(&mut self) -> Result<(), SupervisedProcessError> {
        let cancellation = self
            .writer
            .as_ref()
            .map_or(Ok(()), BootstrapWriter::request_cancel);
        if !self.admitted && self.failure.is_none() {
            self.failure = Some(SupervisedDesktopHandshakeFailure::Cancelled);
            self.channel = None;
        }
        cancellation
    }

    pub(super) fn writer_stopped(&self) -> bool {
        self.writer.is_none()
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        (!self.admitted && self.failure.is_none()).then_some(self.deadline)
    }

    pub(super) fn advance(&mut self, now: Instant) {
        self.poll_writer();

        if !self.admitted && self.failure.is_none() && now >= self.deadline {
            self.fail(SupervisedDesktopHandshakeFailure::ReadyTimeout);
        }

        if self.failure.is_none() && !self.admitted && self.writer.is_none() {
            self.poll_ready();
        }
    }

    fn poll_writer(&mut self) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let result = match writer.try_finish() {
            Ok(Some(result)) => result,
            Ok(None) => return,
            Err(failure) => {
                self.writer = None;
                self.fail(failure);
                return;
            }
        };
        self.writer = None;
        let (channel, write_result) = result;
        match write_result {
            Ok(()) if self.failure.is_none() => self.channel = Some(channel),
            Ok(()) => drop(channel),
            Err(source) => {
                drop(channel);
                self.fail(SupervisedDesktopHandshakeFailure::Write(source));
            }
        }
    }

    fn poll_ready(&mut self) {
        let mut scratch = [0_u8; READY_READ_CHUNK_BYTES];
        for _ in 0..READY_READ_BUDGET_PER_ADVANCE {
            let read = match self
                .channel
                .as_mut()
                .expect("a completed bootstrap writer returns its channel")
                .try_read(&mut scratch)
            {
                Ok(0) => return,
                Ok(read) => read,
                Err(SupervisedProcessError::Bootstrap(source))
                    if matches!(
                        source.kind(),
                        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    match self
                        .admission
                        .observe_eof(&self.launch, &self.runtime_expectation)
                    {
                        Ok(ready) => {
                            self.admitted = true;
                            self.ready = Some(ready);
                            self.channel = None;
                        }
                        Err(failure) => self.fail(failure),
                    }
                    return;
                }
                Err(source) => {
                    self.fail(SupervisedDesktopHandshakeFailure::Read(source));
                    return;
                }
            };

            if let Err(failure) = self.admission.push(&scratch[..read]) {
                self.fail(failure);
                return;
            }
        }
    }

    fn fail(&mut self, failure: SupervisedDesktopHandshakeFailure) {
        if !self.admitted && self.failure.is_none() {
            if let Some(writer) = self.writer.as_ref() {
                let _ = writer.request_cancel();
            }
            self.failure = Some(failure);
            self.channel = None;
        }
    }
}

struct BootstrapWriter {
    cancellation: SupervisedBootstrapCancellation,
    receiver: Receiver<BootstrapWriteResult>,
    thread: Option<JoinHandle<()>>,
}

type BootstrapWriteResult = (
    SupervisedBootstrapChannel,
    Result<(), SupervisedProcessError>,
);

impl BootstrapWriter {
    fn start(
        mut channel: SupervisedBootstrapChannel,
        bytes: Vec<u8>,
        wake: SupervisedPlayWake,
    ) -> Result<Self, io::Error> {
        let cancellation = channel.cancellation();
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("nara-supervised-bootstrap-writer".to_owned())
            .spawn(move || {
                // The platform token cancels a pending Windows overlapped write and wakes the
                // bounded Linux retry loop. The supervisor does not publish cleanup while this
                // owner still retains its JoinHandle.
                let mut written = 0_usize;
                let result = loop {
                    if written == bytes.len() {
                        break Ok(());
                    }
                    match channel.write_some(&bytes[written..]) {
                        Ok(0) => thread::sleep(BOOTSTRAP_WRITE_RETRY_DELAY),
                        Ok(count) => written += count,
                        Err(error) => break Err(error),
                    }
                };
                let _ = sender.send((channel, result));
                wake.notify();
            })?;
        Ok(Self {
            cancellation,
            receiver,
            thread: Some(thread),
        })
    }

    fn request_cancel(&self) -> Result<(), SupervisedProcessError> {
        self.cancellation.cancel()
    }

    fn try_finish(
        &mut self,
    ) -> Result<Option<BootstrapWriteResult>, SupervisedDesktopHandshakeFailure> {
        match self.receiver.try_recv() {
            Ok(result) => {
                self.join()?;
                Ok(Some(result))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.join()?;
                Err(SupervisedDesktopHandshakeFailure::WriterDisconnected)
            }
        }
    }

    fn join(&mut self) -> Result<(), SupervisedDesktopHandshakeFailure> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| SupervisedDesktopHandshakeFailure::WriterPanicked)
    }
}

impl Drop for BootstrapWriter {
    fn drop(&mut self) {
        let _ = self.cancellation.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct ReadyAdmission {
    bytes: Vec<u8>,
    frame_length: Option<usize>,
}

impl ReadyAdmission {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            frame_length: None,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), SupervisedDesktopHandshakeFailure> {
        let next_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(SupervisedDesktopHandshakeFailure::ReadyTooLarge)?;
        if next_len > MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES {
            return Err(SupervisedDesktopHandshakeFailure::ReadyTooLarge);
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| SupervisedDesktopHandshakeFailure::AllocationFailed)?;
        self.bytes.extend_from_slice(bytes);
        if self.frame_length.is_none() {
            self.frame_length = supervised_desktop_ready_frame_length(&self.bytes)
                .map_err(SupervisedDesktopHandshakeFailure::Wire)?;
        }
        if self
            .frame_length
            .is_some_and(|frame_length| self.bytes.len() > frame_length)
        {
            return Err(SupervisedDesktopHandshakeFailure::DuplicateOrTrailingReady);
        }
        Ok(())
    }

    fn observe_eof(
        &self,
        launch: &SupervisedDesktopLaunchContext,
        runtime_expectation: &SupervisedDesktopRuntimeExpectation,
    ) -> Result<SupervisedDesktopReadyAttestation, SupervisedDesktopHandshakeFailure> {
        let Some(frame_length) = self.frame_length else {
            return Err(SupervisedDesktopHandshakeFailure::EofBeforeReady);
        };
        if self.bytes.len() != frame_length {
            return Err(SupervisedDesktopHandshakeFailure::EofBeforeReady);
        }
        let ready = SupervisedDesktopReadyAttestation::decode_canonical(&self.bytes)
            .map_err(SupervisedDesktopHandshakeFailure::Wire)?;
        if !ready.matches_launch(launch) {
            return Err(SupervisedDesktopHandshakeFailure::LaunchFactsMismatch);
        }
        if !runtime_expectation.matches(ready.runtime_closure()) {
            return Err(SupervisedDesktopHandshakeFailure::RuntimeFactsMismatch);
        }
        Ok(ready)
    }
}

#[derive(Debug, Error)]
pub(super) enum SupervisedDesktopHandshakeStartError {
    #[error("the supervised desktop launch context is invalid")]
    Wire(#[source] SupervisedDesktopWireError),
    #[error("the supervised desktop bootstrap writer could not start")]
    WriterSpawn(#[source] io::Error),
}

#[derive(Debug, Error)]
pub(super) enum SupervisedDesktopHandshakeFailure {
    #[error("the supervised desktop bootstrap writer disconnected without a result")]
    WriterDisconnected,
    #[error("the supervised desktop bootstrap writer panicked")]
    WriterPanicked,
    #[error("the supervised desktop launch context could not be written")]
    Write(#[source] SupervisedProcessError),
    #[error("the supervised desktop READY endpoint could not be read")]
    Read(#[source] SupervisedProcessError),
    #[error("the supervised desktop READY record is invalid")]
    Wire(#[source] SupervisedDesktopWireError),
    #[error("the supervised desktop READY record exceeded its byte limit")]
    ReadyTooLarge,
    #[error("the supervised desktop READY record allocation failed")]
    AllocationFailed,
    #[error("the supervised desktop endpoint carried a duplicate or trailing READY record")]
    DuplicateOrTrailingReady,
    #[error("the supervised desktop endpoint closed before one complete READY record")]
    EofBeforeReady,
    #[error("the supervised desktop READY record did not echo the exact launch facts")]
    LaunchFactsMismatch,
    #[error("the supervised desktop READY record did not attest the expected Runtime facts")]
    RuntimeFactsMismatch,
    #[error("the supervised desktop child did not close one exact READY record before deadline")]
    ReadyTimeout,
    #[error("the supervised desktop handshake was cancelled")]
    Cancelled,
}

impl SupervisedDesktopHandshakeFailure {
    pub(super) const fn kind(&self) -> SupervisedDesktopHandshakeFailureKind {
        match self {
            Self::WriterDisconnected | Self::WriterPanicked => {
                SupervisedDesktopHandshakeFailureKind::Writer
            }
            Self::Write(_) => SupervisedDesktopHandshakeFailureKind::Write,
            Self::Read(_) => SupervisedDesktopHandshakeFailureKind::Read,
            Self::Wire(_) => SupervisedDesktopHandshakeFailureKind::Wire,
            Self::ReadyTooLarge => SupervisedDesktopHandshakeFailureKind::ReadyTooLarge,
            Self::AllocationFailed => SupervisedDesktopHandshakeFailureKind::AllocationFailed,
            Self::DuplicateOrTrailingReady => {
                SupervisedDesktopHandshakeFailureKind::DuplicateOrTrailingReady
            }
            Self::EofBeforeReady => SupervisedDesktopHandshakeFailureKind::EofBeforeReady,
            Self::LaunchFactsMismatch => SupervisedDesktopHandshakeFailureKind::LaunchFactsMismatch,
            Self::RuntimeFactsMismatch => {
                SupervisedDesktopHandshakeFailureKind::RuntimeFactsMismatch
            }
            Self::ReadyTimeout => SupervisedDesktopHandshakeFailureKind::ReadyTimeout,
            Self::Cancelled => SupervisedDesktopHandshakeFailureKind::Cancelled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupervisedDesktopHandshakeFailureKind {
    Writer,
    Write,
    Read,
    Wire,
    ReadyTooLarge,
    AllocationFailed,
    DuplicateOrTrailingReady,
    EofBeforeReady,
    LaunchFactsMismatch,
    RuntimeFactsMismatch,
    ReadyTimeout,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nara::project_host::{
        SupervisedDesktopGenerationId, SupervisedDesktopProductIdentity,
        SupervisedDesktopSessionNonce,
    };

    use super::*;

    #[cfg(windows)]
    const PROJECT_ROOT: &str = r"C:\source\game";
    #[cfg(not(windows))]
    const PROJECT_ROOT: &str = "/source/game";

    fn digest(seed: u8) -> ContentDigest {
        ContentDigest::of_bytes(&[seed; 32])
    }

    fn product_identity() -> SupervisedDesktopProductIdentity {
        SupervisedDesktopProductIdentity::from_cargo_product(
            "nara-test-product",
            "0.1.0",
            "desktop",
        )
        .unwrap()
    }

    fn launch() -> SupervisedDesktopLaunchContext {
        SupervisedDesktopLaunchContext::new(
            PathBuf::from(PROJECT_ROOT),
            "project-id",
            "desktop",
            product_identity(),
            SupervisedDesktopGenerationId::new([7; 32]).unwrap(),
            SupervisedDesktopSessionNonce::new([9; 32]).unwrap(),
            digest(1),
            digest(5),
            b"canonical-source-receipt".to_vec(),
        )
        .unwrap()
    }

    fn runtime_facts() -> SupervisedDesktopRuntimeClosureFacts {
        SupervisedDesktopRuntimeClosureFacts::new(
            digest(2),
            digest(6),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            digest(3),
            digest(4),
        )
        .unwrap()
    }

    fn expectation() -> SupervisedDesktopRuntimeExpectation {
        SupervisedDesktopRuntimeExpectation::new(
            digest(2),
            digest(6),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            digest(3),
        )
        .unwrap()
    }

    #[test]
    fn source_expectation_compares_only_source_derived_runtime_facts() {
        let expectation = expectation();

        assert!(expectation.matches(&runtime_facts()));
        assert!(
            !expectation.matches(
                &SupervisedDesktopRuntimeClosureFacts::new(
                    digest(99),
                    digest(6),
                    runtime_facts().schema_fingerprint(),
                    digest(3),
                    digest(4),
                )
                .unwrap()
            )
        );
        assert!(
            !expectation.matches(
                &SupervisedDesktopRuntimeClosureFacts::new(
                    digest(2),
                    digest(99),
                    runtime_facts().schema_fingerprint(),
                    digest(3),
                    digest(4),
                )
                .unwrap()
            )
        );
        assert!(
            !expectation.matches(
                &SupervisedDesktopRuntimeClosureFacts::new(
                    digest(2),
                    digest(6),
                    "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
                    digest(3),
                    digest(4),
                )
                .unwrap()
            )
        );
        assert!(
            !expectation.matches(
                &SupervisedDesktopRuntimeClosureFacts::new(
                    digest(2),
                    digest(6),
                    runtime_facts().schema_fingerprint(),
                    digest(99),
                    digest(4),
                )
                .unwrap()
            )
        );
        assert!(
            expectation.matches(
                &SupervisedDesktopRuntimeClosureFacts::new(
                    digest(2),
                    digest(6),
                    runtime_facts().schema_fingerprint(),
                    digest(3),
                    digest(99),
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn complete_ready_requires_following_eof_before_admission() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();

        assert_eq!(admission.bytes.len(), admission.frame_length.unwrap());
        assert!(admission.observe_eof(&launch, &expectation()).is_ok());
    }

    #[test]
    fn complete_ready_held_open_until_the_deadline_is_never_admitted() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();
        let deadline = Instant::now();
        let mut owner = SupervisedDesktopHandshakeOwner {
            launch,
            runtime_expectation: expectation(),
            deadline,
            writer: None,
            channel: None,
            admission,
            admitted: false,
            ready: None,
            failure: None,
        };

        owner.advance(deadline);
        assert_eq!(owner.status(), SupervisedDesktopHandshakeStatus::Failed);
        assert!(matches!(
            owner.failure(),
            Some(SupervisedDesktopHandshakeFailure::ReadyTimeout)
        ));
    }

    #[test]
    fn taking_ready_payload_does_not_reverse_the_terminal_admission_marker() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap();
        let mut owner = SupervisedDesktopHandshakeOwner {
            launch,
            runtime_expectation: expectation(),
            deadline: Instant::now(),
            writer: None,
            channel: None,
            admission: ReadyAdmission::new(),
            admitted: true,
            ready: Some(ready),
            failure: None,
        };

        assert!(owner.take_ready().is_some());
        assert_eq!(owner.status(), SupervisedDesktopHandshakeStatus::Ready);
        owner.advance(Instant::now());
        assert_eq!(owner.status(), SupervisedDesktopHandshakeStatus::Ready);
    }

    #[test]
    fn duplicate_or_trailing_ready_fails_closed() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();

        assert!(admission.push(&ready).is_ok());
        assert!(matches!(
            admission.push(&[0]),
            Err(SupervisedDesktopHandshakeFailure::DuplicateOrTrailingReady)
        ));
    }

    #[test]
    fn eof_before_a_complete_ready_fails_closed() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready[..ready.len() - 1]).unwrap();

        assert!(matches!(
            admission.observe_eof(&launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::EofBeforeReady)
        ));
    }

    #[test]
    fn mismatched_launch_or_runtime_facts_never_admit() {
        let expected_launch = launch();
        let different_launch = SupervisedDesktopLaunchContext::new(
            PathBuf::from(PROJECT_ROOT),
            "different-project",
            "desktop",
            product_identity(),
            SupervisedDesktopGenerationId::new([7; 32]).unwrap(),
            SupervisedDesktopSessionNonce::new([9; 32]).unwrap(),
            digest(1),
            digest(5),
            b"canonical-source-receipt".to_vec(),
        )
        .unwrap();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &different_launch,
            product_identity(),
            different_launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();
        assert!(matches!(
            admission.observe_eof(&expected_launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::LaunchFactsMismatch)
        ));

        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &expected_launch,
            product_identity(),
            expected_launch.executable_digest(),
            SupervisedDesktopRuntimeClosureFacts::new(
                digest(8),
                digest(6),
                runtime_facts().schema_fingerprint(),
                digest(3),
                digest(4),
            )
            .unwrap(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();
        assert!(matches!(
            admission.observe_eof(&expected_launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::RuntimeFactsMismatch)
        ));
    }

    #[test]
    fn measured_executable_digest_must_match_the_generation_authority() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            digest(99),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();

        assert!(matches!(
            admission.observe_eof(&launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::LaunchFactsMismatch)
        ));
    }

    #[test]
    fn runtime_authoring_bundle_digest_must_match_the_source_authority() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            SupervisedDesktopRuntimeClosureFacts::new(
                digest(2),
                digest(99),
                runtime_facts().schema_fingerprint(),
                digest(3),
                digest(4),
            )
            .unwrap(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();

        assert!(matches!(
            admission.observe_eof(&launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::RuntimeFactsMismatch)
        ));
    }

    #[test]
    fn runtime_closure_plan_digest_remains_child_contributed_activation_evidence() {
        let launch = launch();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            launch.executable_digest(),
            SupervisedDesktopRuntimeClosureFacts::new(
                digest(2),
                digest(6),
                runtime_facts().schema_fingerprint(),
                digest(3),
                digest(99),
            )
            .unwrap(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();

        let admitted = admission.observe_eof(&launch, &expectation()).unwrap();
        assert_eq!(admitted.runtime_closure().closure_plan_digest(), digest(99));
    }

    #[test]
    fn complete_canonical_source_receipt_must_match_not_only_its_other_launch_facts() {
        let expected_launch = launch();
        let different_source_launch = SupervisedDesktopLaunchContext::new(
            PathBuf::from(PROJECT_ROOT),
            expected_launch.project_stable_id(),
            expected_launch.profile_name(),
            product_identity(),
            expected_launch.generation_id(),
            expected_launch.session_nonce(),
            expected_launch.executable_digest(),
            expected_launch.launch_manifest_digest(),
            b"different-canonical-source-receipt".to_vec(),
        )
        .unwrap();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &different_source_launch,
            product_identity(),
            different_source_launch.executable_digest(),
            runtime_facts(),
        )
        .unwrap()
        .encode_canonical()
        .unwrap();
        let mut admission = ReadyAdmission::new();
        admission.push(&ready).unwrap();

        assert!(matches!(
            admission.observe_eof(&expected_launch, &expectation()),
            Err(SupervisedDesktopHandshakeFailure::LaunchFactsMismatch)
        ));
    }
}
