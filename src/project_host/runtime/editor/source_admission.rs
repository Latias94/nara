use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::EditorDocumentId;

use super::{EditorExecutionLane, EditorProjectSession};
use crate::project_host::file_publication::FilePublicationReadSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceAdmissionError {
    AlreadyOpen,
    HostUnavailable,
    SourceRead,
}

impl SourceAdmissionError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::AlreadyOpen => "project.editor.document-source-already-open",
            Self::HostUnavailable => "project.editor.document-open-busy",
            Self::SourceRead => "project.editor.document-source-open-failed",
        }
    }
}

impl EditorProjectSession {
    pub(super) fn source_project_root(&self) -> DirectoryCapability {
        match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.project_root(),
            EditorExecutionLane::TrustedCompiled(lane) => lane.content_loader.project_root(),
        }
    }

    pub(super) fn exact_source_owner(&self, project_path: &str) -> Option<EditorDocumentId> {
        self.scene_lanes
            .values()
            .find(|lane| lane.project_path.as_ref() == project_path)
            .map(|lane| lane.document)
            .or_else(|| {
                self.prefab_lanes
                    .values()
                    .find(|lane| lane.project_path.as_ref() == project_path)
                    .map(|lane| lane.document)
            })
            .or_else(|| {
                self.extension_documents
                    .lanes()
                    .find(|lane| lane.source.locator() == project_path)
                    .map(|lane| lane.document)
            })
    }

    pub(super) fn exclude_project_source_aliases(
        &self,
        project_path: &str,
    ) -> Result<(), SourceAdmissionError> {
        let path = RelativePath::new(project_path).map_err(|_| SourceAdmissionError::SourceRead)?;
        let current = self
            .source_project_root()
            .open_file(&path)
            .map_err(|_| SourceAdmissionError::SourceRead)?;
        self.exclude_source_aliases(project_path, &current)
    }

    pub(super) fn exclude_source_aliases(
        &self,
        project_path: &str,
        current: &FileCapability,
    ) -> Result<(), SourceAdmissionError> {
        if self.exact_source_owner(project_path).is_some() {
            return Err(SourceAdmissionError::AlreadyOpen);
        }
        for lane in self.scene_lanes.values() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source)?;
        }
        for lane in self.prefab_lanes.values() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source)?;
        }
        for lane in self.extension_documents.lanes() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source)?;
        }
        Ok(())
    }

    pub(super) fn discard_unpublished_document(
        &mut self,
        document: EditorDocumentId,
        previous_active: Option<EditorDocumentId>,
    ) {
        let _ = self.workspace.__discard_document(document);
        if let Some(previous) = previous_active {
            let _ = self.workspace.activate_document(previous);
        }
    }
}

fn exclude_file_alias(
    current: &FileCapability,
    source: FilePublicationReadSource,
) -> Result<(), SourceAdmissionError> {
    // Reopening through retained parent custody observes atomic replacement and directory rename.
    match source.open() {
        Ok(retained) if retained.identity() == current.identity() => {
            Err(SourceAdmissionError::AlreadyOpen)
        }
        Ok(_) => Ok(()),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SourceAdmissionError::SourceRead),
    }
}
