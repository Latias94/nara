use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::{Arc, Mutex},
};

use nara_core::{ByteLimit, ItemLimit};
use nara_image::ImageImportLimits;
use nara_scene::{PrefabExpansionLimits, SceneFileLimits};

macro_rules! define_project_content_budget_kinds {
    (
        tracked {
            $(
                $variant:ident => {
                    label: $label:literal,
                    aggregate_bytes: $aggregate_bytes:literal
                }
            ),+ $(,)?
        }
        aggregate $aggregate:ident => { label: $aggregate_label:literal }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u8)]
        pub enum ProjectContentBudgetKind {
            $($variant,)+
            $aggregate,
        }

        impl ProjectContentBudgetKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+ Self::$aggregate];
            const TRACKED: &'static [Self] = &[$(Self::$variant,)+];

            const fn index(self) -> Option<usize> {
                match self {
                    $(Self::$variant => Some(Self::$variant as usize),)+
                    Self::$aggregate => None,
                }
            }

            const fn contributes_to_aggregate_bytes(self) -> bool {
                match self {
                    $(Self::$variant => $aggregate_bytes,)+
                    Self::$aggregate => false,
                }
            }

            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                    Self::$aggregate => $aggregate_label,
                }
            }
        }
    };
}

define_project_content_budget_kinds! {
    tracked {
        DirectoryDepth => { label: "directory-depth", aggregate_bytes: false },
        DirectoryEntries => { label: "directory-entries", aggregate_bytes: false },
        PathBytes => { label: "path-bytes", aggregate_bytes: true },
        OpenHandles => { label: "open-handles", aggregate_bytes: false },
        Files => { label: "files", aggregate_bytes: false },
        QueuedJobs => { label: "queued-jobs", aggregate_bytes: false },
        InFlightJobs => { label: "in-flight-jobs", aggregate_bytes: false },
        DependencyEdges => { label: "dependency-edges", aggregate_bytes: false },
        EncodedBytes => { label: "encoded-bytes", aggregate_bytes: true },
        WorkBytes => { label: "work-bytes", aggregate_bytes: true },
        ArtifactBytes => { label: "artifact-bytes", aggregate_bytes: true },
        RetainedBytes => { label: "retained-bytes", aggregate_bytes: true },
    }
    aggregate AggregateBytes => { label: "aggregate-bytes" }
}

const TRACKED_KIND_COUNT: usize = ProjectContentBudgetKind::TRACKED.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectContentLimits {
    values: [usize; TRACKED_KIND_COUNT],
    aggregate_bytes: usize,
    scene_file: SceneFileLimits,
    prefab_expansion: PrefabExpansionLimits,
    image_import: ImageImportLimits,
}

impl Default for ProjectContentLimits {
    fn default() -> Self {
        let item = |value| ItemLimit::new(value).expect("project content item limit is non-zero");
        let bytes = |value| ByteLimit::new(value).expect("project content byte limit is non-zero");
        Self {
            values: [
                item(64).get(),
                item(65_536).get(),
                bytes(4 * 1024 * 1024).get(),
                item(16).get(),
                item(16_384).get(),
                item(4_096).get(),
                item(8).get(),
                item(65_536).get(),
                bytes(512 * 1024 * 1024).get(),
                bytes(512 * 1024 * 1024).get(),
                bytes(512 * 1024 * 1024).get(),
                bytes(512 * 1024 * 1024).get(),
            ],
            aggregate_bytes: bytes(1024 * 1024 * 1024).get(),
            scene_file: SceneFileLimits::default(),
            prefab_expansion: PrefabExpansionLimits::default(),
            image_import: ImageImportLimits::default(),
        }
    }
}

impl ProjectContentLimits {
    #[must_use]
    pub const fn limit(self, kind: ProjectContentBudgetKind) -> usize {
        match kind.index() {
            Some(index) => self.values[index],
            None => self.aggregate_bytes,
        }
    }

    #[must_use]
    pub const fn scene_file(self) -> SceneFileLimits {
        self.scene_file
    }

    #[must_use]
    pub const fn prefab_expansion(self) -> PrefabExpansionLimits {
        self.prefab_expansion
    }

    #[must_use]
    pub const fn image_import(self) -> ImageImportLimits {
        self.image_import
    }

    #[must_use]
    pub const fn with_limit(
        mut self,
        kind: ProjectContentBudgetKind,
        limit: usize,
    ) -> Option<Self> {
        if limit == 0 {
            return None;
        }
        match kind.index() {
            Some(index) => self.values[index] = limit,
            None => self.aggregate_bytes = limit,
        }
        Some(self)
    }

    #[must_use]
    pub const fn with_scene_file(mut self, limits: SceneFileLimits) -> Self {
        self.scene_file = limits;
        self
    }

    #[must_use]
    pub const fn with_prefab_expansion(mut self, limits: PrefabExpansionLimits) -> Self {
        self.prefab_expansion = limits;
        self
    }

    #[must_use]
    pub const fn with_image_import(mut self, limits: ImageImportLimits) -> Self {
        self.image_import = limits;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectContentBudgetError {
    kind: ProjectContentBudgetKind,
    requested: usize,
    active: usize,
    limit: usize,
}

impl ProjectContentBudgetError {
    pub(super) const fn synthetic(
        kind: ProjectContentBudgetKind,
        requested: usize,
        limit: usize,
    ) -> Self {
        Self {
            kind,
            requested,
            active: 0,
            limit,
        }
    }

    #[must_use]
    pub const fn kind(self) -> ProjectContentBudgetKind {
        self.kind
    }

    #[must_use]
    pub const fn requested(self) -> usize {
        self.requested
    }

    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Display for ProjectContentBudgetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "project content {} budget request {} exceeds limit {} with {} already active",
            self.kind.as_str(),
            self.requested,
            self.limit,
            self.active,
        )
    }
}

impl Error for ProjectContentBudgetError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectContentBudgetSnapshot {
    active: [usize; TRACKED_KIND_COUNT],
    high_water: [usize; TRACKED_KIND_COUNT],
    aggregate_active_bytes: usize,
    aggregate_high_water_bytes: usize,
    active_reservations: usize,
}

impl ProjectContentBudgetSnapshot {
    #[must_use]
    pub const fn active(self, kind: ProjectContentBudgetKind) -> usize {
        match kind.index() {
            Some(index) => self.active[index],
            None => self.aggregate_active_bytes,
        }
    }

    #[must_use]
    pub const fn high_water(self, kind: ProjectContentBudgetKind) -> usize {
        match kind.index() {
            Some(index) => self.high_water[index],
            None => self.aggregate_high_water_bytes,
        }
    }

    #[must_use]
    pub const fn active_reservations(self) -> usize {
        self.active_reservations
    }
}

#[derive(Clone)]
pub struct ProjectContentBudgetHost {
    inner: Arc<BudgetHostInner>,
}

struct BudgetHostInner {
    limits: ProjectContentLimits,
    state: Mutex<BudgetState>,
}

#[derive(Default)]
struct BudgetState {
    active: [usize; TRACKED_KIND_COUNT],
    high_water: [usize; TRACKED_KIND_COUNT],
    aggregate_high_water_bytes: usize,
    active_reservations: usize,
}

impl ProjectContentBudgetHost {
    #[must_use]
    pub fn new(limits: ProjectContentLimits) -> Self {
        Self {
            inner: Arc::new(BudgetHostInner {
                limits,
                state: Mutex::new(BudgetState::default()),
            }),
        }
    }

    #[must_use]
    pub fn limits(&self) -> ProjectContentLimits {
        self.inner.limits
    }

    #[must_use]
    pub fn snapshot(&self) -> ProjectContentBudgetSnapshot {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        ProjectContentBudgetSnapshot {
            active: state.active,
            high_water: state.high_water,
            aggregate_active_bytes: aggregate_bytes(&state.active)
                .expect("admitted project content byte charges cannot overflow"),
            aggregate_high_water_bytes: state.aggregate_high_water_bytes,
            active_reservations: state.active_reservations,
        }
    }

    pub(super) fn reserve(&self) -> BudgetTicket {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.active_reservations = state
            .active_reservations
            .checked_add(1)
            .expect("project content reservation count cannot overflow");
        BudgetTicket {
            host: self.clone(),
            charge: [0; TRACKED_KIND_COUNT],
            active: true,
        }
    }

    fn resize(
        &self,
        previous: &[usize; TRACKED_KIND_COUNT],
        next: &[usize; TRACKED_KIND_COUNT],
    ) -> Result<(), ProjectContentBudgetError> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut candidate = state.active;
        let mut other_active = state.active;
        for kind in ProjectContentBudgetKind::TRACKED.iter().copied() {
            let index = kind.index().expect("tracked budget kind has an index");
            other_active[index] = state.active[index]
                .checked_sub(previous[index])
                .expect("project content ticket cannot replace another ticket's charge");
            candidate[index] =
                other_active[index]
                    .checked_add(next[index])
                    .ok_or(ProjectContentBudgetError {
                        kind,
                        requested: next[index],
                        active: other_active[index],
                        limit: self.inner.limits.limit(kind),
                    })?;
            let limit = self.inner.limits.limit(kind);
            if candidate[index] > limit {
                return Err(ProjectContentBudgetError {
                    kind,
                    requested: next[index],
                    active: other_active[index],
                    limit,
                });
            }
        }
        let aggregate_limit = self
            .inner
            .limits
            .limit(ProjectContentBudgetKind::AggregateBytes);
        let active_aggregate = aggregate_bytes(&other_active)
            .expect("admitted project content byte charges cannot overflow");
        let requested_aggregate = aggregate_bytes(next).ok_or(ProjectContentBudgetError {
            kind: ProjectContentBudgetKind::AggregateBytes,
            requested: usize::MAX,
            active: active_aggregate,
            limit: aggregate_limit,
        })?;
        let aggregate =
            requested_aggregate
                .checked_add(active_aggregate)
                .ok_or(ProjectContentBudgetError {
                    kind: ProjectContentBudgetKind::AggregateBytes,
                    requested: requested_aggregate,
                    active: active_aggregate,
                    limit: aggregate_limit,
                })?;
        debug_assert_eq!(aggregate_bytes(&candidate), Some(aggregate));
        if aggregate > aggregate_limit {
            return Err(ProjectContentBudgetError {
                kind: ProjectContentBudgetKind::AggregateBytes,
                requested: requested_aggregate,
                active: active_aggregate,
                limit: aggregate_limit,
            });
        }
        state.active = candidate;
        for (high_water, active) in state.high_water.iter_mut().zip(candidate) {
            *high_water = (*high_water).max(active);
        }
        state.aggregate_high_water_bytes = state.aggregate_high_water_bytes.max(aggregate);
        Ok(())
    }

    fn release(&self, charge: &[usize; TRACKED_KIND_COUNT]) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for (active, amount) in state.active.iter_mut().zip(charge) {
            *active = active
                .checked_sub(*amount)
                .expect("project content reservation released more than it owned");
        }
        state.active_reservations = state
            .active_reservations
            .checked_sub(1)
            .expect("project content reservation release count underflowed");
    }
}

impl std::fmt::Debug for ProjectContentBudgetHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectContentBudgetHost")
            .field("limits", &self.inner.limits)
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

pub(super) struct BudgetTicket {
    host: ProjectContentBudgetHost,
    charge: [usize; TRACKED_KIND_COUNT],
    active: bool,
}

impl BudgetTicket {
    pub(super) fn value(&self, kind: ProjectContentBudgetKind) -> usize {
        kind.index().map_or_else(
            || aggregate_bytes(&self.charge).expect("one project content ticket cannot overflow"),
            |index| self.charge[index],
        )
    }

    pub(super) fn set(
        &mut self,
        kind: ProjectContentBudgetKind,
        value: usize,
    ) -> Result<(), ProjectContentBudgetError> {
        let Some(index) = kind.index() else {
            return Err(ProjectContentBudgetError {
                kind,
                requested: value,
                active: aggregate_bytes(&self.charge).unwrap_or(usize::MAX),
                limit: self.host.limits().limit(kind),
            });
        };
        if self.charge[index] == value {
            return Ok(());
        }
        let mut next = self.charge;
        next[index] = value;
        self.host.resize(&self.charge, &next)?;
        self.charge = next;
        Ok(())
    }

    pub(super) fn set_many(
        &mut self,
        values: &[(ProjectContentBudgetKind, usize)],
    ) -> Result<(), ProjectContentBudgetError> {
        let mut next = self.charge;
        for &(kind, value) in values {
            let Some(index) = kind.index() else {
                return Err(ProjectContentBudgetError {
                    kind,
                    requested: value,
                    active: aggregate_bytes(&self.charge).unwrap_or(usize::MAX),
                    limit: self.host.limits().limit(kind),
                });
            };
            next[index] = value;
        }
        if next == self.charge {
            return Ok(());
        }
        self.host.resize(&self.charge, &next)?;
        self.charge = next;
        Ok(())
    }

    pub(super) fn add(
        &mut self,
        kind: ProjectContentBudgetKind,
        amount: usize,
    ) -> Result<(), ProjectContentBudgetError> {
        let value = self
            .value(kind)
            .checked_add(amount)
            .ok_or(ProjectContentBudgetError {
                kind,
                requested: amount,
                active: self.value(kind),
                limit: self.host.limits().limit(kind),
            })?;
        self.set(kind, value)
    }

    pub(super) fn subtract(&mut self, kind: ProjectContentBudgetKind, amount: usize) {
        let value = self
            .value(kind)
            .checked_sub(amount)
            .expect("project content reservation subtraction exceeded its charge");
        self.set(kind, value)
            .expect("shrinking a project content reservation cannot exceed its limit");
    }

    pub(super) fn into_lease(mut self) -> Result<ProjectContentLease, ProjectContentBudgetError> {
        let mut final_charge = [0; TRACKED_KIND_COUNT];
        for kind in [
            ProjectContentBudgetKind::ArtifactBytes,
            ProjectContentBudgetKind::RetainedBytes,
        ] {
            let index = kind.index().expect("tracked budget kind");
            final_charge[index] = self.charge[index];
        }
        self.host.resize(&self.charge, &final_charge)?;
        self.charge = final_charge;
        self.active = false;
        Ok(ProjectContentLease {
            inner: Arc::new(ProjectContentLeaseInner {
                host: self.host.clone(),
                charge: final_charge,
            }),
        })
    }
}

impl Drop for BudgetTicket {
    fn drop(&mut self) {
        if self.active {
            self.host.release(&self.charge);
        }
    }
}

#[derive(Clone)]
pub(crate) struct ProjectContentLease {
    inner: Arc<ProjectContentLeaseInner>,
}

struct ProjectContentLeaseInner {
    host: ProjectContentBudgetHost,
    charge: [usize; TRACKED_KIND_COUNT],
}

impl ProjectContentLease {
    pub(crate) fn reserve_retained(
        &self,
        retained_bytes: usize,
    ) -> Result<Self, ProjectContentBudgetError> {
        let mut ticket = self.inner.host.reserve();
        ticket.set(ProjectContentBudgetKind::RetainedBytes, retained_bytes)?;
        ticket.into_lease()
    }
}

impl Drop for ProjectContentLeaseInner {
    fn drop(&mut self) {
        self.host.release(&self.charge);
    }
}

impl std::fmt::Debug for ProjectContentLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectContentLease")
            .field("shared_owners", &Arc::strong_count(&self.inner))
            .finish_non_exhaustive()
    }
}

fn aggregate_bytes(charge: &[usize; TRACKED_KIND_COUNT]) -> Option<usize> {
    ProjectContentBudgetKind::TRACKED
        .iter()
        .copied()
        .filter(|kind| kind.contributes_to_aggregate_bytes())
        .map(|kind| charge[kind.index().expect("byte budget kind is tracked")])
        .try_fold(0_usize, usize::checked_add)
}

#[cfg(test)]
mod tests {
    use super::{ProjectContentBudgetHost, ProjectContentBudgetKind, ProjectContentLimits};

    #[test]
    fn all_budget_kinds_follow_the_complete_discriminant_range() {
        assert_eq!(
            ProjectContentBudgetKind::ALL.len(),
            ProjectContentBudgetKind::TRACKED.len() + 1
        );
        assert_eq!(
            ProjectContentBudgetKind::ALL.last(),
            Some(&ProjectContentBudgetKind::AggregateBytes)
        );
        assert_eq!(
            ProjectContentBudgetKind::TRACKED,
            &ProjectContentBudgetKind::ALL[..ProjectContentBudgetKind::TRACKED.len()]
        );
        for (index, kind) in ProjectContentBudgetKind::ALL.iter().copied().enumerate() {
            assert_eq!(kind as usize, index, "{kind:?} is out of order");
        }
    }

    #[test]
    fn in_flight_limit_rejects_the_second_concurrent_ticket() {
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::InFlightJobs, 1)
            .unwrap();
        let host = ProjectContentBudgetHost::new(limits);
        let mut first = host.reserve();
        let mut second = host.reserve();

        first
            .set(ProjectContentBudgetKind::InFlightJobs, 1)
            .unwrap();
        let error = second
            .set(ProjectContentBudgetKind::InFlightJobs, 1)
            .unwrap_err();

        assert_eq!(error.kind(), ProjectContentBudgetKind::InFlightJobs);
        assert_eq!(
            host.snapshot()
                .high_water(ProjectContentBudgetKind::InFlightJobs),
            1
        );
        drop(second);
        drop(first);
        let snapshot = host.snapshot();
        assert_eq!(snapshot.active_reservations(), 0);
        for kind in ProjectContentBudgetKind::ALL.iter().copied() {
            assert_eq!(snapshot.active(kind), 0, "{kind:?} charge leaked");
        }
    }

    #[test]
    fn aggregate_arithmetic_overflow_rejects_instead_of_saturating() {
        let mut limits = ProjectContentLimits::default();
        for kind in ProjectContentBudgetKind::TRACKED
            .iter()
            .copied()
            .filter(|kind| kind.contributes_to_aggregate_bytes())
        {
            limits = limits.with_limit(kind, usize::MAX).unwrap();
        }
        limits = limits
            .with_limit(ProjectContentBudgetKind::AggregateBytes, usize::MAX)
            .unwrap();
        let host = ProjectContentBudgetHost::new(limits);
        let mut ticket = host.reserve();
        ticket
            .set(ProjectContentBudgetKind::PathBytes, usize::MAX)
            .unwrap();

        let error = ticket
            .set(ProjectContentBudgetKind::EncodedBytes, 1)
            .unwrap_err();

        assert_eq!(error.kind(), ProjectContentBudgetKind::AggregateBytes);
        assert_eq!(error.requested(), usize::MAX);
        assert_eq!(error.active(), 0);
        drop(ticket);
        assert_eq!(host.snapshot().active_reservations(), 0);
    }

    #[test]
    fn aggregate_error_separates_the_request_from_other_active_tickets() {
        let mut limits = ProjectContentLimits::default();
        for kind in ProjectContentBudgetKind::TRACKED
            .iter()
            .copied()
            .filter(|kind| kind.contributes_to_aggregate_bytes())
        {
            limits = limits.with_limit(kind, 16).unwrap();
        }
        limits = limits
            .with_limit(ProjectContentBudgetKind::AggregateBytes, 10)
            .unwrap();
        let host = ProjectContentBudgetHost::new(limits);
        let mut first = host.reserve();
        let mut second = host.reserve();
        first.set(ProjectContentBudgetKind::PathBytes, 4).unwrap();

        let error = second
            .set(ProjectContentBudgetKind::EncodedBytes, 7)
            .unwrap_err();

        assert_eq!(error.kind(), ProjectContentBudgetKind::AggregateBytes);
        assert_eq!(error.requested(), 7);
        assert_eq!(error.active(), 4);
        assert_eq!(error.limit(), 10);
        drop(second);
        drop(first);
        assert_eq!(host.snapshot().active_reservations(), 0);
    }
}
