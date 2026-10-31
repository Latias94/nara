use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
};

use nara::project_host::{DesktopRun, DesktopRunOutcome, DesktopRunReport};

/// Keeps the original native action available when its finite retirement drive is incomplete.
#[derive(Debug)]
pub(crate) struct DesktopRunIncomplete {
    pub(crate) owner: DesktopRun,
    report: DesktopRunReport,
}

impl Display for DesktopRunIncomplete {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "desktop retirement remains incomplete: {:?}; owner={:?}",
            self.report.diagnostics(),
            self.owner,
        )
    }
}

impl Error for DesktopRunIncomplete {}

pub(crate) fn finish_desktop_run(
    mut owner: DesktopRun,
) -> Result<DesktopRunReport, Box<dyn Error>> {
    let report = owner.execute();
    match report.outcome() {
        DesktopRunOutcome::Completed(_) => Ok(report),
        DesktopRunOutcome::Failed => {
            Err(io::Error::other(format!("desktop run failed: {:?}", report.diagnostics(),)).into())
        }
        DesktopRunOutcome::CleanupIncomplete => {
            Err(Box::new(DesktopRunIncomplete { owner, report }))
        }
    }
}
