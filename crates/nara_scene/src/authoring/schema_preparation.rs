//! Shared byte accounting for finite Schema source preparation.
//!
//! Work is cumulative. Retained incumbents, frozen inputs and outputs stay charged unless their
//! enclosing preparation attempt refuses and drops them. Step scratch is temporary; the logical
//! peak is not measured RSS.
//! Callers supply owning cost models and map refusal details to their operation's diagnostics.

use nara_core::ByteLimit;

#[derive(Debug)]
pub(super) struct SchemaPreparationBudget {
    work_limit: ByteLimit,
    retained_limit: ByteLimit,
    work_bytes: usize,
    peak_retained_bytes: usize,
    retained: usize,
    scratch: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SchemaPreparationError {
    pub(super) kind: &'static str,
    pub(super) stage: &'static str,
}

impl SchemaPreparationBudget {
    pub(super) const fn new(work: ByteLimit, retained: ByteLimit) -> Self {
        Self {
            work_limit: work,
            retained_limit: retained,
            work_bytes: 0,
            peak_retained_bytes: 0,
            retained: 0,
            scratch: 0,
        }
    }

    pub(super) const fn usage(&self) -> (usize, usize) {
        (self.work_bytes, self.peak_retained_bytes)
    }

    pub(super) const fn retention_checkpoint(&self) -> (usize, usize) {
        (self.retained, self.scratch)
    }

    /// Restores entry storage only after a refused attempt has dropped its charged outputs.
    /// Performed work and the historical peak remain cumulative.
    pub(super) fn restore_retention(&mut self, checkpoint: (usize, usize)) {
        (self.retained, self.scratch) = checkpoint;
    }

    pub(super) fn retain(&mut self, bytes: Option<usize>) -> Result<(), SchemaPreparationError> {
        let next = bytes.and_then(|bytes| self.retained.checked_add(bytes));
        let Some(next) = next.filter(|next| *next <= self.retained_limit.get()) else {
            return Err(limit_error("retained", "incumbent-or-candidate"));
        };
        self.retained = next;
        self.observe_peak()
    }

    pub(super) fn begin_step(&mut self, held: Option<usize>) -> Result<(), SchemaPreparationError> {
        self.scratch = held.ok_or_else(|| limit_error("retained", "history-branch"))?;
        self.observe_peak()
    }

    pub(super) fn reserve(
        &mut self,
        stage: &'static str,
        bytes: Option<usize>,
    ) -> Result<(), SchemaPreparationError> {
        let work = bytes.and_then(|bytes| self.work_bytes.checked_add(bytes));
        let Some(work) = work.filter(|work| *work <= self.work_limit.get()) else {
            return Err(limit_error("work", stage));
        };
        let scratch = bytes.and_then(|bytes| self.scratch.checked_add(bytes));
        let Some(scratch) = scratch else {
            return Err(limit_error("retained", stage));
        };
        let total = self.retained.checked_add(scratch);
        if total.is_none_or(|total| total > self.retained_limit.get()) {
            return Err(limit_error("retained", stage));
        }
        self.work_bytes = work;
        self.scratch = scratch;
        self.observe_peak()
    }

    pub(super) fn retain_output(
        &mut self,
        bytes: Option<usize>,
    ) -> Result<(), SchemaPreparationError> {
        let bytes = bytes.ok_or_else(|| limit_error("retained", "classification-output"))?;
        if bytes > self.scratch {
            return Err(limit_error("retained", "classification-admission-model"));
        }
        self.scratch = 0;
        self.retain(Some(bytes))
    }

    pub(super) fn end_step(&mut self) {
        self.scratch = 0;
    }

    fn observe_peak(&mut self) -> Result<(), SchemaPreparationError> {
        let total = self
            .retained
            .checked_add(self.scratch)
            .filter(|total| *total <= self.retained_limit.get())
            .ok_or_else(|| limit_error("retained", "retained-and-scratch"))?;
        self.peak_retained_bytes = self.peak_retained_bytes.max(total);
        Ok(())
    }
}

fn limit_error(kind: &'static str, stage: &'static str) -> SchemaPreparationError {
    SchemaPreparationError { kind, stage }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(work: usize, retained: usize) -> SchemaPreparationBudget {
        SchemaPreparationBudget::new(
            ByteLimit::new(work).unwrap(),
            ByteLimit::new(retained).unwrap(),
        )
    }

    #[test]
    fn work_and_outputs_accumulate_while_step_scratch_is_released() {
        let mut budget = budget(13, 17);
        budget.retain(Some(6)).unwrap();
        budget.begin_step(Some(3)).unwrap();
        budget.reserve("first-output", Some(4)).unwrap();
        assert_eq!(budget.usage(), (4, 13));
        budget.retain_output(Some(4)).unwrap();

        budget.begin_step(Some(2)).unwrap();
        budget.reserve("temporary", Some(3)).unwrap();
        assert_eq!(budget.usage(), (7, 15));
        budget.end_step();

        budget.begin_step(Some(1)).unwrap();
        budget.reserve("second-output", Some(6)).unwrap();
        budget.retain_output(Some(6)).unwrap();
        budget.begin_step(Some(1)).unwrap();
        assert_eq!(budget.usage(), (13, 17));
        assert_eq!(
            budget.reserve("next-step", Some(1)),
            Err(limit_error("work", "next-step")),
        );
        assert_eq!(budget.usage(), (13, 17));
    }

    #[test]
    fn reservation_checks_work_first_and_never_charges_a_refused_reservation() {
        let mut budget = budget(4, 4);
        budget.retain(Some(3)).unwrap();
        budget.begin_step(Some(0)).unwrap();
        for bytes in [None, Some(5)] {
            assert_eq!(
                budget.reserve("both-limits", bytes),
                Err(limit_error("work", "both-limits")),
            );
        }
        assert_eq!(
            budget.reserve("retention", Some(2)),
            Err(limit_error("retained", "retention")),
        );
        assert_eq!(budget.usage(), (0, 3));
        budget.reserve("exact", Some(1)).unwrap();
        assert_eq!(budget.usage(), (1, 4));
    }

    #[test]
    fn refused_attempt_restores_prior_retained_and_scratch_without_refunding_work_or_peak() {
        let mut budget = budget(30, 20);
        budget.retain(Some(4)).unwrap();
        budget.begin_step(Some(3)).unwrap();
        let checkpoint = budget.retention_checkpoint();

        budget.reserve("discarded-output", Some(6)).unwrap();
        budget.retain_output(Some(6)).unwrap();
        budget.reserve("discarded-scratch", Some(10)).unwrap();
        assert_eq!(budget.usage(), (16, 20));
        budget.restore_retention(checkpoint);
        assert_eq!(budget.usage(), (16, 20));

        // Exactly the prior 4 retained bytes and 3 scratch bytes still constrain the retry.
        budget.reserve("retry", Some(13)).unwrap();
        assert_eq!(budget.usage(), (29, 20));
        assert_eq!(
            budget.reserve("retention-exhausted", Some(1)),
            Err(limit_error("retained", "retention-exhausted")),
        );
    }

    #[test]
    fn arithmetic_overflow_refuses_with_the_original_kind_and_stage() {
        let mut budget = budget(usize::MAX, usize::MAX);
        budget.retain(Some(usize::MAX - 1)).unwrap();
        budget.reserve("exact", Some(1)).unwrap();
        assert_eq!(budget.usage(), (1, usize::MAX));
        assert_eq!(
            budget.reserve("total-overflow", Some(1)),
            Err(limit_error("retained", "total-overflow")),
        );
        assert_eq!(
            budget.begin_step(Some(usize::MAX)),
            Err(limit_error("retained", "retained-and-scratch")),
        );
        assert_eq!(
            budget.reserve("scratch-overflow", Some(1)),
            Err(limit_error("retained", "scratch-overflow")),
        );
        assert_eq!(
            budget.reserve("work-overflow", Some(usize::MAX)),
            Err(limit_error("work", "work-overflow")),
        );
        assert_eq!(budget.usage(), (1, usize::MAX));
    }
}
