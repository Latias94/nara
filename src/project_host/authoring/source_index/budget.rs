use super::{
    ProjectSourceIndexBudgetUsage, ProjectSourceIndexDiagnostic, ProjectSourceIndexDiagnosticCode,
    ProjectSourceIndexLimits,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceIndexBudgetError {
    pub(super) code: ProjectSourceIndexDiagnosticCode,
}

pub(super) struct SourceIndexBudget {
    limits: ProjectSourceIndexLimits,
    usage: ProjectSourceIndexBudgetUsage,
}

impl SourceIndexBudget {
    pub(super) const fn new(limits: ProjectSourceIndexLimits) -> Self {
        Self {
            limits,
            usage: ProjectSourceIndexBudgetUsage {
                entries: 0,
                directories: 0,
                max_depth: 0,
                path_bytes: 0,
                diagnostics: 0,
                diagnostic_bytes: 0,
                retained_snapshot_bytes: 0,
            },
        }
    }

    pub(super) fn charge_entry(&mut self, path_bytes: usize) -> Result<(), SourceIndexBudgetError> {
        self.usage.entries = checked_increment(
            self.usage.entries,
            self.limits.max_entries,
            ProjectSourceIndexDiagnosticCode::EntryBudgetExceeded,
        )?;
        self.usage.path_bytes = checked_add_bounded(
            self.usage.path_bytes,
            path_bytes,
            self.limits.max_path_bytes,
            ProjectSourceIndexDiagnosticCode::PathBudgetExceeded,
        )?;
        Ok(())
    }

    pub(super) fn charge_directory(&mut self, depth: usize) -> Result<(), SourceIndexBudgetError> {
        if depth > self.limits.max_depth {
            return Err(SourceIndexBudgetError {
                code: ProjectSourceIndexDiagnosticCode::DepthBudgetExceeded,
            });
        }
        self.usage.directories = checked_increment(
            self.usage.directories,
            self.limits.max_directories,
            ProjectSourceIndexDiagnosticCode::DirectoryBudgetExceeded,
        )?;
        self.usage.max_depth = self.usage.max_depth.max(depth);
        Ok(())
    }

    pub(super) fn charge_diagnostic(
        &mut self,
        diagnostic: &ProjectSourceIndexDiagnostic,
    ) -> Result<(), SourceIndexBudgetError> {
        self.usage.diagnostics = checked_increment(
            self.usage.diagnostics,
            self.limits.max_diagnostics,
            ProjectSourceIndexDiagnosticCode::DiagnosticBudgetExceeded,
        )?;
        self.usage.diagnostic_bytes = checked_add_bounded(
            self.usage.diagnostic_bytes,
            diagnostic.text_bytes(),
            self.limits.max_diagnostic_bytes,
            ProjectSourceIndexDiagnosticCode::DiagnosticBudgetExceeded,
        )?;
        self.charge_retained(diagnostic.retained_bytes())
    }

    pub(super) fn charge_retained(&mut self, bytes: usize) -> Result<(), SourceIndexBudgetError> {
        self.usage.retained_snapshot_bytes = checked_add_bounded(
            self.usage.retained_snapshot_bytes,
            bytes,
            self.limits.max_retained_snapshot_bytes,
            ProjectSourceIndexDiagnosticCode::RetainedSnapshotBudgetExceeded,
        )?;
        Ok(())
    }

    pub(super) const fn usage(&self) -> ProjectSourceIndexBudgetUsage {
        self.usage
    }
}

fn checked_increment(
    current: usize,
    maximum: usize,
    code: ProjectSourceIndexDiagnosticCode,
) -> Result<usize, SourceIndexBudgetError> {
    checked_add_bounded(current, 1, maximum, code)
}

fn checked_add_bounded(
    current: usize,
    additional: usize,
    maximum: usize,
    code: ProjectSourceIndexDiagnosticCode,
) -> Result<usize, SourceIndexBudgetError> {
    let total = current
        .checked_add(additional)
        .ok_or(SourceIndexBudgetError { code })?;
    if total > maximum {
        return Err(SourceIndexBudgetError { code });
    }
    Ok(total)
}
