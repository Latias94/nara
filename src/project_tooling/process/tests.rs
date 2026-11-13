use super::{SupervisedProcessCommand, SupervisedProductEnvironmentPolicy};

#[test]
fn protocol_stdout_capture_is_explicit_and_retained_by_the_command() {
    let command = SupervisedProcessCommand::new("cargo", ".");
    assert_eq!(command.protocol_stdout_limit(), None);
    let limit = std::num::NonZeroUsize::new(16 * 1024 * 1024).unwrap();
    let command = command.bounded_stdout(limit);
    assert_eq!(command.protocol_stdout_limit(), Some(limit));
    assert_eq!(command.clone(), command);
}

#[test]
fn product_environment_allowlist_excludes_authority_and_injection_variables() {
    let policy = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
    for rejected in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "AWS_ACCESS_KEY_ID",
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "DYLD_INSERT_LIBRARIES",
    ] {
        assert!(!policy.allows_name(rejected), "{rejected} must stay sealed");
    }
    assert!(
        SupervisedProductEnvironmentPolicy::Sealed
            .allowlisted_names()
            .is_empty()
    );
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn helper_process() {
    let Ok(mode) = std::env::var("NARA_SHARED_PROCESS_TEST_MODE") else {
        return;
    };
    match mode.as_str() {
        "exit-success" => {
            println!("helper stdout");
            eprintln!("helper stderr");
        }
        "wait" => std::thread::sleep(std::time::Duration::from_secs(30)),
        _ => panic!("unsupported shared process helper mode"),
    }
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
fn helper_command(mode: &str) -> SupervisedProcessCommand {
    let executable = std::env::current_exe().expect("the test executable is available");
    let working_directory = executable
        .parent()
        .expect("the test executable has a parent")
        .to_path_buf();
    SupervisedProcessCommand::new(executable, working_directory)
        .arguments([
            "--exact",
            "project_tooling::process::tests::helper_process",
            "--nocapture",
        ])
        .environment("NARA_SHARED_PROCESS_TEST_MODE", mode)
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
mod loss_observation_tests {
    use std::time::Instant;

    use super::helper_command;
    use crate::project_tooling::process::{
        SupervisedProcessOwner, SupervisedProcessState, default_log_limits,
    };

    #[test]
    fn quiet_running_process_has_a_stable_exit_observation_deadline() {
        let mut owner =
            SupervisedProcessOwner::spawn(&helper_command("wait"), default_log_limits()).unwrap();
        let now = Instant::now();
        owner.advance(now).unwrap();
        let deadline = owner
            .next_deadline()
            .expect("a silent child still needs exit observation");
        assert!(deadline > now);
        assert_eq!(
            owner.next_deadline(),
            Some(deadline),
            "querying a deadline must not postpone it"
        );
    }

    #[test]
    fn advanced_cleanup_phase_does_not_reuse_the_expired_terminate_deadline() {
        let mut owner =
            SupervisedProcessOwner::spawn(&helper_command("wait"), default_log_limits()).unwrap();
        let now = Instant::now();
        owner.terminate_deadline = Some(now);
        owner.force_kill_deadline = Some(now + std::time::Duration::from_secs(1));
        owner.drain_deadline = Some(now + std::time::Duration::from_secs(2));
        owner.state = SupervisedProcessState::ForceKilling;
        owner.force_killed = true;
        assert_eq!(owner.next_deadline(), owner.force_kill_deadline);
        owner.state = SupervisedProcessState::Draining;
        owner.output_observed_drained = true;
        assert_eq!(owner.next_deadline(), owner.force_kill_deadline);
        owner.state = SupervisedProcessState::Complete;
        let completed_deadline = owner.next_deadline();
        owner.state = SupervisedProcessState::Running;
        assert_eq!(completed_deadline, None);
    }

    #[test]
    fn advance_retains_output_loss_without_returning_a_raw_batch() {
        let mut owner =
            SupervisedProcessOwner::spawn(&helper_command("wait"), default_log_limits())
                .expect("the helper process spawns");
        owner.process.test_enqueue_loss();

        let observation = owner.advance(Instant::now()).expect("the process advances");

        assert!(observation.output_loss_observed());
        assert!(owner.output_loss_observed());
    }

    #[test]
    fn stopped_readers_with_pending_protocol_bytes_do_not_hit_the_reader_deadline() {
        let command = helper_command("exit-success")
            .bounded_stdout(std::num::NonZeroUsize::new(4 * 1024 * 1024).unwrap());
        let mut owner = SupervisedProcessOwner::spawn(&command, default_log_limits()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let exited = owner.process.try_wait().unwrap().is_some();
            if exited
                && owner.process.readers_stopped()
                && owner.process.containment_empty().unwrap()
            {
                break;
            }
            assert!(Instant::now() < deadline, "the helper must physically stop");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        while owner.process.try_output().unwrap().is_some() {}
        let expected = vec![b'X'; 3 * 1024 * 1024];
        owner.process.test_enqueue_stdout(expected.clone());

        let now = Instant::now();
        let mut captured = Vec::new();
        for seconds in [0, 2] {
            let observed_at = now + std::time::Duration::from_secs(seconds);
            let (observation, outputs) = owner.advance_with_output(observed_at).unwrap();
            assert_eq!(observation.state, SupervisedProcessState::Draining);
            assert!(
                outputs.len() <= 64,
                "the per-poll record budget is unchanged"
            );
            for output in outputs {
                assert!(!output.loss());
                captured.extend_from_slice(output.bytes());
            }
            assert!(!owner.output_cancel_requested);
            assert!(owner.cleanup_receipt().is_none());
            assert!(
                owner
                    .next_deadline()
                    .is_some_and(|deadline| deadline > observed_at)
            );
        }
        let (observation, outputs) = owner
            .advance_with_output(now + std::time::Duration::from_secs(4))
            .unwrap();
        for output in outputs {
            assert!(!output.loss());
            captured.extend_from_slice(output.bytes());
        }
        assert_eq!(captured, expected);
        assert_eq!(observation.state, SupervisedProcessState::Complete);
        assert!(!owner.output_cancel_requested);
        let receipt = owner.cleanup_receipt().unwrap();
        assert!(receipt.containment_empty() && receipt.readers_stopped());
        assert!(receipt.output_drained());
    }
}
