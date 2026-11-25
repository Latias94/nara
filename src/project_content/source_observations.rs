//! Attempt-local source paths for the original Scene owner, without decoded content retention.

use crate::fs::FileIdentity;
use nara_project::ProjectPath;

use super::*;

#[derive(Debug)]
#[must_use]
pub(crate) struct ProjectSceneLoadAttempt {
    pub(crate) result: Result<ProjectContentSnapshot, ProjectContentError>,
    pub(crate) observations: ProjectSourceObservations,
}

impl ProjectSceneLoadAttempt {
    pub(super) fn new(
        mut result: Result<ProjectContentSnapshot, ProjectContentError>,
        observations: ProjectSourceObservations,
    ) -> Self {
        if let Some(error) = &observations.refusal {
            result = Err(error.clone());
        }
        Self {
            result,
            observations,
        }
    }
}

/// Exact attempted file paths, relative to this load's original project root.
///
/// Both successful and failed opens are retained. This is an executed read cut, not an admitted
/// content closure: successful publication uses the snapshot's immutable `content_closure`.
/// A refused cut must not replace a complete Scene subscription. It retains no file capability,
/// encoded source, decoded product, or content generation.
pub(crate) struct ProjectSourceObservations {
    project_root_identity: FileIdentity,
    paths: Vec<AssetSourcePath>,
    refusal: Option<ProjectContentError>,
    _budget: BudgetTicket,
    #[cfg(test)]
    fail_next_path_allocation: bool,
}

impl fmt::Debug for ProjectSourceObservations {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectSourceObservations")
            .field("project_root_identity", &self.project_root_identity)
            .field("paths", &self.paths)
            .field("complete", &self.is_complete())
            .finish_non_exhaustive()
    }
}

impl ProjectSourceObservations {
    pub(super) fn new(loader: &ProjectContentLoader) -> Self {
        Self {
            project_root_identity: loader.root.identity(),
            paths: Vec::new(),
            refusal: None,
            _budget: loader.budget_host.reserve(),
            #[cfg(test)]
            fail_next_path_allocation: false,
        }
    }

    pub(crate) fn project_root_identity(&self) -> FileIdentity {
        self.project_root_identity
    }

    pub(crate) fn paths(&self) -> &[AssetSourcePath] {
        &self.paths
    }

    /// False means observation capacity was refused; the retained prefix is not a full cut.
    pub(crate) fn is_complete(&self) -> bool {
        self.refusal.is_none()
    }

    pub(super) fn record(
        &mut self,
        source_root: Option<&ProjectPath>,
        path: &str,
    ) -> Result<(), ProjectContentError> {
        if let Some(error) = &self.refusal {
            return Err(error.clone());
        }
        let work_before = self._budget.value(ProjectContentBudgetKind::WorkBytes);
        let files_before = self._budget.value(ProjectContentBudgetKind::Files);
        let retained_before = self._budget.value(ProjectContentBudgetKind::RetainedBytes);
        let result = self.record_inner(source_root, path);
        self._budget
            .set(ProjectContentBudgetKind::WorkBytes, work_before)
            .expect("restoring observation work cannot exceed its limit");
        if let Err(error) = &result {
            self._budget
                .set_many(&[
                    (ProjectContentBudgetKind::Files, files_before),
                    (ProjectContentBudgetKind::RetainedBytes, retained_before),
                ])
                .expect("restoring observation retention cannot exceed its limits");
            self.refusal = Some(error.clone());
        }
        result
    }

    fn record_inner(
        &mut self,
        source_root: Option<&ProjectPath>,
        path: &str,
    ) -> Result<(), ProjectContentError> {
        let index = match self.paths.binary_search_by(|canonical| match source_root {
            Some(root) => canonical.as_str().bytes().cmp(
                root.as_str()
                    .bytes()
                    .chain(std::iter::once(b'/'))
                    .chain(path.bytes()),
            ),
            None => canonical.as_str().cmp(path),
        }) {
            Ok(_) => return Ok(()),
            Err(index) => index,
        };
        let path_bytes = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                source_root.map_or(0, |root| root.as_str().len()),
                usize::from(source_root.is_some()),
                path.len(),
            ],
        )?;
        let planned_string_bytes = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [path_bytes, STRING_ALLOCATION_OVERHEAD],
        )?;
        self._budget
            .add(ProjectContentBudgetKind::WorkBytes, planned_string_bytes)
            .map_err(ProjectContentError::budget)?;
        let mut canonical = String::new();
        canonical
            .try_reserve_exact(path_bytes)
            .map_err(|_| allocation_failed("project.content.source-observation-path-allocation"))?;
        let string_bytes = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [canonical.capacity(), STRING_ALLOCATION_OVERHEAD],
        )?;
        self._budget
            .add(
                ProjectContentBudgetKind::WorkBytes,
                canonical.capacity() - path_bytes,
            )
            .map_err(ProjectContentError::budget)?;
        if let Some(root) = source_root {
            canonical.push_str(root.as_str());
            canonical.push('/');
        }
        canonical.push_str(path);
        let canonical = AssetSourcePath::new(canonical).map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::InvalidLogicalPath,
                "project.content.observation-path-invalid",
                "Source observation has an invalid project-relative path",
            )
        })?;
        let retained = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                self._budget.value(ProjectContentBudgetKind::RetainedBytes),
                size_of::<AssetSourcePath>(),
                string_bytes,
            ],
        )?;
        let count = checked_plan_sum(
            ProjectContentBudgetKind::Files,
            [self._budget.value(ProjectContentBudgetKind::Files), 1],
        )?;
        let movement = if self.paths.len() == self.paths.capacity() {
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                self.paths.capacity(),
                size_of::<AssetSourcePath>(),
            )?
        } else {
            0
        };
        self._budget
            .set_many(&[
                (ProjectContentBudgetKind::Files, count),
                (ProjectContentBudgetKind::RetainedBytes, retained),
                (ProjectContentBudgetKind::WorkBytes, movement),
            ])
            .map_err(ProjectContentError::budget)?;
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_path_allocation) {
            return Err(allocation_failed(
                "project.content.source-observation-allocation",
            ));
        }
        self.paths
            .try_reserve_exact(1)
            .map_err(|_| allocation_failed("project.content.source-observation-allocation"))?;
        self.paths.insert(index, canonical);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_path_allocation(&mut self) {
        self.fail_next_path_allocation = true;
    }
}
