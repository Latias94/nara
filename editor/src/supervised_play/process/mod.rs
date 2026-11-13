mod spawn;

pub(super) use nara::project_tooling::process::{
    ExactExecutableLaunchAuthority, SupervisedBootstrapCancellation, SupervisedBootstrapChannel,
    SupervisedProcessCleanupReceipt, SupervisedProcessCommand, SupervisedProcessError,
    SupervisedProcessOwner, SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
    default_log_limits,
};
#[cfg(test)]
pub(super) use nara::project_tooling::process::{SupervisedProcessOutput, SupervisedProcessState};
pub(crate) use spawn::{
    SupervisedProcessSpawnFailure, SupervisedProcessSpawnOwner, SupervisedProcessSpawnRetirement,
    SupervisedProcessSpawnTerminal,
};
