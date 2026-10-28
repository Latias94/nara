//! Native Prefab capture and directory publication use the existing import task lane.

use std::{fmt, sync::Arc};

use nara_asset::StableAssetId;
use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryPublicationOutcome, FileCapability,
    RelativeComponent,
};
use nara_scene::SourceDocumentKey;
use nara_tooling::{
    EditorSourceToolInstalledEntry, EditorSourceToolSelectionBasis,
    EditorSourceToolTemplateImportMember,
};

use super::Error;

mod prepare;
mod publish;
#[cfg(all(test, windows, target_arch = "x86_64", target_env = "msvc"))]
mod tests;

pub(super) use prepare::validate_session;
pub(super) use publish::publish;

/// Selects one declared descriptor and an absent project directory below the Prefabs root.
#[derive(Debug, Clone)]
pub struct EditorSourceToolPrefabTemplateImportRequest {
    pub(super) installation: [u8; 32],
    pub(super) descriptor: Arc<str>,
    pub(super) destination: Arc<str>,
}

impl EditorSourceToolPrefabTemplateImportRequest {
    #[must_use]
    pub fn new(
        installation: [u8; 32],
        descriptor: impl Into<Arc<str>>,
        destination: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            installation,
            descriptor: descriptor.into(),
            destination: destination.into(),
        }
    }
}

/// One immutable output member. Document-local entity IDs retain their original namespace.
#[derive(Debug)]
pub struct EditorSourceToolPrefabTemplateFile {
    pub(super) path: Arc<str>,
    pub(super) bytes: Arc<[u8]>,
    pub(super) digest: ContentDigest,
    pub(super) source: Option<SourceDocumentKey>,
}

impl EditorSourceToolPrefabTemplateFile {
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
    /// Sidecar files have no independent source-document key.
    #[must_use]
    pub fn source(&self) -> Option<&SourceDocumentKey> {
        self.source.as_ref()
    }
}

/// Original project, selection and metadata observations with fixed copied identities and bytes.
/// Preparation/drop performs no writes, opening or selection changes.
pub struct PreparedEditorSourceToolPrefabTemplateImport {
    pub(super) root: DirectoryCapability,
    pub(super) basis: EditorSourceToolSelectionBasis,
    pub(super) installation: EditorSourceToolInstalledEntry,
    pub(super) source_directory: DirectoryCapability,
    pub(super) parent: DirectoryCapability,
    pub(super) target: RelativeComponent,
    pub(super) destination: Arc<str>,
    pub(super) entry: Arc<str>,
    pub(super) files: Vec<EditorSourceToolPrefabTemplateFile>,
    pub(super) manifest_digest: ContentDigest,
    pub(super) schema_path: Arc<str>,
    pub(super) schema_digest: ContentDigest,
    pub(super) schema_basis: nara_scene::AuthoringSchemaBasis,
    pub(super) source_policy:
        crate::project_host::authoring::native_source::NativeDocumentSourcePolicy,
    pub(super) identities: Vec<StableAssetId>,
    pub(super) metadata: Vec<(Arc<str>, ContentDigest)>,
    pub(super) project_input: crate::project_host::authoring::AuthoringSchemaOwnerProjectInput,
    pub(super) source_index: Arc<crate::project_host::authoring::ProjectSourceIndexSnapshot>,
    #[cfg(test)]
    pub(super) test_stop_after_created: Option<usize>,
    #[cfg(test)]
    pub(super) test_after_publication: Option<Box<dyn Fn(&DirectoryCapability) + Send + Sync>>,
    #[cfg(test)]
    pub(super) test_after_member_admission: Option<Box<dyn Fn(&DirectoryCapability) + Send + Sync>>,
}

impl fmt::Debug for PreparedEditorSourceToolPrefabTemplateImport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolPrefabTemplateImport")
            .field("destination", &self.destination)
            .field("entry", &self.entry)
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolPrefabTemplateImport {
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }
    #[must_use]
    pub fn files(&self) -> &[EditorSourceToolPrefabTemplateFile] {
        &self.files
    }
}

/// Actual directory outcome and retained live objects; never automatic deletion authority.
///
/// Published members are freshly admitted after staging handles close for directory publication.
/// That read does not assert continuity with closed creation handles. Failed staging retains its
/// actual member handles until this owner closes. Dropping either form preserves project files.
#[derive(Debug)]
pub struct EditorSourceToolPrefabTemplatePublication {
    pub(super) publication: DirectoryPublicationOutcome,
    pub(super) directory: Option<DirectoryCapability>,
    pub(super) custody: StagedPrefabMembers,
    pub(super) error: Option<Error>,
}

impl EditorSourceToolPrefabTemplatePublication {
    #[must_use]
    pub fn publication(&self) -> &DirectoryPublicationOutcome {
        &self.publication
    }
    #[must_use]
    pub const fn directory(&self) -> Option<&DirectoryCapability> {
        self.directory.as_ref()
    }
    #[must_use]
    pub fn created_members(&self) -> &[EditorSourceToolTemplateImportMember] {
        &self.custody.members
    }
    #[must_use]
    pub fn retained_files(&self) -> usize {
        self.custody.files.len().max(self.custody.cleanup.len())
    }
    #[must_use]
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    #[must_use]
    pub fn created_directories(&self) -> &[Arc<str>] {
        &self.custody.created_directories
    }
    #[must_use]
    pub fn filesystem_error(&self) -> Option<&nara_fs::FsError> {
        self.custody.filesystem_error.as_ref()
    }
    #[must_use]
    pub fn retained_directories(&self) -> usize {
        self.custody.directories.len() + usize::from(self.custody.unverified_directory.is_some())
    }
}

#[derive(Debug, Default)]
pub(super) struct StagedPrefabMembers {
    members: Vec<EditorSourceToolTemplateImportMember>,
    files: Vec<FileCapability>,
    cleanup: Vec<nara_fs::TemporaryFileCleanup>,
    directories: std::collections::BTreeMap<String, DirectoryCapability>,
    created_directories: Vec<Arc<str>>,
    unverified_directory: Option<nara_fs::CreatedDirectoryCustody>,
    filesystem_error: Option<nara_fs::FsError>,
}
