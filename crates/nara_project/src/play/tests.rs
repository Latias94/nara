use std::{
    env, fs,
    io::{self, Write as _},
    process::{Command, Stdio},
};

use super::*;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const MODE_FILE: &str = "play-mode";
const READY_FILE: &str = "descendant-ready";
const EXIT_FILE: &str = "exit-now";

struct TestProject(tempfile::TempDir);

impl TestProject {
    fn new(mode: &str) -> Self {
        let project = Self(tempfile::tempdir().unwrap());
        fs::write(project.0.path().join(MODE_FILE), mode).unwrap();
        project
    }

    fn launch(&self) -> PlayProcess {
        PlayProcess::launch(
            self.0.path(),
            &env::current_exe().unwrap(),
            &helper_arguments("tree_helper"),
            ProjectTrust::Trusted,
        )
        .unwrap()
    }
}

#[test]
fn an_untrusted_project_cannot_launch_or_build() {
    let project = TestProject::new("hold");
    let executable = env::current_exe().unwrap();
    let launch = PlayProcess::launch(
        project.0.path(),
        &executable,
        &helper_arguments("tree_helper"),
        ProjectTrust::Untrusted,
    );
    assert!(matches!(launch, Err(error) if error.kind() == io::ErrorKind::PermissionDenied));

    let request = BuildRequest::new(project.0.path(), "demo", "game");
    assert!(matches!(
        build(&request, ProjectTrust::Untrusted, |_| panic!(
            "Cargo executed"
        )),
        Err(BuildError::UntrustedProject)
    ));
    assert!(!project.0.path().join(READY_FILE).exists());
}

#[test]
fn stop_and_restart_reap_the_leader_and_terminate_a_descendant_without_output_pipes() {
    let project = TestProject::new("hold");
    let mut process = project.launch();
    let ((first_leader, first_descendant), mut output) = observe_tree(&mut process);
    let leader_probe = ProcessProbe::open(first_leader);
    let descendant = ProcessProbe::open(first_descendant);
    assert!(descendant.active());
    assert!(
        output
            .iter()
            .any(|line| line.stream == OutputStream::Stderr && line.text == "project mode: hold")
    );
    assert!(!output.iter().any(|line| line.text == "final tail"));

    process.restart(|line| output.push(line)).unwrap();
    assert!(!leader_probe.active());
    assert!(!descendant.active());
    #[cfg(unix)]
    assert_reaped(first_leader);
    assert!(
        output
            .iter()
            .any(|line| line.stream == OutputStream::Stderr && line.text == "final tail")
    );

    let ((second_leader, second_descendant), _) = observe_tree(&mut process);
    let leader_probe = ProcessProbe::open(second_leader);
    let descendant = ProcessProbe::open(second_descendant);
    assert!(descendant.active());
    assert!(!process.stop(|_| {}).unwrap().success());
    assert!(!leader_probe.active());
    assert!(!descendant.active());
    #[cfg(unix)]
    assert_reaped(second_leader);
    assert!(
        process
            .poll(|line| panic!("unexpected final line: {line:?}"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn natural_exit_preserves_the_leader_status_and_cleans_up_remaining_descendants() {
    let project = TestProject::new("exit");
    let mut process = project.launch();
    let ((leader, descendant), _) = observe_tree(&mut process);
    let leader_probe = ProcessProbe::open(leader);
    let descendant = ProcessProbe::open(descendant);
    fs::write(project.0.path().join(EXIT_FILE), "exit").unwrap();
    assert!(process.wait(|_| {}).unwrap().success());
    assert!(!leader_probe.active());
    assert!(!descendant.active());
    #[cfg(unix)]
    assert_reaped(leader);
}

#[test]
fn dropping_the_owner_terminates_the_contained_tree() {
    let project = TestProject::new("hold");
    let mut process = project.launch();
    let ((leader, descendant), _) = observe_tree(&mut process);
    let leader_probe = ProcessProbe::open(leader);
    let descendant = ProcessProbe::open(descendant);
    drop(process);
    assert!(
        !leader_probe.active(),
        "drop returned before the leader exited"
    );
    #[cfg(windows)]
    project.0.close().unwrap();
    let deadline = Instant::now() + TEST_TIMEOUT;
    while descendant.active() {
        assert!(Instant::now() < deadline, "drop left an active process");
        thread::sleep(POLL_INTERVAL);
    }
    #[cfg(unix)]
    assert_reaped(leader);
}

#[test]
fn oversized_stdout_stops_and_reaps_a_producer_tree() {
    let project = TestProject::new("oversized");
    let mut tree = None;
    let error = capture_stdout(
        project.0.path(),
        &env::current_exe().unwrap(),
        &helper_arguments("tree_helper"),
        1024,
        || false,
        |line| {
            if let Some(pids) = line.text.strip_prefix("started tree ") {
                let (leader, descendant) = pids.split_once(' ').unwrap();
                let leader = leader.parse().unwrap();
                tree = Some((
                    leader,
                    ProcessProbe::open(leader),
                    ProcessProbe::open(descendant.parse().unwrap()),
                ));
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    let (leader, leader_probe, descendant) = tree.unwrap();
    assert!(!leader_probe.active());
    assert!(!descendant.active());
    #[cfg(unix)]
    assert_reaped(leader);
    let _ = leader;
}

#[test]
#[ignore = "launched by the Play process tests"]
#[allow(
    clippy::zombie_processes,
    reason = "the parent test supervises the process tree"
)]
fn tree_helper() {
    let Ok(mode) = fs::read_to_string(MODE_FILE) else {
        return;
    };
    #[cfg(windows)]
    let _snapshot = {
        use std::os::windows::fs::OpenOptionsExt as _;

        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open("snapshot-lock")
            .unwrap()
    };
    let descendant = Command::new(env::current_exe().unwrap())
        .args(helper_arguments("descendant_helper"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + TEST_TIMEOUT;
    let expected = descendant.id().to_string();
    while !fs::read_to_string(READY_FILE).is_ok_and(|ready| ready == expected) {
        assert!(Instant::now() < deadline, "descendant did not start");
        thread::sleep(POLL_INTERVAL);
    }
    let mut stderr = io::stderr().lock();
    write!(stderr, "project mode: {mode}\r\nfinal tail").unwrap();
    stderr.flush().unwrap();
    println!("play-tree {} {}", std::process::id(), descendant.id());
    io::stdout().flush().unwrap();
    if mode == "oversized" {
        writeln!(
            stderr,
            "\nstarted tree {} {}",
            std::process::id(),
            descendant.id()
        )
        .unwrap();
        stderr.flush().unwrap();
        let _ = io::stdout().write_all(&[b'x'; 16 * 1024]);
        thread::sleep(Duration::from_secs(30));
    } else if mode == "hold" {
        thread::sleep(Duration::from_secs(30));
    } else {
        let deadline = Instant::now() + TEST_TIMEOUT;
        while !Path::new(EXIT_FILE).is_file() {
            assert!(
                Instant::now() < deadline,
                "parent did not request natural exit"
            );
            thread::sleep(POLL_INTERVAL);
        }
    }
}

#[test]
#[ignore = "launched by the Play process tests"]
fn descendant_helper() {
    if !Path::new(MODE_FILE).is_file() {
        return;
    }
    #[cfg(unix)]
    {
        // SAFETY: The helper changes only its own SIGTERM disposition to SIG_IGN.
        assert_ne!(
            unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) },
            libc::SIG_ERR
        );
    }
    fs::write(READY_FILE, std::process::id().to_string()).unwrap();
    thread::sleep(Duration::from_secs(30));
}

fn helper_arguments(name: &str) -> [OsString; 5] {
    let (_, module) = module_path!().split_once("::").unwrap();
    [
        "--ignored".into(),
        "--nocapture".into(),
        "--exact".into(),
        format!("{module}::{name}").into(),
        "--test-threads=1".into(),
    ]
}

fn observe_tree(process: &mut PlayProcess) -> ((u32, u32), Vec<OutputLine>) {
    let deadline = Instant::now() + TEST_TIMEOUT;
    let mut output = Vec::new();
    loop {
        let status = process.poll(|line| output.push(line)).unwrap();
        if let Some(pids) = output.iter().find_map(|line| {
            let (_, pids) = line.text.split_once("play-tree ")?;
            let (leader, descendant) = pids.split_once(' ')?;
            Some((leader.parse().ok()?, descendant.parse().ok()?))
        }) {
            return (pids, output);
        }
        assert!(
            status.is_none() && Instant::now() < deadline,
            "tree helper did not start: {output:?}"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

#[cfg(windows)]
struct ProcessProbe(OwnedHandle);

#[cfg(windows)]
impl ProcessProbe {
    fn open(pid: u32) -> Self {
        // SAFETY: A successful call returns a new owned handle for the observed PID.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!handle.is_null(), "{}", io::Error::last_os_error());
        // SAFETY: OpenProcess transferred ownership to this caller.
        Self(unsafe { OwnedHandle::from_raw_handle(handle) })
    }

    fn active(&self) -> bool {
        // SAFETY: The probe owns a valid synchronization handle.
        let result = unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) };
        assert!(result == WAIT_OBJECT_0 || result == WAIT_TIMEOUT);
        result == WAIT_TIMEOUT
    }
}

#[cfg(unix)]
struct ProcessProbe(u32);

#[cfg(unix)]
impl ProcessProbe {
    fn open(pid: u32) -> Self {
        Self(pid)
    }

    #[cfg(target_os = "linux")]
    fn active(&self) -> bool {
        match fs::read_to_string(format!("/proc/{}/stat", self.0)) {
            Ok(record) => {
                let (_, fields) = record.rsplit_once(") ").unwrap();
                !fields.starts_with(['Z', 'X', 'x'])
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                false
            }
            Err(error) => panic!("could not inspect process: {error}"),
        }
    }

    #[cfg(target_os = "macos")]
    fn active(&self) -> bool {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        // SAFETY: `info` has space for one process record of the requested flavor.
        let read = unsafe {
            libc::proc_pidinfo(
                self.0 as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if read == 0 {
            assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
            return false;
        }
        assert_eq!(read, size);
        // SAFETY: libproc wrote a complete process record.
        unsafe { info.assume_init() }.pbi_status != libc::SZOMB
    }
}

#[cfg(unix)]
fn assert_reaped(pid: u32) {
    let mut status = 0;
    // SAFETY: This writes at most one status word and does not block on a live child.
    assert_eq!(
        unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}
