use std::{
    fmt::{self, Debug, Formatter},
    sync::{Arc, Mutex, MutexGuard},
};

use nara_core::ByteLimit;

use crate::limits::{
    ImageImportBudgetError, ImageImportLimitKind, ImageImportLimits, ImageImportLimitsError,
    ImageImportMemoryPlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ImageImportCharge {
    encoded_bytes: usize,
    decoder_work_bytes: usize,
    rgba_bytes: usize,
    publication_overlap_bytes: usize,
    total_bytes: usize,
}

impl ImageImportCharge {
    #[cfg(test)]
    pub(crate) const fn encoded(bytes: usize) -> Self {
        Self {
            encoded_bytes: bytes,
            decoder_work_bytes: 0,
            rgba_bytes: 0,
            publication_overlap_bytes: 0,
            total_bytes: bytes,
        }
    }

    pub(crate) fn admission(
        encoded_bytes: usize,
        publication_overlap_bytes: usize,
        aggregate_limit: ByteLimit,
    ) -> Result<Self, ImageImportBudgetError> {
        let total_bytes = encoded_bytes
            .checked_add(publication_overlap_bytes)
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::AggregateInFlightBytes,
                    None,
                    aggregate_limit.get() as u64,
                )
            })?;
        Ok(Self {
            encoded_bytes,
            decoder_work_bytes: 0,
            rgba_bytes: 0,
            publication_overlap_bytes,
            total_bytes,
        })
    }

    pub(crate) const fn peak(plan: ImageImportMemoryPlan) -> Self {
        Self {
            encoded_bytes: plan.encoded_allocation_bytes(),
            decoder_work_bytes: plan.decoder_work_bytes(),
            rgba_bytes: plan.rgba_bytes(),
            publication_overlap_bytes: plan.publication_overlap_bytes(),
            total_bytes: plan.peak_bytes(),
        }
    }

    pub(crate) const fn publication(plan: ImageImportMemoryPlan) -> Self {
        Self {
            encoded_bytes: 0,
            decoder_work_bytes: 0,
            rgba_bytes: plan.rgba_bytes(),
            publication_overlap_bytes: plan.publication_overlap_bytes(),
            total_bytes: plan.publication_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageImportBudgetSnapshot {
    active_reservations: u64,
    active_bytes: usize,
    high_water_bytes: usize,
    active_encoded_bytes: usize,
    high_water_encoded_bytes: usize,
    active_decoder_work_bytes: usize,
    high_water_decoder_work_bytes: usize,
    active_rgba_bytes: usize,
    high_water_rgba_bytes: usize,
    active_publication_overlap_bytes: usize,
    high_water_publication_overlap_bytes: usize,
    total_reserved_bytes: u64,
    total_released_bytes: u64,
    accepted_reservations: u64,
    rejected_reservations: u64,
    released_reservations: u64,
}

impl ImageImportBudgetSnapshot {
    #[must_use]
    pub const fn active_reservations(self) -> u64 {
        self.active_reservations
    }

    #[must_use]
    pub const fn active_bytes(self) -> usize {
        self.active_bytes
    }

    #[must_use]
    pub const fn high_water_bytes(self) -> usize {
        self.high_water_bytes
    }

    #[must_use]
    pub const fn active_encoded_bytes(self) -> usize {
        self.active_encoded_bytes
    }

    #[must_use]
    pub const fn high_water_encoded_bytes(self) -> usize {
        self.high_water_encoded_bytes
    }

    #[must_use]
    pub const fn active_decoder_work_bytes(self) -> usize {
        self.active_decoder_work_bytes
    }

    #[must_use]
    pub const fn high_water_decoder_work_bytes(self) -> usize {
        self.high_water_decoder_work_bytes
    }

    #[must_use]
    pub const fn active_rgba_bytes(self) -> usize {
        self.active_rgba_bytes
    }

    #[must_use]
    pub const fn high_water_rgba_bytes(self) -> usize {
        self.high_water_rgba_bytes
    }

    #[must_use]
    pub const fn active_publication_overlap_bytes(self) -> usize {
        self.active_publication_overlap_bytes
    }

    #[must_use]
    pub const fn high_water_publication_overlap_bytes(self) -> usize {
        self.high_water_publication_overlap_bytes
    }

    #[must_use]
    pub const fn total_reserved_bytes(self) -> u64 {
        self.total_reserved_bytes
    }

    #[must_use]
    pub const fn total_released_bytes(self) -> u64 {
        self.total_released_bytes
    }

    #[must_use]
    pub const fn accepted_reservations(self) -> u64 {
        self.accepted_reservations
    }

    #[must_use]
    pub const fn rejected_reservations(self) -> u64 {
        self.rejected_reservations
    }

    #[must_use]
    pub const fn released_reservations(self) -> u64 {
        self.released_reservations
    }
}

#[derive(Clone)]
pub struct ImageImportBudgetHost {
    limit: ByteLimit,
    publication_overlap_limit: ByteLimit,
    budget: ImageImportBudget,
}

impl ImageImportBudgetHost {
    pub fn new(limits: ImageImportLimits) -> Result<Self, ImageImportLimitsError> {
        let limits = limits.validate()?;
        let limit = limits.max_in_flight_bytes();
        Ok(Self {
            limit,
            publication_overlap_limit: limits.max_rgba_bytes(),
            budget: ImageImportBudget::new(limit),
        })
    }

    #[must_use]
    pub const fn limit(&self) -> ByteLimit {
        self.limit
    }

    #[must_use]
    pub const fn publication_overlap_limit(&self) -> ByteLimit {
        self.publication_overlap_limit
    }

    #[must_use]
    pub fn snapshot(&self) -> ImageImportBudgetSnapshot {
        self.budget.snapshot()
    }

    pub(crate) fn reserve(
        &self,
        charge: ImageImportCharge,
    ) -> Result<ImageImportReservation, ImageImportBudgetError> {
        self.budget.reserve(charge)
    }
}

impl Debug for ImageImportBudgetHost {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageImportBudgetHost")
            .field("limit", &self.limit)
            .field("publication_overlap_limit", &self.publication_overlap_limit)
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct ImageImportBudget {
    inner: Arc<ImageImportBudgetInner>,
}

struct ImageImportBudgetInner {
    limit: usize,
    state: Mutex<ImageImportBudgetSnapshot>,
}

impl ImageImportBudget {
    pub(crate) fn new(limit: ByteLimit) -> Self {
        Self {
            inner: Arc::new(ImageImportBudgetInner {
                limit: limit.get(),
                state: Mutex::new(ImageImportBudgetSnapshot::default()),
            }),
        }
    }

    pub(crate) fn reserve(
        &self,
        charge: ImageImportCharge,
    ) -> Result<ImageImportReservation, ImageImportBudgetError> {
        let mut state = lock_unpoisoned(&self.inner.state);
        let Some(next) = state.active_bytes.checked_add(charge.total_bytes) else {
            state.rejected_reservations = state.rejected_reservations.saturating_add(1);
            return Err(ImageImportBudgetError::aggregate(
                charge.total_bytes as u64,
                state.active_bytes as u64,
                self.inner.limit as u64,
            ));
        };
        if next > self.inner.limit {
            state.rejected_reservations = state.rejected_reservations.saturating_add(1);
            return Err(ImageImportBudgetError::aggregate(
                charge.total_bytes as u64,
                state.active_bytes as u64,
                self.inner.limit as u64,
            ));
        }
        state.active_bytes = next;
        state.high_water_bytes = state.high_water_bytes.max(next);
        add_charge(&mut state, charge);
        state.active_reservations = state.active_reservations.saturating_add(1);
        state.accepted_reservations = state.accepted_reservations.saturating_add(1);
        state.total_reserved_bytes = state
            .total_reserved_bytes
            .saturating_add(charge.total_bytes as u64);
        drop(state);

        Ok(ImageImportReservation {
            budget: self.clone(),
            charge,
        })
    }

    #[must_use]
    pub(crate) fn snapshot(&self) -> ImageImportBudgetSnapshot {
        *lock_unpoisoned(&self.inner.state)
    }
}

impl Debug for ImageImportBudget {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageImportBudget")
            .field("limit", &self.inner.limit)
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

pub(crate) struct ImageImportReservation {
    budget: ImageImportBudget,
    charge: ImageImportCharge,
}

impl ImageImportReservation {
    pub(crate) fn resize(
        &mut self,
        charge: ImageImportCharge,
    ) -> Result<(), ImageImportBudgetError> {
        if charge == self.charge {
            return Ok(());
        }

        let mut state = lock_unpoisoned(&self.budget.inner.state);
        if charge.total_bytes > self.charge.total_bytes {
            let delta = charge.total_bytes - self.charge.total_bytes;
            let Some(next) = state.active_bytes.checked_add(delta) else {
                state.rejected_reservations = state.rejected_reservations.saturating_add(1);
                return Err(ImageImportBudgetError::aggregate(
                    charge.total_bytes as u64,
                    state.active_bytes as u64,
                    self.budget.inner.limit as u64,
                ));
            };
            if next > self.budget.inner.limit {
                state.rejected_reservations = state.rejected_reservations.saturating_add(1);
                return Err(ImageImportBudgetError::aggregate(
                    charge.total_bytes as u64,
                    state.active_bytes as u64,
                    self.budget.inner.limit as u64,
                ));
            }
            state.active_bytes = next;
            state.high_water_bytes = state.high_water_bytes.max(next);
            state.total_reserved_bytes = state.total_reserved_bytes.saturating_add(delta as u64);
        } else {
            let delta = self.charge.total_bytes - charge.total_bytes;
            state.active_bytes = state.active_bytes.saturating_sub(delta);
            state.total_released_bytes = state.total_released_bytes.saturating_add(delta as u64);
        }
        remove_charge(&mut state, self.charge);
        add_charge(&mut state, charge);
        self.charge = charge;
        Ok(())
    }
}

impl Debug for ImageImportReservation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageImportReservation")
            .field("charge", &self.charge)
            .finish_non_exhaustive()
    }
}

impl Drop for ImageImportReservation {
    fn drop(&mut self) {
        let mut state = lock_unpoisoned(&self.budget.inner.state);
        state.active_bytes = state.active_bytes.saturating_sub(self.charge.total_bytes);
        remove_charge(&mut state, self.charge);
        state.active_reservations = state.active_reservations.saturating_sub(1);
        state.total_released_bytes = state
            .total_released_bytes
            .saturating_add(self.charge.total_bytes as u64);
        state.released_reservations = state.released_reservations.saturating_add(1);
    }
}

fn add_charge(state: &mut ImageImportBudgetSnapshot, charge: ImageImportCharge) {
    state.active_encoded_bytes = state
        .active_encoded_bytes
        .saturating_add(charge.encoded_bytes);
    state.high_water_encoded_bytes = state
        .high_water_encoded_bytes
        .max(state.active_encoded_bytes);
    state.active_decoder_work_bytes = state
        .active_decoder_work_bytes
        .saturating_add(charge.decoder_work_bytes);
    state.high_water_decoder_work_bytes = state
        .high_water_decoder_work_bytes
        .max(state.active_decoder_work_bytes);
    state.active_rgba_bytes = state.active_rgba_bytes.saturating_add(charge.rgba_bytes);
    state.high_water_rgba_bytes = state.high_water_rgba_bytes.max(state.active_rgba_bytes);
    state.active_publication_overlap_bytes = state
        .active_publication_overlap_bytes
        .saturating_add(charge.publication_overlap_bytes);
    state.high_water_publication_overlap_bytes = state
        .high_water_publication_overlap_bytes
        .max(state.active_publication_overlap_bytes);
}

fn remove_charge(state: &mut ImageImportBudgetSnapshot, charge: ImageImportCharge) {
    state.active_encoded_bytes = state
        .active_encoded_bytes
        .saturating_sub(charge.encoded_bytes);
    state.active_decoder_work_bytes = state
        .active_decoder_work_bytes
        .saturating_sub(charge.decoder_work_bytes);
    state.active_rgba_bytes = state.active_rgba_bytes.saturating_sub(charge.rgba_bytes);
    state.active_publication_overlap_bytes = state
        .active_publication_overlap_bytes
        .saturating_sub(charge.publication_overlap_bytes);
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        num::{NonZeroU32, NonZeroU64},
        sync::Barrier,
        thread,
    };

    #[test]
    fn reservation_resize_is_atomic_and_release_is_exact() {
        let budget = ImageImportBudget::new(ByteLimit::new(10).unwrap());
        let mut reservation = budget.reserve(ImageImportCharge::encoded(4)).unwrap();
        reservation.resize(ImageImportCharge::encoded(9)).unwrap();

        let rejected = reservation
            .resize(ImageImportCharge::encoded(11))
            .unwrap_err();
        assert_eq!(
            rejected.kind(),
            ImageImportLimitKind::AggregateInFlightBytes
        );
        assert_eq!(budget.snapshot().active_bytes(), 9);

        reservation.resize(ImageImportCharge::encoded(3)).unwrap();
        drop(reservation);
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.active_bytes(), 0);
        assert_eq!(snapshot.active_reservations(), 0);
        assert_eq!(
            snapshot.total_reserved_bytes(),
            snapshot.total_released_bytes()
        );
        assert_eq!(snapshot.high_water_bytes(), 9);
        assert_eq!(snapshot.rejected_reservations(), 1);
        assert_eq!(snapshot.released_reservations(), 1);
    }

    #[test]
    fn memory_plan_checks_each_semantic_limit_before_peak_math() {
        let limits = ImageImportLimits::default()
            .with_max_encoded_bytes(ByteLimit::new(8).unwrap())
            .with_max_width(NonZeroU32::new(2).unwrap())
            .with_max_height(NonZeroU32::new(2).unwrap())
            .with_max_pixels(NonZeroU64::new(4).unwrap())
            .with_max_rgba_bytes(ByteLimit::new(16).unwrap());
        let plan = ImageImportMemoryPlan::for_png(limits, 8, 8, 2, 2, 16).unwrap();
        assert_eq!(plan.rgba_bytes(), 16);
        assert_eq!(plan.publication_overlap_bytes(), 16);

        assert_budget_error(
            ImageImportMemoryPlan::for_png(limits, 9, 9, 2, 2, 0).unwrap_err(),
            ImageImportLimitKind::EncodedBytes,
            Some(9),
        );
        assert_budget_error(
            ImageImportMemoryPlan::for_png(limits, 8, 8, 3, 2, 0).unwrap_err(),
            ImageImportLimitKind::Width,
            Some(3),
        );
        assert_budget_error(
            ImageImportMemoryPlan::for_png(limits, 8, 8, 2, 3, 0).unwrap_err(),
            ImageImportLimitKind::Height,
            Some(3),
        );

        let pixel_limits = limits
            .with_max_width(NonZeroU32::new(3).unwrap())
            .with_max_height(NonZeroU32::new(2).unwrap());
        assert_budget_error(
            ImageImportMemoryPlan::for_png(pixel_limits, 8, 8, 3, 2, 0).unwrap_err(),
            ImageImportLimitKind::Pixels,
            Some(6),
        );

        let rgba_limits = pixel_limits
            .with_max_pixels(NonZeroU64::new(6).unwrap())
            .with_max_rgba_bytes(ByteLimit::new(23).unwrap());
        assert_budget_error(
            ImageImportMemoryPlan::for_png(rgba_limits, 8, 8, 3, 2, 0).unwrap_err(),
            ImageImportLimitKind::RgbaBytes,
            Some(24),
        );

        let work_limits = limits
            .with_max_decoder_work_bytes(ByteLimit::new(plan.decoder_work_bytes() - 1).unwrap());
        assert_budget_error(
            ImageImportMemoryPlan::for_png(work_limits, 8, 8, 2, 2, 0).unwrap_err(),
            ImageImportLimitKind::DecoderWorkBytes,
            Some(plan.decoder_work_bytes() as u64),
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn memory_plan_rejects_checked_arithmetic_overflow() {
        let limits = ImageImportLimits::default()
            .with_max_width(NonZeroU32::MAX)
            .with_max_height(NonZeroU32::MAX)
            .with_max_pixels(NonZeroU64::MAX)
            .with_max_rgba_bytes(ByteLimit::new(usize::MAX).unwrap())
            .with_max_decoder_work_bytes(ByteLimit::new(usize::MAX).unwrap())
            .with_max_in_flight_bytes(ByteLimit::new(usize::MAX).unwrap());

        assert_budget_error(
            ImageImportMemoryPlan::for_png(limits, 1, 1, u32::MAX, u32::MAX, 0).unwrap_err(),
            ImageImportLimitKind::RgbaBytes,
            None,
        );
        assert_budget_error(
            ImageImportMemoryPlan::for_png(limits, 1, 1, 1, 1, usize::MAX).unwrap_err(),
            ImageImportLimitKind::AggregateInFlightBytes,
            None,
        );
    }

    #[test]
    fn category_charges_follow_peak_and_publication_lifetimes() {
        let plan = ImageImportMemoryPlan::for_png(ImageImportLimits::default(), 128, 128, 2, 2, 16)
            .unwrap();
        let budget = ImageImportBudget::new(ByteLimit::new(plan.peak_bytes()).unwrap());
        let mut reservation = budget.reserve(ImageImportCharge::encoded(128)).unwrap();
        reservation.resize(ImageImportCharge::peak(plan)).unwrap();

        let peak = budget.snapshot();
        assert_eq!(peak.active_bytes(), plan.peak_bytes());
        assert_eq!(peak.active_encoded_bytes(), plan.encoded_bytes());
        assert_eq!(peak.active_decoder_work_bytes(), plan.decoder_work_bytes());
        assert_eq!(peak.active_rgba_bytes(), plan.rgba_bytes());
        assert_eq!(
            peak.active_publication_overlap_bytes(),
            plan.publication_overlap_bytes()
        );

        reservation
            .resize(ImageImportCharge::publication(plan))
            .unwrap();
        let publication = budget.snapshot();
        assert_eq!(publication.active_encoded_bytes(), 0);
        assert_eq!(publication.active_decoder_work_bytes(), 0);
        assert_eq!(publication.active_rgba_bytes(), plan.rgba_bytes());
        assert_eq!(
            publication.active_publication_overlap_bytes(),
            plan.publication_overlap_bytes()
        );

        drop(reservation);
        assert_budget_fully_released(budget.snapshot());
    }

    #[test]
    fn concurrent_reservations_admit_exact_aggregate_and_reject_limit_plus_one() {
        let budget = ImageImportBudget::new(ByteLimit::new(10).unwrap());
        let base = budget.reserve(ImageImportCharge::encoded(4)).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let exact_budget = budget.clone();
        let exact_barrier = Arc::clone(&barrier);
        let exact = thread::spawn(move || {
            exact_barrier.wait();
            exact_budget.reserve(ImageImportCharge::encoded(6))
        });
        let overflow_budget = budget.clone();
        let overflow_barrier = Arc::clone(&barrier);
        let overflow = thread::spawn(move || {
            overflow_barrier.wait();
            overflow_budget.reserve(ImageImportCharge::encoded(7))
        });
        barrier.wait();

        let exact = exact.join().unwrap().unwrap();
        let overflow = overflow.join().unwrap().unwrap_err();
        assert_eq!(
            overflow.kind(),
            ImageImportLimitKind::AggregateInFlightBytes
        );
        assert_eq!(budget.snapshot().active_bytes(), 10);
        assert_eq!(budget.snapshot().high_water_bytes(), 10);
        assert_eq!(budget.snapshot().high_water_encoded_bytes(), 10);

        drop(exact);
        drop(base);
        assert_budget_fully_released(budget.snapshot());
    }

    #[test]
    fn concurrent_peak_upgrades_are_atomic_across_complete_category_charges() {
        let plan =
            ImageImportMemoryPlan::for_png(ImageImportLimits::default(), 1, 1, 1, 1, 0).unwrap();
        let limit = ByteLimit::new(plan.peak_bytes().checked_add(1).unwrap()).unwrap();
        let budget = ImageImportBudget::new(limit);
        let start = Arc::new(Barrier::new(2));
        let hold = Arc::new(Barrier::new(2));
        let workers = (0..2)
            .map(|_| {
                let budget = budget.clone();
                let start = Arc::clone(&start);
                let hold = Arc::clone(&hold);
                thread::spawn(move || {
                    let mut reservation = budget
                        .reserve(ImageImportCharge::admission(1, 0, limit).unwrap())
                        .unwrap();
                    start.wait();
                    let upgraded = reservation.resize(ImageImportCharge::peak(plan));
                    hold.wait();
                    upgraded.is_ok()
                })
            })
            .collect::<Vec<_>>();

        let successes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|succeeded| *succeeded)
            .count();

        assert_eq!(successes, 1);
        let snapshot = budget.snapshot();
        assert!(snapshot.high_water_bytes() <= limit.get());
        assert!(snapshot.high_water_decoder_work_bytes() <= limit.get());
        assert!(snapshot.high_water_rgba_bytes() <= limit.get());
        assert_budget_fully_released(snapshot);
    }

    fn assert_budget_error(
        error: ImageImportBudgetError,
        kind: ImageImportLimitKind,
        observed: Option<u64>,
    ) {
        assert_eq!(error.kind(), kind);
        assert_eq!(error.observed(), observed);
    }

    fn assert_budget_fully_released(snapshot: ImageImportBudgetSnapshot) {
        assert_eq!(snapshot.active_bytes(), 0);
        assert_eq!(snapshot.active_reservations(), 0);
        assert_eq!(snapshot.active_encoded_bytes(), 0);
        assert_eq!(snapshot.active_decoder_work_bytes(), 0);
        assert_eq!(snapshot.active_rgba_bytes(), 0);
        assert_eq!(snapshot.active_publication_overlap_bytes(), 0);
        assert_eq!(
            snapshot.total_reserved_bytes(),
            snapshot.total_released_bytes()
        );
        assert_eq!(
            snapshot.accepted_reservations(),
            snapshot.released_reservations()
        );
    }
}
