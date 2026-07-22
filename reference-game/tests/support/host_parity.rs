use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[path = "child_process.rs"]
mod child_process;

use child_process::{ChildOutputLimits, run_child_with_timeout};

pub const MAX_PARITY_ENVELOPE_BYTES: usize = 16 * 1024;
const HOST_PARITY_OUTPUT_LIMITS: ChildOutputLimits =
    ChildOutputLimits::new(MAX_PARITY_ENVELOPE_BYTES + 2, 4 * 1024);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostMode {
    Headless,
    Desktop,
    Editor,
}

impl HostMode {
    const fn argument(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Desktop => "desktop",
            Self::Editor => "editor",
        }
    }
}

pub fn run_host(mode: HostMode) -> String {
    require_desktop_environment(mode);
    let environment = ChildEnvironment::new(mode);
    let mut command = Command::new(env!("CARGO_BIN_EXE_host_parity_probe"));
    command
        .arg(mode.argument())
        .current_dir(&environment.cwd)
        .env("HOME", &environment.home)
        .env("USERPROFILE", &environment.home)
        .env("XDG_CACHE_HOME", environment.home.join("cache"))
        .env("XDG_CONFIG_HOME", environment.home.join("config"))
        .env("XDG_DATA_HOME", environment.home.join("data"))
        .env("APPDATA", environment.home.join("appdata"))
        .env("LOCALAPPDATA", environment.home.join("local-appdata"));
    let output = run_child_with_timeout(
        command,
        Duration::from_secs(45),
        HOST_PARITY_OUTPUT_LIMITS,
        "host parity probe",
    );
    assert!(
        output.status.success(),
        "{} probe failed: stdout={}; stderr={}",
        mode.argument(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        output.stderr.is_empty(),
        "{} probe wrote unexpected stderr: {}",
        mode.argument(),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        output.stdout.len() <= MAX_PARITY_ENVELOPE_BYTES + 2,
        "{} probe exceeded the bounded envelope",
        mode.argument(),
    );
    let output = String::from_utf8(output.stdout).expect("the parity envelope must be UTF-8");
    let envelope = output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .expect("the parity probe must emit exactly one terminated envelope");
    assert!(!envelope.contains('\n') && !envelope.contains('\r'));
    assert!(envelope.starts_with("nara-host-parity-v1|"));
    envelope.to_owned()
}

#[cfg(target_os = "linux")]
fn require_desktop_environment(mode: HostMode) {
    if mode == HostMode::Desktop {
        assert!(
            std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some(),
            "the desktop parity probe requires X11/Xvfb or Wayland; the renderer will select a low-power or fallback adapter",
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn require_desktop_environment(_mode: HostMode) {}

struct ChildEnvironment {
    root: PathBuf,
    cwd: PathBuf,
    home: PathBuf,
}

impl ChildEnvironment {
    fn new(mode: HostMode) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nara-host-parity-{}-{}-{nonce}",
            std::process::id(),
            mode.argument(),
        ));
        let cwd = root.join("cwd");
        let home = root.join("home");
        for directory in [
            cwd.as_path(),
            home.as_path(),
            &home.join("cache"),
            &home.join("config"),
            &home.join("data"),
            &home.join("appdata"),
            &home.join("local-appdata"),
        ] {
            fs::create_dir_all(directory).expect("the parity child environment should be created");
        }
        Self { root, cwd, home }
    }
}

impl Drop for ChildEnvironment {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("the parity child environment should be removable");
    }
}
