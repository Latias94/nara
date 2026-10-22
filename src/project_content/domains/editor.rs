use nara_fs::{ContentDigest, FileIdentity, RelativePath};

use super::*;

struct ObservedSource {
    path: AssetPath,
    identity: FileIdentity,
    digest: ContentDigest,
    bytes: Arc<[u8]>,
}

pub(crate) struct EditorSourceValidation {
    sources: Vec<ObservedSource>,
    _budget: budget::BudgetTicket,
}

impl EditorSourceValidation {
    pub(crate) fn revalidate(
        &self,
        assets: &DirectoryCapability,
    ) -> Result<(), ProjectContentError> {
        for observed in &self.sources {
            let path = RelativePath::new(observed.path.as_str()).map_err(|_| source_changed())?;
            let file = assets.open_file(&path).map_err(|_| source_changed())?;
            if file.identity() != observed.identity {
                return Err(source_changed());
            }
            file.verify_digest(observed.digest, observed.bytes.len() as u64)
                .map_err(|_| source_changed())?;
        }
        Ok(())
    }
}

struct EditorSourceReader<'a> {
    assets: &'a DirectoryCapability,
    budget: budget::BudgetTicket,
    sources: Vec<ObservedSource>,
}

impl EditorSourceReader<'_> {
    fn read(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, ProjectContentError> {
        if let Some(source) = self.sources.iter().find(|source| source.path == *path) {
            if source.bytes.len() > limit.get() {
                return Err(source_changed());
            }
            return Ok(Arc::clone(&source.bytes));
        }
        self.budget
            .add(ProjectContentBudgetKind::Files, 1)
            .and_then(|()| {
                self.budget
                    .add(ProjectContentBudgetKind::PathBytes, path.as_str().len())
            })
            .and_then(|()| {
                self.budget.add(
                    ProjectContentBudgetKind::WorkBytes,
                    limit
                        .get()
                        .saturating_mul(2)
                        .saturating_add(size_of::<ObservedSource>()),
                )
            })
            .map_err(ProjectContentError::budget)?;
        let relative = RelativePath::new(path.as_str()).map_err(|_| source_changed())?;
        let file = self
            .assets
            .open_file(&relative)
            .map_err(|_| source_changed())?;
        let bytes = file
            .read_to_end_bounded(limit.get() as u64)
            .map_err(|_| source_changed())?;
        self.budget.subtract(
            ProjectContentBudgetKind::WorkBytes,
            (limit.get() - bytes.len()) * 2,
        );
        let digest = ContentDigest::of_bytes(&bytes);
        let bytes: Arc<[u8]> = bytes.into();
        self.sources.push(ObservedSource {
            path: path.clone(),
            identity: file.identity(),
            digest,
            bytes: Arc::clone(&bytes),
        });
        Ok(bytes)
    }
}

impl SourceReader for EditorSourceReader<'_> {
    fn read_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, ProjectContentError> {
        self.budget
            .add(ProjectContentBudgetKind::DependencyEdges, 1)
            .map_err(ProjectContentError::budget)?;
        self.read(path, limit)
    }
}

pub(crate) fn validate_editor_source<D: ProjectAssetDomain>(
    assets: &DirectoryCapability,
    metadata: &AssetMetaIdentityCandidate,
    expected_source: FileIdentity,
    budget: &ProjectContentBudgetHost,
) -> Result<EditorSourceValidation, ProjectContentError> {
    let limits = D::limits();
    let mut ticket = budget.reserve();
    let reserved = limits
        .work_bytes
        .get()
        .checked_add(limits.retained_bytes.get())
        .and_then(|value| metadata.retained_bytes()?.checked_add(value))
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
    ticket
        .add(ProjectContentBudgetKind::WorkBytes, reserved)
        .map_err(ProjectContentError::budget)?;
    let mut reader = EditorSourceReader {
        assets,
        budget: ticket,
        sources: Vec::new(),
    };
    let bytes = reader.read(metadata.identity().path(), limits.source_bytes)?;
    if reader.sources[0].identity != expected_source {
        return Err(source_changed());
    }
    let prepared = catch_unwind(AssertUnwindSafe(|| {
        D::prepare(&mut ProjectAssetSource {
            metadata,
            bytes,
            reader: &mut reader,
        })
        .map(drop)
    }));
    prepared.map_err(|_| {
        ProjectContentError::single(
            ProjectContentErrorKind::AssetDomain,
            "project.content.domain-panicked",
            "The asset domain could not prepare this source",
        )
    })??;
    Ok(EditorSourceValidation {
        sources: reader.sources,
        _budget: reader.budget,
    })
}

fn source_changed() -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::AssetDomain,
        "project.editor.source-import-source-changed",
        "A required asset source is unavailable or changed",
    )
}
