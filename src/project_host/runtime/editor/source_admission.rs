use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::EditorDocumentId;

use super::{EditorExecutionLane, EditorProjectSession};

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
        if project_path.eq_ignore_ascii_case(super::source_tools::SELECTION_PATH)
            || project_path.eq_ignore_ascii_case(super::source_tools::CONTENT_PATH)
            || self.exact_source_owner(project_path).is_some()
        {
            return Err(SourceAdmissionError::AlreadyOpen);
        }
        // Reserve the current metadata objects even before parsing or Host admission succeeds.
        let root = self
            .source_project_root()
            .strict_read_only()
            .map_err(|_| SourceAdmissionError::SourceRead)?;
        for reserved in [
            super::source_tools::SELECTION_PATH,
            super::source_tools::CONTENT_PATH,
        ] {
            let path = RelativePath::new(reserved).expect("fixed source-tool metadata path");
            exclude_file_alias(current, root.open_file_exact(&path))?;
        }
        for lane in self.scene_lanes.values() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source.open())?;
        }
        for lane in self.prefab_lanes.values() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source.open())?;
        }
        for lane in self.extension_documents.lanes() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(SourceAdmissionError::HostUnavailable)?;
            exclude_file_alias(current, source.open())?;
        }
        if let Some(source) = self
            .source_tools
            .selection_read_source()
            .map_err(|_| SourceAdmissionError::HostUnavailable)?
        {
            exclude_file_alias(current, source.open())?;
        }
        if let Some(source) = self
            .source_tools
            .content_read_source()
            .map_err(|_| SourceAdmissionError::HostUnavailable)?
        {
            exclude_file_alias(current, source.open())?;
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
    source: Result<FileCapability, FsError>,
) -> Result<(), SourceAdmissionError> {
    // Live-object comparison also covers capabilities issued by independent Host sessions.
    match source {
        Ok(retained) => match retained
            .same_object(current)
            .map_err(|_| SourceAdmissionError::SourceRead)?
        {
            true => Err(SourceAdmissionError::AlreadyOpen),
            false => Ok(()),
        },
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SourceAdmissionError::SourceRead),
    }
}
