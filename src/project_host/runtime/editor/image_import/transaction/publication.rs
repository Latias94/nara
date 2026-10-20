use super::*;

pub(super) enum MetadataPublication {
    Published {
        source: FilePublicationReadSource,
        binding: FilePublicationBinding,
    },
    Uncertain,
    Failed(EditorImageImportFailure),
}

impl EditorImageImportHost {
    pub(super) fn publish_metadata(
        &mut self,
        runtime: &EditorAuthoringRuntime,
        publication: FilePublicationHost,
        bytes: &[u8],
    ) -> MetadataPublication {
        let Some(publication) = runtime.host_custody().retain_publication(publication) else {
            return MetadataPublication::Failed(EditorImageImportFailure::Runtime(
                EditorImageRuntimeFailure::RuntimeUnavailable,
            ));
        };
        debug_assert!(self.publication_cleanup.is_none());
        // Register the complete Host before mutation. This independent latch also survives
        // unwinding between publication and its normal cleanup/receipt handling.
        self.publication_cleanup = Some(publication.clone());
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
                    let (_, binding) = candidate.into_parts();
                    host.commit_published(binding);
                    MetadataPublication::Published {
                        source: host.read_source(),
                        binding: host.binding(),
                    }
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
        // Pending imports need only fresh-read authority and the accepted exact binding.
        if !cleanup_pending && publication.retire() {
            self.publication_cleanup = None;
        }
        outcome
    }
}
