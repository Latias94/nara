//! Actual managed child custody, independent of the public Session's separate edit assertions.

use super::super::{TOOL_DOMAIN, process};
use super::*;
use nara_core::ItemLimit;
use nara_tasks::{
    TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
};
use std::{
    path::PathBuf,
    sync::{Weak, atomic::Ordering},
    thread,
    time::Instant,
};

struct ReleaseGate(Arc<process::ProcessTestGate>);
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        self.0.release.store(true, Ordering::Release);
    }
}

fn wait(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "managed child custody did not reach its boundary"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn command() -> (EditorSourceToolCommand, Weak<HostCustody>) {
    use crate::project_host::open_explicit_product_project_root;
    let dotnet = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_DOTNET").expect("run the behavior-language fixture"),
    );
    let bootstrap = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_BOOTSTRAP_DIR").expect("built managed bootstrap"),
    );
    let digest = |path: PathBuf| ContentDigest::of_bytes(&std::fs::read(path).unwrap());
    let host = EditorSourceToolManagedHost::new(
        open_explicit_product_project_root(dotnet.parent().unwrap()).unwrap(),
        open_explicit_product_project_root(&bootstrap).unwrap(),
        EditorSourceToolManagedHostDigests {
            launcher: digest(dotnet),
            bootstrap: digest(bootstrap.join(BOOTSTRAP)),
            deps: digest(bootstrap.join(BOOTSTRAP_DEPS)),
            runtimeconfig: digest(bootstrap.join(BOOTSTRAP_CONFIG)),
        },
        &std::env::var("NARA_SOURCE_TOOL_FRAMEWORK").unwrap(),
        Duration::from_secs(30),
    )
    .unwrap();
    let bytes = std::fs::read(
        PathBuf::from(std::env::var_os("NARA_SOURCE_TOOL_V1_DIR").unwrap())
            .join("EnemyStatsTool.dll"),
    )
    .unwrap();
    let manifest = ContentDigest::of_bytes(&bytes);
    let mut prefix = b"NARATOOL".to_vec();
    for value in [1_u32, 1, 0] {
        prefix.extend_from_slice(&value.to_le_bytes());
    }
    prefix.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    prefix.extend_from_slice(&bytes);
    let weak = Arc::downgrade(&host.custody);
    let mut command = host.custody.command.clone();
    command.approved = Some(Arc::new(ApprovedManagedLaunch {
        host: Arc::clone(&host.custody),
        prefix: prefix.into(),
        provenance: ApprovedInstallationProvenance {
            directory: host.custody.bootstrap.share(),
            package: Arc::from("nara.test.custody"),
            installation: [1; 32],
            receipt: manifest,
            manifest,
        },
    }));
    (command, weak)
}

#[test]
#[ignore = "requires the actual C# tool/bootstrap; run behavior-languages/run.py"]
fn managed_child_cancellation_retains_approved_custody_through_late_spawn_and_io_return() {
    for before_spawn in [true, false] {
        let config = TaskPoolConfig::default()
            .with_kind(
                TaskPoolKind::Io,
                TaskKindConfig::new(ItemLimit::new(1).unwrap(), ItemLimit::new(2).unwrap()),
            )
            .unwrap();
        let pools = TaskPools::try_new_lazy(config).unwrap();
        let gate = ReleaseGate(Arc::new(process::ProcessTestGate::default()));
        let gates = process::ProcessTestGates {
            before_spawn: before_spawn.then(|| Arc::clone(&gate.0)),
            before_io_return: (!before_spawn).then(|| Arc::clone(&gate.0)),
        };
        let (command, custody) = command();
        let input = br#"{"protocol_version":1,"request":1,"operation":"describe","name":"","fields":{},"parameters":{}}"#.to_vec();
        let request = process::ProcessRequest::new(command, input).with_test_gates(gates);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(1, TOOL_DOMAIN),
                move |token| request.run(token),
            )
            .into_handle()
            .unwrap();
        let returned = task.return_observer();
        wait(|| gate.0.entered.load(Ordering::Acquire));
        task.cancel();
        assert_eq!(returned.state(), TaskReturnState::Pending);
        assert!(
            custody.upgrade().is_some(),
            "logical cancellation dropped native/input authority"
        );
        gate.0.release.store(true, Ordering::Release);
        wait(|| returned.state() != TaskReturnState::Pending);
        assert!(
            custody.upgrade().is_none(),
            "returned child or pipe retained approved authority"
        );
        pools.shutdown_blocking().unwrap();
    }
}
