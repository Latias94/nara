//! Launch-context values for supervised desktop product activation.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use nara_fs::ContentDigest;
use nara_project::DESKTOP_SUPERVISED_PLAY_V1_CONTRACT;

use super::super::{
    root::validate_absolute_path, source::MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
};
use super::{
    MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES, SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
    SupervisedDesktopWireError,
    codec::{
        FrameEncoder, FrameReader, decode_frame, encode_frame, validate_executable_digest,
        validate_project_root_wire_size, validate_text,
    },
};

pub(crate) const SUPERVISED_DESKTOP_LAUNCH_MAGIC: &[u8] = b"NARA_SPL_LAUNCH\0";
pub(crate) const MAX_PROJECT_IDENTITY_BYTES: usize = 128;
pub(crate) const MAX_PRODUCT_IDENTITY_BYTES: usize = 256;
pub(super) const MAX_EXECUTION_CONTRACT_BYTES: usize = 64;
pub(super) const MAX_PROFILE_NAME_BYTES: usize = 128;

/// Fresh per-launch identity carried only by one supervised desktop session.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SupervisedDesktopSessionNonce([u8; 32]);

impl SupervisedDesktopSessionNonce {
    pub fn new(bytes: [u8; 32]) -> Result<Self, SupervisedDesktopWireError> {
        if bytes == [0; 32] {
            return Err(SupervisedDesktopWireError::InvalidField("session nonce"));
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SupervisedDesktopSessionNonce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupervisedDesktopSessionNonce(..)")
    }
}

/// Immutable executable generation selected for one supervised desktop session.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SupervisedDesktopGenerationId([u8; 32]);

impl SupervisedDesktopGenerationId {
    pub fn new(bytes: [u8; 32]) -> Result<Self, SupervisedDesktopWireError> {
        if bytes == [0; 32] {
            return Err(SupervisedDesktopWireError::InvalidField(
                "executable generation identity",
            ));
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SupervisedDesktopGenerationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupervisedDesktopGenerationId(..)")
    }
}

/// Closed identity of the compiled desktop product that contributes READY.
#[doc(hidden)]
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SupervisedDesktopProductIdentity(String);

impl SupervisedDesktopProductIdentity {
    pub(super) fn new(value: impl Into<String>) -> Result<Self, SupervisedDesktopWireError> {
        let value = value.into();
        validate_text(&value, MAX_PRODUCT_IDENTITY_BYTES, "product identity")?;
        validate_cargo_product_identity(&value)?;
        Ok(Self(value))
    }

    /// Constructs the canonical identity shared by a Cargo-selected generation and its child.
    pub fn from_cargo_product(
        package_name: &str,
        package_version: &str,
        binary_target_name: &str,
    ) -> Result<Self, SupervisedDesktopWireError> {
        validate_cargo_identity_segment(package_name, "Cargo package name", |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
        })?;
        validate_cargo_identity_segment(package_version, "Cargo package version", |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')
        })?;
        validate_cargo_identity_segment(binary_target_name, "Cargo binary target name", |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
        })?;
        Self::new(format!(
            "cargo-product/v1/{package_name}@{package_version}/{binary_target_name}"
        ))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SupervisedDesktopProductIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SupervisedDesktopProductIdentity")
            .field(&self.0)
            .finish()
    }
}

/// Bounded, capability-free value sent to one supervised desktop child before Runtime startup.
///
/// The absolute project root belongs to the per-launch session envelope. The persistent source
/// receipt remains opaque canonical bytes and contains no absolute root, capability, native handle,
/// or process identity.
#[doc(hidden)]
#[derive(Clone, PartialEq, Eq)]
pub struct SupervisedDesktopLaunchContext {
    pub(super) project_root: PathBuf,
    pub(super) project_stable_id: String,
    pub(super) profile_name: String,
    pub(super) product_identity: SupervisedDesktopProductIdentity,
    pub(super) generation_id: SupervisedDesktopGenerationId,
    pub(super) session_nonce: SupervisedDesktopSessionNonce,
    pub(super) executable_digest: ContentDigest,
    pub(super) launch_manifest_digest: ContentDigest,
    pub(super) source_receipt_digest: ContentDigest,
    pub(super) source_receipt: Box<[u8]>,
}

impl SupervisedDesktopLaunchContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_root: PathBuf,
        project_stable_id: impl Into<String>,
        profile_name: impl Into<String>,
        product_identity: SupervisedDesktopProductIdentity,
        generation_id: SupervisedDesktopGenerationId,
        session_nonce: SupervisedDesktopSessionNonce,
        executable_digest: ContentDigest,
        launch_manifest_digest: ContentDigest,
        source_receipt: Vec<u8>,
    ) -> Result<Self, SupervisedDesktopWireError> {
        let project_root = validate_absolute_path(&project_root, true)
            .map_err(SupervisedDesktopWireError::ProjectRoot)?;
        let project_stable_id = project_stable_id.into();
        validate_text(
            &project_stable_id,
            MAX_PROJECT_IDENTITY_BYTES,
            "project stable identity",
        )?;
        let profile_name = profile_name.into();
        validate_text(&profile_name, MAX_PROFILE_NAME_BYTES, "profile name")?;
        validate_project_root_wire_size(&project_root)?;
        validate_executable_digest(executable_digest, "executable digest")?;
        if launch_manifest_digest.length() == 0 {
            return Err(SupervisedDesktopWireError::InvalidField(
                "launch manifest digest",
            ));
        }
        if source_receipt.is_empty() {
            return Err(SupervisedDesktopWireError::InvalidField("source receipt"));
        }
        if source_receipt.len() > MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES {
            return Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "source receipt",
                limit: MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
            });
        }

        let context = Self {
            project_root,
            project_stable_id,
            profile_name,
            product_identity,
            generation_id,
            session_nonce,
            executable_digest,
            launch_manifest_digest,
            source_receipt_digest: ContentDigest::of_bytes(&source_receipt),
            source_receipt: source_receipt.into_boxed_slice(),
        };
        context.encode_canonical()?;
        Ok(context)
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION
    }

    #[must_use]
    pub const fn execution_contract(&self) -> &'static str {
        DESKTOP_SUPERVISED_PLAY_V1_CONTRACT
    }

    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    #[must_use]
    pub fn project_stable_id(&self) -> &str {
        &self.project_stable_id
    }

    #[must_use]
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    #[must_use]
    pub const fn product_identity(&self) -> &SupervisedDesktopProductIdentity {
        &self.product_identity
    }

    #[must_use]
    pub const fn generation_id(&self) -> SupervisedDesktopGenerationId {
        self.generation_id
    }

    #[must_use]
    pub const fn session_nonce(&self) -> SupervisedDesktopSessionNonce {
        self.session_nonce
    }

    #[must_use]
    pub const fn executable_digest(&self) -> ContentDigest {
        self.executable_digest
    }

    #[must_use]
    pub const fn launch_manifest_digest(&self) -> ContentDigest {
        self.launch_manifest_digest
    }

    #[must_use]
    pub const fn source_receipt_digest(&self) -> ContentDigest {
        self.source_receipt_digest
    }

    #[must_use]
    pub fn source_receipt(&self) -> &[u8] {
        &self.source_receipt
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, SupervisedDesktopWireError> {
        let mut payload = FrameEncoder::new(MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES);
        payload.write_text(
            DESKTOP_SUPERVISED_PLAY_V1_CONTRACT,
            MAX_EXECUTION_CONTRACT_BYTES,
            "execution contract",
        )?;
        payload.write_raw(self.session_nonce.as_bytes())?;
        payload.write_raw(self.generation_id.as_bytes())?;
        payload.write_digest(self.executable_digest)?;
        payload.write_digest(self.launch_manifest_digest)?;
        payload.write_digest(self.source_receipt_digest)?;
        payload.write_text(
            &self.project_stable_id,
            MAX_PROJECT_IDENTITY_BYTES,
            "project stable identity",
        )?;
        payload.write_text(&self.profile_name, MAX_PROFILE_NAME_BYTES, "profile name")?;
        payload.write_text(
            self.product_identity.as_str(),
            MAX_PRODUCT_IDENTITY_BYTES,
            "product identity",
        )?;
        payload.write_path(&self.project_root)?;
        payload.write_bytes(
            &self.source_receipt,
            MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
            "source receipt",
        )?;
        encode_frame(
            SUPERVISED_DESKTOP_LAUNCH_MAGIC,
            SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
            payload.finish(),
            MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
        )
    }

    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, SupervisedDesktopWireError> {
        let payload = decode_frame(
            encoded,
            SUPERVISED_DESKTOP_LAUNCH_MAGIC,
            SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
            MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
        )?;
        let mut reader = FrameReader::new(payload);
        if reader.read_text(MAX_EXECUTION_CONTRACT_BYTES, "execution contract")?
            != DESKTOP_SUPERVISED_PLAY_V1_CONTRACT
        {
            return Err(SupervisedDesktopWireError::FactMismatch(
                "execution contract",
            ));
        }
        let session_nonce = SupervisedDesktopSessionNonce::new(reader.read_array::<32>()?)?;
        let generation_id = SupervisedDesktopGenerationId::new(reader.read_array::<32>()?)?;
        let executable_digest = reader.read_digest()?;
        let launch_manifest_digest = reader.read_digest()?;
        let encoded_source_receipt_digest = reader.read_digest()?;
        let project_stable_id = reader
            .read_text(MAX_PROJECT_IDENTITY_BYTES, "project stable identity")?
            .to_owned();
        let profile_name = reader
            .read_text(MAX_PROFILE_NAME_BYTES, "profile name")?
            .to_owned();
        let product_identity = SupervisedDesktopProductIdentity::new(
            reader
                .read_text(MAX_PRODUCT_IDENTITY_BYTES, "product identity")?
                .to_owned(),
        )?;
        let project_root = reader.read_path()?;
        let source_receipt = reader
            .read_bytes(
                MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
                "source receipt",
            )?
            .to_vec();
        reader.finish()?;

        let context = Self::new(
            project_root,
            project_stable_id,
            profile_name,
            product_identity,
            generation_id,
            session_nonce,
            executable_digest,
            launch_manifest_digest,
            source_receipt,
        )?;
        if context.source_receipt_digest != encoded_source_receipt_digest {
            return Err(SupervisedDesktopWireError::FactMismatch(
                "source receipt digest",
            ));
        }
        if context.encode_canonical()?.as_slice() != encoded {
            return Err(SupervisedDesktopWireError::NonCanonicalEncoding);
        }
        Ok(context)
    }
}

impl fmt::Debug for SupervisedDesktopLaunchContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupervisedDesktopLaunchContext")
            .field("version", &self.version())
            .field("project_root", &self.project_root)
            .field("project_stable_id", &self.project_stable_id)
            .field("profile_name", &self.profile_name)
            .field("product_identity", &self.product_identity)
            .field("generation_id", &self.generation_id)
            .field("session_nonce", &self.session_nonce)
            .field("executable_digest", &self.executable_digest)
            .field("launch_manifest_digest", &self.launch_manifest_digest)
            .field("source_receipt_digest", &self.source_receipt_digest)
            .field("source_receipt_bytes", &self.source_receipt.len())
            .finish()
    }
}

fn validate_cargo_identity_segment(
    value: &str,
    field: &'static str,
    accepted: impl Fn(u8) -> bool,
) -> Result<(), SupervisedDesktopWireError> {
    if value.is_empty() || !value.bytes().all(accepted) {
        return Err(SupervisedDesktopWireError::InvalidField(field));
    }
    if value.len() > MAX_PRODUCT_IDENTITY_BYTES {
        return Err(SupervisedDesktopWireError::FieldTooLarge {
            field,
            limit: MAX_PRODUCT_IDENTITY_BYTES,
        });
    }
    Ok(())
}

fn validate_cargo_product_identity(value: &str) -> Result<(), SupervisedDesktopWireError> {
    let body = value
        .strip_prefix("cargo-product/v1/")
        .ok_or(SupervisedDesktopWireError::InvalidField("product identity"))?;
    let (package, binary_target_name) = body
        .split_once('/')
        .ok_or(SupervisedDesktopWireError::InvalidField("product identity"))?;
    let (package_name, package_version) = package
        .split_once('@')
        .ok_or(SupervisedDesktopWireError::InvalidField("product identity"))?;
    validate_cargo_identity_segment(package_name, "Cargo package name", |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
    })?;
    validate_cargo_identity_segment(package_version, "Cargo package version", |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')
    })?;
    validate_cargo_identity_segment(binary_target_name, "Cargo binary target name", |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
    })
}
