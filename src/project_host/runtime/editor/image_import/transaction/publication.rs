use super::*;

pub(super) enum MetadataPublication {
    Published(RetainedHost<FilePublicationHost>),
    Uncertain,
    Failed(EditorImageImportFailure),
}

impl EditorImageImportHost {
    pub(super) fn retain_metadata(
        &mut self,
        runtime: &EditorAuthoringRuntime,
        publication: FilePublicationHost,
    ) -> Result<RetainedHost<FilePublicationHost>, EditorImageImportFailure> {
        let publication = runtime
            .host_custody()
            .retain_publication(publication)
            .ok_or(EditorImageImportFailure::Runtime(
                EditorImageRuntimeFailure::RuntimeUnavailable,
            ))?;
        debug_assert!(self.publication_cleanup.is_none());
        // The latch survives every early return and unwind until pending takes ownership.
        self.publication_cleanup = Some(publication.clone());
        Ok(publication)
    }

    pub(super) fn retire_metadata(&mut self, publication: RetainedHost<FilePublicationHost>) {
        if !publication.retire() {
            debug_assert!(self.publication_cleanup.is_none());
            self.publication_cleanup = Some(publication);
        }
    }

    pub(super) fn publish_metadata(
        &mut self,
        runtime: &EditorAuthoringRuntime,
        publication: FilePublicationHost,
        bytes: &[u8],
    ) -> MetadataPublication {
        let publication = match self.retain_metadata(runtime, publication) {
            Ok(publication) => publication,
            Err(failure) => return MetadataPublication::Failed(failure),
        };
        // Register the complete Host before mutation. This independent latch also survives
        // unwinding between publication and its normal cleanup/receipt handling.
        let prepared = publication.try_with(|host| {
            #[cfg(test)]
            if let Some(control) = self.fail_next_metadata_after_write.take() {
                host.test_fail_after_write_with_control(&control);
            }
            let outcome = host.publish(bytes);
            #[cfg(test)]
            assert!(
                !std::mem::take(&mut self.panic_after_metadata_publication),
                "injected panic after metadata publication"
            );
            if host.has_cleanup() {
                return (
                    MetadataPublication::Failed(EditorImageImportFailure::CleanupPending),
                    true,
                );
            }
            let outcome = match outcome {
                FilePublicationOutcome::Published(candidate) => {
                    #[cfg(test)]
                    if std::mem::take(&mut self.reject_next_valid_metadata_receipt) {
                        let _ = candidate.into_parts();
                        host.mark_uncertain();
                        return (MetadataPublication::Uncertain, false);
                    }
                    let (_, source) = candidate.into_parts();
                    host.commit_published(source);
                    MetadataPublication::Published(publication.clone())
                }
                FilePublicationOutcome::Rejected(reason) => {
                    MetadataPublication::Failed(map_publication_rejection(reason))
                }
                FilePublicationOutcome::Failed(stage) => {
                    MetadataPublication::Failed(map_publication_failure(stage))
                }
                FilePublicationOutcome::Uncertain(_) => MetadataPublication::Uncertain,
            };
            (outcome, false)
        });
        let Some((outcome, cleanup_pending)) = prepared else {
            // A closing runtime may already have retired the registration. Never fall
            // back to an unregistered writer when that authority is unavailable.
            if publication.retire() {
                self.publication_cleanup = None;
            }
            return MetadataPublication::Failed(EditorImageImportFailure::Busy);
        };
        // Custody/retirement calls must not run while borrowing the publication Host.
        // Successful publication keeps its accepted leaf continuously alive through pending.
        if !matches!(&outcome, MetadataPublication::Published(_))
            && !cleanup_pending
            && publication.retire()
        {
            self.publication_cleanup = None;
        }
        outcome
    }
}
