mod close;
mod execution;

use std::time::Duration;

use nara_core::{ItemLimit, TimeLimit};

use super::*;

fn items(value: usize) -> ItemLimit {
    ItemLimit::new(value).expect("test limits are non-zero")
}

fn time(value: Duration) -> TimeLimit {
    TimeLimit::new(value).expect("test limits are non-zero")
}

fn test_config(pending: usize) -> TaskPoolConfig {
    let kind = TaskKindConfig::new(ItemLimit::ONE, items(pending));
    TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(
            time(Duration::from_millis(250)),
            time(Duration::from_millis(250)),
            time(Duration::from_millis(250)),
        ),
    )
    .expect("test task configuration is valid")
}

fn request(domain: u64) -> TaskSpawnRequest {
    TaskSpawnRequest::new(17, TaskDomainKey::new(domain))
}

fn inline_pools(pending: usize) -> TaskPools {
    TaskPools::inline_for_tests(test_config(pending)).unwrap()
}

fn accepted<T>(outcome: TaskSpawnOutcome<T>) -> TaskHandle<T> {
    match outcome {
        TaskSpawnOutcome::Accepted(handle) => handle,
        TaskSpawnOutcome::Coalesced { handle, .. } => handle,
        TaskSpawnOutcome::Rejected(rejection) => {
            panic!("expected accepted task, got {rejection:?}")
        }
    }
}
