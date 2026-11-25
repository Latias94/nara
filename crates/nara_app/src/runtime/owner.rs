//! Runtime world ownership and close driving.

use std::{
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::Duration,
};

use bevy_platform::time::Instant;
use nara_ecs::{
    World,
    bevy_ecs::error::ErrorHandler,
    bevy_ecs::observer::Observer,
    bevy_ecs::schedule::{Schedules, SingleThreadedExecutor},
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel},
};

use crate::{
    AppExitRequests, CoreStage, FixedTime, FixedUpdateSet, FrameCleanup, FrameIntent,
    FrameIntentSet, InstalledPlugin, InstalledRuntimeAuthorityValidator, PluginFailureReport,
    PluginHook, PluginId, PluginLifecycleState, PluginShutdownError, RealTime, RenderTime,
    RuntimeFrameStatus, RuntimeTimeSettings, StartupStage, VirtualTime,
};

use super::close::{OwnerCloseAttempt, OwnerCloseState, PluginShutdownState};
#[cfg(test)]
use super::initialize_world;
use super::{
    DriverScopeAuthorityPolicy, RuntimeCandidateRetirementState, RuntimeCloseCause,
    RuntimeCloseEvidence, RuntimeClosePolicy, RuntimeFault, RuntimeFaultBridgeEpoch,
    RuntimeFaultBridgeIdentity, RuntimeFaultBridgeRevision, RuntimeFaultReporter,
    RuntimeGeneration, RuntimeObligationLedger, RuntimeObligationLedgerError, RuntimeScopeOutcome,
    capture_execution_error, initialize_runtime_fault_bridge, run_runtime_fault_bridge_maintenance,
    runtime_fault_bridge_authority_fault, validate_runtime_fault_bridge_authority,
};

pub(crate) type RuntimeOwner = RuntimeOwnerState;

pub(crate) struct RuntimeOwnerState {
    _thread_affinity: PhantomData<Rc<()>>,
    pub(crate) world: World,
    pub(crate) execution_profile: crate::ExecutionProfile,
    pub(crate) schedules: Schedules,
    pub(crate) plugins: Vec<InstalledPlugin>,
    pub(crate) runtime_fault_reporter: RuntimeFaultReporter,
    pub(crate) managed_runtime_generation: Option<RuntimeGeneration>,
    pub(crate) runtime_error_handler: Option<ErrorHandler>,
    pub(crate) runtime_fault_bridge: Option<RuntimeFaultBridgeIdentity>,
    pub(crate) runtime_authority_validators: Vec<InstalledRuntimeAuthorityValidator>,
    pub(crate) simulation_conditions: Vec<fn(&World) -> bool>,
    pub(crate) pending_observers: Vec<Observer>,
    pub(crate) plugin_lifecycle: PluginLifecycleState,
    pub(crate) plugin_failure_report: Option<PluginFailureReport>,
    pub(crate) active_plugin_hook: Option<(PluginId, PluginHook)>,
    pub(crate) started: bool,
    pub(crate) obligations: RuntimeObligationLedger,
    pub(super) close_policy: RuntimeClosePolicy,
    close_evidence: RuntimeCloseEvidence,
    close_state: OwnerCloseState,
}

impl RuntimeOwnerState {
    pub(super) fn prepend_host_obligations(
        &mut self,
        mut obligations: RuntimeObligationLedger,
    ) -> Result<(), RuntimeObligationLedgerError> {
        let admission = obligations.preflight_append(&self.obligations);
        obligations.append_for_retirement(std::mem::take(&mut self.obligations));
        self.obligations = obligations;
        admission
    }

    #[cfg(test)]
    pub(crate) fn admit_configuration_for_tests(&mut self) {
        initialize_world(self).unwrap();
    }

    pub(crate) fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(RealTime::default());
        world.insert_resource(VirtualTime::default());
        world.insert_resource(RuntimeTimeSettings::default());
        world.insert_resource(FixedTime::default());
        world.insert_resource(RenderTime::default());
        world.insert_resource(RuntimeFrameStatus::default());
        world.insert_resource(AppExitRequests::default());
        let runtime_fault_reporter = RuntimeFaultReporter::new();
        initialize_runtime_fault_bridge(&mut world, runtime_fault_reporter.clone());

        let mut schedules = Schedules::new();
        for label in StartupStage::ALL
            .into_iter()
            .map(|stage| stage.intern())
            .chain(CoreStage::ALL.into_iter().map(|stage| stage.intern()))
            .chain([FrameIntent.intern(), FrameCleanup.intern()])
        {
            let mut schedule = Schedule::new(label);
            schedule.set_executor(SingleThreadedExecutor::new());
            schedules.insert(schedule);
        }
        schedules
            .get_mut(CoreStage::FixedUpdate)
            .expect("the fixed-update schedule is created above")
            .configure_sets(
                (
                    FixedUpdateSet::Prepare,
                    FixedUpdateSet::Simulate,
                    FixedUpdateSet::Finalize,
                )
                    .chain(),
            );
        schedules
            .get_mut(FrameIntent)
            .expect("the frame-intent schedule is created above")
            .configure_sets((FrameIntentSet::Prepare, FrameIntentSet::Collect).chain());

        Self {
            _thread_affinity: PhantomData,
            world,
            execution_profile: crate::ExecutionProfile::Serial,
            schedules,
            plugins: Vec::new(),
            obligations: RuntimeObligationLedger::new(),
            runtime_fault_reporter,
            close_policy: RuntimeClosePolicy::default(),
            close_evidence: RuntimeCloseEvidence::default(),
            close_state: OwnerCloseState::Open,
            managed_runtime_generation: None,
            runtime_error_handler: None,
            runtime_fault_bridge: None,
            runtime_authority_validators: Vec::new(),
            simulation_conditions: Vec::new(),
            pending_observers: Vec::new(),
            plugin_lifecycle: PluginLifecycleState::Configuring,
            plugin_failure_report: None,
            active_plugin_hook: None,
            started: false,
        }
    }

    pub(crate) fn simulation_is_ready(&self) -> bool {
        self.simulation_conditions
            .iter()
            .all(|condition| condition(&self.world))
    }

    pub(super) fn begin_close(&mut self) {
        if matches!(self.close_state, OwnerCloseState::Open) {
            self.obligations
                .update_pending_evidence(&mut self.close_evidence);
            self.runtime_fault_reporter.mark_closing();
            self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                deadline: deadline_after(self.close_policy.timeout()),
                plugin_shutdown: PluginShutdownState::Pending,
            });
        }
    }

    pub(super) fn retry_close(&mut self) {
        match self.close_state {
            OwnerCloseState::Open => self.begin_close(),
            OwnerCloseState::Incomplete { plugin_shutdown } => {
                self.runtime_fault_reporter.mark_closing();
                self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                    deadline: deadline_after(self.close_policy.timeout()),
                    plugin_shutdown,
                });
            }
            OwnerCloseState::Closing(_) | OwnerCloseState::Complete => {}
        }
    }

    pub(super) fn finish_close_attempt(&mut self) {
        self.retry_close();
        #[cfg(target_arch = "wasm32")]
        self.drive_close();

        #[cfg(not(target_arch = "wasm32"))]
        loop {
            self.drive_close();
            let OwnerCloseState::Closing(attempt) = self.close_state else {
                return;
            };
            let remaining = attempt.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.close_evidence
                    .record(RuntimeCloseCause::DeadlineExceeded);
                self.close_state = OwnerCloseState::Incomplete {
                    plugin_shutdown: attempt.plugin_shutdown,
                };
                return;
            }
            std::thread::sleep(Duration::from_millis(1).min(remaining));
        }
    }

    pub(super) fn drive_close(&mut self) {
        let OwnerCloseState::Closing(mut attempt) = self.close_state else {
            return;
        };

        let bridge_epoch = if self.runtime_error_handler.is_some() {
            match self.begin_fault_bridge_epoch() {
                Ok(epoch) => Some(epoch),
                Err(fault) => {
                    self.runtime_fault_reporter.record_canonical(fault);
                    None
                }
            }
        } else {
            None
        };
        let reporter = self.runtime_fault_reporter.clone();
        let close_pass = capture_execution_error(|| {
            if attempt.plugin_shutdown == PluginShutdownState::Pending {
                attempt.plugin_shutdown = match self.shutdown_plugins() {
                    Ok(()) => PluginShutdownState::Complete,
                    Err(PluginShutdownError::Failure(_)) => {
                        self.close_evidence
                            .record(RuntimeCloseCause::PluginShutdown);
                        PluginShutdownState::Complete
                    }
                    Err(PluginShutdownError::HookActive) => PluginShutdownState::Pending,
                };
            }

            let participant_failed = self
                .obligations
                .drive_close_once_with_evidence(&mut self.world, &mut self.close_evidence);
            (attempt, participant_failed)
        });
        self.obligations
            .update_pending_evidence(&mut self.close_evidence);
        let (attempt, participant_failed) = match close_pass {
            Ok(close_pass) => close_pass,
            Err(fault) => {
                reporter.record_canonical(fault);
                return;
            }
        };

        if let Some(epoch) = bridge_epoch
            && let Err(fault) = self.validate_fault_bridge_epoch(epoch)
        {
            self.runtime_fault_reporter.record_canonical(fault);
        }

        let all_complete = self.obligations.is_close_complete();
        if all_complete && attempt.plugin_shutdown == PluginShutdownState::Complete {
            self.runtime_fault_reporter.mark_closed();
            self.close_state = OwnerCloseState::Complete;
        } else if participant_failed || Instant::now() >= attempt.deadline {
            if !all_complete && Instant::now() >= attempt.deadline {
                self.close_evidence
                    .record(RuntimeCloseCause::DeadlineExceeded);
            }
            self.close_state = OwnerCloseState::Incomplete {
                plugin_shutdown: attempt.plugin_shutdown,
            };
        } else {
            self.close_state = OwnerCloseState::Closing(attempt);
        }
    }

    pub(super) fn reporter(&self) -> &RuntimeFaultReporter {
        &self.runtime_fault_reporter
    }

    pub(crate) fn begin_fault_bridge_epoch(
        &mut self,
    ) -> Result<RuntimeFaultBridgeEpoch, RuntimeFault> {
        self.begin_fault_bridge_epoch_with_maintenance(|world| {
            world.increment_change_tick();
            world.check_change_ticks();
        })
    }

    fn begin_fault_bridge_epoch_with_maintenance(
        &mut self,
        maintenance: impl FnOnce(&mut World),
    ) -> Result<RuntimeFaultBridgeEpoch, RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        let revision = self.world.resource::<RuntimeFaultBridgeRevision>().0;
        let reporter = self.runtime_fault_reporter.clone();
        let fault_before = reporter.fault();
        capture_execution_error(|| {
            run_runtime_fault_bridge_maintenance(&mut self.world, maintenance)
        })??;
        self.validate_fault_bridge_authority()?;
        if self.world.resource::<RuntimeFaultBridgeRevision>().0 != revision {
            return Err(runtime_fault_bridge_authority_fault());
        }
        if fault_before.is_none()
            && let Some(fault) = reporter.fault()
        {
            return Err(fault);
        }
        let epoch = RuntimeFaultBridgeEpoch::capture(&self.world)
            .ok_or_else(runtime_fault_bridge_authority_fault)?;
        self.world.increment_change_tick();
        Ok(epoch)
    }

    pub(crate) fn validate_fault_bridge_epoch(
        &self,
        expected: RuntimeFaultBridgeEpoch,
    ) -> Result<(), RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        if expected.is_unchanged(&self.world) {
            Ok(())
        } else {
            Err(runtime_fault_bridge_authority_fault())
        }
    }

    fn validate_fault_bridge_authority(&self) -> Result<(), RuntimeFault> {
        validate_runtime_fault_bridge_authority(
            &self.world,
            &self.runtime_fault_reporter,
            self.runtime_error_handler,
            self.runtime_fault_bridge,
        )
    }

    pub(super) fn capture_driver_scope<R>(
        &mut self,
        authority_policy: DriverScopeAuthorityPolicy,
        operation: impl FnOnce(&mut World) -> R,
    ) -> Result<RuntimeScopeOutcome<R>, RuntimeFault> {
        let reporter = self.runtime_fault_reporter.clone();
        let fault_before = reporter.fault();
        if authority_policy == DriverScopeAuthorityPolicy::Full {
            self.validate_managed_runtime_authority()
                .map_err(|fault| reporter.record_canonical(fault))?;
        }
        let bridge_epoch = (authority_policy == DriverScopeAuthorityPolicy::Full)
            .then(|| self.begin_fault_bridge_epoch())
            .transpose()
            .map_err(|fault| reporter.record_canonical(fault))?;

        let result = capture_execution_error(|| operation(&mut self.world))
            .map_err(|fault| reporter.record_canonical(fault))?;

        let post_operation_fault = bridge_epoch
            .and_then(|epoch| self.validate_fault_bridge_epoch(epoch).err())
            .or_else(|| {
                (authority_policy == DriverScopeAuthorityPolicy::Full)
                    .then(|| self.validate_managed_runtime_authority().err())
                    .flatten()
            })
            .map(|fault| reporter.record_canonical(fault))
            .or_else(|| fault_before.is_none().then(|| reporter.fault()).flatten());
        Ok(RuntimeScopeOutcome {
            output: result,
            post_operation_fault,
        })
    }

    pub(super) fn close_evidence(&self) -> &RuntimeCloseEvidence {
        &self.close_evidence
    }

    pub(super) fn close_state(&self) -> RuntimeCandidateRetirementState {
        match self.close_state {
            OwnerCloseState::Open | OwnerCloseState::Closing(_) => {
                RuntimeCandidateRetirementState::Retiring
            }
            OwnerCloseState::Incomplete { .. } => {
                RuntimeCandidateRetirementState::RetirementIncomplete
            }
            OwnerCloseState::Complete => RuntimeCandidateRetirementState::Retired,
        }
    }
}

impl Drop for RuntimeOwnerState {
    fn drop(&mut self) {
        if self.close_state() != RuntimeCandidateRetirementState::Retired {
            // Explicit close returns its original owner for retry. Drop always releases it.
            let _ = catch_unwind(AssertUnwindSafe(|| self.finish_close_attempt()));
        }
    }
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now)
}
