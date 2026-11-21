//! Actual public Host comparison of two reconciliation intervals with identical native watchers.

#![cfg(all(
    windows,
    feature = "desktop",
    feature = "cost-measurement",
    feature = "development"
))]
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "support/development_notifications.rs"]
mod measurement;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::{
    fs::OpenOptions,
    io::{self, Write},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use nara::{
    fs::ContentDigest,
    project_tooling::cargo_build::{CapturedCargoExecutable, CargoBuildFailure, build_executable},
};
use serde_json::{Value, json};
use two_level_game::development::{DevelopmentHost, DevelopmentPlacement, protocol};

use measurement::{PAIRS, RunRecord, Variant, generation, nanos, result_json, timestamp};
use support::Fixture;

const CHILD_SOURCE: &str = "src/bin/continuity.rs";
const NOTIFICATION_INTERVAL: &str =
    "const IDLE_RECONCILE_INTERVAL: Duration = Duration::from_millis(250);";
const PERIODIC_INTERVAL: &str =
    "const IDLE_RECONCILE_INTERVAL: Duration = Duration::from_millis(2);";

struct BuiltVariant {
    fixture: Fixture,
    executable: CapturedCargoExecutable,
    original_source: String,
    build: Value,
}

fn build_variant(placement: DevelopmentPlacement, variant: Variant) -> BuiltVariant {
    let fixture = Fixture::new();
    let original_source = fixture.read(CHILD_SOURCE);
    assert_eq!(original_source.matches(NOTIFICATION_INTERVAL).count(), 1);
    assert_eq!(original_source.matches(PERIODIC_INTERVAL).count(), 0);
    if variant == Variant::Periodic {
        fixture.write(
            CHILD_SOURCE,
            &original_source.replacen(NOTIFICATION_INTERVAL, PERIODIC_INTERVAL, 1),
        );
    }
    let request = match placement {
        DevelopmentPlacement::SourceHeadless => fixture.request(),
        DevelopmentPlacement::SourceDesktop => fixture.desktop_request(),
        DevelopmentPlacement::SourceDesktopAudio => {
            panic!("the notification comparison selects only the original silent placements")
        }
    };
    let started_unix_ns = timestamp();
    let started = Instant::now();
    let executable = match build_executable(&request, |_| Ok(())) {
        Ok(executable) => executable,
        Err(failure) => {
            let message = format!("{failure:?}");
            if matches!(&failure, CargoBuildFailure::CleanupIncomplete(_)) {
                eprintln!(
                    "notification comparison retains original build custody at {}: {message}",
                    fixture.root.display(),
                );
                std::mem::forget((failure, fixture));
            }
            panic!("actual notification comparison build failed: {message}");
        }
    };
    let build_wall_ns = nanos(started.elapsed());
    let provenance = executable.provenance();
    assert_eq!(provenance.selection(), request.selection());
    let mut resolved = request.selection().features().to_vec();
    resolved.push("local-player".to_owned());
    assert_eq!(provenance.resolved_features(), resolved);
    assert!(!provenance.selection().default_features());
    assert_eq!(
        executable.digest(),
        ContentDigest::of_bytes(executable.bytes())
    );
    for input in provenance.declared_inputs() {
        assert_eq!(
            input.digest(),
            ContentDigest::of_bytes(&std::fs::read(fixture.project.join(input.path())).unwrap()),
            "captured declared input {}",
            input.path(),
        );
    }
    let build = json!({
        "placement": placement, "variant": variant,
        "started_unix_ns": started_unix_ns, "finished_unix_ns": timestamp(),
        "build_wall_ns": build_wall_ns, "compiler_and_tools": provenance.toolchain(),
        "child_profile": provenance.selection().profile(),
        "target": provenance.selection().target_triple(),
        "requested_features": provenance.selection().features(),
        "resolved_child_features": provenance.resolved_features(),
        "child_default_features": provenance.selection().default_features(),
        "declared_child_inputs": provenance.declared_inputs(),
        "provenance_digest": format!("{:?}", provenance.digest()),
        "metadata_digest": format!("{:?}", provenance.metadata_digest()),
        "executable_digest": format!("{:?}", executable.digest()),
        "cargo_diagnostics": format!("{:?}", executable.diagnostics()),
        "original_continuity_source_digest": format!("{:?}", ContentDigest::of_bytes(original_source.as_bytes())),
    });
    BuiltVariant {
        fixture,
        executable,
        original_source,
        build,
    }
}

fn assert_only_interval_changed(variants: &[BuiltVariant; 2]) {
    assert_eq!(variants[0].original_source, variants[1].original_source);
    assert_eq!(
        variants[0].fixture.read(CHILD_SOURCE),
        variants[1].fixture.read(CHILD_SOURCE).replacen(
            NOTIFICATION_INTERVAL,
            PERIODIC_INTERVAL,
            1
        ),
    );
    let periodic = variants[0].executable.provenance().declared_inputs();
    let notification = variants[1].executable.provenance().declared_inputs();
    assert_eq!(periodic.len(), notification.len());
    for (periodic, notification) in periodic.iter().zip(notification) {
        assert_eq!(periodic.path(), notification.path());
        if periodic.path() != CHILD_SOURCE {
            assert_eq!(
                periodic.digest(),
                notification.digest(),
                "{}",
                periodic.path()
            );
        }
    }
}

fn write_report(output: &Path, report: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(report).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()
}

#[test]
#[ignore = "actual nested builds and twelve native game processes; run serially on an idle Windows hardware desktop with CARGO_BUILD_JOBS=1 and a new absolute NARA_DEVELOPMENT_NOTIFICATIONS_OUTPUT JSON path"]
fn public_hosts_compare_native_request_notifications_with_periodic_reconciliation() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    assert_eq!(protocol::PROTOCOL_VERSION, 7);
    let output = PathBuf::from(
        std::env::var_os("NARA_DEVELOPMENT_NOTIFICATIONS_OUTPUT")
            .expect("supply an explicit new absolute JSON result path"),
    );
    assert!(output.is_absolute() && !output.exists());
    assert!(output.parent().unwrap().is_dir());
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let git = |arguments: &[&str]| {
        let result = Command::new("git")
            .current_dir(&repository)
            .args(arguments)
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap()
    };
    let setup = json!({
        "repository_head": git(&["rev-parse", "HEAD"]).trim(),
        "repository_status": git(&["status", "--short", "--untracked-files=normal"]),
        "measurement_source_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("development_notifications.rs"))),
        "measurement_support_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("support/development_notifications.rs"))),
        "original_fixture_source_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("../../../tests/support/continuity.rs"))),
        "acceptance_helper_source_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("support/development_host.rs"))),
        "source_scope": "declared copied inputs and captured bytes; setup HEAD/status do not snapshot concurrent root dependency edits",
    });
    let started_unix_ns = timestamp();
    let mut builds = Vec::new();
    let mut runs = Vec::with_capacity(PAIRS * 4);
    let mut fixture_cleanup = Vec::new();
    for placement in [
        DevelopmentPlacement::SourceHeadless,
        DevelopmentPlacement::SourceDesktop,
    ] {
        let variants = [
            build_variant(placement, Variant::Periodic),
            build_variant(placement, Variant::Notification),
        ];
        assert_only_interval_changed(&variants);
        builds.extend(variants.iter().map(|variant| variant.build.clone()));
        for pair in 0..PAIRS {
            for position in 0..2 {
                let index = (pair + position) % 2;
                let built = &variants[index];
                let variant = [Variant::Periodic, Variant::Notification][index];
                let parent = built
                    .fixture
                    .directory(&format!("notifications-{pair}-{position}"));
                let mut host =
                    DevelopmentHost::with_placement(&built.fixture.project, &parent, placement)
                        .unwrap();
                let mut record = RunRecord::new(runs.len(), pair, position, placement, variant);
                let exercise = catch_unwind(AssertUnwindSafe(|| {
                    let started = Instant::now();
                    let result = host.start(built.executable.clone());
                    record.startup = json!({
                        "elapsed_ns": nanos(started.elapsed()), "result": result_json(&result),
                        "control_service": host.control_service(),
                    });
                    let ready = result.unwrap();
                    let active = host.active().unwrap();
                    record.active = Some(generation(active));
                    assert_ne!(active.pid, 0);
                    assert_eq!(active.executable, built.executable.digest());
                    assert_eq!(active.provenance, built.executable.provenance().digest());
                    assert_eq!(ready.logical_tick, 0);
                    measurement::exercise(&mut host, &mut record);
                }));
                if let Err(panic) = exercise {
                    record
                        .failures
                        .push(measurement::panic_message(panic.as_ref()));
                }
                record.before_close = host.control_service();
                let cleanup_started = Instant::now();
                let cleanup = catch_unwind(AssertUnwindSafe(|| {
                    acceptance::close_to_completion(&mut host)
                }));
                if !matches!(&cleanup, Ok(true)) {
                    eprintln!(
                        "notification comparison retains original Host and source/work owners at {}: {:?}",
                        built.fixture.root.display(),
                        host.cleanup_error(),
                    );
                    record.failures.push(format!(
                        "original cleanup remains incomplete: {}",
                        cleanup.as_ref().err().map_or_else(
                            || "Close did not complete".to_owned(),
                            |panic| measurement::panic_message(panic.as_ref()),
                        ),
                    ));
                    record.finish_cleanup(&host, nanos(cleanup_started.elapsed()), false);
                    runs.push(record);
                    let retained: Vec<_> = variants
                        .iter()
                        .map(|built| built.fixture.root.clone())
                        .collect();
                    let report = json!({
                        "format": 1, "started_unix_ns": started_unix_ns, "finished_unix_ns": timestamp(),
                        "platform": format!("windows-{}", std::env::consts::ARCH),
                        "outer_debug_assertions": cfg!(debug_assertions), "protocol_version": protocol::PROTOCOL_VERSION,
                        "setup": setup, "builds": builds, "runs": runs, "fixture_cleanup": fixture_cleanup,
                        "accepted": false, "partial": true, "retained_until_process_exit": retained,
                        "failures": ["original cleanup failed; original Host and Fixture owners retained"],
                        "method": measurement::method(),
                    });
                    std::mem::forget((host, variants));
                    let saved = write_report(&output, &report);
                    if let Err(error) = &saved {
                        eprintln!(
                            "partial notification report could not be saved at {}: {error}",
                            output.display()
                        );
                    }
                    match cleanup {
                        Err(panic) => resume_unwind(panic),
                        Ok(_) => {
                            panic!(
                                "original notification comparison cleanup remains incomplete; partial report at {}: {saved:?}",
                                output.display()
                            )
                        }
                    }
                }
                record.finish_cleanup(&host, nanos(cleanup_started.elapsed()), true);
                drop(host);
                eprintln!(
                    "development_notifications: placement={placement:?} pair={pair} position={position} variant={variant:?} failures={}",
                    record.failures.len(),
                );
                runs.push(record);
            }
        }
        let roots: Vec<_> = variants
            .iter()
            .map(|built| built.fixture.root.clone())
            .collect();
        drop(variants);
        fixture_cleanup.extend(roots.into_iter().map(|root| {
            json!({
                "root": root, "removed_by_original_fixture": !root.exists(),
            })
        }));
    }
    let (summaries, mut failures) = measurement::evaluate(&runs);
    for cleanup in &fixture_cleanup {
        if cleanup["removed_by_original_fixture"] != true {
            failures.push(format!(
                "original Fixture cleanup remains incomplete: {cleanup}"
            ));
        }
    }
    let report = json!({
        "format": 1, "started_unix_ns": started_unix_ns, "finished_unix_ns": timestamp(),
        "platform": format!("windows-{}", std::env::consts::ARCH),
        "outer_debug_assertions": cfg!(debug_assertions), "protocol_version": protocol::PROTOCOL_VERSION,
        "setup": setup, "builds": builds, "runs": runs, "summaries": summaries,
        "fixture_cleanup": fixture_cleanup, "failures": failures, "accepted": failures.is_empty(),
        "method": measurement::method(),
    });
    write_report(&output, &report).unwrap();
    assert!(
        failures.is_empty(),
        "notification comparison rejected; full raw report at {}: {failures:#?}",
        output.display()
    );
}
