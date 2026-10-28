//! Bounded installation and recovery facts, without live object or deletion authority.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use nara_fs::{ContentDigest, RelativeComponent, RelativePath};
use nara_tooling::{
    EditorSourceToolInstalledSelection, EditorSourceToolReclamationMember as ReclamationMember,
    EditorSourceToolReclamationMemberState as ReclamationMemberState,
    EditorSourceToolReclamationRecord as Reclamation,
    EditorSourceToolReclamationRecordPhase as ReclamationPhase,
};
use serde::{Deserialize, Serialize};

pub(super) use nara_tooling::{
    EditorSourceToolArtifactError as Error, EditorSourceToolArtifactInstallation as Installation,
    EditorSourceToolArtifactMember as Member, EditorSourceToolArtifactObservation as Observation,
    EditorSourceToolArtifactRole as Role,
};

pub(super) const BYTE_LIMIT: u64 = 2 * 1024 * 1024;
pub(super) const MAX_INSTALLATIONS: usize = 32;
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Inventory {
    pub(super) installations: Vec<Installation>,
    pub(super) reclamations: Vec<Reclamation>,
}

impl Inventory {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.installations.len() > MAX_INSTALLATIONS
            || self.reclamations.len() > MAX_RECLAMATIONS
        {
            return Err(Error::TooLarge);
        }
        for (index, installation) in self.installations.iter().enumerate() {
            validate_installation(installation)?;
            if self.installations[..index].iter().any(|previous| {
                previous.installed.installation == installation.installed.installation
                    || previous
                        .installed
                        .directory
                        .eq_ignore_ascii_case(&installation.installed.directory)
            }) {
                return Err(Error::InvalidRecord);
            }
        }
        for (index, reclamation) in self.reclamations.iter().enumerate() {
            self.validate_reclamation(reclamation)?;
            if self.reclamations[..index]
                .iter()
                .any(|previous| previous.installation == reclamation.installation)
            {
                return Err(Error::InvalidRecord);
            }
        }
        Ok(())
    }

    /// Repeating exact immutable facts may refresh observations; it never rebinds an installation.
    pub(super) fn register(&self, installation: Installation) -> Result<Self, Error> {
        self.validate()?;
        validate_installation(&installation)?;
        let mut next = self.clone();
        if let Some(existing) = next
            .installations
            .iter_mut()
            .find(|entry| entry.installed.installation == installation.installed.installation)
        {
            if !same_facts(existing, &installation) {
                return Err(Error::Changed);
            }
            *existing = installation;
        } else {
            if next.installations.len() == MAX_INSTALLATIONS {
                return Err(Error::TooLarge);
            }
            next.installations.push(installation);
        }
        next.validate()?;
        next.canonicalize();
        Ok(next)
    }

    /// Replaces only this installation's recovery facts after a new explicit review.
    /// Validating or decoding the record never recreates that review's live authority.
    pub(super) fn with_reclamation(&self, reclamation: &Reclamation) -> Result<Self, Error> {
        self.validate()?;
        self.validate_reclamation(reclamation)?;
        let existing = self
            .reclamations
            .iter()
            .position(|entry| entry.installation == reclamation.installation);
        if existing.is_none() && self.reclamations.len() == MAX_RECLAMATIONS {
            return Err(Error::TooLarge);
        }
        let mut next = self.clone();
        match existing {
            Some(index) => next.reclamations[index] = reclamation.clone(),
            None => next.reclamations.push(reclamation.clone()),
        }
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
        let installation = self
            .installations
            .iter()
            .find(|entry| entry.installed.installation == reclamation.installation)
            .ok_or(Error::Unregistered)?;
        let mut paths = BTreeSet::new();
        for member in &reclamation.members {
            validate_path(&member.path)?;
            if !installation
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
        self.installations
            .sort_unstable_by_key(|entry| entry.installed.installation);
        for installation in &mut self.installations {
            installation
                .members
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
        }
        self.reclamations
            .sort_unstable_by_key(|entry| entry.installation);
        for reclamation in &mut self.reclamations {
            reclamation
                .members
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
        }
    }
}

pub(super) fn same_facts(left: &Installation, right: &Installation) -> bool {
    left.installed == right.installed
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

fn validate_installation(entry: &Installation) -> Result<(), Error> {
    let installed = &entry.installed;
    if !valid_package(&installed.package)
        || installed.installation == [0; 32]
        || !(1..=MAX_RECEIPT_BYTES).contains(&installed.receipt.length())
        || !(1..=MAX_METADATA_BYTES).contains(&installed.manifest.length())
    {
        return Err(Error::InvalidRecord);
    }
    validate_directory(&installed.directory)?;
    if entry.members.len() > MAX_MEMBERS {
        return Err(Error::TooLarge);
    }
    let mut metadata = 0;
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
                    MANIFEST_NAME => installed.manifest,
                    RECEIPT_NAME => installed.receipt,
                    _ => return Err(Error::InvalidRecord),
                };
                if member.baseline != expected {
                    return Err(Error::InvalidRecord);
                }
                metadata += 1;
            }
            Role::Tool | Role::Content => {
                if matches!(folded.as_str(), MANIFEST_NAME | RECEIPT_NAME) {
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
    if metadata != 2 || tools == 0 {
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
    installations: Vec<InstallationRecord<'a>>,
    #[serde(borrow)]
    reclamations: Vec<ReclamationRecord<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallationRecord<'a> {
    #[serde(borrow)]
    package: Cow<'a, str>,
    #[serde(borrow)]
    directory: Cow<'a, str>,
    installation: [u8; 32],
    receipt: DigestRecord,
    manifest: DigestRecord,
    #[serde(borrow)]
    members: Vec<MemberRecord<'a>>,
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
    installation: [u8; 32],
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
}

impl From<Role> for RoleRecord {
    fn from(role: Role) -> Self {
        match role {
            Role::Tool => Self::Tool,
            Role::Content => Self::Content,
            Role::Metadata => Self::Metadata,
        }
    }
}

impl From<RoleRecord> for Role {
    fn from(role: RoleRecord) -> Self {
        match role {
            RoleRecord::Tool => Self::Tool,
            RoleRecord::Content => Self::Content,
            RoleRecord::Metadata => Self::Metadata,
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
    let mut installations: Vec<_> = inventory
        .installations
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
            InstallationRecord {
                package: Cow::Borrowed(&entry.installed.package),
                directory: Cow::Borrowed(&entry.installed.directory),
                installation: entry.installed.installation,
                receipt: entry.installed.receipt.into(),
                manifest: entry.installed.manifest.into(),
                members,
            }
        })
        .collect();
    installations.sort_unstable_by_key(|entry| entry.installation);
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
                installation: entry.installation,
                phase: entry.phase.into(),
                members,
            }
        })
        .collect();
    reclamations.sort_unstable_by_key(|entry| entry.installation);
    let mut output = RecordBytes {
        bytes: Vec::new(),
        oversized: false,
    };
    if serde_json::to_writer(
        &mut output,
        &Record {
            format_version: 1,
            installations,
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
    if record.installations.len() > MAX_INSTALLATIONS
        || record.reclamations.len() > MAX_RECLAMATIONS
    {
        return Err(Error::TooLarge);
    }
    let mut inventory = Inventory {
        installations: record
            .installations
            .into_iter()
            .map(|entry| Installation {
                installed: EditorSourceToolInstalledSelection {
                    package: entry.package.as_ref().into(),
                    directory: entry.directory.as_ref().into(),
                    installation: entry.installation,
                    receipt: entry.receipt.into(),
                    manifest: entry.manifest.into(),
                },
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
                installation: entry.installation,
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
