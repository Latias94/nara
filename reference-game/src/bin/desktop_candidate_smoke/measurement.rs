use std::collections::{BTreeMap, BTreeSet};

use nara::ui_render::advanced::UiPresentationBackendGeneration;

pub(super) const WARMUP_SAMPLE_COUNT: usize = 4;
pub(super) const MEASUREMENT_SAMPLE_COUNT: usize = 16;
const MAX_SUCCESSOR_PRODUCT_FRAMES: usize = WARMUP_SAMPLE_COUNT + MEASUREMENT_SAMPLE_COUNT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CandidateCompletionSample {
    pub(super) device_epoch: u64,
    pub(super) app_frame_index: u64,
    pub(super) render_transaction_frame_index: u64,
    pub(super) duration_ns: u64,
}

impl CandidateCompletionSample {
    pub(super) const fn new(
        device_epoch: u64,
        app_frame_index: u64,
        render_transaction_frame_index: u64,
        duration_ns: u64,
    ) -> Self {
        Self {
            device_epoch,
            app_frame_index,
            render_transaction_frame_index,
            duration_ns,
        }
    }

    pub(super) fn to_json(self) -> String {
        format!(
            concat!(
                "{{\"device_epoch\":{},\"app_frame_index\":{},",
                "\"render_transaction_frame_index\":{},\"duration_ns\":{}}}"
            ),
            self.device_epoch,
            self.app_frame_index,
            self.render_transaction_frame_index,
            self.duration_ns
        )
    }

    const fn frame_pair(self) -> FramePair {
        (self.app_frame_index, self.render_transaction_frame_index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CandidateMeasurementIdentity {
    pub(super) backend_generation: UiPresentationBackendGeneration,
    pub(super) app_frame_index: u64,
    pub(super) render_transaction_frame_index: u64,
}

impl CandidateMeasurementIdentity {
    pub(super) const fn backend_instance_id(self) -> u64 {
        self.backend_generation.backend_instance_id()
    }

    pub(super) const fn device_epoch(self) -> u64 {
        self.backend_generation.device_epoch()
    }

    const fn frame_pair(self) -> FramePair {
        (self.app_frame_index, self.render_transaction_frame_index)
    }
}

type FramePair = (u64, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateMeasurementStage {
    Warmup,
    Measure,
    Seal,
}

#[derive(Debug)]
pub(super) struct CandidateMeasurement {
    stage: CandidateMeasurementStage,
    identity: CandidateMeasurementIdentity,
    warmup_samples: usize,
    completion_samples: Vec<CandidateCompletionSample>,
    presented_completion: Option<CandidateCompletionSample>,
    admitted_product_frames: BTreeSet<FramePair>,
    pending_completions: BTreeMap<FramePair, CandidateCompletionSample>,
    seen_samples: BTreeSet<FramePair>,
}

impl CandidateMeasurement {
    /// Arms at the first exact product presentation.
    ///
    /// Every completion observed before the readiness boundary is deliberately discarded. The
    /// exact baseline completion may arrive on a later non-blocking poll; warmup starts only from
    /// later owner-admitted product presentations.
    pub(super) fn arm(identity: CandidateMeasurementIdentity) -> Self {
        Self {
            stage: CandidateMeasurementStage::Warmup,
            identity,
            warmup_samples: 0,
            completion_samples: Vec::with_capacity(MEASUREMENT_SAMPLE_COUNT),
            presented_completion: None,
            admitted_product_frames: BTreeSet::from([identity.frame_pair()]),
            pending_completions: BTreeMap::new(),
            seen_samples: BTreeSet::new(),
        }
    }

    pub(super) const fn identity(&self) -> CandidateMeasurementIdentity {
        self.identity
    }

    pub(super) fn observe_arm_boundary(
        &mut self,
        samples: impl IntoIterator<Item = CandidateCompletionSample>,
    ) -> Result<(), &'static str> {
        for sample in samples {
            if sample.render_transaction_frame_index == self.identity.render_transaction_frame_index
            {
                self.observe(sample)?;
            }
        }
        Ok(())
    }

    pub(super) fn admit_product_frame(
        &mut self,
        product: CandidateMeasurementIdentity,
    ) -> Result<(), &'static str> {
        if product.render_transaction_frame_index < self.identity.render_transaction_frame_index {
            return Ok(());
        }
        self.validate_product_identity(product)?;
        let pair = product.frame_pair();
        if pair == self.identity.frame_pair()
            || self.stage == CandidateMeasurementStage::Seal
            || self.seen_samples.contains(&pair)
            || self.admitted_product_frames.contains(&pair)
        {
            return Ok(());
        }
        if self.successor_product_frame_count() == MAX_SUCCESSOR_PRODUCT_FRAMES {
            return Ok(());
        }
        self.admitted_product_frames.insert(pair);
        if let Some(sample) = self.pending_completions.remove(&pair) {
            self.accept_admitted_sample(sample)?;
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        sample: CandidateCompletionSample,
    ) -> Result<(), &'static str> {
        if sample.render_transaction_frame_index < self.identity.render_transaction_frame_index {
            return Ok(());
        }
        self.validate_completion_identity(sample)?;
        if sample.duration_ns == 0 {
            return Err("desktop candidate received a zero-duration GPU completion");
        }
        let pair = sample.frame_pair();
        if self.seen_samples.contains(&pair) || self.pending_completions.contains_key(&pair) {
            return Err("desktop candidate received a duplicate GPU completion identity");
        }
        if self.admitted_product_frames.contains(&pair) {
            self.accept_admitted_sample(sample)
        } else {
            self.pending_completions.insert(pair, sample);
            self.trim_pending_completions();
            Ok(())
        }
    }

    pub(super) fn seal_input(
        &self,
    ) -> Option<(CandidateCompletionSample, &[CandidateCompletionSample])> {
        (self.stage == CandidateMeasurementStage::Seal).then(|| {
            (
                self.presented_completion
                    .expect("a sealed candidate measurement must retain the exact completion"),
                self.completion_samples.as_slice(),
            )
        })
    }

    fn accept_admitted_sample(
        &mut self,
        sample: CandidateCompletionSample,
    ) -> Result<(), &'static str> {
        let pair = sample.frame_pair();
        if !self.admitted_product_frames.remove(&pair) {
            return Err("desktop candidate completed a product frame without an admission");
        }
        self.seen_samples.insert(pair);
        if pair == self.identity.frame_pair() {
            self.presented_completion = Some(sample);
            self.try_seal();
            return Ok(());
        }

        match self.stage {
            CandidateMeasurementStage::Warmup => {
                self.warmup_samples += 1;
                if self.warmup_samples == WARMUP_SAMPLE_COUNT {
                    self.stage = CandidateMeasurementStage::Measure;
                }
            }
            CandidateMeasurementStage::Measure => {
                if self.completion_samples.len() < MEASUREMENT_SAMPLE_COUNT {
                    self.completion_samples.push(sample);
                }
            }
            CandidateMeasurementStage::Seal => {}
        }
        self.try_seal();
        Ok(())
    }

    fn trim_pending_completions(&mut self) {
        let retained = MAX_SUCCESSOR_PRODUCT_FRAMES + 1;
        while self.pending_completions.len() > retained {
            self.pending_completions.pop_first();
        }
    }

    fn successor_product_frame_count(&self) -> usize {
        self.admitted_product_frames
            .len()
            .saturating_add(self.seen_samples.len())
            .saturating_sub(1)
    }

    fn validate_product_identity(
        &self,
        product: CandidateMeasurementIdentity,
    ) -> Result<(), &'static str> {
        if product.backend_generation != self.identity.backend_generation {
            return Err(
                "desktop candidate admitted a product frame from a foreign WGPU generation",
            );
        }
        if product.render_transaction_frame_index == self.identity.render_transaction_frame_index {
            if product.app_frame_index != self.identity.app_frame_index {
                return Err("desktop candidate product frame carried the wrong App frame");
            }
        } else if product.app_frame_index <= self.identity.app_frame_index {
            return Err("desktop candidate product frame did not advance its App frame");
        }
        Ok(())
    }

    fn validate_completion_identity(
        &self,
        sample: CandidateCompletionSample,
    ) -> Result<(), &'static str> {
        if sample.device_epoch != self.identity.device_epoch() {
            return Err(
                "desktop candidate retained a completion from a foreign WGPU device generation",
            );
        }
        if sample.render_transaction_frame_index == self.identity.render_transaction_frame_index {
            if sample.app_frame_index != self.identity.app_frame_index {
                return Err(
                    "desktop candidate exact Render completion carried the wrong App frame",
                );
            }
        } else if sample.app_frame_index <= self.identity.app_frame_index {
            return Err(
                "desktop candidate successor Render completion did not advance its App frame",
            );
        }
        Ok(())
    }

    fn try_seal(&mut self) {
        if self.stage == CandidateMeasurementStage::Measure
            && self.completion_samples.len() == MEASUREMENT_SAMPLE_COUNT
            && self.presented_completion.is_some()
        {
            self.stage = CandidateMeasurementStage::Seal;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity() -> CandidateMeasurementIdentity {
        CandidateMeasurementIdentity {
            backend_generation: UiPresentationBackendGeneration::new(7, 3).unwrap(),
            app_frame_index: 11,
            render_transaction_frame_index: 10,
        }
    }

    #[test]
    fn arm_discards_every_pre_ready_completion() {
        let measurement = CandidateMeasurement::arm(test_identity());

        assert_eq!(measurement.stage, CandidateMeasurementStage::Warmup);
        assert_eq!(measurement.warmup_samples, 0);
        assert!(measurement.completion_samples.is_empty());
        assert_eq!(measurement.presented_completion, None);
    }

    #[test]
    fn a_completion_that_precedes_its_owner_admission_is_correlated_later() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        measurement.observe(sample(12, 11, 110)).unwrap();

        assert_eq!(measurement.warmup_samples, 0);
        measurement.admit_product_frame(product(12, 11)).unwrap();

        assert_eq!(measurement.warmup_samples, 1);
        assert!(measurement.pending_completions.is_empty());
    }

    #[test]
    fn arm_boundary_retains_only_the_exact_presented_completion() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        measurement
            .observe_arm_boundary([sample(10, 9, 90), sample(11, 10, 100), sample(12, 11, 110)])
            .unwrap();

        assert_eq!(measurement.presented_completion, Some(sample(11, 10, 100)));
        assert_eq!(measurement.warmup_samples, 0);
        assert!(measurement.pending_completions.is_empty());
    }

    #[test]
    fn unadmitted_successor_completions_never_advance_the_window() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        for offset in 1..=32 {
            measurement
                .observe(sample(11 + offset, 10 + offset, 100 + offset))
                .unwrap();
        }

        assert_eq!(measurement.stage, CandidateMeasurementStage::Warmup);
        assert_eq!(measurement.warmup_samples, 0);
        assert!(measurement.completion_samples.is_empty());
        assert!(measurement.seal_input().is_none());
        assert!(measurement.pending_completions.len() <= MAX_SUCCESSOR_PRODUCT_FRAMES + 1);
    }

    #[test]
    fn zero_through_warmup_count_remain_pending() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        measurement.observe(sample(11, 10, 100)).unwrap();
        assert!(measurement.seal_input().is_none());

        for offset in 1..=WARMUP_SAMPLE_COUNT as u64 {
            let product = product(11 + offset, 10 + offset);
            measurement.admit_product_frame(product).unwrap();
            measurement
                .observe(sample(
                    product.app_frame_index,
                    product.render_transaction_frame_index,
                    100 + offset,
                ))
                .unwrap();
            assert!(measurement.seal_input().is_none());
        }

        assert_eq!(measurement.stage, CandidateMeasurementStage::Measure);
        assert_eq!(measurement.warmup_samples, WARMUP_SAMPLE_COUNT);
        assert!(measurement.completion_samples.is_empty());
    }

    #[test]
    fn exact_app_and_render_identity_seals_after_bounded_product_measurement() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        measurement.observe(sample(11, 10, 100)).unwrap();
        for offset in 1..=(WARMUP_SAMPLE_COUNT + MEASUREMENT_SAMPLE_COUNT) as u64 {
            let product = product(11 + offset, 10 + offset);
            measurement.admit_product_frame(product).unwrap();
            measurement
                .observe(sample(
                    product.app_frame_index,
                    product.render_transaction_frame_index,
                    100 + offset,
                ))
                .unwrap();
        }

        let (exact, measured) = measurement
            .seal_input()
            .expect("the exact product completion and bounded window must seal");
        assert_eq!(exact, sample(11, 10, 100));
        assert_eq!(measured.len(), MEASUREMENT_SAMPLE_COUNT);
        assert_eq!(measured[0].app_frame_index, 16);
        assert_eq!(measured[0].render_transaction_frame_index, 15);
    }

    #[test]
    fn product_measurement_without_the_exact_completion_does_not_seal() {
        let mut measurement = CandidateMeasurement::arm(test_identity());
        for offset in 1..=(WARMUP_SAMPLE_COUNT + MEASUREMENT_SAMPLE_COUNT) as u64 {
            let product = product(11 + offset, 10 + offset);
            measurement.admit_product_frame(product).unwrap();
            measurement
                .observe(sample(
                    product.app_frame_index,
                    product.render_transaction_frame_index,
                    100 + offset,
                ))
                .unwrap();
        }

        assert_eq!(measurement.stage, CandidateMeasurementStage::Measure);
        assert!(measurement.seal_input().is_none());
    }

    #[test]
    fn foreign_device_mismatched_exact_and_zero_duration_fail_closed() {
        let mut foreign = CandidateMeasurement::arm(test_identity());
        assert!(
            foreign
                .observe(CandidateCompletionSample::new(4, 11, 10, 100))
                .is_err()
        );

        let mut mismatched = CandidateMeasurement::arm(test_identity());
        assert!(mismatched.observe(sample(12, 10, 100)).is_err());

        let mut zero_duration = CandidateMeasurement::arm(test_identity());
        assert!(zero_duration.observe(sample(11, 10, 0)).is_err());
    }

    #[test]
    fn duplicate_completion_and_foreign_successors_fail_closed() {
        let mut duplicate = CandidateMeasurement::arm(test_identity());
        duplicate.observe(sample(11, 10, 100)).unwrap();
        assert!(duplicate.observe(sample(11, 10, 100)).is_err());

        let mut foreign_product = CandidateMeasurement::arm(test_identity());
        let mut foreign = product(12, 11);
        foreign.backend_generation = UiPresentationBackendGeneration::new(8, 3).unwrap();
        assert!(foreign_product.admit_product_frame(foreign).is_err());

        let mut foreign_completion = CandidateMeasurement::arm(test_identity());
        foreign_completion
            .admit_product_frame(product(12, 11))
            .unwrap();
        assert!(
            foreign_completion
                .observe(CandidateCompletionSample::new(4, 12, 11, 100))
                .is_err()
        );

        let mut stagnant_app = CandidateMeasurement::arm(test_identity());
        assert!(stagnant_app.admit_product_frame(product(11, 11)).is_err());
    }

    #[test]
    fn completions_and_product_frames_older_than_readiness_are_discarded() {
        let mut measurement = CandidateMeasurement::arm(test_identity());

        measurement.admit_product_frame(product(10, 9)).unwrap();
        measurement.observe(sample(10, 9, 90)).unwrap();

        assert_eq!(measurement.stage, CandidateMeasurementStage::Warmup);
        assert_eq!(measurement.successor_product_frame_count(), 0);
        assert!(measurement.seen_samples.is_empty());
    }

    fn product(
        app_frame_index: u64,
        render_transaction_frame_index: u64,
    ) -> CandidateMeasurementIdentity {
        CandidateMeasurementIdentity {
            backend_generation: test_identity().backend_generation,
            app_frame_index,
            render_transaction_frame_index,
        }
    }

    fn sample(
        app_frame_index: u64,
        render_transaction_frame_index: u64,
        duration_ns: u64,
    ) -> CandidateCompletionSample {
        CandidateCompletionSample::new(
            test_identity().device_epoch(),
            app_frame_index,
            render_transaction_frame_index,
            duration_ns,
        )
    }
}
