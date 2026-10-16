use std::{env, thread, time::Duration};

use super::*;

#[test]
fn rustc_identity_requires_one_bounded_host_target() {
    let identity = parse_rustc_identity(
        b"rustc 1.91.0\nbinary: rustc\nhost: x86_64-pc-windows-msvc\nrelease: 1.91.0\n",
    )
    .expect("one rustc host target is admitted");
    assert_eq!(identity.host_target, "x86_64-pc-windows-msvc");

    assert!(parse_rustc_identity(b"rustc 1.91.0\n").is_none());
    assert!(
        parse_rustc_identity(b"rustc 1.91.0\nhost: x86_64-pc-windows-msvc\nhost: other-target\n")
            .is_none()
    );
}

#[test]
fn bounded_stdout_rejects_growth_before_appending_it() {
    let mut output = BoundedStdout::new(4);
    output.push(b"1234").unwrap();
    assert!(matches!(
        output.push(b"5"),
        Err(BoundedStdoutError::LimitExceeded)
    ));
    assert_eq!(output.take(), b"1234");
}

#[test]
fn tool_admission_rejects_relative_process_authority() {
    assert!(matches!(
        DesktopBuildTools::new(PathBuf::from("cargo"), PathBuf::from("rustc")),
        Err(DesktopBuildToolError::OverrideNotAbsolute {
            tool: DesktopBuildTool::Cargo
        })
    ));
}

#[cfg(windows)]
#[test]
fn tool_admission_preserves_behavioral_symlink_proxy_name() {
    let path = env::var_os("PATH").expect("the Rust test environment provides PATH");
    let proxy = env::split_paths(&path)
        .map(|directory| directory.join("rustc.exe"))
        .find(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|metadata| metadata.file_type().is_symlink())
        });
    let Some(proxy) = proxy else {
        return;
    };
    let expected = fs::canonicalize(proxy.parent().expect("the rustc proxy has a parent"))
        .unwrap()
        .join(proxy.file_name().expect("the rustc proxy has a file name"));

    let admitted = DesktopBuildTools::new(proxy.clone(), proxy)
        .unwrap()
        .rustc()
        .to_path_buf();

    assert_eq!(admitted, expected);
}

#[test]
fn revision_mismatch_names_the_first_changed_authority_member() {
    let admitted = DesktopBuildInputRevision::new(
        ContentDigest::of_bytes(b"manifest-a"),
        ContentDigest::of_bytes(b"lock-a"),
    );
    assert_eq!(
        revision_mismatch(
            admitted,
            DesktopBuildInputRevision::new(
                ContentDigest::of_bytes(b"manifest-b"),
                ContentDigest::of_bytes(b"lock-b"),
            ),
        ),
        Some(DesktopBuildInputMember::Manifest)
    );
    assert_eq!(
        revision_mismatch(
            admitted,
            DesktopBuildInputRevision::new(admitted.manifest(), ContentDigest::of_bytes(b"lock-b"),),
        ),
        Some(DesktopBuildInputMember::Lockfile)
    );
}

#[test]
fn cancellation_becomes_terminal_only_after_supervised_cleanup() {
    let executable = env::current_exe().unwrap();
    let project_root = executable.parent().unwrap().to_path_buf();
    let request = DesktopBuildRequest {
        manifest_path: project_root.join("Cargo.toml"),
        lockfile_path: project_root.join("Cargo.lock"),
        project_root,
        expected_revision: DesktopBuildInputRevision::new(
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"lockfile"),
        ),
        tools: DesktopBuildTools {
            cargo: executable.clone(),
            rustc: executable,
        },
    };
    let process = SupervisedProcessOwner::spawn(
        &crate::supervised_play::tests::helper_command("wait"),
        default_log_limits(),
    )
    .unwrap();
    let mut attempt = DesktopBuildAttempt::from_spawned_process(request, process);
    let start = Instant::now();

    attempt.request_cancel(start).unwrap();
    assert!(attempt.terminal().is_none());

    let deadline = start + Duration::from_secs(10);
    while attempt.terminal().is_none() {
        attempt.advance(Instant::now()).unwrap();
        assert!(Instant::now() < deadline, "build cleanup timed out");
        thread::sleep(Duration::from_millis(5));
    }

    assert!(matches!(
        attempt.terminal(),
        Some(DesktopBuildTerminal::Cancelled)
    ));
    assert!(attempt.process.is_none());
}
