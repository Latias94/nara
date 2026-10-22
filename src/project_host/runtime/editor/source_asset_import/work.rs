use nara_fs::DirectoryCapability;
use nara_tasks::{TaskDomainKey, TaskHandle, TaskPoolKind, TaskSpawnRequest, TaskTerminal};

use super::super::{authoring_runtime::EditorAuthoringRuntime, persistence::RetainedHost};
use super::*;
use crate::project_host::persistence::FilePublicationOutcome;

pub(super) struct PendingSourceImport {
    task: TaskHandle<EditorSourceAssetImportAttempt>,
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
}

impl PendingSourceImport {
    pub(super) fn start(
        runtime: &EditorAuthoringRuntime,
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
        } = work;
        let stable_id = metadata.identity().stable_id();
        let writer = publication.clone();
        let task = runtime.persistence_tasks().spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(0, TaskDomainKey::new(0x736f_7572_696d_7074)),
            move |context| {
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
                    return failed(
                        "project.editor.source-import-cancelled",
                        "The source metadata import was cancelled",
                    );
                }
                if let Err(error) = validation.revalidate(&assets) {
                    return EditorSourceAssetImportAttempt::Failed {
                        diagnostics: error.diagnostics().clone(),
                    };
                }
                if context.is_cancelled() {
                    return failed(
                        "project.editor.source-import-cancelled",
                        "The source metadata import was cancelled",
                    );
                }
                writer
                    .with_worker(|host| match host.publish(encoded.as_bytes()) {
                        FilePublicationOutcome::Published(receipt) => {
                            let (_, binding) = receipt.into_parts();
                            host.commit_published(binding);
                            EditorSourceAssetImportAttempt::MetadataPublished { stable_id }
                        }
                        FilePublicationOutcome::Uncertain(_) => {
                            EditorSourceAssetImportAttempt::ReconcileRequired
                        }
                        FilePublicationOutcome::Rejected(_) | FilePublicationOutcome::Failed(_) => {
                            failed(
                                "project.editor.source-import-publication-failed",
                                "The asset metadata could not be published",
                            )
                        }
                    })
                    .unwrap_or_else(|| {
                        failed(
                            "project.editor.source-import-cancelled",
                            "The metadata writer was retired",
                        )
                    })
            },
        );
        match task.into_handle() {
            Ok(task) => Ok(Self {
                task,
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
        if self.outcome.is_none() {
            self.outcome = Some(match self.task.try_take()? {
                TaskTerminal::Completed(outcome) => outcome,
                TaskTerminal::Cancelled(_) | TaskTerminal::Failed(_) => failed(
                    "project.editor.source-import-interrupted",
                    "The source metadata import did not complete",
                ),
            });
        }
        if !self.publication.retire() {
            return None;
        }
        self.outcome.take()
    }

    pub(super) fn cancel(self) {
        drop(self);
    }
}

impl Drop for PendingSourceImport {
    fn drop(&mut self) {
        self.task.cancel();
        self.task.try_take();
        self.publication.retire();
    }
}
