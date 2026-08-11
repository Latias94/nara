#[cfg(windows)]
use std::{
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(windows)]
use nara::{
    app::AppExit,
    project_host::{DesktopRun, DesktopRunOutcome},
};
#[cfg(windows)]
use nara_reference_game::{ReferenceDesktopPlugin, advanced_wave_desktop_intent_after};

#[cfg(windows)]
mod desktop_support;
#[cfg(windows)]
mod desktop_accessibility_probe {
    pub(crate) mod evidence;
    pub(crate) mod platform;
    pub(crate) mod runtime;
}
#[cfg(windows)]
mod support;

#[cfg(windows)]
use desktop_accessibility_probe::{evidence::ProbeEvidence, platform, runtime};
#[cfg(windows)]
use support::project_root::open_project_root;

#[cfg(windows)]
const PROBE_DEADLINE: Duration = Duration::from_secs(30);
#[cfg(windows)]
const CLEANUP_DEADLINE: Duration = Duration::from_secs(20);

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("desktop_accessibility_probe.unsupported_platform");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> ExitCode {
    let root = match open_project_root() {
        Ok(root) => root,
        Err(_) => return fail("desktop_accessibility_probe.root_failed"),
    };
    let evidence = Arc::new(ProbeEvidence::default());
    let definition = runtime::definition(Arc::clone(&evidence), PROBE_DEADLINE);
    let intent = advanced_wave_desktop_intent_after::<ReferenceDesktopPlugin>(definition);
    let mut run = DesktopRun::new(root, intent);
    let automation = platform::spawn(Arc::clone(&evidence), PROBE_DEADLINE);

    let report = match drive_to_terminal(&mut run) {
        Some(report) => report,
        None => {
            evidence.request_stop();
            let _ = automation.join();
            return fail("desktop_accessibility_probe.cleanup_timeout");
        }
    };
    evidence.request_stop();
    let automation_error = match automation.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some("the UI Automation client thread panicked".to_owned()),
    };
    let trace = evidence.snapshot();
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
        || report.diagnostics().has_errors()
        || !trace.is_complete()
        || automation_error.is_some()
    {
        let diagnostics = report
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_str(), diagnostic.fields()))
            .collect::<Vec<_>>();
        eprintln!(
            "desktop_accessibility_probe.details outcome={:?} trace={trace:?} automation_error={automation_error:?} diagnostics={diagnostics:?}",
            report.outcome(),
        );
        return fail("desktop_accessibility_probe.product_path_failed");
    }

    let action = trace
        .action
        .as_ref()
        .expect("a complete probe retains exact journal evidence");
    println!(
        "desktop_accessibility_probe: ok hwnd={} runtime_id={:?} journal={}<{}<={} armed_frame_revision={} presentation_epoch={} frame_revision={} target={:?} generation={} wgpu_frames={}..{}",
        trace.hwnd.expect("a complete probe retains the exact HWND"),
        trace
            .retry_runtime_id
            .as_ref()
            .expect("a complete probe retains the Retry runtime ID"),
        trace
            .journal_floor
            .expect("a complete probe retains the runtime-owned journal floor"),
        action.sequence,
        trace
            .journal_barrier
            .expect("a complete probe retains the post-platform journal barrier"),
        trace
            .armed_frame_revision
            .expect("a complete probe retains the runtime-armed frame revision"),
        action.presentation_epoch,
        action.frame_revision,
        action.target,
        trace
            .run_generation
            .expect("a complete probe retains the successor generation"),
        trace
            .before_wgpu_frame
            .expect("a complete probe retains the baseline WGPU frame"),
        trace
            .after_wgpu_frame
            .expect("a complete probe retains the successor WGPU frame"),
    );
    ExitCode::SUCCESS
}

#[cfg(windows)]
fn drive_to_terminal(run: &mut DesktopRun) -> Option<nara::project_host::DesktopRunReport> {
    let mut cleanup_deadline = None;
    loop {
        let report = run.execute();
        if report.outcome() != DesktopRunOutcome::CleanupIncomplete {
            return Some(report);
        }
        let now = Instant::now();
        let deadline = *cleanup_deadline
            .get_or_insert_with(|| now.checked_add(CLEANUP_DEADLINE).unwrap_or(now));
        if now >= deadline {
            return None;
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

#[cfg(windows)]
fn fail(code: &'static str) -> ExitCode {
    eprintln!("{code}");
    ExitCode::FAILURE
}
