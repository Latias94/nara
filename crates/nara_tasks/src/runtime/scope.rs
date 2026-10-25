use super::*;

/// Host-owned bounded execution facilities shared by independently retired activities.
///
/// Retain this owner while its scopes are in use. Scope retirement never joins these workers.
/// Closing or dropping the facilities stops all admission and retains unfinished physical work
/// through the same shutdown and quarantine protocol as standalone [`TaskPools`].
#[derive(Debug)]
pub struct TaskFacilities {
    pools: TaskPools,
}

impl TaskFacilities {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::try_new(config).map(|pools| Self { pools })
    }

    /// Creates optional first-use worker activation with the same custody as [`TaskPools::try_new_lazy`].
    pub fn try_new_lazy(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::try_new_lazy(config).map(|pools| Self { pools })
    }

    /// Creates explicit test facilities without worker threads.
    pub fn inline_for_tests(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::inline_for_tests(config).map(|pools| Self { pools })
    }

    #[must_use]
    pub fn config(&self) -> &TaskPoolConfig {
        self.pools.config()
    }

    /// Physical facility counters, including work from every activity.
    #[must_use]
    pub fn stats(&self) -> TaskStats {
        self.pools.stats()
    }

    #[must_use]
    pub fn scope(&self) -> TaskScope {
        let shared = Arc::new(TaskScopeShared::default());
        let mut pools = self.pools.copy_facade();
        pools.instance_token = Arc::new(TaskPoolInstanceToken::default());
        pools.scope = Some(shared);
        TaskScope { pools }
    }

    pub fn run_pending_for_tests(&self) -> TaskInlineRunReport {
        self.pools.run_pending_for_tests()
    }

    /// Closes the physical facilities, including unfinished work from all scopes.
    ///
    /// Normally the Host first retires its scopes, then joins workers here. An emergency facility
    /// close can cancel their work but does not substitute for observing each activity's return.
    pub fn shutdown_blocking(&self) -> Result<TaskShutdownReport, TaskShutdownError> {
        self.pools.shutdown_blocking()
    }
}

/// Move-only owner of a set of admitted tasks, independent of the worker lifetime.
///
/// Use [`Self::spawner`] to supply a World or a child task with admission to this same activity.
/// Work spawned through that facade is registered atomically with admission, including child work.
/// Closing rejects further admission, cancels queued and running work, and observes executor-owned
/// capture/late-result disposal. Published outputs and independently escaped work retain their own
/// owners. A scope does not join workers or prove domain-native resource retirement.
#[derive(Debug)]
pub struct TaskScope {
    pools: TaskPools,
}

impl TaskScope {
    #[must_use]
    pub fn spawner(&self) -> TaskPools {
        self.pools.copy_facade()
    }

    /// Stops admission and publishes cancellation without running pending capture destructors.
    ///
    /// This visits the facility's bounded queues. Pending captures move to the existing retained
    /// disposal lanes; saturation keeps return Pending. Running closures remain cooperative.
    pub fn begin_close(&mut self) {
        let scope = self.pools.scope.as_ref().expect("scope owns its activity");
        {
            let mut activity = scope.lock_admission();
            if activity.closed {
                return;
            }
            activity.closed = true;
        }
        let mut retired = VecDeque::new();
        for kind in TaskPoolKind::ALL {
            let shared = &self.pools.executor(kind).shared;
            let mut queue = lock_unpoisoned(&shared.queue);
            let mut retained = VecDeque::with_capacity(queue.pending.len());
            while let Some(pending) = queue.pending.front() {
                if same_scope(&pending.scope, &self.pools.scope) {
                    let pending = queue
                        .take_for_disposal(0, shared)
                        .expect("matching pending task retains queue custody");
                    pending
                        .control
                        .state
                        .cancel(TaskCancellationReason::ScopeClosed);
                    retired.push_back(pending);
                } else {
                    retained.push_back(
                        queue
                            .pending
                            .pop_front()
                            .expect("retained task stays queued"),
                    );
                }
            }
            queue.pending = retained;
            for running in queue.running.values() {
                if same_scope(&running.scope, &self.pools.scope)
                    && let Some(control) = &running.control
                {
                    // Borrow in place: a strong snapshot could become the last output owner.
                    control.state.cancel(TaskCancellationReason::ScopeClosed);
                }
            }
        }
        if !retired.is_empty() {
            retain_abandoned_task_owner(AbandonedTaskPoolOwner {
                pending: retired,
                workers: None,
                disposal_owner: None,
            });
        }
    }

    /// Starts or continues close. Physical completion and cleanup success are separate facts.
    pub fn poll_close(&mut self) -> TaskScopeCloseReport {
        self.begin_close();
        let scope = self.pools.scope.as_ref().expect("scope owns its activity");
        if scope.pending.load(Ordering::Acquire) != 0 {
            retry_task_owner_reaper();
        }
        TaskScopeCloseReport {
            pending: scope.pending.load(Ordering::Acquire),
            cleanup_failed: scope.cleanup_failed.load(Ordering::Acquire),
        }
    }
}

impl Drop for TaskScope {
    fn drop(&mut self) {
        self.begin_close();
    }
}

/// Activity return evidence; never evidence that shared workers joined.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskScopeCloseReport {
    pub pending: usize,
    pub cleanup_failed: bool,
}

impl TaskScopeCloseReport {
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.pending == 0
    }
}

#[derive(Debug, Default)]
pub(super) struct TaskScopeShared {
    admission: Mutex<TaskScopeAdmission>,
    pending: AtomicUsize,
    cleanup_failed: AtomicBool,
}

impl TaskScopeShared {
    pub(super) fn lock_admission(&self) -> MutexGuard<'_, TaskScopeAdmission> {
        lock_unpoisoned(&self.admission)
    }

    pub(super) fn admit_task(&self) {
        self.pending.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn finish_task(&self, outcome: TaskCleanupOutcome) {
        if outcome == TaskCleanupOutcome::Failed {
            self.cleanup_failed.store(true, Ordering::Release);
        }
        let previous = self.pending.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "scope return accounting underflowed");
    }
}

#[derive(Debug, Default)]
pub(super) struct TaskScopeAdmission {
    closed: bool,
}

impl TaskScopeAdmission {
    pub(super) fn is_closed(&self) -> bool {
        self.closed
    }
}

impl TaskPools {
    fn copy_facade(&self) -> Self {
        Self {
            config: self.config,
            next_id: self.next_id.clone(),
            instance_token: self.instance_token.clone(),
            io: self.io.copy_facade(),
            compute: self.compute.copy_facade(),
            async_compute: self.async_compute.copy_facade(),
            close_owner: Mutex::new(None),
            scope: self.scope.clone(),
        }
    }
}

impl TaskPoolExecutor {
    fn copy_facade(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            inline: self.inline,
            workers: self.workers.clone(),
        }
    }
}

pub(super) fn same_scope(
    left: &Option<Arc<TaskScopeShared>>,
    right: &Option<Arc<TaskScopeShared>>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        _ => false,
    }
}

pub(super) fn prepare_admission(pending: &mut PendingJob) {
    pending.control.state.return_observer().record_admission();
    pending.admitted_at = Instant::now();
}

impl RuntimeCloseParticipant for TaskScope {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        TaskScope::begin_close(self);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let report = TaskScope::poll_close(self);
        context.insert_resource(report).map_err(|_| {
            RuntimeCloseParticipantError::terminal("nara.tasks.runtime-resource-access")
        })?;
        if !report.is_complete() {
            return Ok(RuntimeCloseProgress::Pending);
        }
        let token = self.pools.ownership_token();
        if context
            .world()
            .get_resource::<TaskPools>()
            .is_some_and(|pools| pools.has_ownership_token(&token))
        {
            context.remove_resource::<TaskPools>().map_err(|_| {
                RuntimeCloseParticipantError::terminal("nara.tasks.runtime-resource-access")
            })?;
        }
        if report.cleanup_failed {
            Ok(RuntimeCloseProgress::CompleteWithError {
                code: "nara.tasks.scope-cleanup-failed",
            })
        } else {
            Ok(RuntimeCloseProgress::Complete)
        }
    }
}
