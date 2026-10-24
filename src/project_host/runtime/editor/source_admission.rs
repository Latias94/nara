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
    pub(super) fn validate_reopened_native_source(
        &self,
        project_path: &str,
        expected: &nara_scene::SourceDocumentKey,
        file_identity: nara_fs::FileIdentity,
        admitted: &crate::project_host::authoring::native_source::AdmittedNativeDocumentSource,
    ) -> Result<(), nara_tooling::EditorPersistenceRejection> {
        let rejected = || nara_tooling::EditorPersistenceRejection::SourceBindingMismatch;
        admitted
            .revalidate(&self.source_project_root())
            .map_err(|_| rejected())?;
        let path = RelativePath::new(project_path).map_err(|_| rejected())?;
        let current = self
            .source_project_root()
            .open_file_exact(&path)
            .map_err(|_| rejected())?;
        if admitted.key() != expected
            || admitted.source_identity() != file_identity
            || current.identity() != file_identity
        {
            return Err(nara_tooling::EditorPersistenceRejection::SourceBindingMismatch);
        }
        Ok(())
    }

    pub(super) fn exclude_native_source_key(
        &self,
        project_path: &str,
        key: &nara_scene::SourceDocumentKey,
    ) -> Result<(), &'static str> {
        let lane_conflict = self
            .scene_lanes
            .values()
            .map(|lane| (lane.project_path.as_ref(), &lane.source_key))
            .chain(
                self.prefab_lanes
                    .values()
                    .map(|lane| (lane.project_path.as_ref(), &lane.source_key)),
            )
            .any(|(path, existing)| path != project_path && existing.origin() == key.origin());
        let snapshot_conflict = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => {
                (snapshot.startup_source().project_path().as_str() != project_path
                    && snapshot.startup_native_source().key().origin() == key.origin())
                    || snapshot.prefab_sources().any(|prefab| {
                        prefab.source().project_path().as_str() != project_path
                            && prefab.native_source().key().origin() == key.origin()
                    })
                    || snapshot.asset_sources().any(|asset| {
                        matches!(key.origin(), nara_scene::SourceDocumentOrigin::StableAsset { stable_id }
                            if *stable_id == asset.record().stable_id()
                                && asset.source().project_path().as_str() != project_path)
                    })
            }
            EditorExecutionLane::TrustedCompiled(lane) => {
                let snapshot = &lane.snapshot;
                let roots = &lane.content_candidate.settings().paths;
                (!matches_project_path(project_path, roots.scenes.as_str(), snapshot.selected_scene_path().as_str())
                    && snapshot.selected_scene_source_key().origin() == key.origin())
                    || snapshot.prefabs().iter().any(|prefab| {
                        !matches_project_path(project_path, roots.prefabs.as_str(), prefab.path().as_str())
                            && prefab.source_key().origin() == key.origin()
                    })
                    || snapshot.asset_database().records().any(|asset| {
                        let root = if asset.source_kind() == &nara_asset::AssetSourceKind::Prefab {
                            &roots.prefabs
                        } else {
                            &roots.assets
                        };
                        matches!(key.origin(), nara_scene::SourceDocumentOrigin::StableAsset { stable_id }
                            if *stable_id == asset.stable_id()
                                && !matches_project_path(project_path, root.as_str(), asset.path().as_str()))
                    })
            }
        };
        if lane_conflict || snapshot_conflict {
            return Err("project.editor.document-source-identity-conflict");
        }
        Ok(())
    }

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

fn matches_project_path(project_path: &str, root: &str, logical_path: &str) -> bool {
    project_path
        .strip_prefix(root)
        .and_then(|suffix| suffix.strip_prefix('/'))
        == Some(logical_path)
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
