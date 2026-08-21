#![allow(
    dead_code,
    reason = "SPL-U0 publishes the value-only contract consumed by later supervised-play units"
)]

use std::{error::Error, fmt};

use nara_fs::ContentDigest;
use nara_project::{DESKTOP_SUPERVISED_PLAY_V1_CONTRACT, ProjectPath, ProjectPathError};

const SOURCE_RECEIPT_MAGIC: &[u8] = b"nara.editor-external-play-source.v1\0";
const MAX_SOURCE_FACTS: usize = 4_096;
const MAX_IDENTITY_BYTES: usize = 128;
const MAX_PROFILE_BYTES: usize = 128;
const MAX_PROJECT_PATH_BYTES: usize = 4_096;
pub(crate) const MAX_EXTERNAL_PLAY_SOURCE_RECEIPT_BYTES: usize = 256 * 1024;
pub(crate) const EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EditorExternalPlaySourceKind {
    Scene,
    Prefab,
    AssetMetadata,
    AssetSource,
    CargoManifest,
    CargoLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorExternalPlayTarget {
    Desktop,
}

impl EditorExternalPlayTarget {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Desktop => 1,
        }
    }

    fn from_wire_value(value: u8) -> Result<Self, EditorExternalPlaySourceReceiptError> {
        match value {
            1 => Ok(Self::Desktop),
            _ => Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "product target is unsupported",
            )),
        }
    }
}

impl EditorExternalPlaySourceKind {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Scene => 1,
            Self::Prefab => 2,
            Self::AssetMetadata => 3,
            Self::AssetSource => 4,
            Self::CargoManifest => 5,
            Self::CargoLock => 6,
        }
    }

    fn from_wire_value(value: u8) -> Result<Self, EditorExternalPlaySourceReceiptError> {
        match value {
            1 => Ok(Self::Scene),
            2 => Ok(Self::Prefab),
            3 => Ok(Self::AssetMetadata),
            4 => Ok(Self::AssetSource),
            5 => Ok(Self::CargoManifest),
            6 => Ok(Self::CargoLock),
            _ => Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "source kind is unsupported",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorExternalPlaySourceFact {
    kind: EditorExternalPlaySourceKind,
    project_path: ProjectPath,
    digest: ContentDigest,
}

impl EditorExternalPlaySourceFact {
    pub(crate) const fn new(
        kind: EditorExternalPlaySourceKind,
        project_path: ProjectPath,
        digest: ContentDigest,
    ) -> Self {
        Self {
            kind,
            project_path,
            digest,
        }
    }

    pub(crate) const fn kind(&self) -> EditorExternalPlaySourceKind {
        self.kind
    }

    pub(crate) const fn project_path(&self) -> &ProjectPath {
        &self.project_path
    }

    pub(crate) const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorExternalPlaySourceReceipt {
    project_stable_id: String,
    profile_name: String,
    target: EditorExternalPlayTarget,
    launch_context_version: u16,
    manifest_digest: ContentDigest,
    authoring_bundle_digest: ContentDigest,
    content_digest: ContentDigest,
    schema_fingerprint: [u8; 32],
    source_facts: Box<[EditorExternalPlaySourceFact]>,
}

impl EditorExternalPlaySourceReceipt {
    pub(crate) fn new(
        project_stable_id: String,
        profile_name: String,
        target: EditorExternalPlayTarget,
        launch_context_version: u16,
        manifest_digest: ContentDigest,
        authoring_bundle_digest: ContentDigest,
        content_digest: ContentDigest,
        schema_fingerprint: [u8; 32],
        mut source_facts: Vec<EditorExternalPlaySourceFact>,
    ) -> Result<Self, EditorExternalPlaySourceReceiptError> {
        validate_identifier(
            &project_stable_id,
            MAX_IDENTITY_BYTES,
            "project stable identity",
        )?;
        validate_identifier(&profile_name, MAX_PROFILE_BYTES, "profile name")?;
        if launch_context_version == 0 {
            return Err(EditorExternalPlaySourceReceiptError::InvalidField(
                "launch context version",
            ));
        }
        if source_facts.len() > MAX_SOURCE_FACTS {
            return Err(EditorExternalPlaySourceReceiptError::TooManySourceFacts);
        }
        if source_facts
            .iter()
            .any(|fact| fact.project_path().as_str().len() > MAX_PROJECT_PATH_BYTES)
        {
            return Err(EditorExternalPlaySourceReceiptError::InvalidField(
                "source project path",
            ));
        }

        source_facts.sort_by(|left, right| {
            left.project_path()
                .as_str()
                .cmp(right.project_path().as_str())
                .then_with(|| left.kind().cmp(&right.kind()))
        });
        if source_facts
            .windows(2)
            .any(|facts| facts[0].project_path().as_str() == facts[1].project_path().as_str())
        {
            return Err(EditorExternalPlaySourceReceiptError::DuplicateSourcePath);
        }

        let receipt = Self {
            project_stable_id,
            profile_name,
            target,
            launch_context_version,
            manifest_digest,
            authoring_bundle_digest,
            content_digest,
            schema_fingerprint,
            source_facts: source_facts.into_boxed_slice(),
        };
        if receipt.encode_canonical()?.len() > MAX_EXTERNAL_PLAY_SOURCE_RECEIPT_BYTES {
            return Err(EditorExternalPlaySourceReceiptError::EncodedTooLarge);
        }
        Ok(receipt)
    }

    pub(crate) fn project_stable_id(&self) -> &str {
        &self.project_stable_id
    }

    pub(crate) fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub(crate) const fn target(&self) -> EditorExternalPlayTarget {
        self.target
    }

    pub(crate) const fn launch_context_version(&self) -> u16 {
        self.launch_context_version
    }

    pub(crate) const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    pub(crate) const fn authoring_bundle_digest(&self) -> ContentDigest {
        self.authoring_bundle_digest
    }

    pub(crate) const fn content_digest(&self) -> ContentDigest {
        self.content_digest
    }

    pub(crate) const fn schema_fingerprint(&self) -> [u8; 32] {
        self.schema_fingerprint
    }

    pub(crate) fn source_facts(&self) -> &[EditorExternalPlaySourceFact] {
        &self.source_facts
    }

    pub(crate) fn encode_canonical(&self) -> Result<Vec<u8>, EditorExternalPlaySourceReceiptError> {
        let mut encoded = Vec::with_capacity(256 + self.source_facts.len() * 64);
        encoded.extend_from_slice(SOURCE_RECEIPT_MAGIC);
        encoded.push(self.target.wire_value());
        write_string(&mut encoded, DESKTOP_SUPERVISED_PLAY_V1_CONTRACT)?;
        write_string(&mut encoded, &self.project_stable_id)?;
        write_string(&mut encoded, &self.profile_name)?;
        encoded.extend_from_slice(&self.launch_context_version.to_le_bytes());
        write_digest(&mut encoded, self.manifest_digest);
        write_digest(&mut encoded, self.authoring_bundle_digest);
        write_digest(&mut encoded, self.content_digest);
        encoded.extend_from_slice(&self.schema_fingerprint);
        let fact_count = u16::try_from(self.source_facts.len())
            .map_err(|_| EditorExternalPlaySourceReceiptError::TooManySourceFacts)?;
        encoded.extend_from_slice(&fact_count.to_le_bytes());
        for fact in &self.source_facts {
            encoded.push(fact.kind().wire_value());
            write_string(&mut encoded, fact.project_path().as_str())?;
            write_digest(&mut encoded, fact.digest());
        }
        if encoded.len() > MAX_EXTERNAL_PLAY_SOURCE_RECEIPT_BYTES {
            return Err(EditorExternalPlaySourceReceiptError::EncodedTooLarge);
        }
        Ok(encoded)
    }

    pub(crate) fn decode_canonical(
        encoded: &[u8],
    ) -> Result<Self, EditorExternalPlaySourceReceiptError> {
        if encoded.len() > MAX_EXTERNAL_PLAY_SOURCE_RECEIPT_BYTES {
            return Err(EditorExternalPlaySourceReceiptError::EncodedTooLarge);
        }
        let mut reader = ReceiptReader::new(encoded);
        if reader.read_exact(SOURCE_RECEIPT_MAGIC.len())? != SOURCE_RECEIPT_MAGIC {
            return Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "source receipt magic does not match",
            ));
        }
        let target = EditorExternalPlayTarget::from_wire_value(reader.read_u8()?)?;
        if reader.read_string(MAX_IDENTITY_BYTES)? != DESKTOP_SUPERVISED_PLAY_V1_CONTRACT {
            return Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "execution contract is unsupported",
            ));
        }
        let project_stable_id = reader.read_string(MAX_IDENTITY_BYTES)?.to_owned();
        let profile_name = reader.read_string(MAX_PROFILE_BYTES)?.to_owned();
        let launch_context_version = reader.read_u16()?;
        let manifest_digest = reader.read_digest()?;
        let authoring_bundle_digest = reader.read_digest()?;
        let content_digest = reader.read_digest()?;
        let schema_fingerprint = reader.read_array::<32>()?;
        let fact_count = usize::from(reader.read_u16()?);
        if fact_count > MAX_SOURCE_FACTS {
            return Err(EditorExternalPlaySourceReceiptError::TooManySourceFacts);
        }
        let mut source_facts = Vec::with_capacity(fact_count);
        for _ in 0..fact_count {
            let kind = EditorExternalPlaySourceKind::from_wire_value(reader.read_u8()?)?;
            let project_path =
                ProjectPath::new(reader.read_string(MAX_PROJECT_PATH_BYTES)?.to_owned())?;
            let digest = reader.read_digest()?;
            source_facts.push(EditorExternalPlaySourceFact::new(
                kind,
                project_path,
                digest,
            ));
        }
        if !reader.is_finished() {
            return Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "source receipt contains trailing bytes",
            ));
        }

        let receipt = Self::new(
            project_stable_id,
            profile_name,
            target,
            launch_context_version,
            manifest_digest,
            authoring_bundle_digest,
            content_digest,
            schema_fingerprint,
            source_facts,
        )?;
        if receipt.encode_canonical()?.as_slice() != encoded {
            return Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "source receipt is not canonical",
            ));
        }
        Ok(receipt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorExternalPlaySourceReceiptError {
    InvalidField(&'static str),
    TooManySourceFacts,
    DuplicateSourcePath,
    EncodedTooLarge,
    InvalidEncoding(&'static str),
    InvalidProjectPath(ProjectPathError),
}

impl fmt::Display for EditorExternalPlaySourceReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(formatter, "external Play {field} is invalid"),
            Self::TooManySourceFacts => {
                formatter.write_str("external Play source receipt has too many source facts")
            }
            Self::DuplicateSourcePath => formatter
                .write_str("external Play source receipt contains a duplicate project path"),
            Self::EncodedTooLarge => {
                formatter.write_str("external Play source receipt exceeds its byte limit")
            }
            Self::InvalidEncoding(reason) => {
                write!(
                    formatter,
                    "external Play source receipt encoding is invalid: {reason}"
                )
            }
            Self::InvalidProjectPath(error) => {
                write!(formatter, "external Play source path is invalid: {error}")
            }
        }
    }
}

impl Error for EditorExternalPlaySourceReceiptError {}

impl From<ProjectPathError> for EditorExternalPlaySourceReceiptError {
    fn from(error: ProjectPathError) -> Self {
        Self::InvalidProjectPath(error)
    }
}

fn validate_identifier(
    value: &str,
    maximum_bytes: usize,
    field: &'static str,
) -> Result<(), EditorExternalPlaySourceReceiptError> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(EditorExternalPlaySourceReceiptError::InvalidField(field));
    }
    Ok(())
}

fn write_string(
    encoded: &mut Vec<u8>,
    value: &str,
) -> Result<(), EditorExternalPlaySourceReceiptError> {
    let length = u16::try_from(value.len())
        .map_err(|_| EditorExternalPlaySourceReceiptError::EncodedTooLarge)?;
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_digest(encoded: &mut Vec<u8>, digest: ContentDigest) {
    encoded.extend_from_slice(&digest.length().to_le_bytes());
    encoded.extend_from_slice(digest.as_bytes());
}

struct ReceiptReader<'a> {
    encoded: &'a [u8],
    position: usize,
}

impl<'a> ReceiptReader<'a> {
    const fn new(encoded: &'a [u8]) -> Self {
        Self {
            encoded,
            position: 0,
        }
    }

    fn read_exact(
        &mut self,
        length: usize,
    ) -> Result<&'a [u8], EditorExternalPlaySourceReceiptError> {
        let end = self.position.checked_add(length).ok_or(
            EditorExternalPlaySourceReceiptError::InvalidEncoding("length overflow"),
        )?;
        let value = self.encoded.get(self.position..end).ok_or(
            EditorExternalPlaySourceReceiptError::InvalidEncoding("source receipt is truncated"),
        )?;
        self.position = end;
        Ok(value)
    }

    fn read_array<const N: usize>(
        &mut self,
    ) -> Result<[u8; N], EditorExternalPlaySourceReceiptError> {
        self.read_exact(N)?.try_into().map_err(|_| {
            EditorExternalPlaySourceReceiptError::InvalidEncoding("array length does not match")
        })
    }

    fn read_u8(&mut self) -> Result<u8, EditorExternalPlaySourceReceiptError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, EditorExternalPlaySourceReceiptError> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, EditorExternalPlaySourceReceiptError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    fn read_string(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<&'a str, EditorExternalPlaySourceReceiptError> {
        let length = usize::from(self.read_u16()?);
        if length > maximum_bytes {
            return Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(
                "string exceeds its byte limit",
            ));
        }
        std::str::from_utf8(self.read_exact(length)?).map_err(|_| {
            EditorExternalPlaySourceReceiptError::InvalidEncoding("string is not UTF-8")
        })
    }

    fn read_digest(&mut self) -> Result<ContentDigest, EditorExternalPlaySourceReceiptError> {
        let length = self.read_u64()?;
        let hash = self.read_array::<32>()?;
        Ok(ContentDigest::from_parts(length, hash))
    }

    const fn is_finished(&self) -> bool {
        self.position == self.encoded.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_receipt(
        profile: &str,
        launch_context_version: u16,
        schema_fingerprint: [u8; 32],
        scene: &[u8],
    ) -> EditorExternalPlaySourceReceipt {
        EditorExternalPlaySourceReceipt::new(
            "018f27e2-52b8-7bd4-b38f-68cb5725891f".to_owned(),
            profile.to_owned(),
            EditorExternalPlayTarget::Desktop,
            launch_context_version,
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"authoring bundle"),
            ContentDigest::of_bytes(b"content"),
            schema_fingerprint,
            vec![
                EditorExternalPlaySourceFact::new(
                    EditorExternalPlaySourceKind::CargoManifest,
                    ProjectPath::new("Cargo.toml").unwrap(),
                    ContentDigest::of_bytes(b"cargo manifest"),
                ),
                EditorExternalPlaySourceFact::new(
                    EditorExternalPlaySourceKind::Scene,
                    ProjectPath::new("scenes/main.scene.json").unwrap(),
                    ContentDigest::of_bytes(scene),
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn source_receipt_roundtrips_canonically_without_host_authority() {
        let receipt = source_receipt(
            "desktop",
            EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
            [7; 32],
            b"scene",
        );

        let encoded = receipt.encode_canonical().unwrap();
        let decoded = EditorExternalPlaySourceReceipt::decode_canonical(&encoded).unwrap();

        assert_eq!(decoded, receipt);
        assert_eq!(decoded.encode_canonical().unwrap(), encoded);
        assert!(encoded.len() <= MAX_EXTERNAL_PLAY_SOURCE_RECEIPT_BYTES);
        for forbidden in [
            b"F:\\SourceCodes".as_slice(),
            b"DirectoryCapability",
            b"FileIdentity",
        ] {
            assert!(
                !encoded
                    .windows(forbidden.len())
                    .any(|bytes| bytes == forbidden)
            );
        }
    }

    #[test]
    fn source_receipt_identity_includes_profile_schema_content_and_launch_context() {
        let receipt = source_receipt(
            "desktop",
            EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
            [7; 32],
            b"scene",
        );

        assert_ne!(
            receipt,
            source_receipt(
                "release",
                EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
                [7; 32],
                b"scene"
            )
        );
        assert_ne!(
            receipt,
            source_receipt(
                "desktop",
                EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
                [8; 32],
                b"scene"
            )
        );
        assert_ne!(
            receipt,
            source_receipt(
                "desktop",
                EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
                [7; 32],
                b"changed scene"
            )
        );
        assert_ne!(
            receipt,
            source_receipt(
                "desktop",
                EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION + 1,
                [7; 32],
                b"scene"
            )
        );
    }

    #[test]
    fn source_receipt_rejects_absolute_and_noncanonical_source_paths() {
        assert!(ProjectPath::new("F:/project/scenes/main.scene.json").is_err());
        assert!(ProjectPath::new("../scenes/main.scene.json").is_err());

        let receipt = source_receipt(
            "desktop",
            EXTERNAL_PLAY_LAUNCH_CONTEXT_VERSION,
            [7; 32],
            b"scene",
        );
        let mut encoded = receipt.encode_canonical().unwrap();
        encoded.push(0);
        assert!(matches!(
            EditorExternalPlaySourceReceipt::decode_canonical(&encoded),
            Err(EditorExternalPlaySourceReceiptError::InvalidEncoding(_))
        ));
    }
}
