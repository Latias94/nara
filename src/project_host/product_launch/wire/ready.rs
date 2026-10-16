//! READY attestation values for supervised desktop product activation.

use nara_fs::ContentDigest;
use nara_project::DESKTOP_SUPERVISED_PLAY_V1_CONTRACT;

use super::super::source::MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES;
use super::{
    MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES, SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION,
    SupervisedDesktopWireError,
    codec::{
        FrameEncoder, FrameReader, decode_frame, encode_frame, frame_length,
        validate_executable_digest, validate_text,
    },
    launch::{
        MAX_EXECUTION_CONTRACT_BYTES, MAX_PRODUCT_IDENTITY_BYTES, MAX_PROFILE_NAME_BYTES,
        MAX_PROJECT_IDENTITY_BYTES, SupervisedDesktopGenerationId, SupervisedDesktopLaunchContext,
        SupervisedDesktopProductIdentity, SupervisedDesktopSessionNonce,
    },
};

const SUPERVISED_DESKTOP_READY_MAGIC: &[u8] = b"NARA_SPL_READY\0";
const SCHEMA_FINGERPRINT_HEX_BYTES: usize = 64;

/// Strict Runtime closure facts contributed only by the compiled child after startup.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedDesktopRuntimeClosureFacts {
    manifest_digest: ContentDigest,
    authoring_bundle_digest: ContentDigest,
    schema_fingerprint: String,
    content_digest: ContentDigest,
    closure_plan_digest: ContentDigest,
}

impl SupervisedDesktopRuntimeClosureFacts {
    pub fn new(
        manifest_digest: ContentDigest,
        authoring_bundle_digest: ContentDigest,
        schema_fingerprint: impl Into<String>,
        content_digest: ContentDigest,
        closure_plan_digest: ContentDigest,
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
        if closure_plan_digest.length() == 0 {
            return Err(SupervisedDesktopWireError::InvalidField(
                "runtime closure-plan digest",
            ));
        }
        let schema_fingerprint = schema_fingerprint.into();
        validate_schema_fingerprint(&schema_fingerprint)?;
        Ok(Self {
            manifest_digest,
            authoring_bundle_digest,
            schema_fingerprint,
            content_digest,
            closure_plan_digest,
        })
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub const fn authoring_bundle_digest(&self) -> ContentDigest {
        self.authoring_bundle_digest
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> &str {
        &self.schema_fingerprint
    }

    #[must_use]
    pub const fn content_digest(&self) -> ContentDigest {
        self.content_digest
    }

    #[must_use]
    pub const fn closure_plan_digest(&self) -> ContentDigest {
        self.closure_plan_digest
    }
}

/// Canonical one-shot READY value emitted only after one exact playable presentation.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedDesktopReadyAttestation {
    project_stable_id: String,
    profile_name: String,
    product_identity: SupervisedDesktopProductIdentity,
    generation_id: SupervisedDesktopGenerationId,
    session_nonce: SupervisedDesktopSessionNonce,
    executable_digest: ContentDigest,
    launch_manifest_digest: ContentDigest,
    source_receipt_digest: ContentDigest,
    source_receipt: Box<[u8]>,
    runtime_closure: SupervisedDesktopRuntimeClosureFacts,
}

impl SupervisedDesktopReadyAttestation {
    #[must_use]
    pub fn from_launch(
        launch: &SupervisedDesktopLaunchContext,
        product_identity: SupervisedDesktopProductIdentity,
        executable_digest: ContentDigest,
        runtime_closure: SupervisedDesktopRuntimeClosureFacts,
    ) -> Result<Self, SupervisedDesktopWireError> {
        validate_executable_digest(executable_digest, "measured executable digest")?;
        Ok(Self {
            project_stable_id: launch.project_stable_id.clone(),
            profile_name: launch.profile_name.clone(),
            product_identity,
            generation_id: launch.generation_id,
            session_nonce: launch.session_nonce,
            executable_digest,
            launch_manifest_digest: launch.launch_manifest_digest,
            source_receipt_digest: launch.source_receipt_digest,
            source_receipt: launch.source_receipt.clone(),
            runtime_closure,
        })
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION
    }

    #[must_use]
    pub const fn execution_contract(&self) -> &'static str {
        DESKTOP_SUPERVISED_PLAY_V1_CONTRACT
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

    #[must_use]
    pub const fn runtime_closure(&self) -> &SupervisedDesktopRuntimeClosureFacts {
        &self.runtime_closure
    }

    #[must_use]
    pub fn matches_launch(&self, launch: &SupervisedDesktopLaunchContext) -> bool {
        self.project_stable_id == launch.project_stable_id
            && self.profile_name == launch.profile_name
            && self.product_identity == launch.product_identity
            && self.generation_id == launch.generation_id
            && self.session_nonce == launch.session_nonce
            && self.executable_digest == launch.executable_digest
            && self.launch_manifest_digest == launch.launch_manifest_digest
            && self.source_receipt_digest == launch.source_receipt_digest
            && self.source_receipt == launch.source_receipt
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, SupervisedDesktopWireError> {
        let mut payload = FrameEncoder::new(MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES);
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
        payload.write_bytes(
            &self.source_receipt,
            MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
            "source receipt",
        )?;
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
        payload.write_digest(self.runtime_closure.manifest_digest)?;
        payload.write_digest(self.runtime_closure.authoring_bundle_digest)?;
        payload.write_text(
            &self.runtime_closure.schema_fingerprint,
            SCHEMA_FINGERPRINT_HEX_BYTES,
            "schema fingerprint",
        )?;
        payload.write_digest(self.runtime_closure.content_digest)?;
        payload.write_digest(self.runtime_closure.closure_plan_digest)?;
        encode_frame(
            SUPERVISED_DESKTOP_READY_MAGIC,
            SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION,
            payload.finish(),
            MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES,
        )
    }

    pub fn decode_canonical(encoded: &[u8]) -> Result<Self, SupervisedDesktopWireError> {
        let payload = decode_frame(
            encoded,
            SUPERVISED_DESKTOP_READY_MAGIC,
            SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION,
            MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES,
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
        validate_executable_digest(executable_digest, "measured executable digest")?;
        let launch_manifest_digest = reader.read_digest()?;
        if launch_manifest_digest.length() == 0 {
            return Err(SupervisedDesktopWireError::InvalidField(
                "launch manifest digest",
            ));
        }
        let source_receipt_digest = reader.read_digest()?;
        let source_receipt = reader
            .read_bytes(
                MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
                "source receipt",
            )?
            .to_vec()
            .into_boxed_slice();
        if source_receipt.is_empty() {
            return Err(SupervisedDesktopWireError::InvalidField("source receipt"));
        }
        if ContentDigest::of_bytes(&source_receipt) != source_receipt_digest {
            return Err(SupervisedDesktopWireError::FactMismatch(
                "source receipt digest",
            ));
        }
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
        let runtime_closure = SupervisedDesktopRuntimeClosureFacts::new(
            reader.read_digest()?,
            reader.read_digest()?,
            reader
                .read_text(SCHEMA_FINGERPRINT_HEX_BYTES, "schema fingerprint")?
                .to_owned(),
            reader.read_digest()?,
            reader.read_digest()?,
        )?;
        reader.finish()?;

        let ready = Self {
            project_stable_id,
            profile_name,
            product_identity,
            generation_id,
            session_nonce,
            executable_digest,
            launch_manifest_digest,
            source_receipt_digest,
            source_receipt,
            runtime_closure,
        };
        validate_text(
            &ready.project_stable_id,
            MAX_PROJECT_IDENTITY_BYTES,
            "project stable identity",
        )?;
        validate_text(&ready.profile_name, MAX_PROFILE_NAME_BYTES, "profile name")?;
        if ready.encode_canonical()?.as_slice() != encoded {
            return Err(SupervisedDesktopWireError::NonCanonicalEncoding);
        }
        Ok(ready)
    }
}

/// Returns the complete canonical READY frame length once its bounded header is available.
///
/// Callers may use this with nonblocking reads. Bytes after the returned length are a duplicate or
/// trailing record and must fail the one-shot handshake.
#[doc(hidden)]
pub fn supervised_desktop_ready_frame_length(
    bytes: &[u8],
) -> Result<Option<usize>, SupervisedDesktopWireError> {
    frame_length(
        bytes,
        SUPERVISED_DESKTOP_READY_MAGIC,
        SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION,
        MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES,
    )
}

fn validate_schema_fingerprint(value: &str) -> Result<(), SupervisedDesktopWireError> {
    if value.len() != SCHEMA_FINGERPRINT_HEX_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SupervisedDesktopWireError::InvalidField(
            "schema fingerprint",
        ));
    }
    Ok(())
}
