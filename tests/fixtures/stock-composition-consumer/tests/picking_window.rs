#![forbid(unsafe_code)]
#![cfg(windows)]

//! Launch the prebuilt native consumer so Winit runs on the child process's main thread.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_LOG_BYTES: u64 = 128 * 1024;
static NATIVE_RUN: Mutex<()> = Mutex::new(());

#[test]
#[ignore = "requires two active Windows displays with different DPI, a hardware GPU, and the prebuilt picking_window example"]
fn ordinary_runtime_picking_matches_pixels_across_actual_mixed_dpi_displays() -> Result<(), String>
{
    run_native_picking("--mixed-dpi", false)
}

#[test]
#[ignore = "requires a Windows desktop and the prebuilt picking_window example; allows an explicitly selected software adapter"]
fn ordinary_runtime_picking_matches_pixels_on_one_native_display() -> Result<(), String> {
    run_native_picking("--single-display", false)
}

#[test]
#[ignore = "requires a Windows desktop and the prebuilt picking_window example"]
fn observer_failure_retires_original_window_before_resuming_its_panic() -> Result<(), String> {
    run_native_picking("--single-display", true)
}

fn run_native_picking(mode: &str, fail_observer: bool) -> Result<(), String> {
    let _serial = NATIVE_RUN
        .lock()
        .map_err(|_| "native picking test lock is poisoned")?;
    let backend = std::env::var("WGPU_BACKEND")
        .map_err(|_| "set WGPU_BACKEND explicitly to dx12 or vulkan")?;
    if !matches!(backend.as_str(), "dx12" | "vulkan") {
        return Err("the Windows picking consumer requires WGPU_BACKEND=dx12 or vulkan".into());
    }
    let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK")
        .map_err(|_| "set NARA_WGPU_FORCE_FALLBACK explicitly to 0 or 1")?;
    if !matches!(fallback.as_str(), "0" | "1") {
        return Err("NARA_WGPU_FORCE_FALLBACK must be explicitly 0 or 1".into());
    }
    if mode == "--mixed-dpi" && fallback != "0" {
        return Err("actual mixed-DPI qualification requires NARA_WGPU_FORCE_FALLBACK=0".into());
    }
    let executable = prebuilt_example()?;
    let logs =
        tempfile::tempdir().map_err(|error| format!("create native picking logs: {error}"))?;
    let stdout = logs.path().join("picking-window.stdout");
    let stderr = logs.path().join("picking-window.stderr");
    let output =
        File::create(&stdout).map_err(|error| format!("create stdout capture: {error}"))?;
    let errors =
        File::create(&stderr).map_err(|error| format!("create stderr capture: {error}"))?;
    let mut child = RunningChild(
        Command::new(&executable)
            .arg(mode)
            .args(fail_observer.then_some("--fail-observer"))
            .env("WGPU_BACKEND", &backend)
            .env("NARA_WGPU_FORCE_FALLBACK", &fallback)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(errors))
            .spawn()
            .map_err(|error| {
                format!(
                    "launch prebuilt native picking consumer {}: {error}",
                    executable.display()
                )
            })?,
    );
    let status = wait_for_child(&mut child.0, [&stdout, &stderr]);
    drop(child);
    let output = read_capture(&stdout)?;
    let errors = read_capture(&stderr)?;
    let diagnostics = || format!("mode={mode}\nstdout:\n{output}\nstderr:\n{errors}");
    let status = status.map_err(|error| format!("{error}\n{}", diagnostics()))?;
    if fail_observer {
        if status.code() != Some(101)
            || !errors.contains("injected original picking observer failure")
            || !errors.lines().any(|line| line == "PICKING_FAILURE_CLEANED original_runtime_retired=true original_window_destroyed=true original_pump_returned=true")
            || output.contains("PICKING_WINDOW_OK")
        {
            return Err(format!("the original observer panic must unwind only after native retirement: {status}\n{}",diagnostics()));
        }
        println!(
            "PICKING_FAILURE_CLEANED original_runtime_retired=true original_window_destroyed=true original_pump_returned=true"
        );
        return Ok(());
    }
    if !status.success() {
        return Err(format!(
            "native picking consumer failed: {status}\n{}",
            diagnostics()
        ));
    }
    validate_summary(&output, mode).map_err(|error| format!("{error}\n{}", diagnostics()))?;
    if errors
        .lines()
        .any(|line| line.split_ascii_whitespace().next() == Some("PICKING_WINDOW_OK"))
    {
        return Err(format!(
            "the success marker must occur only once, on stdout\n{}",
            diagnostics()
        ));
    }
    print!("{output}");
    eprint!("{errors}");
    Ok(())
}

fn prebuilt_example() -> Result<PathBuf, String> {
    let executable = match std::env::var_os("NARA_STOCK_PICKING_WINDOW_EXE") {
        Some(path) => PathBuf::from(path),
        None => {
            let test = std::env::current_exe()
                .map_err(|error| format!("locate native picking test executable: {error}"))?;
            test.parent()
                .and_then(Path::parent)
                .ok_or("expected the native test executable under profile/deps")?
                .join("examples")
                .join("picking_window.exe")
        }
    };
    if !executable.is_absolute() || !executable.is_file() {
        return Err(format!(
            "prebuild --example picking_window, or set NARA_STOCK_PICKING_WINDOW_EXE to its absolute path: {}",
            executable.display()
        ));
    }
    Ok(executable)
}

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_for_child(child: &mut Child, captures: [&Path; 2]) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("observe the native picking child: {error}"))?
        {
            return Ok(status);
        }
        for path in captures {
            let bytes = fs::metadata(path)
                .map_err(|error| format!("observe native log {}: {error}", path.display()))?
                .len();
            if bytes > MAX_LOG_BYTES {
                return Err(format!(
                    "native picking log exceeded {MAX_LOG_BYTES} bytes: {}",
                    path.display()
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err("native picking consumer exceeded its 120-second deadline".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_capture(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_LOG_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("read native capture {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_LOG_BYTES {
        return Err(format!(
            "native picking log exceeded {MAX_LOG_BYTES} bytes: {}\n{}",
            path.display(),
            String::from_utf8_lossy(&bytes[..MAX_LOG_BYTES as usize])
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn validate_summary<'a>(output: &'a str, mode: &str) -> Result<&'a str, String> {
    let mut markers = output
        .lines()
        .filter(|line| line.split_ascii_whitespace().next() == Some("PICKING_WINDOW_OK"));
    let summary = markers
        .next()
        .ok_or("native picking consumer omitted PICKING_WINDOW_OK")?;
    if markers.next().is_some() {
        return Err(
            "native picking consumer emitted more than one PICKING_WINDOW_OK marker".into(),
        );
    }
    let mut fields = BTreeMap::new();
    for field in summary.split_ascii_whitespace().skip(1) {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed picking summary field: {field}"))?;
        if key.is_empty() || value.is_empty() || fields.insert(key, value).is_some() {
            return Err(format!("empty or duplicate picking summary field: {field}"));
        }
    }
    for (key, expected) in [
        ("modes", "4"),
        ("world_pointer_checked", "true"),
        ("prefix_cleanup", "true"),
        ("original_runtime_retired", "true"),
        ("original_window_destroyed", "true"),
    ] {
        if fields.get(key).copied() != Some(expected) {
            return Err(format!(
                "native picking summary requires {key}={expected}: {summary}"
            ));
        }
    }
    let replays = fields
        .get("pixel_replays")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            format!("native picking summary requires a pixel_replays count: {summary}")
        })?;
    if replays == 0 {
        return Err(format!(
            "native picking summary requires at least one actual pixel replay: {summary}"
        ));
    }
    let scales = fields
        .get("monitor_scales")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            format!("native picking summary requires a monitor_scales count: {summary}")
        })?;
    let minimum_scales = if mode == "--mixed-dpi" { 2 } else { 1 };
    if scales < minimum_scales {
        return Err(format!(
            "{mode} requires at least {minimum_scales} actual distinct monitor scales: {summary}"
        ));
    }
    Ok(summary)
}
