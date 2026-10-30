//! Checked accounting for the Input owner's shared ordered event log.
//!
//! The owner supplies the complete remaining terminal obligation after each operation. This
//! ledger does not infer endpoint transitions, identify duplicate platform notifications, or track
//! consumer progress. It accounts for counts only; the owner must commit the matching records and
//! topology together and reclaim storage only after the real records have been removed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputCapacityError {
    Capacity,
    Sequence,
    Arithmetic,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct InputCapacityState {
    retained: usize,
    obligations: usize,
    next_sequence: u64,
}

/// One Input owner's retained records and unspent terminal credits.
#[derive(Debug)]
pub(super) struct InputCapacity {
    limit: usize,
    state: InputCapacityState,
}

/// A checked transition, not a reservation. Dropping it changes nothing.
#[must_use = "commit the matching Input operation or discard its capacity plan"]
#[derive(Debug)]
pub(super) struct InputCapacityPlan {
    limit: usize,
    before: InputCapacityState,
    after: InputCapacityState,
}

impl InputCapacity {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            state: InputCapacityState::default(),
        }
    }

    pub(super) const fn retained(&self) -> usize {
        self.state.retained
    }

    pub(super) const fn obligations(&self) -> usize {
        self.state.obligations
    }

    /// The last assigned sequence, matching `InputTimeline::next_sequence`.
    pub(super) const fn next_sequence(&self) -> u64 {
        self.state.next_sequence
    }

    /// Checks all new records and the complete obligations remaining after this operation.
    ///
    /// No records are reclaimed here, including completed records behind a lagging prefix.
    /// Ordinary admission may acquire new obligations or revoke obligations that no longer apply.
    pub(super) fn plan(
        &self,
        drafts: usize,
        obligations_after: usize,
    ) -> Result<InputCapacityPlan, InputCapacityError> {
        let retained = self
            .state
            .retained
            .checked_add(drafts)
            .ok_or(InputCapacityError::Arithmetic)?;
        let occupied = retained
            .checked_add(obligations_after)
            .ok_or(InputCapacityError::Arithmetic)?;
        if occupied > self.limit {
            return Err(InputCapacityError::Capacity);
        }

        let drafts = u64::try_from(drafts).map_err(|_| InputCapacityError::Arithmetic)?;
        let obligations =
            u64::try_from(obligations_after).map_err(|_| InputCapacityError::Arithmetic)?;
        let next_sequence = self
            .state
            .next_sequence
            .checked_add(drafts)
            .ok_or(InputCapacityError::Sequence)?;
        next_sequence
            .checked_add(obligations)
            .ok_or(InputCapacityError::Sequence)?;

        Ok(InputCapacityPlan {
            limit: self.limit,
            before: self.state,
            after: InputCapacityState {
                retained,
                obligations: obligations_after,
                next_sequence,
            },
        })
    }

    /// Converts existing terminal credits into records, optionally refunding obsolete credits.
    ///
    /// Normal releases also use this path. Each draft must consume an outstanding credit, even
    /// when ordinary storage remains available. This checks aggregate spending only; the owner
    /// validates which member, pointer or control obligation is consumed and prevents duplicates.
    pub(super) fn plan_terminal(
        &self,
        drafts: usize,
        obligations_after: usize,
    ) -> Result<InputCapacityPlan, InputCapacityError> {
        let credits = self
            .state
            .obligations
            .checked_sub(obligations_after)
            .ok_or(InputCapacityError::Arithmetic)?;
        if drafts > credits {
            return Err(InputCapacityError::Arithmetic);
        }
        self.plan(drafts, obligations_after)
    }

    /// Commits only while the checked predecessor still matches this owner's accounting.
    ///
    /// The owner must check and commit within the same mutation, before publishing its matching
    /// records. A stale plan is a caller accounting error and leaves this ledger unchanged.
    /// Matching counts are not an owner identity or topology epoch: never carry a plan across
    /// owner operations or apply it to another ledger, even when their counts happen to match.
    pub(super) fn commit(&mut self, plan: InputCapacityPlan) -> Result<(), InputCapacityError> {
        if self.limit != plan.limit || self.state != plan.before {
            return Err(InputCapacityError::Arithmetic);
        }
        self.state = plan.after;
        Ok(())
    }

    /// Reports records actually removed after all of their consumers finished.
    ///
    /// Clearing ingress state or advancing a single consumer is not reclamation. This operation
    /// changes neither terminal obligations nor sequence headroom.
    pub(super) fn reclaim(&mut self, records: usize) -> Result<(), InputCapacityError> {
        let retained = self
            .state
            .retained
            .checked_sub(records)
            .ok_or(InputCapacityError::Arithmetic)?;
        self.state.retained = retained;
        Ok(())
    }
}

impl InputCapacityPlan {
    pub(super) const fn first_sequence(&self) -> Option<u64> {
        if self.before.next_sequence == self.after.next_sequence {
            None
        } else {
            // A nonempty plan already checked that its last sequence fits.
            Some(self.before.next_sequence + 1)
        }
    }

    pub(super) const fn last_sequence(&self) -> Option<u64> {
        if self.before.next_sequence == self.after.next_sequence {
            None
        } else {
            Some(self.after.next_sequence)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are algebra and ordered-merge controls, not endpoint or real-cursor integration tests.
    // Focus loss, close request and physical retirement are three distinct control obligations.
    const HELD: usize = 3;
    const POINTERS: usize = 2;
    const CONTROLS: usize = 3;
    const MIXED: usize = HELD + POINTERS + CONTROLS;

    /// Test-only alternative with separate ordinary and terminal storage and one sequence domain.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct DedicatedCapacity {
        ordinary_limit: usize,
        terminal_limit: usize,
        ordinary_retained: usize,
        terminal_retained: usize,
        obligations: usize,
        next_sequence: u64,
    }

    impl DedicatedCapacity {
        fn new(ordinary_limit: usize, terminal_limit: usize) -> Self {
            Self {
                ordinary_limit,
                terminal_limit,
                ordinary_retained: 0,
                terminal_retained: 0,
                obligations: 0,
                next_sequence: 0,
            }
        }

        fn admit(
            &mut self,
            ordinary: usize,
            terminal: usize,
            obligations_after: usize,
        ) -> Result<Option<(u64, u64)>, InputCapacityError> {
            if terminal != 0 {
                let spent = self
                    .obligations
                    .checked_sub(obligations_after)
                    .ok_or(InputCapacityError::Arithmetic)?;
                if terminal > spent {
                    return Err(InputCapacityError::Arithmetic);
                }
            }
            let ordinary_retained = self
                .ordinary_retained
                .checked_add(ordinary)
                .ok_or(InputCapacityError::Arithmetic)?;
            let terminal_retained = self
                .terminal_retained
                .checked_add(terminal)
                .ok_or(InputCapacityError::Arithmetic)?;
            let terminal_occupied = terminal_retained
                .checked_add(obligations_after)
                .ok_or(InputCapacityError::Arithmetic)?;
            if ordinary_retained > self.ordinary_limit || terminal_occupied > self.terminal_limit {
                return Err(InputCapacityError::Capacity);
            }
            let drafts = ordinary
                .checked_add(terminal)
                .ok_or(InputCapacityError::Arithmetic)?;
            let drafts = u64::try_from(drafts).map_err(|_| InputCapacityError::Arithmetic)?;
            let obligations =
                u64::try_from(obligations_after).map_err(|_| InputCapacityError::Arithmetic)?;
            let last = self
                .next_sequence
                .checked_add(drafts)
                .ok_or(InputCapacityError::Sequence)?;
            last.checked_add(obligations)
                .ok_or(InputCapacityError::Sequence)?;
            let range = if drafts == 0 {
                None
            } else {
                Some((self.next_sequence + 1, last))
            };
            self.ordinary_retained = ordinary_retained;
            self.terminal_retained = terminal_retained;
            self.obligations = obligations_after;
            self.next_sequence = last;
            Ok(range)
        }

        fn reclaim(&mut self, ordinary: usize, terminal: usize) -> Result<(), InputCapacityError> {
            let ordinary_retained = self
                .ordinary_retained
                .checked_sub(ordinary)
                .ok_or(InputCapacityError::Arithmetic)?;
            let terminal_retained = self
                .terminal_retained
                .checked_sub(terminal)
                .ok_or(InputCapacityError::Arithmetic)?;
            self.ordinary_retained = ordinary_retained;
            self.terminal_retained = terminal_retained;
            Ok(())
        }
    }

    #[test]
    fn planning_is_pure_and_stale_terminal_plans_cannot_spend_twice() {
        let mut capacity = InputCapacity::new(4);
        let initial = capacity.state;
        {
            let unused = capacity.plan(1, 1).unwrap();
            assert_eq!(capacity.state, initial);
            assert_eq!(unused.first_sequence(), Some(1));
            assert_eq!(unused.last_sequence(), Some(1));
        }
        assert_eq!(capacity.state, initial);

        let reserve = capacity.plan(0, 1).unwrap();
        assert_eq!(reserve.first_sequence(), None);
        assert_eq!(reserve.last_sequence(), None);
        capacity.commit(reserve).unwrap();
        let first = capacity.plan_terminal(1, 0).unwrap();
        let duplicate = capacity.plan_terminal(1, 0).unwrap();
        capacity.commit(first).unwrap();
        let committed = capacity.state;
        assert_eq!(
            capacity.commit(duplicate),
            Err(InputCapacityError::Arithmetic)
        );
        assert_eq!(capacity.state, committed);
        assert_eq!(
            capacity.plan_terminal(1, 0).unwrap_err(),
            InputCapacityError::Arithmetic
        );
        assert_eq!(capacity.retained(), 1);
        assert_eq!(capacity.obligations(), 0);
    }

    #[test]
    fn mixed_last_press_needs_nine_shared_slots_or_one_plus_eight_dedicated_slots() {
        assert_eq!(MIXED, 8);
        let short = InputCapacity::new(MIXED);
        assert_eq!(
            short.plan(1, MIXED).unwrap_err(),
            InputCapacityError::Capacity
        );
        assert_eq!(short.state, InputCapacityState::default());

        let mut dedicated_short = DedicatedCapacity::new(1, MIXED - 1);
        let initial = dedicated_short;
        assert_eq!(
            dedicated_short.admit(1, 0, MIXED),
            Err(InputCapacityError::Capacity)
        );
        assert_eq!(dedicated_short, initial);

        let mut shared = InputCapacity::new(MIXED + 1);
        let mut dedicated = DedicatedCapacity::new(1, MIXED);
        let last_press = shared.plan(1, MIXED).unwrap();
        assert_eq!(last_press.first_sequence(), Some(1));
        shared.commit(last_press).unwrap();
        assert_eq!(dedicated.admit(1, 0, MIXED).unwrap(), Some((1, 1)));

        // The modeled last press record has actually been removed; its terminal promises remain.
        shared.reclaim(1).unwrap();
        dedicated.reclaim(1, 0).unwrap();
        assert_eq!(shared.obligations(), MIXED);
        let prefix = shared.plan(1, MIXED).unwrap();
        assert_eq!(prefix.first_sequence(), Some(2));
        shared.commit(prefix).unwrap();
        assert_eq!(dedicated.admit(1, 0, MIXED).unwrap(), Some((2, 2)));
        assert_eq!(
            shared.plan(1, MIXED).unwrap_err(),
            InputCapacityError::Capacity
        );

        // All records remain held by a modeled lagging consumer. Only the explicit reclaim below
        // releases storage. The shared timeline and dedicated ordered merge assign identical ranges.
        for (drafts, obligations_after, first, last, retained) in [
            (HELD + POINTERS + 1, 2, 3, 8, 7), // Focus loss.
            (1, 1, 9, 9, 8),                   // Close request.
            (1, 0, 10, 10, 9),                 // Physical retirement.
        ] {
            let plan = shared.plan_terminal(drafts, obligations_after).unwrap();
            assert_eq!(plan.first_sequence(), Some(first));
            assert_eq!(plan.last_sequence(), Some(last));
            shared.commit(plan).unwrap();
            assert_eq!(
                dedicated.admit(0, drafts, obligations_after).unwrap(),
                Some((first, last))
            );
            assert_eq!(shared.retained(), retained);
            assert_eq!(shared.obligations(), obligations_after);
            assert_eq!(shared.retained() + shared.obligations(), MIXED + 1);
            assert_eq!(dedicated.ordinary_retained, 1);
            assert_eq!(dedicated.terminal_retained + dedicated.obligations, MIXED);
        }

        let before = shared.state;
        assert_eq!(shared.plan(1, 0).unwrap_err(), InputCapacityError::Capacity);
        assert_eq!(
            shared.reclaim(MIXED + 2),
            Err(InputCapacityError::Arithmetic)
        );
        assert_eq!(shared.state, before);
        shared.reclaim(MIXED + 1).unwrap();
        dedicated.reclaim(1, MIXED).unwrap();
        assert_eq!(shared.retained(), 0);
        assert_eq!(shared.next_sequence(), 10);
        assert_eq!(dedicated.ordinary_retained, 0);
        assert_eq!(dedicated.terminal_retained, 0);
        assert_eq!(dedicated.next_sequence, shared.next_sequence());
    }

    #[test]
    fn geometry_adds_a_credit_and_cannot_refund_an_unpositioned_pointer_leave() {
        let geometry = 1;
        let obligations = MIXED + geometry;
        let short = InputCapacity::new(obligations);
        assert_eq!(
            short.plan(1, obligations).unwrap_err(),
            InputCapacityError::Capacity
        );
        let mut shared = InputCapacity::new(obligations + 1);
        let mut dedicated = DedicatedCapacity::new(1, obligations);
        let press = shared.plan(1, obligations).unwrap();
        shared.commit(press).unwrap();
        dedicated.admit(1, 0, obligations).unwrap();
        shared.reclaim(1).unwrap();
        dedicated.reclaim(1, 0).unwrap();

        let prefix = shared.plan(1, obligations).unwrap();
        shared.commit(prefix).unwrap();
        dedicated.admit(1, 0, obligations).unwrap();
        let resize = shared.plan_terminal(1, MIXED).unwrap();
        shared.commit(resize).unwrap();
        dedicated.admit(0, 1, MIXED).unwrap();
        assert_eq!(shared.obligations(), HELD + POINTERS + CONTROLS);

        // Clearing positions does not end a held pointer's leave promise. An explicit Left for one
        // now-unpositioned pointer still needs its button release and a leave, not just the release.
        let after_left = (HELD - 1) + (POINTERS - 1) + CONTROLS;
        let left = shared.plan_terminal(2, after_left).unwrap();
        shared.commit(left).unwrap();
        dedicated.admit(0, 2, after_left).unwrap();
        assert_eq!(shared.retained(), 4);
        assert_eq!(shared.obligations(), 6);
        assert_eq!(shared.retained() + shared.obligations(), obligations + 1);
        assert_eq!(
            dedicated.terminal_retained + dedicated.obligations,
            obligations
        );
    }

    #[test]
    fn release_only_preserves_pointer_and_window_credits_until_their_transitions() {
        let mut capacity = InputCapacity::new(6);
        let press = capacity.plan(1, 1 + 1 + CONTROLS).unwrap();
        capacity.commit(press).unwrap();
        capacity.reclaim(1).unwrap();

        let release = capacity.plan_terminal(1, 1 + CONTROLS).unwrap();
        capacity.commit(release).unwrap();
        assert_eq!(capacity.retained(), 1);
        assert_eq!(capacity.obligations(), 1 + CONTROLS);
        let left = capacity.plan_terminal(1, CONTROLS).unwrap();
        capacity.commit(left).unwrap();
        assert_eq!(capacity.retained(), 2);
        assert_eq!(capacity.obligations(), CONTROLS);
        assert_eq!(
            capacity.plan_terminal(1, CONTROLS).unwrap_err(),
            InputCapacityError::Arithmetic
        );
    }

    #[test]
    fn focus_can_refund_unused_pointer_credits_without_reclaiming_any_record() {
        let held = 1;
        let known_pointers = 2;
        let positioned_pointers = 1;
        let obligations = held + known_pointers + CONTROLS;
        let mut capacity = InputCapacity::new(obligations + 1);
        let prefix = capacity.plan(1, obligations).unwrap();
        capacity.commit(prefix).unwrap();

        // The caller ends both pointer obligations, emits only the positioned pointer's leave,
        // and retains the distinct close/retire controls. The unused leave is a credit refund.
        let drafts = held + positioned_pointers + 1;
        let focus = capacity.plan_terminal(drafts, CONTROLS - 1).unwrap();
        capacity.commit(focus).unwrap();
        assert_eq!(capacity.retained(), 1 + drafts);
        assert_eq!(capacity.obligations(), CONTROLS - 1);
        assert_eq!(capacity.retained() + capacity.obligations(), obligations);
        let ordinary = capacity.plan(1, CONTROLS - 1).unwrap();
        capacity.commit(ordinary).unwrap();
        assert_eq!(capacity.retained(), 2 + drafts);
    }

    #[test]
    fn final_reserved_sequence_is_usable_but_reclamation_does_not_restore_headroom() {
        let mut shared = InputCapacity::new(4);
        shared.state.next_sequence = u64::MAX - 2;
        let mut dedicated = DedicatedCapacity::new(1, 2);
        dedicated.next_sequence = shared.next_sequence();
        let initial = shared.state;
        assert_eq!(shared.plan(1, 2).unwrap_err(), InputCapacityError::Sequence);
        assert_eq!(shared.state, initial);
        let dedicated_initial = dedicated;
        assert_eq!(dedicated.admit(1, 0, 2), Err(InputCapacityError::Sequence));
        assert_eq!(dedicated, dedicated_initial);

        let press = shared.plan(1, 1).unwrap();
        assert_eq!(press.first_sequence(), Some(u64::MAX - 1));
        shared.commit(press).unwrap();
        assert_eq!(
            dedicated.admit(1, 0, 1).unwrap(),
            Some((u64::MAX - 1, u64::MAX - 1))
        );
        let release = shared.plan_terminal(1, 0).unwrap();
        assert_eq!(release.first_sequence(), Some(u64::MAX));
        shared.commit(release).unwrap();
        assert_eq!(
            dedicated.admit(0, 1, 0).unwrap(),
            Some((u64::MAX, u64::MAX))
        );
        shared.reclaim(2).unwrap();
        dedicated.reclaim(1, 1).unwrap();
        assert_eq!(shared.retained(), 0);
        assert_eq!(shared.next_sequence(), u64::MAX);
        assert_eq!(shared.plan(1, 0).unwrap_err(), InputCapacityError::Sequence);
        assert_eq!(shared.plan(0, 1).unwrap_err(), InputCapacityError::Sequence);
        assert_eq!(dedicated.admit(1, 0, 0), Err(InputCapacityError::Sequence));
        assert_eq!(dedicated.admit(0, 0, 1), Err(InputCapacityError::Sequence));
    }

    #[test]
    fn checked_count_overflow_and_invalid_refunds_leave_the_ledger_unchanged() {
        let mut capacity = InputCapacity::new(usize::MAX);
        assert_eq!(
            capacity.plan(usize::MAX, 1).unwrap_err(),
            InputCapacityError::Arithmetic
        );
        assert_eq!(capacity.state, InputCapacityState::default());
        assert_eq!(capacity.reclaim(1), Err(InputCapacityError::Arithmetic));
        assert_eq!(
            capacity.plan_terminal(0, 1).unwrap_err(),
            InputCapacityError::Arithmetic
        );

        // Counts are checked without allocating storage proportional to them.
        let full = capacity.plan(usize::MAX, 0).unwrap();
        capacity.commit(full).unwrap();
        let before = capacity.state;
        assert_eq!(
            capacity.plan(1, 0).unwrap_err(),
            InputCapacityError::Arithmetic
        );
        assert_eq!(capacity.state, before);
    }

    #[test]
    fn dedicated_records_require_sequence_merge_for_press_release_press_order() {
        let mut shared = InputCapacity::new(4);
        let mut dedicated = DedicatedCapacity::new(2, 2);
        let mut dedicated_short = DedicatedCapacity::new(2, 1);
        let first = shared.plan(1, 1).unwrap();
        let press_one = (first.first_sequence().unwrap(), "press");
        shared.commit(first).unwrap();
        dedicated.admit(1, 0, 1).unwrap();
        dedicated_short.admit(1, 0, 1).unwrap();
        let release = shared.plan_terminal(1, 0).unwrap();
        let release_one = (release.first_sequence().unwrap(), "release");
        shared.commit(release).unwrap();
        dedicated.admit(0, 1, 0).unwrap();
        dedicated_short.admit(0, 1, 0).unwrap();
        let second = shared.plan(1, 1).unwrap();
        let press_two = (second.first_sequence().unwrap(), "press");
        shared.commit(second).unwrap();
        // A retained terminal occupies the dedicated slot, so the new press's release promise
        // needs a second terminal slot even though no second release has been published yet.
        assert_eq!(
            dedicated_short.admit(1, 0, 1),
            Err(InputCapacityError::Capacity)
        );
        assert_eq!(dedicated.admit(1, 0, 1).unwrap(), Some((3, 3)));

        let expected = [press_one, release_one, press_two];
        let ordinary = [press_one, press_two];
        let terminal = [release_one];
        assert_ne!([ordinary[0], ordinary[1], terminal[0]], expected);
        let mut ordinary = ordinary.iter().peekable();
        let mut terminal = terminal.iter().peekable();
        for expected in expected {
            let take_ordinary = match (ordinary.peek(), terminal.peek()) {
                (Some(ordinary), Some(terminal)) => ordinary.0 < terminal.0,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => panic!("the modeled merge lost an admitted record"),
            };
            let actual = if take_ordinary {
                ordinary.next()
            } else {
                terminal.next()
            };
            assert_eq!(actual.copied(), Some(expected));
        }
        assert!(ordinary.next().is_none());
        assert!(terminal.next().is_none());
        assert_eq!(dedicated.next_sequence, shared.next_sequence());
    }
}
