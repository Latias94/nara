use std::sync::{Mutex, TryLockError};

use nara_fs::DirectoryCapability;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
};

use super::super::persistence::RetainedHost;
use super::*;
use crate::project_host::persistence::FilePublicationOutcome;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

#[derive(Default)]
struct SourceImportResult {
    publication_started: bool,
    outcome: Option<EditorSourceAssetImportAttempt>,
}

pub(super) struct PendingSourceImport {
    // Logical cancellation cannot discard the worker's actual publication result.
    task: TaskHandle<()>,
    result: Arc<Mutex<SourceImportResult>>,
    publication: RetainedHost<FilePublicationHost>,
    outcome: Option<EditorSourceAssetImportAttempt>,
}

pub(super) struct SourceImportWork {
    pub(super) provider: Arc<dyn SourceImporter>,
    pub(super) assets: DirectoryCapability,
    pub(super) metadata: AssetMetaIdentityCandidate,
    pub(super) encoded: String,
    pub(super) expected_source: FileIdentity,
    pub(super) budget: ProjectContentBudgetHost,
    #[cfg(test)]
    pub(super) gate: Option<SourceImportTestGate>,
}

impl PendingSourceImport {
    pub(super) fn start(
        pools: &TaskPools,
        publication: RetainedHost<FilePublicationHost>,
        work: SourceImportWork,
    ) -> Result<Self, Rejection> {
        let SourceImportWork {
            provider,
            assets,
            metadata,
            encoded,
            expected_source,
            budget,
            #[cfg(test)]
            mut gate,
        } = work;
        let stable_id = metadata.identity().stable_id();
        let writer = publication.clone();
        let result = Arc::new(Mutex::new(SourceImportResult::default()));
        let worker_result = Arc::clone(&result);
        let task = pools.spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(0, TaskDomainKey::new(0x736f_7572_696d_7074)),
            move |context| {
                #[cfg(test)]
                wait_at(&mut gate, SourceImportTestPhase::BeforeWork);
                let outcome = (|| {
                    if context.is_cancelled() {
                        return cancelled();
                    }
                    let validation =
                        match provider.validate(&assets, &metadata, expected_source, &budget) {
                            Ok(validation) => validation,
                            Err(error) => {
                                return EditorSourceAssetImportAttempt::Failed {
                                    diagnostics: error.diagnostics().clone(),
                                };
                            }
                        };
                    if context.is_cancelled() {
                        return cancelled();
                    }
                    if let Err(error) = validation.revalidate(&assets) {
                        return EditorSourceAssetImportAttempt::Failed {
                            diagnostics: error.diagnostics().clone(),
                        };
                    }
                    if context.is_cancelled() {
                        return cancelled();
                    }
                    writer
                        .with_worker(|host| {
                            worker_result
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .publication_started = true;
                            match host.publish(encoded.as_bytes()) {
                                FilePublicationOutcome::Published(receipt) => {
                                    let (_, source) = receipt.into_parts();
                                    host.commit_published(source);
                                    EditorSourceAssetImportAttempt::MetadataPublished { stable_id }
                                }
                                FilePublicationOutcome::Uncertain(_) => {
                                    EditorSourceAssetImportAttempt::ReconcileRequired
                                }
                                FilePublicationOutcome::Rejected(_)
                                | FilePublicationOutcome::Failed(_) => failed(
                                    "project.editor.source-import-publication-failed",
                                    "The asset metadata could not be published",
                                ),
                            }
                        })
                        .unwrap_or_else(cancelled)
                })();
                worker_result
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .outcome = Some(outcome);
                #[cfg(test)]
                wait_at(&mut gate, SourceImportTestPhase::AfterWork);
            },
        );
        match task.into_handle() {
            Ok(task) => Ok(Self {
                task,
                result,
                publication,
                outcome: None,
            }),
            Err(_) => {
                publication.retire();
                Err(Rejection::Busy)
            }
        }
    }

    pub(super) fn poll(&mut self) -> Option<EditorSourceAssetImportAttempt> {
        // A terminal cancellation is not physical return. Failed executor cleanup also
        // cannot certify quiescence; retain this owner for the runtime's failure handling.
        if self.task.return_observer().state() != TaskReturnState::Returned {
            return None;
        }
        if self.outcome.is_none() {
            let mut result = match self.result.try_lock() {
                Ok(result) => result,
                Err(TryLockError::WouldBlock) => return None,
                Err(TryLockError::Poisoned(error)) => error.into_inner(),
            };
            self.outcome = Some(result.outcome.take().unwrap_or_else(|| {
                if result.publication_started {
                    EditorSourceAssetImportAttempt::ReconcileRequired
                } else {
                    failed(
                        "project.editor.source-import-interrupted",
                        "The source metadata import did not complete",
                    )
                }
            }));
            self.task.try_take();
        }
        if !self.publication.retire() {
            return None;
        }
        self.outcome.take()
    }

    pub(super) fn cancel(&self) {
        self.task.cancel();
    }
}

fn cancelled() -> EditorSourceAssetImportAttempt {
    failed(
        "project.editor.source-import-cancelled",
        "The source metadata import was cancelled",
    )
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceImportTestPhase {
    BeforeWork,
    AfterWork,
}

#[cfg(test)]
pub(super) struct SourceImportTestGate {
    phase: SourceImportTestPhase,
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
fn wait_at(gate: &mut Option<SourceImportTestGate>, phase: SourceImportTestPhase) {
    if gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        let gate = gate
            .take()
            .expect("the selected source import gate is retained");
        let _ = gate.entered.send(());
        gate.release
            .recv_timeout(std::time::Duration::from_secs(15))
            .expect("the source import test must release its worker");
    }
}

impl Drop for PendingSourceImport {
    fn drop(&mut self) {
        self.task.cancel();
        self.task.try_take();
        self.publication.retire();
    }
}
