#[cfg(feature = "hud-runtime")]
use crate::hud::{UiBackendSubmissionIdentity, UiPresentationBackendGeneration};
use nara_ecs::Resource;

#[cfg(feature = "hud-runtime")]
use crate::gpu::backend::WgpuSubmittedUiFacts;
use crate::gpu::{
    backend::{WgpuBackendState, WgpuFrameTransactionStats},
    presented::WgpuPresentedFrameReceipt,
};

/// CPU-owned, read-only evidence for one concrete backend frame transaction.
///
/// Cloning this projection retains no GPU objects, backend access or presentation authority.
/// The default value has no issued namespace or presentation. Transaction counters alone never
/// produce a presentation receipt or a submitted UI fact.
#[derive(Resource, Debug, Default, Clone)]
pub struct WgpuRenderObservation {
    instance_id: u64,
    device_epoch: u64,
    state: WgpuBackendState,
    transaction: WgpuFrameTransactionStats,
    presented: Option<WgpuPresentedFrameReceipt>,
    presented_surface_extent: Option<crate::Extent2d>,
    #[cfg(feature = "hud-runtime")]
    facts: Vec<WgpuSubmittedUiFacts>,
}

impl WgpuRenderObservation {
    /// Projects one owner-issued snapshot without growing its admitted UI batch.
    ///
    /// Inconsistent or previous-transaction evidence is discarded; the original counters remain
    /// exact. UI evidence is bounded by this transaction's already admitted packet count rather
    /// than accumulated across transactions.
    pub(crate) fn from_parts(
        instance_id: u64,
        device_epoch: u64,
        state: WgpuBackendState,
        transaction: WgpuFrameTransactionStats,
        presented: Option<WgpuPresentedFrameReceipt>,
        #[cfg(feature = "hud-runtime")] mut facts: Vec<WgpuSubmittedUiFacts>,
    ) -> Self {
        let current_transaction = if instance_id != 0
            && device_epoch != 0
            && state == WgpuBackendState::Ready
            && transaction.queue_submissions() != 0
        {
            transaction
                .app_frame_index()
                .zip(transaction.render_transaction_frame_index())
        } else {
            None
        };
        let presented = presented.filter(|receipt| {
            transaction.presents() != 0
                && receipt.backend_instance_id() == instance_id
                && receipt.device_epoch() == device_epoch
                && current_transaction
                    == Some((
                        receipt.app_frame_index(),
                        receipt.render_transaction_frame_index(),
                    ))
        });
        #[cfg(feature = "hud-runtime")]
        {
            if current_transaction.is_some()
                && u64::try_from(facts.len())
                    .is_ok_and(|count| count <= transaction.ui_packet_count())
            {
                facts.retain(|fact| {
                    fact.backend_generation().backend_instance_id() == instance_id
                        && fact.backend_generation().device_epoch() == device_epoch
                        && current_transaction
                            == Some((
                                fact.app_frame_index(),
                                fact.render_transaction_frame_index(),
                            ))
                });
            } else {
                facts.clear();
            }
            if facts.is_empty() {
                facts = Vec::new();
            }
        }
        Self {
            instance_id,
            device_epoch,
            state,
            transaction,
            presented,
            presented_surface_extent: None,
            #[cfg(feature = "hud-runtime")]
            facts,
        }
    }

    /// Process-local issued backend identity; zero means no owner has published a snapshot.
    #[must_use]
    pub const fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Issued device epoch within this backend instance; zero means no initialized device.
    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn state(&self) -> WgpuBackendState {
        self.state
    }

    #[must_use]
    pub const fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.transaction
    }

    /// Returns the configured physical extent of this transaction's exact presented surface.
    /// Other targets and transactions without an issued presentation have no such observation.
    #[must_use]
    pub fn configured_surface_extent(
        &self,
        window: nara_window::WindowId,
    ) -> Option<crate::Extent2d> {
        self.presented
            .filter(|receipt| receipt.window_id() == window)?;
        self.presented_surface_extent
    }

    pub(crate) fn record_presented_surface_extent(&mut self, extent: Option<crate::Extent2d>) {
        self.presented_surface_extent = self.presented.and(extent);
    }

    pub(crate) const fn presented_frame_this_transaction(
        &self,
    ) -> Option<WgpuPresentedFrameReceipt> {
        self.presented
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn submitted_ui_facts(
        &self,
        submission: UiBackendSubmissionIdentity,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Option<WgpuSubmittedUiFacts> {
        let mut matches = self.facts.iter().copied().filter(|fact| {
            fact.submission() == submission && fact.backend_generation() == backend_generation
        });
        let fact = matches.next()?;
        matches.next().is_none().then_some(fact)
    }
}

#[cfg(test)]
mod tests {
    use nara_app::{App, RuntimeCandidateRetirementState, RuntimeGeneration, RuntimeInstance};
    use nara_window::WindowId;

    use super::*;
    use crate::gpu::backend::WgpuRenderBackend;

    fn transaction(app_frame: Option<u64>, render_frame: u64) -> WgpuFrameTransactionStats {
        let mut backend = WgpuRenderBackend::default();
        backend.begin_frame_transaction(render_frame, app_frame);
        backend.record_runtime_program_submission();
        backend.commit_runtime_presentation(presented_receipt(), true);
        let stats = backend.frame_transaction_stats();
        #[cfg(feature = "hud-runtime")]
        let stats = {
            let mut stats = stats;
            stats.record_ui_payload(crate::gpu::frame::WgpuUiPayloadFacts {
                packet_count: 2,
                ..Default::default()
            });
            stats
        };
        stats
    }

    fn presented_receipt() -> WgpuPresentedFrameReceipt {
        let runtime = RuntimeInstance::start(App::new()).unwrap();
        let generation: RuntimeGeneration = runtime.generation();
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        WgpuPresentedFrameReceipt::new(generation, WindowId::PRIMARY, 5, 17, 3, 10, 9)
    }

    #[test]
    fn default_observation_has_no_issued_or_presented_facts() {
        let observation = WgpuRenderObservation::default();

        assert_eq!(observation.instance_id(), 0);
        assert_eq!(observation.device_epoch(), 0);
        assert_eq!(observation.state(), WgpuBackendState::Uninitialized);
        assert_eq!(
            observation.frame_transaction_stats(),
            WgpuFrameTransactionStats::default()
        );
        assert_eq!(observation.presented_frame_this_transaction(), None);
        #[cfg(feature = "hud-runtime")]
        assert!(observation.facts.is_empty());
    }

    #[test]
    fn presentation_requires_its_exact_namespace_and_current_transaction() {
        let receipt = presented_receipt();
        let stats = transaction(Some(10), 9);
        let observation = WgpuRenderObservation::from_parts(
            17,
            3,
            WgpuBackendState::Ready,
            stats,
            Some(receipt),
            #[cfg(feature = "hud-runtime")]
            Vec::new(),
        );
        let retained = observation.clone();
        assert_eq!(retained.instance_id(), 17);
        assert_eq!(retained.device_epoch(), 3);
        assert_eq!(retained.state(), WgpuBackendState::Ready);
        assert_eq!(retained.frame_transaction_stats(), stats);
        assert_eq!(retained.presented_frame_this_transaction(), Some(receipt));

        let mut empty_backend = WgpuRenderBackend::default();
        empty_backend.begin_frame_transaction(9, Some(10));
        let empty_stats = empty_backend.frame_transaction_stats();
        let mut image_backend = WgpuRenderBackend::default();
        image_backend.begin_frame_transaction(9, Some(10));
        image_backend.record_editor_image_submission();
        for (instance, epoch, state, stats) in [
            (0, 3, WgpuBackendState::Ready, stats),
            (17, 0, WgpuBackendState::Ready, stats),
            (18, 3, WgpuBackendState::Ready, stats),
            (17, 4, WgpuBackendState::Ready, stats),
            (17, 3, WgpuBackendState::Unavailable, stats),
            (17, 3, WgpuBackendState::Ready, empty_stats),
            (
                17,
                3,
                WgpuBackendState::Ready,
                image_backend.frame_transaction_stats(),
            ),
            (17, 3, WgpuBackendState::Ready, transaction(Some(11), 9)),
            (17, 3, WgpuBackendState::Ready, transaction(Some(10), 10)),
            (17, 3, WgpuBackendState::Ready, transaction(None, 9)),
            (
                17,
                3,
                WgpuBackendState::Ready,
                WgpuFrameTransactionStats::default(),
            ),
        ] {
            let next = WgpuRenderObservation::from_parts(
                instance,
                epoch,
                state,
                stats,
                Some(receipt),
                #[cfg(feature = "hud-runtime")]
                Vec::new(),
            );
            assert_eq!(next.frame_transaction_stats(), stats);
            assert_eq!(next.presented_frame_this_transaction(), None);
            assert_eq!(retained.presented_frame_this_transaction(), Some(receipt));
        }
    }

    #[test]
    fn presentation_counters_cannot_synthesize_a_receipt() {
        let stats = transaction(Some(10), 9);
        assert_eq!(stats.presents(), 1);
        let observation = WgpuRenderObservation::from_parts(
            17,
            3,
            WgpuBackendState::Ready,
            stats,
            None,
            #[cfg(feature = "hud-runtime")]
            Vec::new(),
        );

        assert_eq!(observation.presented_frame_this_transaction(), None);
    }

    #[cfg(feature = "hud-runtime")]
    fn submitted_ui_facts() -> Vec<WgpuSubmittedUiFacts> {
        use std::sync::Arc;

        use crate::hud::{
            UiPreparedImages, UiPresentationAttemptId, UiPresentationCapabilityGeneration,
            UiPresentationEpoch, UiPresentationPlanner, UiPresentationRuntimeGeneration,
            presentation::{UiBackendSubmission, UiPresentationOutcomes},
        };
        use crate::{RenderTarget, ViewportRect};
        use nara_ui::kernel::{
            UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiSurfaceId, UiTheme,
        };

        let frame = UiCompiler::new(
            nara_ui::text::bundled_font_face().unwrap(),
            Default::default(),
        )
        .unwrap()
        .compile(
            &UiAuthoringSnapshotBuilder::new(1).build().unwrap(),
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let packet = UiPresentationPlanner::default()
            .plan(frame, target, &UiPreparedImages::default())
            .unwrap();
        let prepared = [1, 2]
            .into_iter()
            .map(|attempt| {
                crate::hud::presentation::prepare_backend_submission(
                    UiPresentationAttemptId::new(attempt).unwrap(),
                    Arc::clone(&packet),
                    UiPresentationEpoch::new(1).unwrap(),
                    UiPresentationRuntimeGeneration::new(7).unwrap(),
                    UiPresentationCapabilityGeneration::new(11).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let mut outcomes = UiPresentationOutcomes::default();
        let permits = outcomes.reserve_backend(&prepared).unwrap();
        let admitted = prepared
            .into_iter()
            .zip(permits)
            .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
            .collect();
        let submitted = crate::hud::presentation::bind_backend_submission_batch(
            admitted,
            UiPresentationBackendGeneration::new(17, 3).unwrap(),
        )
        .unwrap();
        let lowered = crate::gpu::ui::lower_ui_packets(
            &[packet],
            target,
            Some(WindowId::PRIMARY),
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap();
        WgpuRenderBackend::prepare_submitted_ui_facts(
            &submitted,
            &lowered.packet_facts,
            crate::gpu::backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .unwrap()
    }

    #[cfg(feature = "hud-runtime")]
    #[test]
    fn ui_facts_match_the_exact_attempt_in_the_current_namespace_and_transaction() {
        let facts = submitted_ui_facts();
        let first = facts[0];
        let second = facts[1];
        assert_eq!(first.submission().packet(), second.submission().packet());
        assert_ne!(first.submission(), second.submission());
        let stats = transaction(Some(10), 9);
        let expected = facts.clone();
        let admitted_buffer = facts.as_ptr();
        let observation =
            WgpuRenderObservation::from_parts(17, 3, WgpuBackendState::Ready, stats, None, facts);
        assert_eq!(observation.facts.as_ptr(), admitted_buffer);
        for expected in expected.iter().copied() {
            assert_eq!(
                observation
                    .submitted_ui_facts(expected.submission(), expected.backend_generation()),
                Some(expected)
            );
        }
        assert_eq!(observation.presented_frame_this_transaction(), None);
        let facts = expected;
        for foreign in [
            UiPresentationBackendGeneration::new(18, 3).unwrap(),
            UiPresentationBackendGeneration::new(17, 4).unwrap(),
        ] {
            assert_eq!(
                observation.submitted_ui_facts(first.submission(), foreign),
                None
            );
        }

        let mut empty_backend = WgpuRenderBackend::default();
        empty_backend.begin_frame_transaction(9, Some(10));
        for (instance, epoch, state, stats) in [
            (18, 3, WgpuBackendState::Ready, stats),
            (17, 4, WgpuBackendState::Ready, stats),
            (17, 3, WgpuBackendState::Unavailable, stats),
            (
                17,
                3,
                WgpuBackendState::Ready,
                empty_backend.frame_transaction_stats(),
            ),
            (17, 3, WgpuBackendState::Ready, transaction(Some(11), 9)),
            (17, 3, WgpuBackendState::Ready, transaction(Some(10), 10)),
            (17, 3, WgpuBackendState::Ready, transaction(None, 9)),
            (
                17,
                3,
                WgpuBackendState::Ready,
                WgpuFrameTransactionStats::default(),
            ),
        ] {
            let next = WgpuRenderObservation::from_parts(
                instance,
                epoch,
                state,
                stats,
                None,
                facts.clone(),
            );
            assert!(next.facts.is_empty());
            assert_eq!(next.frame_transaction_stats(), stats);
            assert_eq!(
                observation.submitted_ui_facts(first.submission(), first.backend_generation()),
                Some(first)
            );
        }
    }

    #[cfg(feature = "hud-runtime")]
    #[test]
    fn ui_facts_cannot_exceed_the_admitted_packets_or_resolve_ambiguous_attempts() {
        let first = submitted_ui_facts()[0];
        let observation = WgpuRenderObservation::from_parts(
            17,
            3,
            WgpuBackendState::Ready,
            transaction(Some(10), 9),
            None,
            vec![first; 3],
        );
        assert!(observation.facts.is_empty());

        let ambiguous = WgpuRenderObservation::from_parts(
            17,
            3,
            WgpuBackendState::Ready,
            transaction(Some(10), 9),
            None,
            vec![first; 2],
        );
        assert_eq!(
            ambiguous.submitted_ui_facts(first.submission(), first.backend_generation()),
            None
        );
    }
}
