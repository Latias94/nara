//! Bounded installed-content facts. No persisted value reconstructs live file authority.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    io::Write,
    sync::Arc,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use nara_fs::{ContentDigest, RelativeComponent, RelativePath};
use nara_tooling::EditorSourceToolInstalledSelection;
use serde::{Deserialize, Serialize};

pub(super) use nara_tooling::{
    EditorSourceToolContentError as Error, EditorSourceToolContentObservation as Observation,
    EditorSourceToolContentRole as Role,
};

pub(super) const BYTE_LIMIT: u64 = 2 * 1024 * 1024;
pub(super) const MAX_INSTALLATIONS: usize = 32;
pub(super) const MAX_MEMBERS: usize = 66;
const MAX_PAYLOADS: usize = 64;
const MAX_PATH_BYTES: usize = 512;
const MAX_METADATA_BYTES: u64 = 65_536;
const MAX_RECEIPT_BYTES: u64 = 4096;
const MAX_PAYLOAD_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MANIFEST_NAME: &str = "nara-tool.json";
const RECEIPT_NAME: &str = ".nara-installation.json";
const RECORD_PARENT: &str = ".nara/source-tools";

/// Retains withdrawn installations until an explicit owning workflow settles their facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Ledger {
    pub(super) installations: Vec<Installation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Installation {
    pub(super) installed: EditorSourceToolInstalledSelection,
    pub(super) members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Member {
    pub(super) path: Arc<str>,
    pub(super) role: Role,
    pub(super) baseline: ContentDigest,
    pub(super) modified: bool,
    pub(super) adopted: bool,
    pub(super) observation: Observation,
}

impl Ledger {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.installations.len() > MAX_INSTALLATIONS {
            return Err(Error::TooLarge);
        }
        for (index, installation) in self.installations.iter().enumerate() {
            installation.validate()?;
            let directory = installation.installed.directory.to_ascii_lowercase();
            if self.installations[..index].iter().any(|previous| {
                previous.installed.installation == installation.installed.installation
                    || overlapping(
                        &previous.installed.directory.to_ascii_lowercase(),
                        &directory,
                    )
            }) {
                return Err(Error::InvalidRecord);
            }
        }
        Ok(())
    }

    /// Adds immutable installation facts or repeats them without replacing protection or status.
    ///
    /// Re-registration never imports mutable state into an existing entry. In particular a fresh
    /// clean inspection cannot reset a sticky flag or replace a pending/removed observation.
    pub(super) fn register_installation(&self, installation: Installation) -> Result<Self, Error> {
        self.validate()?;
        installation.validate()?;
        let mut next = self.clone();
        if let Some(existing) = self
            .installations
            .iter()
            .find(|existing| existing.installed.installation == installation.installed.installation)
        {
            if !existing.same_baseline(&installation) {
                return Err(Error::Changed);
            }
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

    pub(super) fn member_mut(
        &mut self,
        installation: &[u8; 32],
        path: &str,
    ) -> Result<&mut Member, Error> {
        validate_path(path)?;
        self.installations
            .iter_mut()
            .find(|entry| &entry.installed.installation == installation)
            .and_then(|entry| {
                entry
                    .members
                    .iter_mut()
                    .find(|member| member.path.as_ref() == path)
            })
            .ok_or(Error::Unmanaged)
    }

    pub(super) fn set_adopted(&mut self, installation: &[u8; 32], path: &str) -> Result<(), Error> {
        let member = self.member_mut(installation, path)?;
        if member.role != Role::Content {
            return Err(Error::InvalidPath);
        }
        member.adopted = true;
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
    }
}

impl Installation {
    fn validate(&self) -> Result<(), Error> {
        let installed = &self.installed;
        if !valid_package(&installed.package)
            || installed.installation == [0; 32]
            || !(1..=MAX_RECEIPT_BYTES).contains(&installed.receipt.length())
            || !(1..=MAX_METADATA_BYTES).contains(&installed.manifest.length())
        {
            return Err(Error::InvalidRecord);
        }
        validate_path(&installed.directory)?;
        let directory = installed.directory.to_ascii_lowercase();
        if directory == RECORD_PARENT || descendant_of(RECORD_PARENT, &directory) {
            return Err(Error::InvalidPath);
        }
        if self.members.len() > MAX_MEMBERS {
            return Err(Error::TooLarge);
        }
        let mut payloads = 0;
        let mut payload_bytes = 0_u64;
        let mut paths = BTreeSet::new();
        let mut directories = BTreeMap::new();
        for member in &self.members {
            validate_path(&member.path)?;
            let folded_path = member.path.to_ascii_lowercase();
            let metadata = match member.path.as_ref() {
                MANIFEST_NAME => Some(installed.manifest),
                RECEIPT_NAME => Some(installed.receipt),
                _ => None,
            };
            match member.role {
                Role::Metadata => {
                    if metadata != Some(member.baseline) {
                        return Err(Error::InvalidRecord);
                    }
                }
                Role::Tool | Role::Content => {
                    if matches!(folded_path.as_str(), MANIFEST_NAME | RECEIPT_NAME) {
                        return Err(Error::InvalidRecord);
                    }
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
            if !paths.insert(folded_path) {
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
        for path in &paths {
            for (separator, _) in path.match_indices('/') {
                if paths.contains(&path[..separator]) {
                    return Err(Error::InvalidPath);
                }
            }
        }
        Ok(())
    }

    fn same_baseline(&self, other: &Self) -> bool {
        self.installed == other.installed
            && self.members.len() == other.members.len()
            && self.members.iter().all(|member| {
                other.members.iter().any(|candidate| {
                    candidate.path == member.path
                        && candidate.role == member.role
                        && candidate.baseline == member.baseline
                })
            })
    }
}

fn overlapping(left: &str, right: &str) -> bool {
    left == right || descendant_of(left, right) || descendant_of(right, left)
}

fn descendant_of(path: &str, parent: &str) -> bool {
    path.strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn valid_package(value: &str) -> bool {
    value.is_ascii()
        && !value.is_empty()
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
    // Mirrors release::manifest::validate_path, which is private to that release owner.
    if !value.is_ascii()
        || value.is_empty()
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
    modified: bool,
    adopted: bool,
    observation: ObservationRecord,
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
    Pending,
    Removed,
    Missing,
    Failed,
}

impl From<Observation> for ObservationRecord {
    fn from(observation: Observation) -> Self {
        match observation {
            Observation::Present => Self::Present,
            Observation::Pending => Self::Pending,
            Observation::Removed => Self::Removed,
            Observation::Missing => Self::Missing,
            Observation::Failed => Self::Failed,
        }
    }
}

impl From<ObservationRecord> for Observation {
    fn from(observation: ObservationRecord) -> Self {
        match observation {
            ObservationRecord::Present => Self::Present,
            ObservationRecord::Pending => Self::Pending,
            ObservationRecord::Removed => Self::Removed,
            ObservationRecord::Missing => Self::Missing,
            ObservationRecord::Failed => Self::Failed,
        }
    }
}

pub(super) fn encode(ledger: &Ledger) -> Result<Vec<u8>, Error> {
    ledger.validate()?;
    let mut installations: Vec<_> = ledger
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
                    modified: member.modified,
                    adopted: member.adopted,
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
    let record = Record {
        format_version: 1,
        installations,
    };
    let mut output = RecordBytes {
        bytes: Vec::new(),
        oversized: false,
    };
    if serde_json::to_writer(&mut output, &record).is_err() {
        return Err(if output.oversized {
            Error::TooLarge
        } else {
            Error::InvalidRecord
        });
    }
    Ok(output.bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Ledger, Error> {
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(Error::TooLarge);
    }
    // Structural limits and duplicate-key rejection precede DTO vectors and owned strings.
    // The largest valid container is the 66-member list; hash/identity arrays have 32 items.
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(8).expect("nonzero content depth"),
        ItemLimit::new(120_000).expect("nonzero content node limit"),
        ItemLimit::new(MAX_MEMBERS).expect("nonzero content container limit"),
        ByteLimit::new(MAX_PATH_BYTES).expect("nonzero content string limit"),
        ByteLimit::new(BYTE_LIMIT as usize).expect("nonzero content aggregate string limit"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|_| Error::InvalidRecord)?;
    deserializer.end().map_err(|_| Error::InvalidRecord)?;
    let record: Record = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
    if record.format_version != 1 {
        return Err(Error::UnsupportedFormat);
    }
    if record.installations.len() > MAX_INSTALLATIONS
        || record
            .installations
            .iter()
            .any(|entry| entry.members.len() > MAX_MEMBERS)
    {
        return Err(Error::TooLarge);
    }
    let mut ledger = Ledger {
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
                        modified: member.modified,
                        adopted: member.adopted,
                        observation: member.observation.into(),
                    })
                    .collect(),
            })
            .collect(),
    };
    ledger.validate()?;
    ledger.canonicalize();
    Ok(ledger)
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
                "source-tool content record byte limit",
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
