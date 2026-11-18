use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::{
    ProcessLogLimits, ProcessOutputStream, SupervisedProcessCleanupReceipt,
    SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
};
use serde_json::{Value, json};

use super::{OUTPUT_BYTES, window::Window};

pub const POLL: Duration = Duration::from_millis(10);
pub const READY: &str = "TWO_LEVEL_DESKTOP_READY ";
pub const OBSERVATION: &str = "TWO_LEVEL_DESKTOP_OBSERVATION ";
pub const RESULT: &str = "TWO_LEVEL_DESKTOP_RESULT ";

pub struct NativeProcess {
    owner: Option<SupervisedProcessOwner>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    parsed: usize,
    title: Option<String>,
    window: Option<Window>,
}

impl NativeProcess {
    pub fn spawn(command: &SupervisedProcessCommand, title: Option<String>) -> Self {
        Self::new(
            SupervisedProcessOwner::spawn(command, log_limits()).unwrap(),
            title,
        )
    }

    pub fn new(owner: SupervisedProcessOwner, title: Option<String>) -> Self {
        Self {
            owner: Some(owner),
            stdout: Vec::new(),
            stderr: Vec::new(),
            parsed: 0,
            title,
            window: None,
        }
    }

    pub fn pid(&self) -> u32 {
        self.owner.as_ref().unwrap().process_id()
    }

    pub fn complete(&self) -> bool {
        self.owner.as_ref().unwrap().state() == SupervisedProcessState::Complete
    }

    pub fn bind_window(&mut self) -> Window {
        let window = Window::find(self.pid(), self.title.as_ref().unwrap())
            .unwrap()
            .expect("one exact owned game window");
        self.window = Some(window);
        window
    }

    fn advance(&mut self) -> Result<(), String> {
        let (_, outputs) = self
            .owner
            .as_mut()
            .unwrap()
            .advance_with_output(Instant::now())
            .map_err(|error| error.to_string())?;
        for output in outputs {
            if output.loss()
                || output.bytes().len()
                    > OUTPUT_BYTES.saturating_sub(self.stdout.len() + self.stderr.len())
            {
                return Err("native stdout/stderr exceeded the cumulative 4 MiB allowance".into());
            }
            let bytes = match output.stream() {
                ProcessOutputStream::Stdout => &mut self.stdout,
                ProcessOutputStream::Stderr => &mut self.stderr,
            };
            bytes.extend_from_slice(output.bytes());
        }
        Ok(())
    }

    pub fn poll(&mut self) {
        if let Err(error) = self.advance() {
            panic!("{error}; {}", self.diagnostics());
        }
    }

    pub fn records(&mut self) -> Vec<(&'static str, Value)> {
        let mut result = Vec::new();
        while let Some(length) = self.stdout[self.parsed..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = self.parsed + length;
            let line = std::str::from_utf8(&self.stdout[self.parsed..end])
                .expect("UTF-8 Player stdout")
                .trim_end_matches('\r');
            assert!(line.len() <= 256 * 1024, "bounded game observation line");
            for prefix in [READY, OBSERVATION, RESULT] {
                if let Some(text) = line.strip_prefix(prefix) {
                    result.push((
                        prefix,
                        serde_json::from_str(text).expect("complete Player evidence JSON"),
                    ));
                    break;
                }
            }
            self.parsed = end + 1;
        }
        result
    }

    pub fn stdout(&self) -> String {
        String::from_utf8(self.stdout.clone()).unwrap()
    }

    pub fn receipt(&self) -> SupervisedProcessCleanupReceipt {
        self.owner
            .as_ref()
            .unwrap()
            .cleanup_receipt()
            .expect("original native cleanup receipt")
    }

    pub fn assert_clean(&self) {
        self.assert_physical_cleanup();
        assert!(
            !self.receipt().termination_requested(),
            "{}",
            self.diagnostics()
        );
    }

    pub fn assert_cooker_clean(&self) {
        assert!(self.title.is_none() && self.window.is_none());
        self.assert_physical_cleanup();
        assert!(self.receipt().exit().success(), "{}", self.diagnostics());
        eprintln!("COOKED_PLAYER_COOKER_CLEANUP {}", self.cleanup_json());
    }

    fn assert_physical_cleanup(&self) {
        let receipt = self.receipt();
        assert!(
            receipt.containment_empty()
                && receipt.readers_stopped()
                && receipt.output_drained()
                && !receipt.force_killed(),
            "{}",
            self.diagnostics()
        );
        assert!(
            !self.owner.as_ref().unwrap().output_loss_observed(),
            "{}",
            self.diagnostics()
        );
        if let Some(title) = &self.title {
            assert!(
                Window::find(self.pid(), title).unwrap().is_none(),
                "owned window survived original process exit"
            );
            if let Some(window) = self.window {
                assert!(
                    !window.exists(),
                    "original HWND survived original process exit"
                );
            }
        }
    }

    pub fn cleanup_json(&self) -> Value {
        let receipt = self.receipt();
        json!({"pid": self.pid(), "code": receipt.exit().code(), "success": receipt.exit().success(),
            "containment_empty": receipt.containment_empty(), "readers_stopped": receipt.readers_stopped(),
            "output_drained": receipt.output_drained(), "force_killed": receipt.force_killed(),
            "termination_requested": receipt.termination_requested(),
            "window_destroyed": self.window.is_none_or(|window| !window.exists())})
    }

    pub fn finish(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.poll();
            if self.complete() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native process exceeded {timeout:?}: {}",
                self.diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    pub fn close_window(&mut self) {
        if let Some(window) = self.window {
            window.close(self.title.as_ref().unwrap()).unwrap();
        } else if let Some(title) = &self.title
            && let Some(window) = Window::find(self.pid(), title).unwrap()
        {
            self.window = Some(window);
            window.close(title).unwrap();
        }
    }

    pub fn retire_normally(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(title) = &self.title {
                let window = self
                    .window
                    .or_else(|| Window::find(self.pid(), title).ok().flatten());
                if let Some(window) = window {
                    self.window = Some(window);
                    if window.exists() {
                        let _ = window.close(title);
                    }
                }
            }
            if let Err(error) = self.advance() {
                eprintln!("normal cleanup observation: {error}");
            }
            if self.complete() {
                let receipt = self.receipt();
                return receipt.containment_empty()
                    && receipt.readers_stopped()
                    && receipt.output_drained()
                    && !receipt.force_killed()
                    && self.window.is_none_or(|window| !window.exists());
            }
            thread::sleep(POLL);
        }
        false
    }

    pub fn diagnostics(&self) -> String {
        let owner = self.owner.as_ref().unwrap();
        format!(
            "pid={} state={:?} cleanup={:?} stdout={} stderr={}",
            owner.process_id(),
            owner.state(),
            owner.cleanup_receipt(),
            String::from_utf8_lossy(&self.stdout[self.stdout.len().saturating_sub(8192)..]),
            String::from_utf8_lossy(&self.stderr[self.stderr.len().saturating_sub(8192)..])
        )
    }
}

impl Drop for NativeProcess {
    fn drop(&mut self) {
        if !self.retire_normally(Duration::from_secs(30)) {
            eprintln!(
                "retaining original native custody without forced termination: {}",
                self.diagnostics()
            );
            std::mem::forget(self.owner.take());
        }
    }
}

pub fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::try_from_counts(512, OUTPUT_BYTES).unwrap()
}

pub fn record(text: &str, prefix: &str) -> Value {
    let mut records = text.lines().filter_map(|line| line.strip_prefix(prefix));
    let record = records
        .next()
        .unwrap_or_else(|| panic!("missing {prefix}: {text}"));
    assert!(records.next().is_none(), "duplicate {prefix}");
    serde_json::from_str(record).unwrap()
}

pub fn read_bounded(path: &Path, limit: usize) -> Vec<u8> {
    let file = File::open(path).unwrap();
    assert!(
        file.metadata().unwrap().len() <= limit as u64,
        "{}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).unwrap();
    assert!(
        bytes.len() <= limit,
        "{} grew beyond its bound",
        path.display()
    );
    bytes
}

pub fn remove_checked_tree(root: &Path) {
    assert!(root.is_absolute() && root.canonicalize().unwrap() == root);
    let mut pending = vec![root.to_owned()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        assert!(visited <= 16_384, "bounded copied project/cache");
        let metadata = fs::symlink_metadata(&path).unwrap();
        use std::os::windows::fs::MetadataExt;
        assert_eq!(
            metadata.file_attributes() & 0x400,
            0,
            "no reparse points in owned deletion"
        );
        assert!(!metadata.file_type().is_symlink() && path.canonicalize().unwrap() == path);
        assert!(path.starts_with(root));
        if metadata.is_dir() {
            pending.extend(
                fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            assert!(pending.len() <= 16_384);
        } else {
            assert!(metadata.is_file());
        }
    }
    fs::remove_dir_all(root).unwrap();
}
