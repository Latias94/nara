//! Existing Tasks own finite I/O; received results survive logical task cancellation.

use std::sync::{Arc, Mutex, TryLockError};

use nara_fs::{DirectoryCapability, RelativePath};
use nara_tasks::{TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest};
use nara_tooling::EditorSourceToolAttemptId;

use super::{
    ArtifactCustody, Error, PreparedEditorSourceToolArtifacts, record, store, validate_registration,
};
use crate::project_host::{
    file_publication::{
        FilePublicationEvidence, FilePublicationFailureStage, FilePublicationHost,
        FilePublicationOutcome, FilePublicationRejection,
    },
    runtime::editor::persistence::{HostCustody, RetainedHost},
};

pub(super) enum Request {
    Observe,
    Publish {
        bytes: Arc<[u8]>,
        host: Option<RetainedHost<FilePublicationHost>>,
        registration: Option<ArtifactCustody>,
    },
}

pub(super) enum IoResult {
    Observed {
        inventory: record::Inventory,
        host: Option<RetainedHost<FilePublicationHost>>,
    },
    Published {
        host: RetainedHost<FilePublicationHost>,
        // Preserve the actual replace/sync receipts independently of final admission.
        _evidence: FilePublicationEvidence,
        accepted: bool,
    },
    Refused {
        host: RetainedHost<FilePublicationHost>,
        error: Error,
        _outcome: PublicationRefusal,
    },
}

pub(super) enum PublicationRefusal {
    ParentChanged,
    Rejected { _reason: FilePublicationRejection },
    Failed { _stage: FilePublicationFailureStage },
}

struct Work {
    request: Option<Request>,
    started: bool,
    result: Option<Result<IoResult, Error>>,
}

pub(super) struct Completed {
    pub(super) result: Result<IoResult, Error>,
    pub(super) return_state: TaskReturnState,
}

pub(super) struct ReceivedPublication {
    pub(super) _candidate: PreparedEditorSourceToolArtifacts,
    pub(super) _completed: Completed,
}

pub(super) struct Pending {
    work: Arc<Mutex<Work>>,
    pub(super) task: Option<TaskHandle<()>>,
    rejected_return: TaskReturnState,
    pub(super) attempt: EditorSourceToolAttemptId,
    pub(super) candidate: Option<PreparedEditorSourceToolArtifacts>,
    pub(super) reconcile: bool,
}

impl Pending {
    pub(super) fn start(
        root: DirectoryCapability,
        custody: HostCustody,
        pools: &TaskPools,
        request: Request,
        candidate: Option<PreparedEditorSourceToolArtifacts>,
        attempt: EditorSourceToolAttemptId,
        reconcile: bool,
        #[cfg(all(test, windows))] gate: Option<Arc<super::tests::IoGate>>,
    ) -> Self {
        let work = Arc::new(Mutex::new(Work {
            request: Some(request),
            started: false,
            result: None,
        }));
        let retained = Arc::clone(&work);
        let spawn = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, super::super::TOOL_DOMAIN),
                move |context| {
                    let mut work = retained.lock().unwrap_or_else(|error| error.into_inner());
                    if context.is_cancelled() {
                        work.result = Some(Err(Error::TaskRejected));
                        return;
                    }
                    work.started = true;
                    let request = work
                        .request
                        .take()
                        .expect("one artifact task consumes one request");
                    work.result = Some(run_io(&root, &custody, request));
                    #[cfg(all(test, windows))]
                    if let Some(gate) = gate {
                        gate.wait_after_io();
                    }
                },
            )
            .into_handle();
        let (task, rejected_return) = match spawn {
            Ok(task) => (Some(task), TaskReturnState::Returned),
            Err(rejected) => (None, rejected.return_state),
        };
        Self {
            work,
            task,
            rejected_return,
            attempt,
            candidate,
            reconcile,
        }
    }

    pub(super) fn poll(&mut self) -> Option<Completed> {
        let return_state = self
            .task
            .as_ref()
            .map_or(self.rejected_return, |task| task.return_observer().state());
        if return_state == TaskReturnState::Pending {
            return None;
        }
        let mut work = match self.work.try_lock() {
            Ok(work) => work,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        Some(Completed {
            result: work.result.take().unwrap_or(Err(if work.started {
                Error::RequiresReconcile
            } else {
                Error::TaskRejected
            })),
            return_state,
        })
    }
}

fn run_io(
    root: &DirectoryCapability,
    custody: &HostCustody,
    request: Request,
) -> Result<IoResult, Error> {
    match request {
        Request::Observe => {
            let loaded = store::load(root)?;
            let host = loaded
                .host
                .map(|host| custody.retain_publication(host).ok_or(Error::Closing))
                .transpose()?;
            Ok(IoResult::Observed {
                inventory: loaded.inventory,
                host,
            })
        }
        Request::Publish {
            bytes,
            host,
            registration,
        } => {
            // Revalidate the original registration, never substitute another namespace owner.
            if let Some(registration) = registration {
                validate_registration(root, &registration)?;
            }
            let host = match host {
                Some(host) => host,
                None => custody
                    .retain_publication(store::create_missing_host(root)?)
                    .ok_or(Error::Closing)?,
            };
            let parent_matches = root
                .open_directory_exact(&fixed_path(store::PARENT))
                .is_ok_and(|parent| {
                    host.with_worker(|host| host.parent_identity() == parent.identity())
                        .unwrap_or(false)
                });
            if !parent_matches {
                return Ok(IoResult::Refused {
                    host,
                    error: Error::Changed,
                    _outcome: PublicationRefusal::ParentChanged,
                });
            }
            let outcome = host
                .with_worker(|host| match host.publish(&bytes) {
                    FilePublicationOutcome::Published(accepted) => {
                        let (evidence, source) = accepted.into_parts();
                        host.commit_published(source);
                        Ok((evidence, true))
                    }
                    FilePublicationOutcome::Uncertain(evidence) => Ok((evidence, false)),
                    FilePublicationOutcome::Rejected(reason) => Err((
                        match reason {
                            FilePublicationRejection::TargetChanged
                            | FilePublicationRejection::TargetDeleted => Error::Changed,
                            FilePublicationRejection::RequiresReconcile => Error::RequiresReconcile,
                            FilePublicationRejection::CleanupPending => Error::Busy,
                            _ => Error::Write,
                        },
                        PublicationRefusal::Rejected { _reason: reason },
                    )),
                    FilePublicationOutcome::Failed(stage) => {
                        Err((Error::Write, PublicationRefusal::Failed { _stage: stage }))
                    }
                })
                .ok_or(Error::Closing)?;
            Ok(match outcome {
                Ok((evidence, accepted)) => {
                    let admitted = host
                        .with_worker(|host| admits_publication(root, host))
                        .unwrap_or(false);
                    if !admitted {
                        host.with_worker(|host| host.mark_uncertain());
                    }
                    IoResult::Published {
                        host,
                        _evidence: evidence,
                        accepted: accepted && admitted,
                    }
                }
                Err((error, outcome)) => IoResult::Refused {
                    host,
                    error,
                    _outcome: outcome,
                },
            })
        }
    }
}

fn admits_publication(root: &DirectoryCapability, host: &FilePublicationHost) -> bool {
    let Some((identity, Some(parent), digest)) = host.accepted_source_observation() else {
        return false;
    };
    let Ok(current_parent) = root.open_directory_exact(&fixed_path(store::PARENT)) else {
        return false;
    };
    current_parent.identity() == parent
        && current_parent
            .open_file_exact(&fixed_path(store::NAME))
            .is_ok_and(|current| {
                current.identity() == identity
                    && current.verify_digest(digest, store::BYTE_LIMIT).is_ok()
            })
}

fn fixed_path(value: &str) -> RelativePath {
    RelativePath::new(value).expect("the engine-owned artifact inventory path is valid")
}
