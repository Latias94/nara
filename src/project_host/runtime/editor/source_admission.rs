use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::EditorDocumentId;

use crate::project_host::file_publication::FilePublicationReadSource;

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
            lane.host
                .try_with(|host| {
                    exclude_publication_alias(
                        current,
                        host.same_source_object(current),
                        host.read_source(),
                    )
                })
                .ok_or(SourceAdmissionError::HostUnavailable)??;
        }
        for lane in self.prefab_lanes.values() {
            lane.host
                .try_with(|host| {
                    exclude_publication_alias(
                        current,
                        host.same_source_object(current),
                        host.read_source(),
                    )
                })
                .ok_or(SourceAdmissionError::HostUnavailable)??;
        }
        for lane in self.extension_documents.lanes() {
            lane.host
                .try_with(|host| {
                    exclude_publication_alias(
                        current,
                        host.same_source_object(current),
                        host.read_source(),
                    )
                })
                .ok_or(SourceAdmissionError::HostUnavailable)??;
        }
        self.source_tools.exclude_metadata_source_aliases(current)
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

pub(super) fn exclude_publication_alias(
    current: &FileCapability,
    retained: Result<bool, FsError>,
    source: FilePublicationReadSource,
) -> Result<(), SourceAdmissionError> {
    match retained {
        Ok(true) => Err(SourceAdmissionError::AlreadyOpen),
        Ok(false) => exclude_file_alias(current, source.open()),
        Err(_) => Err(SourceAdmissionError::SourceRead),
    }
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

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use nara_fs::{CapabilityRights, ContentDigest, HostCapabilityOptions, TrustMode};

    use super::*;
    use crate::project_host::file_publication::FilePublicationHost;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    const SOURCE: &str = "source.json";
    const MOVED: &str = "moved.json";
    const ALIAS: &str = "alias.json";
    const UNRELATED: &str = "unrelated.json";

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "nara-source-admission-alias-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn capability(&self) -> DirectoryCapability {
            #[cfg(windows)]
            let directory = {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                    FILE_SHARE_READ, FILE_SHARE_WRITE,
                };

                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(&self.0)
                    .unwrap()
            };
            #[cfg(target_os = "linux")]
            let directory = fs::File::open(&self.0).unwrap();
            DirectoryCapability::from_host_handle(
                directory,
                HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for name in [SOURCE, MOVED, ALIAS, UNRELATED] {
                match fs::remove_file(self.0.join(name)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("failed to remove fixture leaf {name}: {error}"),
                }
            }
            fs::remove_dir(&self.0).unwrap();
        }
    }

    #[test]
    fn publication_alias_exclusion_keeps_retained_and_current_namespace_objects_reserved() {
        let fixture = Fixture::new();
        let bytes = b"accepted source";
        fs::write(fixture.0.join(SOURCE), bytes).unwrap();
        fs::write(fixture.0.join(UNRELATED), bytes).unwrap();
        let root = fixture.capability();
        let source = root.open_file(&RelativePath::new(SOURCE).unwrap()).unwrap();
        let mut host = FilePublicationHost::from_existing(
            root,
            nara_fs::RelativeComponent::new(SOURCE).unwrap(),
            source,
            ContentDigest::of_bytes(bytes),
            1024,
        );
        fs::rename(fixture.0.join(SOURCE), fixture.0.join(MOVED)).unwrap();
        fs::hard_link(fixture.0.join(MOVED), fixture.0.join(ALIAS)).unwrap();
        let independent = fixture.capability();
        let alias = independent
            .open_file(&RelativePath::new(ALIAS).unwrap())
            .unwrap();
        let unrelated = independent
            .open_file(&RelativePath::new(UNRELATED).unwrap())
            .unwrap();
        assert_eq!(
            exclude_publication_alias(&alias, host.same_source_object(&alias), host.read_source()),
            Err(SourceAdmissionError::AlreadyOpen)
        );
        assert_eq!(
            exclude_publication_alias(
                &unrelated,
                host.same_source_object(&unrelated),
                host.read_source(),
            ),
            Ok(())
        );

        fs::write(fixture.0.join(SOURCE), b"current replacement").unwrap();
        let current = independent
            .open_file(&RelativePath::new(SOURCE).unwrap())
            .unwrap();
        assert!(!host.same_source_object(&current).unwrap());
        assert_eq!(
            exclude_publication_alias(
                &current,
                host.same_source_object(&current),
                host.read_source(),
            ),
            Err(SourceAdmissionError::AlreadyOpen)
        );
        host.mark_uncertain();
        assert_eq!(host.accepted_source_observation(), None);
        fs::remove_file(fixture.0.join(MOVED)).unwrap();
        fs::remove_file(fixture.0.join(ALIAS)).unwrap();
        for candidate in [&alias, &current] {
            assert_eq!(
                exclude_publication_alias(
                    candidate,
                    host.same_source_object(candidate),
                    host.read_source(),
                ),
                Err(SourceAdmissionError::AlreadyOpen)
            );
        }
        assert_eq!(
            exclude_publication_alias(
                &unrelated,
                host.same_source_object(&unrelated),
                host.read_source(),
            ),
            Ok(())
        );
        assert_eq!(
            exclude_publication_alias(&unrelated, Err(FsError::NotRegularFile), host.read_source(),),
            Err(SourceAdmissionError::SourceRead)
        );
    }
}
