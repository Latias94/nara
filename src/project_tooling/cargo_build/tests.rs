use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::*;

struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nara-cargo-owner-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara-cargo-owner-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn separate_directory_sessions_revalidate_the_same_object_and_exclude_reentry() {
    let root = Root::new();
    let lease = TargetBuildLease::acquire_at(root.0.clone()).unwrap();
    lease.verify().unwrap();
    assert!(matches!(
        TargetBuildLease::acquire_at(root.0.clone()),
        Err(CargoBuildFailure::TargetBusy)
    ));
    drop(lease);
    TargetBuildLease::acquire_at(root.0.clone())
        .unwrap()
        .verify()
        .unwrap();
}

#[test]
fn failed_reap_and_forwarded_cleanup_keep_every_original_output_lease() {
    let first = Root::new();
    let second = Root::new();
    let first_lease = TargetBuildLease::acquire_at(first.0.clone()).unwrap();
    let second_lease = TargetBuildLease::acquire_at(second.0.clone()).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "project_tooling::cargo_build::tests::direct_cleanup_child",
            "--ignored",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut waits = 0;
    let error = finish_child_with(child, |_| {
        waits += 1;
        Err(io::Error::other("injected wait failure"))
    })
    .unwrap_err();
    assert_eq!(waits, 2);
    let CargoBuildFailure::CleanupIncomplete(mut custody) = error else {
        panic!("an unconfirmed direct child must keep cleanup custody")
    };
    // This is the same accumulation performed when an outer build callback forwards an inner
    // incomplete cleanup. Neither output owner may be overwritten by the other one.
    custody.leases.push(first_lease);
    custody.leases.push(second_lease);
    assert!(custody.process_id().is_some());
    for root in [&first, &second] {
        assert!(matches!(
            TargetBuildLease::acquire_at(root.0.clone()),
            Err(CargoBuildFailure::TargetBusy)
        ));
    }
    custody.retry_cleanup().unwrap();
    assert_eq!(custody.process_id(), None);
    for root in [&first, &second] {
        TargetBuildLease::acquire_at(root.0.clone())
            .unwrap()
            .verify()
            .unwrap();
    }
}

#[test]
fn output_limit_and_bootstrap_error_preserve_unconfirmed_child_custody() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "project_tooling::cargo_build::tests::direct_cleanup_child",
            "--ignored",
            "--nocapture",
        ])
        .stderr(Stdio::null());
    let error = bounded_status_output_with(&mut command, 1, |child| {
        finish_child_with(child, |_| Err(io::Error::other("injected wait failure")))
    })
    .unwrap_err();
    let error = crate::project_tooling::ProjectToolInvocationError::from(error);
    let crate::project_tooling::ProjectToolInvocationError::ProcessOwner(
        CargoBuildFailure::CleanupIncomplete(mut custody),
    ) = error
    else {
        panic!("output overflow and bootstrap forwarding must retain the unconfirmed child")
    };
    assert!(custody.process_id().is_some());
    custody.retry_cleanup().unwrap();
    assert_eq!(custody.process_id(), None);
}

#[test]
#[ignore = "direct child helper; the owning cleanup test starts and reaps it explicitly"]
fn direct_cleanup_child() {
    std::thread::sleep(Duration::from_secs(20));
}
