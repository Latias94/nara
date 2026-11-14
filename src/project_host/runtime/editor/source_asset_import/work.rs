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
    _metadata_retention: crate::project_content::ProjectContentLease,
    outcome: Option<EditorSourceAssetImportAttempt>,
    #[cfg(feature = "editor-work-observation")]
    observation: Option<tracing::Span>,
}

pub(super) struct SourceImportWork {
    pub(super) provider: Arc<dyn SourceImporter>,
    pub(super) assets: DirectoryCapability,
    pub(super) metadata: AssetMetaIdentityCandidate,
    pub(super) previous: Option<AssetMetaIdentityCandidate>,
    pub(super) encoded: String,
    pub(super) expected_source: FileIdentity,
    pub(super) budget: ProjectContentBudgetHost,
    #[cfg(test)]
    pub(super) gate: Option<SourceImportTestGate>,
}

struct RetainedSourceImportWork {
    work: SourceImportWork,
    // Fields drop in declaration order, including when a queued closure is discarded.
    _metadata_retention: crate::project_content::ProjectContentLease,
}

impl PendingSourceImport {
    pub(super) fn start(
        pools: &TaskPools,
        publication: RetainedHost<FilePublicationHost>,
        work: SourceImportWork,
    ) -> Result<Self, Rejection> {
        #[cfg(feature = "editor-work-observation")]
        let observation = tracing::info_span!(
            target: "nara::editor::work",
            "editor_source_import",
            outcome = "pending",
            task_id = tracing::field::Empty,
            admission_to_start_ns = tracing::field::Empty,
        );
        let metadata_retention = match work.budget.retain_editor_source_import_metadata(
            &work.metadata,
            work.previous.as_ref(),
            work.encoded.capacity(),
        ) {
            Ok(retention) => retention,
            Err(_) => {
                publication.retire();
                #[cfg(feature = "editor-work-observation")]
                observation.record("outcome", "busy");
                return Err(Rejection::Busy);
            }
        };
        // The pending owner and queued closure share one charge. Dropping either owner
        // cannot release retained inputs before actual executor disposal.
        let stable_id = work.metadata.identity().stable_id();
        let retained_work = RetainedSourceImportWork {
            work,
            _metadata_retention: metadata_retention.clone(),
        };
        let writer = publication.clone();
        let result = Arc::new(Mutex::new(SourceImportResult::default()));
        let worker_result = Arc::clone(&result);
        #[cfg(feature = "editor-work-observation")]
        let worker_dispatch = tracing::dispatcher::get_default(Clone::clone);
        #[cfg(feature = "editor-work-observation")]
        let worker_observation = observation.clone();
        let task = pools.spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(0, TaskDomainKey::new(0x736f_7572_696d_7074)),
            move |context| {
                #[cfg(feature = "editor-work-observation")]
                let _dispatch = tracing::dispatcher::set_default(&worker_dispatch);
                #[cfg(feature = "editor-work-observation")]
                let _observation = worker_observation.enter();
                let mut retained_work = retained_work;
                let SourceImportWork {
                    provider,
                    assets,
                    metadata,
                    previous,
                    encoded,
                    expected_source,
                    budget,
                    #[cfg(test)]
                    gate,
                } = &mut retained_work.work;
                #[cfg(test)]
                wait_at(gate, SourceImportTestPhase::BeforeWork);
                let outcome = (|| {
                    if context.is_cancelled() {
                        return cancelled();
                    }
                    let validation = {
                        #[cfg(feature = "editor-work-observation")]
                        let observation = tracing::info_span!(
                            target: "nara::editor::work",
                            parent: &worker_observation,
                            "editor_source_import_validate",
                            outcome = "pending",
                        );
                        #[cfg(feature = "editor-work-observation")]
                        let _observation = observation.enter();
                        let validation = provider.validate(
                            assets,
                            metadata,
                            *expected_source,
                            budget,
                            previous.as_ref(),
                        );
                        #[cfg(feature = "editor-work-observation")]
                        observation.record(
                            "outcome",
                            if validation.is_ok() {
                                "complete"
                            } else {
                                "failed"
                            },
                        );
                        validation
                    };
                    let validation = match validation {
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
                    let revalidation = {
                        #[cfg(feature = "editor-work-observation")]
                        let observation = tracing::info_span!(
                            target: "nara::editor::work",
                            parent: &worker_observation,
                            "editor_source_import_revalidate",
                            outcome = "pending",
                        );
                        #[cfg(feature = "editor-work-observation")]
                        let _observation = observation.enter();
                        let revalidation = validation.revalidate(assets);
                        #[cfg(feature = "editor-work-observation")]
                        observation.record(
                            "outcome",
                            if revalidation.is_ok() {
                                "complete"
                            } else {
                                "failed"
                            },
                        );
                        revalidation
                    };
                    if let Err(error) = revalidation {
                        return EditorSourceAssetImportAttempt::Failed {
                            diagnostics: error.diagnostics().clone(),
                        };
                    }
                    if context.is_cancelled() {
                        return cancelled();
                    }
                    writer
                        .with_worker(|host| {
                            #[cfg(feature = "editor-work-observation")]
                            let observation = tracing::info_span!(
                                target: "nara::editor::work",
                                parent: &worker_observation,
                                "editor_source_import_publish",
                                outcome = "pending",
                            );
                            #[cfg(feature = "editor-work-observation")]
                            let _observation = observation.enter();
                            worker_result
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .publication_started = true;
                            let publication = host.publish(encoded.as_bytes());
                            #[cfg(feature = "editor-work-observation")]
                            observation.record(
                                "outcome",
                                match &publication {
                                    FilePublicationOutcome::Published(_) => "published",
                                    FilePublicationOutcome::Uncertain(_) => "reconcile",
                                    FilePublicationOutcome::Rejected(_) => "rejected",
                                    FilePublicationOutcome::Failed(_) => "failed",
                                },
                            );
                            match publication {
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
                wait_at(gate, SourceImportTestPhase::AfterWork);
            },
        );
        match task.into_handle() {
            Ok(task) => {
                #[cfg(feature = "editor-work-observation")]
                {
                    observation.record("task_id", task.id().raw());
                    observation.record("outcome", "admitted");
                }
                Ok(Self {
                    task,
                    result,
                    publication,
                    _metadata_retention: metadata_retention,
                    outcome: None,
                    #[cfg(feature = "editor-work-observation")]
                    observation: Some(observation),
                })
            }
            Err(_) => {
                publication.retire();
                #[cfg(feature = "editor-work-observation")]
                observation.record("outcome", "busy");
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
        #[cfg(feature = "editor-work-observation")]
        if let Some(observation) = self.observation.take() {
            observation.record(
                "outcome",
                match self.outcome.as_ref() {
                    Some(EditorSourceAssetImportAttempt::MetadataPublished { .. }) => "published",
                    Some(EditorSourceAssetImportAttempt::Failed { .. }) => "failed",
                    Some(EditorSourceAssetImportAttempt::ReconcileRequired) => "reconcile",
                    Some(EditorSourceAssetImportAttempt::Idle)
                    | Some(EditorSourceAssetImportAttempt::Preparing)
                    | None => "incomplete",
                },
            );
            if let Some(admission_to_start) = self.task.admission_to_start() {
                observation.record(
                    "admission_to_start_ns",
                    u64::try_from(admission_to_start.as_nanos()).unwrap_or(u64::MAX),
                );
            }
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
        #[cfg(feature = "editor-work-observation")]
        if let Some(observation) = &self.observation {
            observation.record("outcome", "abandoned");
        }
        self.task.cancel();
        self.task.try_take();
        self.publication.retire();
    }
}
