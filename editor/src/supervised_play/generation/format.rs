use std::fmt::{self, Debug, Formatter};

use nara::fs::{ContentDigest, RelativePath};

use super::store::GenerationStoreError;

const MAX_SOURCE_RECEIPT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_LAUNCH_MANIFEST_BYTES: usize = 512 * 1024;
const MAX_TEXT_FIELD_BYTES: usize = 8 * 1024;
const MAX_FEATURE_BYTES: usize = 256;
const MAX_FEATURES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ExecutableGenerationId([u8; 32]);

impl ExecutableGenerationId {
    #[must_use]
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn hex(self) -> String {
        hex_bytes(&self.0)
    }
}

impl Debug for ExecutableGenerationId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ExecutableGenerationId")
            .field(&self.hex())
            .finish()
    }
}

pub(crate) struct GenerationSourceFacts {
    execution_contract_version: String,
    launch_context_version: String,
    project_identity: String,
    profile: String,
    source_receipt: Vec<u8>,
}

impl GenerationSourceFacts {
    pub(crate) fn new(
        execution_contract_version: impl Into<String>,
        launch_context_version: impl Into<String>,
        project_identity: impl Into<String>,
        profile: impl Into<String>,
        source_receipt: Vec<u8>,
    ) -> Self {
        Self {
            execution_contract_version: execution_contract_version.into(),
            launch_context_version: launch_context_version.into(),
            project_identity: project_identity.into(),
            profile: profile.into(),
            source_receipt,
        }
    }
}

pub(crate) struct GenerationTargetFacts {
    package: GenerationPackageFacts,
    target_name: String,
    target_triple: String,
    cargo_profile: String,
    features: Vec<String>,
}

impl GenerationTargetFacts {
    pub(crate) fn new(
        package: GenerationPackageFacts,
        target_name: impl Into<String>,
        target_triple: impl Into<String>,
        cargo_profile: impl Into<String>,
        features: Vec<String>,
    ) -> Self {
        Self {
            package,
            target_name: target_name.into(),
            target_triple: target_triple.into(),
            cargo_profile: cargo_profile.into(),
            features,
        }
    }
}

pub(crate) struct GenerationPackageFacts {
    identity_digest: ContentDigest,
    name: String,
    version: String,
    manifest_relative_path: String,
}

impl GenerationPackageFacts {
    pub(crate) fn new(
        identity_digest: ContentDigest,
        name: impl Into<String>,
        version: impl Into<String>,
        manifest_relative_path: impl Into<String>,
    ) -> Self {
        Self {
            identity_digest,
            name: name.into(),
            version: version.into(),
            manifest_relative_path: manifest_relative_path.into(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GenerationBuildObservation {
    manifest_digest: ContentDigest,
    lockfile_digest: Option<ContentDigest>,
    cargo_metadata_digest: ContentDigest,
    cargo_version_digest: ContentDigest,
    rustc_verbose_version_digest: ContentDigest,
    build_invocation_digest: ContentDigest,
}

impl GenerationBuildObservation {
    pub(crate) const fn new(
        manifest_digest: ContentDigest,
        lockfile_digest: Option<ContentDigest>,
        cargo_metadata_digest: ContentDigest,
        cargo_version_digest: ContentDigest,
        rustc_verbose_version_digest: ContentDigest,
        build_invocation_digest: ContentDigest,
    ) -> Self {
        Self {
            manifest_digest,
            lockfile_digest,
            cargo_metadata_digest,
            cargo_version_digest,
            rustc_verbose_version_digest,
            build_invocation_digest,
        }
    }
}

pub(crate) struct GenerationManifestInput {
    source: GenerationSourceFacts,
    target: GenerationTargetFacts,
    build: GenerationBuildObservation,
}

impl GenerationManifestInput {
    pub(crate) const fn new(
        source: GenerationSourceFacts,
        target: GenerationTargetFacts,
        build: GenerationBuildObservation,
    ) -> Self {
        Self {
            source,
            target,
            build,
        }
    }

    pub(crate) fn source_receipt_digest(&self) -> ContentDigest {
        ContentDigest::of_bytes(&self.source.source_receipt)
    }
}

pub(crate) fn generation_id(
    store_id: &str,
    sequence: u64,
    executable_digest: ContentDigest,
) -> ExecutableGenerationId {
    let mut bytes = Vec::with_capacity(store_id.len() + 8 + 8 + 32);
    bytes.extend_from_slice(store_id.as_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&executable_digest.length().to_le_bytes());
    bytes.extend_from_slice(executable_digest.as_bytes());
    ExecutableGenerationId(*ContentDigest::of_bytes(&bytes).as_bytes())
}

pub(crate) fn encode_launch_manifest(
    generation: ExecutableGenerationId,
    executable_digest: ContentDigest,
    input: &GenerationManifestInput,
) -> Result<Vec<u8>, GenerationStoreError> {
    if input.source.source_receipt.is_empty() {
        return Err(GenerationStoreError::SourceReceiptEmpty);
    }
    if input.source.source_receipt.len() > MAX_SOURCE_RECEIPT_BYTES {
        return Err(GenerationStoreError::SourceReceiptTooLarge);
    }
    if input.target.features.len() > MAX_FEATURES {
        return Err(GenerationStoreError::TooManyFeatures);
    }
    if input.target.package.manifest_relative_path.contains('\\')
        || RelativePath::new(&input.target.package.manifest_relative_path).is_err()
    {
        return Err(GenerationStoreError::ManifestRelativePathInvalid);
    }
    let mut features = input
        .target
        .features
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    features.sort_unstable();
    features.dedup();

    let mut encoder = ManifestEncoder::new();
    encoder.write_raw(b"NARA_SPL_GENERATION\0")?;
    encoder.write_u16(1)?;
    encoder.write_raw(generation.as_bytes())?;
    encoder.write_digest(executable_digest)?;
    encoder.write_text(
        "execution-contract-version",
        &input.source.execution_contract_version,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "launch-context-version",
        &input.source.launch_context_version,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "project-identity",
        &input.source.project_identity,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text("profile", &input.source.profile, MAX_TEXT_FIELD_BYTES)?;
    encoder.write_digest(input.target.package.identity_digest)?;
    encoder.write_text(
        "package-name",
        &input.target.package.name,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "package-version",
        &input.target.package.version,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "manifest-relative-path",
        &input.target.package.manifest_relative_path,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "target-name",
        &input.target.target_name,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "target-triple",
        &input.target.target_triple,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_text(
        "cargo-profile",
        &input.target.cargo_profile,
        MAX_TEXT_FIELD_BYTES,
    )?;
    encoder.write_u16(
        u16::try_from(features.len()).map_err(|_| GenerationStoreError::TooManyFeatures)?,
    )?;
    for feature in features {
        encoder.write_text("feature", feature, MAX_FEATURE_BYTES)?;
    }
    encoder.write_digest(input.build.manifest_digest)?;
    match input.build.lockfile_digest {
        Some(digest) => {
            encoder.write_u8(1)?;
            encoder.write_digest(digest)?;
        }
        None => encoder.write_u8(0)?,
    }
    encoder.write_digest(input.build.cargo_metadata_digest)?;
    encoder.write_digest(input.build.cargo_version_digest)?;
    encoder.write_digest(input.build.rustc_verbose_version_digest)?;
    encoder.write_digest(input.build.build_invocation_digest)?;
    encoder.write_bytes(&input.source.source_receipt)?;
    encoder.finish()
}

struct ManifestEncoder {
    bytes: Vec<u8>,
}

impl ManifestEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn write_u8(&mut self, value: u8) -> Result<(), GenerationStoreError> {
        self.write_raw(&[value])
    }

    fn write_u16(&mut self, value: u16) -> Result<(), GenerationStoreError> {
        self.write_raw(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> Result<(), GenerationStoreError> {
        self.write_raw(&value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) -> Result<(), GenerationStoreError> {
        self.write_raw(&value.to_le_bytes())
    }

    fn write_digest(&mut self, digest: ContentDigest) -> Result<(), GenerationStoreError> {
        self.write_u64(digest.length())?;
        self.write_raw(digest.as_bytes())
    }

    fn write_text(
        &mut self,
        field: &'static str,
        value: &str,
        limit: usize,
    ) -> Result<(), GenerationStoreError> {
        if value.is_empty() {
            return Err(GenerationStoreError::ManifestTextEmpty { field });
        }
        if value.len() > limit {
            return Err(GenerationStoreError::ManifestTextTooLarge { field, limit });
        }
        self.write_bytes(value.as_bytes())
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), GenerationStoreError> {
        let length =
            u32::try_from(value.len()).map_err(|_| GenerationStoreError::LaunchManifestTooLarge)?;
        self.write_u32(length)?;
        self.write_raw(value)
    }

    fn write_raw(&mut self, value: &[u8]) -> Result<(), GenerationStoreError> {
        let next_len = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(GenerationStoreError::LaunchManifestTooLarge)?;
        if next_len > MAX_LAUNCH_MANIFEST_BYTES {
            return Err(GenerationStoreError::LaunchManifestTooLarge);
        }
        self.bytes
            .try_reserve(value.len())
            .map_err(|_| GenerationStoreError::AllocationFailed)?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn finish(self) -> Result<Vec<u8>, GenerationStoreError> {
        if self.bytes.len() > MAX_LAUNCH_MANIFEST_BYTES {
            Err(GenerationStoreError::LaunchManifestTooLarge)
        } else {
            Ok(self.bytes)
        }
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
