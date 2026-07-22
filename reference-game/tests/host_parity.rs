#![cfg(feature = "host-parity")]

#[path = "support/host_parity.rs"]
mod host_parity;

use host_parity::{HostMode, MAX_PARITY_ENVELOPE_BYTES, run_host};

#[test]
fn real_product_hosts_publish_the_same_bounded_semantics_and_fault() {
    let headless = run_host(HostMode::Headless);
    let desktop = run_host(HostMode::Desktop);
    let editor = run_host(HostMode::Editor);

    assert_eq!(headless, desktop);
    assert_eq!(headless, editor);
    assert!(headless.len() <= MAX_PARITY_ENVELOPE_BYTES);
    for required in [
        "accepted@2@",
        "duplicate@2@",
        "@duplicate",
        "future@602@",
        "@too-far-future,602,1,600",
        "over-budget@2@",
        "@payload-byte-limit,",
        "late@1@",
        "@late,1,1",
        "|wave=1,2,running,",
        "|player=player,",
        "|enemies=enemy-anchor-2/enemy,",
        "|fault=system,nara.ecs.fallible-execution,3",
    ] {
        assert!(
            headless.contains(required),
            "missing {required}: {headless}"
        );
    }
}

#[test]
fn host_parity_probe_stays_on_the_public_bounded_product_surface() {
    let probe = include_str!("../src/bin/host_parity_probe.rs");
    let support = include_str!("support/host_parity.rs");
    let child_process = include_str!("support/child_process.rs");
    let project = include_str!("../nara.toml");

    for required in [
        "HeadlessRun::new",
        "DesktopRun::new",
        "EditorProjectSession::open",
        "sync_channel(1)",
        "MAX_PARITY_ENVELOPE_BYTES",
        "GameplayCommandSet::Capture",
        "FixedUpdateSet::Simulate",
    ] {
        assert!(probe.contains(required), "missing public proof: {required}");
    }
    for forbidden in [
        "ProjectHost",
        "RuntimeInstance",
        "RuntimeAdmissionReservation",
        "__RuntimeDriverPort",
        "RuntimeDriverScope",
        "RuntimeWorldAccess",
        "World",
        "WorldSnapshot",
        "ScenePatch",
        "EditorWorkspaceCommand",
        "EventBus",
        "serde_json",
    ] {
        assert!(
            !probe.contains(forbidden)
                && !support.contains(forbidden)
                && !child_process.contains(forbidden),
            "the host parity proof leaked a forbidden shortcut: {forbidden}",
        );
    }
    assert!(!project.contains("host-parity-probe"));
    assert!(support.contains("current_dir(&environment.cwd)"));
    assert!(support.contains(".env(\"HOME\", &environment.home)"));
    assert!(child_process.contains("terminate_and_reap"));
}
