#![cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use nara::{
    app::{AppExit, RuntimeControl},
    project_host::{
        DesktopRun, DesktopRunIntent, DesktopRunProgress, open_explicit_product_project_root,
    },
};

struct InvalidSource(PathBuf);

impl InvalidSource {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = parent.join(format!("nara-desktop-poll-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("nara.toml"), b"deliberately invalid = [\n").unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for InvalidSource {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        if self
            .0
            .canonicalize()
            .is_ok_and(|path| path == self.0 && path.parent() == Some(parent.as_path()))
            && self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-desktop-poll-")
        {
            let _ = std::fs::remove_file(self.0.join("nara.toml"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
}

#[test]
fn close_before_source_start_does_not_admit_source_or_construct_a_native_loop() {
    let fixture = InvalidSource::new();
    let root = open_explicit_product_project_root(fixture.path()).unwrap();
    let mut desktop = DesktopRun::new(root, DesktopRunIntent::new());
    assert_eq!(desktop.progress(), DesktopRunProgress::NotStarted);
    assert!(desktop.capture(|_| ()).is_err());
    assert!(desktop.observe(|_| ()).is_err());
    assert!(desktop.request_control(RuntimeControl::Pause).is_err());
    desktop.request_close();
    assert_eq!(
        desktop.poll(Duration::ZERO),
        DesktopRunProgress::Completed(AppExit::Requested)
    );
    assert_eq!(
        desktop.poll(Duration::ZERO),
        DesktopRunProgress::Completed(AppExit::Requested)
    );
    assert!(desktop.runtime_generation().is_none());
    assert!(desktop.take_event_loop().is_none());
    assert!(!desktop.execute().diagnostics().has_errors());
}

#[test]
fn failed_source_poll_remains_failed_without_publishing_a_runtime() {
    let fixture = InvalidSource::new();
    let root = open_explicit_product_project_root(fixture.path()).unwrap();
    let mut desktop = DesktopRun::new(root, DesktopRunIntent::new());
    assert_eq!(desktop.poll(Duration::ZERO), DesktopRunProgress::Failed);
    assert_eq!(
        desktop.poll(Duration::from_millis(1)),
        DesktopRunProgress::Failed
    );
    assert!(desktop.execute().diagnostics().has_errors());
    assert!(desktop.runtime_generation().is_none());
    assert!(desktop.reveal().is_err());
    assert!(desktop.take_event_loop().is_none());
}
