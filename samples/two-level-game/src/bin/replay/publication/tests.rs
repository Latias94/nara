use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::super::{Failure as CliFailure, RESULT_PREFIX, write_outcome};
use super::*;

const BYTES: &[u8] = b"complete replay recording\nwith a final newline\n";
const FOREIGN: &[u8] = b"another actor owns these bytes\n";

struct Fixture {
    temporary_parent: PathBuf,
    directory: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary_parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = temporary_parent.join(format!(
            "two-level-replay-publication-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        let directory = fs::canonicalize(directory).unwrap();
        assert_eq!(directory.parent(), Some(temporary_parent.as_path()));
        Self {
            output: directory.join("recording.json"),
            temporary_parent,
            directory,
        }
    }

    fn temporary(&self) -> PathBuf {
        let entries = fs::read_dir(&self.directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "tmp"))
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        entries[0].clone()
    }

    fn blocked_cleanup(&self) -> PublicationFailure {
        let directory = self.directory.clone();
        publish_inner(
            &self.output,
            BYTES,
            Faults {
                checkpoint: Some(Box::new(move |point| {
                    if point == Checkpoint::Written {
                        #[cfg(windows)]
                        {
                            let temporary = fs::read_dir(&directory)
                                .unwrap()
                                .next()
                                .unwrap()
                                .unwrap()
                                .path();
                            let mut permissions = fs::metadata(&temporary).unwrap().permissions();
                            permissions.set_readonly(true);
                            fs::set_permissions(temporary, permissions).unwrap();
                        }
                        #[cfg(not(windows))]
                        let _ = &directory;
                        return Err(interrupted(FsOperation::Write));
                    }
                    Ok(())
                })),
                ..Faults::default()
            },
        )
        .unwrap_err()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let Ok(directory) = fs::canonicalize(&self.directory) else {
            return;
        };
        if directory == self.directory
            && directory.parent() == Some(self.temporary_parent.as_path())
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

fn interrupted(operation: FsOperation) -> FsError {
    FsError::Io {
        operation,
        source: io::Error::other("injected replay publication failure"),
    }
}

fn assert_prepublication(failure: &PublicationFailure) {
    assert_eq!(failure.publication().result()["status"], "not_published");
    assert_eq!(failure.publication().result()["name_published"], "unknown");
    assert!(failure.cleanup_pending());
}

fn assert_published_failure(failure: PublicationFailure) -> CliFailure {
    assert!(!failure.cleanup_pending());
    let failure = CliFailure::publication(failure);
    let result = failure.result();
    assert_eq!(result["ok"], false);
    assert_eq!(result["cleanup_pending"], false);
    assert_eq!(result["error"]["stage"], "output");
    assert_eq!(result["publication"]["status"], "published_uncertain");
    assert_eq!(result["publication"]["data_synced"], "achieved");
    assert_eq!(result["publication"]["file_metadata_synced"], "achieved");
    assert_eq!(result["publication"]["name_published"], "achieved");
    assert!(result.get("report").is_none());
    failure
}

#[test]
fn partial_write_failure_keeps_final_absent_and_retains_the_actual_candidate() {
    let fixture = Fixture::new();
    let failure = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            fail_write_after: Some(7),
            ..Faults::default()
        },
    )
    .unwrap_err();
    assert_prepublication(&failure);
    assert!(!fixture.output.exists());
    let temporary = fixture.temporary();
    assert_eq!(fs::read(&temporary).unwrap(), &BYTES[..7]);
    let parent = open_parent(&fixture.directory).unwrap();
    let file = parent
        .open_child_file(&RelativeComponent::new(temporary.file_name().unwrap()).unwrap())
        .unwrap();
    let cleanup = failure.source.cleanup().unwrap();
    assert!(cleanup.identity().is_some());
    assert!(cleanup.same_object(&file).unwrap());
    assert!(matches!(
        failure.source.error(),
        FsError::Io {
            operation: FsOperation::Write,
            ..
        }
    ));
    drop(file);
    let failure = CliFailure::publication(failure);
    assert_eq!(failure.result()["publication"]["status"], "not_published");
    assert!(!fixture.output.exists());
    #[cfg(windows)]
    {
        assert_eq!(failure.result()["cleanup_pending"], false);
        assert!(!temporary.exists());
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(failure.result()["cleanup_pending"], true);
        assert_eq!(fs::read(temporary).unwrap(), &BYTES[..7]);
    }
}

#[test]
fn failure_after_complete_write_or_file_sync_never_publishes_final_bytes() {
    for stop in [Checkpoint::Written, Checkpoint::Synced] {
        let fixture = Fixture::new();
        let output = fixture.output.clone();
        let failure = publish_inner(
            &fixture.output,
            BYTES,
            Faults {
                checkpoint: Some(Box::new(move |point| {
                    assert!(!output.exists());
                    if point == stop {
                        Err(interrupted(FsOperation::SyncFile))
                    } else {
                        Ok(())
                    }
                })),
                ..Faults::default()
            },
        )
        .unwrap_err();
        assert_prepublication(&failure);
        assert!(!fixture.output.exists());
        assert_eq!(fs::read(fixture.temporary()).unwrap(), BYTES);
        assert_eq!(
            failure.publication().durability.data_synced(),
            if stop == Checkpoint::Synced {
                StageStatus::Achieved
            } else {
                StageStatus::Unknown
            },
        );
    }
}

#[test]
fn existing_output_refuses_without_creating_a_temporary() {
    let fixture = Fixture::new();
    fs::write(&fixture.output, FOREIGN).unwrap();
    let failure = publish(&fixture.output, BYTES).unwrap_err();
    assert!(!failure.cleanup_pending());
    assert_eq!(failure.publication().result()["status"], "not_published");
    assert!(matches!(
        failure.source.error(),
        FsError::TargetStateMismatch
    ));
    assert_eq!(fs::read(&fixture.output).unwrap(), FOREIGN);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);
}

#[test]
fn a_destination_created_after_candidate_sync_is_preserved() {
    let fixture = Fixture::new();
    let output = fixture.output.clone();
    let failure = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            checkpoint: Some(Box::new(move |point| {
                if point == Checkpoint::Synced {
                    File::create_new(&output)
                        .unwrap()
                        .write_all(FOREIGN)
                        .unwrap();
                }
                Ok(())
            })),
            ..Faults::default()
        },
    )
    .unwrap_err();
    assert_prepublication(&failure);
    assert!(matches!(
        failure.source.error(),
        FsError::TargetStateMismatch
    ));
    assert_eq!(fs::read(&fixture.output).unwrap(), FOREIGN);
    let temporary = fixture.temporary();
    assert_eq!(fs::read(&temporary).unwrap(), BYTES);
    let failure = CliFailure::publication(failure);
    assert_eq!(failure.result()["publication"]["status"], "not_published");
    assert_eq!(fs::read(&fixture.output).unwrap(), FOREIGN);
    #[cfg(windows)]
    assert!(!temporary.exists());
    #[cfg(target_os = "linux")]
    assert_eq!(failure.result()["cleanup_pending"], true);
}

#[test]
fn cleanup_failure_keeps_the_original_owner_inside_the_cli_failure() {
    let fixture = Fixture::new();
    let failure = fixture.blocked_cleanup();
    let identity = failure.source.cleanup().unwrap().identity();
    let mut failure = CliFailure::publication(failure);
    assert_eq!(failure.result()["cleanup_pending"], true);
    assert_eq!(failure.result()["publication"]["status"], "not_published");
    assert!(failure.runtime.is_none());
    assert!(!fixture.output.exists());
    let pending = failure.publication.as_mut().unwrap();
    assert_eq!(pending.source.cleanup().unwrap().identity(), identity);
    assert!(!pending.retry_cleanup());
    assert_eq!(pending.source.cleanup().unwrap().identity(), identity);
    let temporary = fixture.temporary();
    let parent = open_parent(&fixture.directory).unwrap();
    let file = parent
        .open_child_file(&RelativeComponent::new(temporary.file_name().unwrap()).unwrap())
        .unwrap();
    assert!(
        pending
            .source
            .cleanup()
            .unwrap()
            .same_object(&file)
            .unwrap()
    );
    assert_eq!(fs::read(&temporary).unwrap(), BYTES);
    drop(file);
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(&temporary).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&temporary, permissions).unwrap();
        assert!(pending.retry_cleanup());
        assert!(!pending.cleanup_pending());
        assert!(!temporary.exists());
    }
    #[cfg(target_os = "linux")]
    {
        assert!(!pending.retry_cleanup());
        assert!(pending.cleanup_pending());
        assert!(temporary.exists());
    }
    assert_eq!(
        failure.result()["cleanup_pending"],
        cfg!(target_os = "linux")
    );
    assert!(!fixture.output.exists());
}

#[test]
fn cleanup_never_deletes_a_foreign_file_at_the_former_temporary_name() {
    let fixture = Fixture::new();
    let directory = fixture.directory.clone();
    let moved = fixture.directory.join("moved-candidate");
    let moved_for_hook = moved.clone();
    let failure = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            checkpoint: Some(Box::new(move |point| {
                if point == Checkpoint::Written {
                    let temporary = fs::read_dir(&directory)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    fs::rename(&temporary, &moved_for_hook).unwrap();
                    File::create_new(temporary)
                        .unwrap()
                        .write_all(FOREIGN)
                        .unwrap();
                    return Err(interrupted(FsOperation::Write));
                }
                Ok(())
            })),
            ..Faults::default()
        },
    )
    .unwrap_err();
    assert_eq!(fs::read(&moved).unwrap(), BYTES);
    let former_temporary = fixture.temporary();
    assert_eq!(fs::read(&former_temporary).unwrap(), FOREIGN);
    let mut failure = CliFailure::publication(failure);
    failure.publication.as_mut().unwrap().retry_cleanup();
    assert_eq!(fs::read(&former_temporary).unwrap(), FOREIGN);
    assert!(!fixture.output.exists());
    #[cfg(windows)]
    {
        assert!(!moved.exists());
        assert_eq!(failure.result()["cleanup_pending"], false);
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(fs::read(moved).unwrap(), BYTES);
        assert_eq!(failure.result()["cleanup_pending"], true);
    }
}

#[test]
fn successful_publication_contains_only_complete_bytes_and_reports_actual_durability() {
    let fixture = Fixture::new();
    let publication = publish(&fixture.output, BYTES).unwrap();
    assert_eq!(fs::read(&fixture.output).unwrap(), BYTES);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);
    let result = publication.result();
    assert_eq!(result["status"], "published");
    assert_eq!(result["data_synced"], "achieved");
    assert_eq!(result["file_metadata_synced"], "achieved");
    assert_eq!(result["name_published"], "achieved");
    #[cfg(windows)]
    assert_eq!(result["parent_directory_synced"], "unsupported");
    #[cfg(target_os = "linux")]
    assert_eq!(result["parent_directory_synced"], "achieved");
}

#[test]
fn failure_after_name_publication_never_removes_the_published_file() {
    let fixture = Fixture::new();
    let output = fixture.output.clone();
    let failure = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            checkpoint: Some(Box::new(move |point| {
                if point == Checkpoint::Published {
                    assert_eq!(fs::read(&output).unwrap(), BYTES);
                    return Err(interrupted(FsOperation::Read));
                }
                Ok(())
            })),
            ..Faults::default()
        },
    )
    .unwrap_err();
    let mut failure = assert_published_failure(failure);
    assert!(failure.publication.as_mut().unwrap().retry_cleanup());
    assert_eq!(fs::read(&fixture.output).unwrap(), BYTES);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);
}

#[test]
fn postpublication_replacement_or_modified_bytes_are_uncertain_and_preserved() {
    for replace in [false, true] {
        let fixture = Fixture::new();
        let output = fixture.output.clone();
        let original = fixture.directory.join("published-original");
        let original_for_hook = original.clone();
        let failure = publish_inner(
            &fixture.output,
            BYTES,
            Faults {
                checkpoint: Some(Box::new(move |point| {
                    if point == Checkpoint::Published {
                        if replace {
                            fs::rename(&output, &original_for_hook).unwrap();
                            File::create_new(&output)
                                .unwrap()
                                .write_all(FOREIGN)
                                .unwrap();
                        } else {
                            fs::write(&output, FOREIGN).unwrap();
                        }
                    }
                    Ok(())
                })),
                ..Faults::default()
            },
        )
        .unwrap_err();
        let mut failure = assert_published_failure(failure);
        assert!(failure.publication.as_mut().unwrap().retry_cleanup());
        assert_eq!(fs::read(&fixture.output).unwrap(), FOREIGN);
        if replace {
            assert_eq!(fs::read(original).unwrap(), BYTES);
        }
    }
}

#[test]
fn parent_sync_failure_is_a_published_uncertain_outcome_with_no_cleanup_authority() {
    let fixture = Fixture::new();
    let failure = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            parent_sync: Some(Err(interrupted(FsOperation::SyncDirectory))),
            ..Faults::default()
        },
    )
    .unwrap_err();
    let failure = assert_published_failure(failure);
    assert_eq!(
        failure.result()["publication"]["parent_directory_synced"],
        "unknown"
    );
    assert_eq!(fs::read(&fixture.output).unwrap(), BYTES);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);
}

#[test]
fn unsupported_parent_sync_reports_limited_durability_without_claiming_a_sync() {
    let fixture = Fixture::new();
    let publication = publish_inner(
        &fixture.output,
        BYTES,
        Faults {
            parent_sync: Some(Err(FsError::Unsupported {
                operation: FsOperation::SyncDirectory,
                capability: "injected unsupported parent directory sync",
            })),
            ..Faults::default()
        },
    )
    .unwrap();
    assert_eq!(publication.result()["status"], "published");
    assert_eq!(
        publication.result()["parent_directory_synced"],
        "unsupported"
    );
    assert_eq!(fs::read(&fixture.output).unwrap(), BYTES);
}

#[test]
fn failed_result_write_or_flush_keeps_the_original_cleanup_owner_pending() {
    struct FailedStdout {
        fail_write: bool,
        bytes: Vec<u8>,
    }

    impl Write for FailedStdout {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    for fail_write in [false, true] {
        let fixture = Fixture::new();
        let failure = fixture.blocked_cleanup();
        let identity = failure.source.cleanup().unwrap().identity();
        let mut outcome = Err::<Value, _>(CliFailure::publication(failure));
        let mut stdout = FailedStdout {
            fail_write,
            bytes: Vec::new(),
        };
        let error = write_outcome(&outcome, &mut stdout).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        if !fail_write {
            let text = std::str::from_utf8(&stdout.bytes).unwrap();
            assert!(text.ends_with('\n'));
            assert_eq!(text.lines().count(), 1);
            let result: Value =
                serde_json::from_str(text.strip_prefix(RESULT_PREFIX).unwrap().trim_end()).unwrap();
            assert_eq!(result["cleanup_pending"], true);
            assert_eq!(result["publication"]["status"], "not_published");
        }
        let failure = outcome.as_mut().unwrap_err();
        assert_eq!(failure.result()["cleanup_pending"], true);
        assert!(failure.runtime.is_none());
        let pending = failure.publication.as_mut().unwrap();
        assert_eq!(pending.source.cleanup().unwrap().identity(), identity);
        let temporary = fixture.temporary();
        let parent = open_parent(&fixture.directory).unwrap();
        let file = parent
            .open_child_file(&RelativeComponent::new(temporary.file_name().unwrap()).unwrap())
            .unwrap();
        assert!(
            pending
                .source
                .cleanup()
                .unwrap()
                .same_object(&file)
                .unwrap()
        );
        assert_eq!(fs::read(&temporary).unwrap(), BYTES);
        drop(file);
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&temporary).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&temporary, permissions).unwrap();
            assert!(pending.retry_cleanup());
            assert!(!temporary.exists());
        }
        #[cfg(target_os = "linux")]
        assert!(!pending.retry_cleanup());
        assert!(!fixture.output.exists());
    }
}
