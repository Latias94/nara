//! Ordinary headless product launch owns source selection and process cleanup policy.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara_ecs::Resource;
use nara_fs::FsError;
use nara_gameplay::GameplayCommandSubmission;

use super::{
    PRODUCT_CLEANUP_TIMEOUT, exit_for_incomplete_product_cleanup, open_product_project_root,
};
use crate::{
    ProductRecipe,
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport},
};

/// The ordinary process entrypoint for one headless product run.
///
/// This owner opens the packaged sibling source or supplied development root, then drives runtime
/// cleanup to a terminal result. Use [`HeadlessRun`] directly when an embedded Host must retain an
/// incomplete owner instead of terminating the process.
pub struct HeadlessProductRun<O>
where
    O: Resource + Clone,
{
    development_root: PathBuf,
    intent: HeadlessRunIntent<O>,
    commands: Vec<GameplayCommandSubmission>,
}

impl<O> HeadlessProductRun<O>
where
    O: Resource + Clone,
{
    /// Creates the ordinary headless process run from one intent and semantic command batch.
    #[must_use]
    pub fn new(
        development_root: impl AsRef<Path>,
        intent: HeadlessRunIntent<O>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self {
            development_root: development_root.as_ref().to_path_buf(),
            intent,
            commands,
        }
    }

    /// Creates an exact-tick headless process run from one recipe.
    #[must_use]
    pub fn from_recipe(
        development_root: impl AsRef<Path>,
        recipe: ProductRecipe,
        fixed_ticks: std::num::NonZeroU32,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self::new(
            development_root,
            HeadlessRunIntent::new(fixed_ticks).with_recipe(recipe),
            commands,
        )
    }

    /// Runs to a terminal result, explicitly exiting if live cleanup exceeds its fixed budget.
    ///
    /// The deadline is checked between cleanup drives; callbacks must return promptly.
    /// `Ok(report)` can still report startup, execution, or teardown failure; inspect its outcome.
    pub fn run(self) -> Result<HeadlessRunReport<O>, FsError> {
        let project_root = open_product_project_root(&self.development_root)?;
        let run = HeadlessRun::new(project_root, self.intent, self.commands);
        Ok(finish_product_run(run, PRODUCT_CLEANUP_TIMEOUT))
    }
}

fn finish_product_run<O>(mut run: HeadlessRun<O>, cleanup_timeout: Duration) -> HeadlessRunReport<O>
where
    O: Resource + Clone,
{
    let mut cleanup_deadline = None;
    loop {
        let report = run.execute_bounded();
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        let now = Instant::now();
        let Some(deadline) = cleanup_deadline else {
            cleanup_deadline = Some(now + cleanup_timeout);
            // Drive the retained owner once before checking its cleanup deadline.
            continue;
        };
        if now >= deadline {
            exit_for_incomplete_product_cleanup(
                report.diagnostics(),
                &run.incomplete_cleanup_diagnostics(),
            );
        }
        std::thread::sleep(Duration::from_millis(1).min(deadline.saturating_duration_since(now)));
    }
}
