//! Shared final result consolidation for native Editor runners.

use std::time::{Duration, Instant};

use nara::{
    app::RuntimeCloseEvidence,
    project_host::{EditorProjectCloseReceipt, EditorProjectOpenTasksCloseReport},
};

use crate::EditorApplication;

use super::EditorRunError;

/// Drives the existing Open Host cleanup tail until completion or the finite deadline.
pub(crate) fn finish_open_host_shutdown(
    application: &mut EditorApplication,
    deadline: Instant,
) -> EditorProjectOpenTasksCloseReport {
    loop {
        let report = application.poll_open_host_shutdown();
        if report.is_complete() || Instant::now() >= deadline {
            return report;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Consolidates cleanup evidence while retaining any primary failure.
///
/// Pass `None` when the concrete native owner's error set is empty.
pub(crate) fn editor_run_result(
    primary: Option<EditorRunError>,
    native_cleanup: Option<&str>,
    project_cleanup: Option<&RuntimeCloseEvidence>,
    session_cleanup: Option<&EditorProjectCloseReceipt>,
    open_tasks: Option<&EditorProjectOpenTasksCloseReport>,
) -> Result<(), EditorRunError> {
    let mut cleanup = String::new();
    if let Some(native_cleanup) = native_cleanup {
        cleanup = format!("native graphics retired with errors: {native_cleanup}");
    }
    if let Some(evidence) = project_cleanup {
        if !cleanup.is_empty() {
            cleanup.push_str("; ");
        }
        cleanup.push_str(&format!(
            "failed project Open retired with cleanup errors: {evidence:?}"
        ));
    }
    if let Some(receipt) = session_cleanup.filter(|receipt| receipt.has_cleanup_failure()) {
        if !cleanup.is_empty() {
            cleanup.push_str("; ");
        }
        cleanup.push_str(&format!(
            "project Session retired with cleanup errors: {receipt:?}"
        ));
    }
    if let Some(report) =
        open_tasks.filter(|report| !report.is_complete() || report.has_cleanup_failure())
    {
        if !cleanup.is_empty() {
            cleanup.push_str("; ");
        }
        let status = if report.is_complete() {
            "retired with cleanup errors"
        } else {
            "retirement remains incomplete"
        };
        cleanup.push_str(&format!("project Open Host Tasks {status}: {report:?}"));
    }
    match (primary, cleanup.is_empty()) {
        (Some(primary), false) => Err(EditorRunError::Cleanup {
            primary: Box::new(primary),
            cleanup,
        }),
        (Some(primary), true) => Err(primary),
        (None, false) => Err(EditorRunError::Platform(cleanup)),
        (None, true) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use crate::EditorLaunchOptions;

    use super::*;

    #[test]
    fn clean_open_host_shutdown_is_accepted_without_native_cleanup() {
        let mut application = EditorApplication::new(EditorLaunchOptions::project_manager());
        let report = finish_open_host_shutdown(&mut application, Instant::now());

        assert!(report.is_complete());
        assert!(!report.has_cleanup_failure());
        assert_eq!(application.open_host_tasks_close_report(), report);
        assert!(editor_run_result(None, None, None, None, Some(&report)).is_ok());
        assert!(editor_run_result(None, None, None, None, None).is_ok());
    }

    #[test]
    fn primary_error_keeps_its_variant_with_and_without_native_cleanup() {
        let error = editor_run_result(
            Some(EditorRunError::EventLoop("lost event pump".to_owned())),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, EditorRunError::EventLoop(message) if message == "lost event pump"));

        let error = editor_run_result(
            Some(EditorRunError::EventLoop("lost event pump".to_owned())),
            Some("producer cleanup; device cleanup"),
            None,
            None,
            None,
        )
        .unwrap_err();
        let EditorRunError::Cleanup { primary, cleanup } = error else {
            panic!("native cleanup must retain the primary error: {error}");
        };
        assert!(
            matches!(*primary, EditorRunError::EventLoop(message) if message == "lost event pump")
        );
        assert_eq!(
            cleanup,
            "native graphics retired with errors: producer cleanup; device cleanup"
        );
    }

    #[test]
    fn native_cleanup_without_a_primary_is_a_platform_error_even_for_an_empty_message() {
        for message in ["producer cleanup; device cleanup", ""] {
            let error = editor_run_result(None, Some(message), None, None, None).unwrap_err();
            assert!(matches!(
                error,
                EditorRunError::Platform(cleanup)
                    if cleanup == format!("native graphics retired with errors: {message}")
            ));
        }
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn native_runtime_and_session_cleanup_keep_their_order_without_a_primary() {
        let receipt = crate::app::failed_trusted_session_close_receipt();
        let evidence = receipt.play_close_evidence().unwrap();
        let error = editor_run_result(
            None,
            Some("native cleanup probe"),
            Some(evidence),
            Some(&receipt),
            None,
        )
        .unwrap_err();
        let EditorRunError::Platform(cleanup) = error else {
            panic!("cleanup without a primary must be a platform error: {error}");
        };
        assert_eq!(
            cleanup,
            format!(
                "native graphics retired with errors: native cleanup probe; failed project Open retired with cleanup errors: {evidence:?}; project Session retired with cleanup errors: {receipt:?}"
            )
        );
    }
}
