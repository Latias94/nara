//! Bounded artifact publication and recovery facts, without live object or deletion authority.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use nara_fs::{ContentDigest, RelativeComponent, RelativePath};
use nara_tooling::{
    EditorSourceToolReclamationMember as ReclamationMember,
    EditorSourceToolReclamationMemberState as ReclamationMemberState,
    EditorSourceToolReclamationRecord as Reclamation,
    EditorSourceToolReclamationRecordPhase as ReclamationPhase,
};
use serde::{Deserialize, Serialize};

pub(super) use nara_tooling::{
    EditorSourceToolArtifact as Artifact, EditorSourceToolArtifactError as Error,
    EditorSourceToolArtifactMember as Member, EditorSourceToolArtifactObservation as Observation,
    EditorSourceToolArtifactPublication as Publication, EditorSourceToolArtifactRole as Role,
};

pub(super) const BYTE_LIMIT: u64 = 2 * 1024 * 1024;
pub(super) const MAX_ARTIFACTS: usize = 32;
pub(super) const MAX_RECLAMATIONS: usize = 32;
pub(super) const MAX_MEMBERS: usize = 66;
const MAX_PAYLOADS: usize = 64;
const MAX_PATH_BYTES: usize = 512;
const MAX_METADATA_BYTES: u64 = 65_536;
const MAX_RECEIPT_BYTES: u64 = 4096;
const MAX_PAYLOAD_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MANIFEST_NAME: &str = "nara-tool.json";
const RECEIPT_NAME: &str = ".nara-installation.json";
const RECEIPT_TEMPORARY_PREFIX: &str = ".nara-tool-stage-";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Inventory {
    pub(super) artifacts: Vec<Artifact>,
    pub(super) reclamations: Vec<Reclamation>,
}

impl Inventory {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.artifacts.len() > MAX_ARTIFACTS || self.reclamations.len() > MAX_RECLAMATIONS {
            return Err(Error::TooLarge);
        }
        for (index, artifact) in self.artifacts.iter().enumerate() {
            validate_artifact(artifact)?;
            if self.artifacts[..index].iter().any(|previous| {
                previous.id == artifact.id
                    || previous.directory.eq_ignore_ascii_case(&artifact.directory)
            }) {
                return Err(Error::InvalidRecord);
            }
        }
        for (index, reclamation) in self.reclamations.iter().enumerate() {
            self.validate_reclamation(reclamation)?;
            if self.reclamations[..index].iter().any(|previous| {
                previous.artifact == reclamation.artifact || previous.id == reclamation.id
            }) {
                return Err(Error::InvalidRecord);
            }
        }
        Ok(())
    }

    /// Repeating exact immutable facts may refresh observations; it never rebinds an artifact.
    pub(super) fn register(&self, artifact: Artifact) -> Result<Self, Error> {
        self.validate()?;
        validate_artifact(&artifact)?;
        let mut next = self.clone();
        if let Some(existing) = next
            .artifacts
            .iter_mut()
            .find(|entry| entry.id == artifact.id)
        {
            if !same_facts(existing, &artifact) {
                return Err(Error::Changed);
            }
            *existing = artifact;
        } else {
            if next.artifacts.len() == MAX_ARTIFACTS {
                return Err(Error::TooLarge);
            }
            next.artifacts.push(artifact);
        }
        next.validate()?;
        next.canonicalize();
        Ok(next)
    }

    /// Replaces only this artifact's recovery facts after a new explicit review.
    /// Validating or decoding the record never recreates that review's live authority.
    pub(super) fn with_reclamation(&self, reclamation: &Reclamation) -> Result<Self, Error> {
        self.validate()?;
        self.validate_reclamation(reclamation)?;
        let existing = self
            .reclamations
            .iter()
            .position(|entry| entry.artifact == reclamation.artifact);
        if existing.is_none() && self.reclamations.len() == MAX_RECLAMATIONS {
            return Err(Error::TooLarge);
        }
        let mut next = self.clone();
        match existing {
            Some(index) => next.reclamations[index] = reclamation.clone(),
            None => next.reclamations.push(reclamation.clone()),
        }
        next.validate()?;
        next.canonicalize();
        Ok(next)
    }

    fn validate_reclamation(&self, reclamation: &Reclamation) -> Result<(), Error> {
        if reclamation.members.len() > MAX_MEMBERS {
            return Err(Error::TooLarge);
        }
        if reclamation.id == [0; 32] {
            return Err(Error::InvalidRecord);
        }
        let artifact = self
            .artifacts
            .iter()
            .find(|entry| entry.id == reclamation.artifact)
            .ok_or(Error::Unregistered)?;
        let mut paths = BTreeSet::new();
        for member in &reclamation.members {
            validate_path(&member.path)?;
            if !artifact
                .members
                .iter()
                .any(|registered| registered.path == member.path)
                || !paths.insert(member.path.as_ref())
                || (reclamation.phase == ReclamationPhase::Intent
                    && !matches!(
                        member.state,
                        ReclamationMemberState::Pending | ReclamationMemberState::Missing
                    ))
            {
                return Err(Error::InvalidRecord);
            }
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        self.artifacts.sort_unstable_by_key(|entry| entry.id);
        for artifact in &mut self.artifacts {
            artifact
                .members
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
        }
        self.reclamations
            .sort_unstable_by_key(|entry| entry.artifact);
        for reclamation in &mut self.reclamations {
            reclamation
                .members
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
        }
    }
}

pub(super) fn same_facts(left: &Artifact, right: &Artifact) -> bool {
    left.id == right.id
        && left.package == right.package
        && left.directory == right.directory
        && left.manifest == right.manifest
        && left.publication == right.publication
        && left.members.len() == right.members.len()
        && left.members.iter().all(|member| {
            right.members.iter().any(|candidate| {
                candidate.path == member.path
                    && candidate.role == member.role
                    && candidate.baseline == member.baseline
            })
        })
}

pub(super) fn validate_directory(value: &str) -> Result<(), Error> {
    if value.len() > MAX_PATH_BYTES || !super::super::artifact_store::validate_path(value) {
        return Err(Error::InvalidPath);
    }
    Ok(())
}

fn validate_artifact(entry: &Artifact) -> Result<(), Error> {
    if !valid_package(&entry.package)
        || entry.id == [0; 32]
        || !(1..=MAX_METADATA_BYTES).contains(&entry.manifest.length())
    {
        return Err(Error::InvalidRecord);
    }
    let receipt = match entry.publication {
        Publication::Ready { receipt } => {
            if !(1..=MAX_RECEIPT_BYTES).contains(&receipt.length()) {
                return Err(Error::InvalidRecord);
            }
            Some(receipt)
        }
        Publication::Incomplete => None,
    };
    validate_directory(&entry.directory)?;
    if entry.members.len() > MAX_MEMBERS {
        return Err(Error::TooLarge);
    }
    let mut metadata = 0;
    let mut receipt_temporaries = 0;
    let mut tools = 0;
    let mut payloads = 0;
    let mut payload_bytes = 0_u64;
    let mut paths = BTreeSet::new();
    let mut directories = BTreeMap::new();
    for member in &entry.members {
        validate_path(&member.path)?;
        let folded = member.path.to_ascii_lowercase();
        match member.role {
            Role::Metadata => {
                let expected = match member.path.as_ref() {
                    MANIFEST_NAME => entry.manifest,
                    RECEIPT_NAME => receipt.ok_or(Error::InvalidRecord)?,
                    _ => return Err(Error::InvalidRecord),
                };
                if member.baseline != expected {
                    return Err(Error::InvalidRecord);
                }
                metadata += 1;
            }
            Role::ReceiptTemporary => {
                let nonce = member
                    .path
                    .strip_prefix(RECEIPT_TEMPORARY_PREFIX)
                    .ok_or(Error::InvalidRecord)?;
                if receipt.is_some()
                    || nonce.len() != 32
                    || !nonce
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    || member.baseline.length() > MAX_RECEIPT_BYTES
                {
                    return Err(Error::InvalidRecord);
                }
                receipt_temporaries += 1;
                if receipt_temporaries > 1 {
                    return Err(Error::InvalidRecord);
                }
            }
            Role::Tool | Role::Content => {
                let root_name = folded.split('/').next().ok_or(Error::InvalidPath)?;
                if matches!(root_name, MANIFEST_NAME | RECEIPT_NAME) {
                    return Err(Error::InvalidRecord);
                }
                tools += usize::from(member.role == Role::Tool);
                payloads += 1;
                payload_bytes = payload_bytes
                    .checked_add(member.baseline.length())
                    .ok_or(Error::TooLarge)?;
                if payloads > MAX_PAYLOADS
                    || member.baseline.length() > MAX_PAYLOAD_BYTES
                    || payload_bytes > MAX_TOTAL_PAYLOAD_BYTES
                {
                    return Err(Error::TooLarge);
                }
            }
        }
        if !paths.insert(folded) {
            return Err(Error::InvalidPath);
        }
        for (separator, _) in member.path.match_indices('/') {
            let prefix = &member.path[..separator];
            if directories
                .insert(prefix.to_ascii_lowercase(), prefix)
                .is_some_and(|previous| previous != prefix)
            {
                return Err(Error::InvalidPath);
            }
        }
    }
    if receipt.is_some() && (metadata != 2 || tools == 0) {
        return Err(Error::InvalidRecord);
    }
    for path in &paths {
        for (separator, _) in path.match_indices('/') {
            if paths.contains(&path[..separator]) {
                return Err(Error::InvalidPath);
            }
        }
    }
    Ok(())
}

fn valid_package(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn validate_path(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > MAX_PATH_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        || value.split('/').count() > 8
    {
        return Err(Error::InvalidPath);
    }
    for component in value.split('/') {
        if matches!(component, "" | "." | "..") || component.ends_with('.') {
            return Err(Error::InvalidPath);
        }
        RelativeComponent::new(component).map_err(|_| Error::InvalidPath)?;
    }
    RelativePath::new(value).map_err(|_| Error::InvalidPath)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record<'a> {
    format_version: u32,
    #[serde(borrow)]
    artifacts: Vec<ArtifactRecord<'a>>,
    #[serde(borrow)]
    reclamations: Vec<ReclamationRecord<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactRecord<'a> {
    id: [u8; 32],
    #[serde(borrow)]
    package: Cow<'a, str>,
    #[serde(borrow)]
    directory: Cow<'a, str>,
    manifest: DigestRecord,
    publication: PublicationRecord,
    #[serde(borrow)]
    members: Vec<MemberRecord<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum PublicationRecord {
    Ready { receipt: DigestRecord },
    Incomplete {},
}

impl From<Publication> for PublicationRecord {
    fn from(publication: Publication) -> Self {
        match publication {
            Publication::Ready { receipt } => Self::Ready {
                receipt: receipt.into(),
            },
            Publication::Incomplete => Self::Incomplete {},
        }
    }
}

impl From<PublicationRecord> for Publication {
    fn from(publication: PublicationRecord) -> Self {
        match publication {
            PublicationRecord::Ready { receipt } => Self::Ready {
                receipt: receipt.into(),
            },
            PublicationRecord::Incomplete {} => Self::Incomplete,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberRecord<'a> {
    #[serde(borrow)]
    path: Cow<'a, str>,
    role: RoleRecord,
    baseline: DigestRecord,
    observation: ObservationRecord,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReclamationRecord<'a> {
    id: [u8; 32],
    artifact: [u8; 32],
    phase: ReclamationPhaseRecord,
    #[serde(borrow)]
    members: Vec<ReclamationMemberRecord<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReclamationMemberRecord<'a> {
    #[serde(borrow)]
    path: Cow<'a, str>,
    state: ReclamationMemberStateRecord,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReclamationPhaseRecord {
    Intent,
    Results,
}

impl From<ReclamationPhase> for ReclamationPhaseRecord {
    fn from(phase: ReclamationPhase) -> Self {
        match phase {
            ReclamationPhase::Intent => Self::Intent,
            ReclamationPhase::Results => Self::Results,
        }
    }
}

impl From<ReclamationPhaseRecord> for ReclamationPhase {
    fn from(phase: ReclamationPhaseRecord) -> Self {
        match phase {
            ReclamationPhaseRecord::Intent => Self::Intent,
            ReclamationPhaseRecord::Results => Self::Results,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReclamationMemberStateRecord {
    Pending,
    Removed,
    Missing,
    Failed,
}

impl From<ReclamationMemberState> for ReclamationMemberStateRecord {
    fn from(state: ReclamationMemberState) -> Self {
        match state {
            ReclamationMemberState::Pending => Self::Pending,
            ReclamationMemberState::Removed => Self::Removed,
            ReclamationMemberState::Missing => Self::Missing,
            ReclamationMemberState::Failed => Self::Failed,
        }
    }
}

impl From<ReclamationMemberStateRecord> for ReclamationMemberState {
    fn from(state: ReclamationMemberStateRecord) -> Self {
        match state {
            ReclamationMemberStateRecord::Pending => Self::Pending,
            ReclamationMemberStateRecord::Removed => Self::Removed,
            ReclamationMemberStateRecord::Missing => Self::Missing,
            ReclamationMemberStateRecord::Failed => Self::Failed,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestRecord {
    length: u64,
    hash: [u8; 32],
}

impl From<ContentDigest> for DigestRecord {
    fn from(digest: ContentDigest) -> Self {
        Self {
            length: digest.length(),
            hash: *digest.as_bytes(),
        }
    }
}

impl From<DigestRecord> for ContentDigest {
    fn from(digest: DigestRecord) -> Self {
        Self::from_parts(digest.length, digest.hash)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RoleRecord {
    Tool,
    Content,
    Metadata,
    ReceiptTemporary,
}

impl From<Role> for RoleRecord {
    fn from(role: Role) -> Self {
        match role {
            Role::Tool => Self::Tool,
            Role::Content => Self::Content,
            Role::Metadata => Self::Metadata,
            Role::ReceiptTemporary => Self::ReceiptTemporary,
        }
    }
}

impl From<RoleRecord> for Role {
    fn from(role: RoleRecord) -> Self {
        match role {
            RoleRecord::Tool => Self::Tool,
            RoleRecord::Content => Self::Content,
            RoleRecord::Metadata => Self::Metadata,
            RoleRecord::ReceiptTemporary => Self::ReceiptTemporary,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ObservationRecord {
    Present,
    Changed,
    Missing,
    Failed,
}

impl From<Observation> for ObservationRecord {
    fn from(observation: Observation) -> Self {
        match observation {
            Observation::Present => Self::Present,
            Observation::Changed => Self::Changed,
            Observation::Missing => Self::Missing,
            Observation::Failed => Self::Failed,
        }
    }
}

impl From<ObservationRecord> for Observation {
    fn from(observation: ObservationRecord) -> Self {
        match observation {
            ObservationRecord::Present => Self::Present,
            ObservationRecord::Changed => Self::Changed,
            ObservationRecord::Missing => Self::Missing,
            ObservationRecord::Failed => Self::Failed,
        }
    }
}

pub(super) fn encode(inventory: &Inventory) -> Result<Vec<u8>, Error> {
    inventory.validate()?;
    let mut artifacts: Vec<_> = inventory
        .artifacts
        .iter()
        .map(|entry| {
            let mut members: Vec<_> = entry
                .members
                .iter()
                .map(|member| MemberRecord {
                    path: Cow::Borrowed(&member.path),
                    role: member.role.into(),
                    baseline: member.baseline.into(),
                    observation: member.observation.into(),
                })
                .collect();
            members.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            ArtifactRecord {
                id: entry.id,
                package: Cow::Borrowed(&entry.package),
                directory: Cow::Borrowed(&entry.directory),
                manifest: entry.manifest.into(),
                publication: entry.publication.into(),
                members,
            }
        })
        .collect();
    artifacts.sort_unstable_by_key(|entry| entry.id);
    let mut reclamations: Vec<_> = inventory
        .reclamations
        .iter()
        .map(|entry| {
            let mut members: Vec<_> = entry
                .members
                .iter()
                .map(|member| ReclamationMemberRecord {
                    path: Cow::Borrowed(&member.path),
                    state: member.state.into(),
                })
                .collect();
            members.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            ReclamationRecord {
                id: entry.id,
                artifact: entry.artifact,
                phase: entry.phase.into(),
                members,
            }
        })
        .collect();
    reclamations.sort_unstable_by_key(|entry| entry.artifact);
    let mut output = RecordBytes {
        bytes: Vec::new(),
        oversized: false,
    };
    if serde_json::to_writer(
        &mut output,
        &Record {
            format_version: 1,
            artifacts,
            reclamations,
        },
    )
    .is_err()
    {
        return Err(if output.oversized {
            Error::TooLarge
        } else {
            Error::InvalidRecord
        });
    }
    Ok(output.bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Inventory, Error> {
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(Error::TooLarge);
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(8).expect("nonzero artifact depth"),
        ItemLimit::new(120_000).expect("nonzero artifact node limit"),
        ItemLimit::new(MAX_MEMBERS).expect("nonzero artifact container limit"),
        ByteLimit::new(MAX_PATH_BYTES).expect("nonzero artifact string limit"),
        ByteLimit::new(BYTE_LIMIT as usize).expect("nonzero artifact aggregate string limit"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|_| Error::InvalidRecord)?;
    deserializer.end().map_err(|_| Error::InvalidRecord)?;
    let record: Record = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
    if record.format_version != 1 {
        return Err(Error::UnsupportedFormat);
    }
    if record.artifacts.len() > MAX_ARTIFACTS || record.reclamations.len() > MAX_RECLAMATIONS {
        return Err(Error::TooLarge);
    }
    let mut inventory = Inventory {
        artifacts: record
            .artifacts
            .into_iter()
            .map(|entry| Artifact {
                id: entry.id,
                package: entry.package.as_ref().into(),
                directory: entry.directory.as_ref().into(),
                manifest: entry.manifest.into(),
                publication: entry.publication.into(),
                members: entry
                    .members
                    .into_iter()
                    .map(|member| Member {
                        path: member.path.as_ref().into(),
                        role: member.role.into(),
                        baseline: member.baseline.into(),
                        observation: member.observation.into(),
                    })
                    .collect(),
            })
            .collect(),
        reclamations: record
            .reclamations
            .into_iter()
            .map(|entry| Reclamation {
                id: entry.id,
                artifact: entry.artifact,
                phase: entry.phase.into(),
                members: entry
                    .members
                    .into_iter()
                    .map(|member| ReclamationMember {
                        path: member.path.as_ref().into(),
                        state: member.state.into(),
                    })
                    .collect(),
            })
            .collect(),
    };
    inventory.validate()?;
    inventory.canonicalize();
    Ok(inventory)
}

struct RecordBytes {
    bytes: Vec<u8>,
    oversized: bool,
}

impl Write for RecordBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > BYTE_LIMIT as usize - self.bytes.len() {
            self.oversized = true;
            return Err(std::io::Error::other(
                "source-tool artifact inventory byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
