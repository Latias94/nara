//! Public finite request surface and explicit owner-thread commit.

use nara_ecs::bevy_ecs::system::SystemState;
use nara_scene::SceneOccurrenceKey;

use super::*;

/// Exact request identity. Runtime and process-wide sequence prevent cross-World replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectSceneRequestId {
    pub(super) generation: RuntimeGeneration,
    pub(super) sequence: u64,
}

impl ProjectSceneRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.sequence
    }
}

/// An authored source Scene path or complete Scene Product under the original Host authority.
///
/// Source Hosts resolve a Product through its actual primary Scene sidecar in the preparation
/// worker, then require its resolved path to be startup or an explicitly declared Scene root.
/// Processed Hosts require a Product in their original immutable base and reject source paths
/// with [`ProjectSceneRequestError::UndeclaredScene`]. Product selection uses the full identity,
/// including the subproduct ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSceneSelection {
    Source(ProjectPath),
    Product(AssetProductRef),
}

impl From<ProjectPath> for ProjectSceneSelection {
    fn from(path: ProjectPath) -> Self {
        Self::Source(path)
    }
}

impl From<AssetProductRef> for ProjectSceneSelection {
    fn from(product: AssetProductRef) -> Self {
        Self::Product(product)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectSceneRequestError {
    #[error("project Scene request authority is unavailable")]
    Unavailable,
    #[error("another project Scene request or unacknowledged result occupies the slot")]
    Busy,
    #[error("the Scene is not startup or an explicitly declared delivery root")]
    UndeclaredScene,
    #[error("the Scene is not a Deferred dependency of the current Scene")]
    NotDeferredScene,
    #[error("project Scene preparation is unavailable on the I/O task pool")]
    TaskUnavailable,
    #[error("project Scene request identities are exhausted")]
    IdExhausted,
    #[error("project Scene request inputs no longer match the runtime")]
    Stale,
    #[cfg(feature = "runtime-2d")]
    #[error("project Scene content was rejected: {0:?}")]
    Content(ProjectContentErrorKind),
    /// Processed acquisition failed. The request diagnostics retain its safe Runtime category.
    #[error("Runtime Scene content was rejected")]
    RuntimeContent,
    #[error("project Scene commit was rejected")]
    CommitRejected,
}

impl ProjectSceneRequestError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "project.scene.request-unavailable",
            Self::Busy => "project.scene.request-busy",
            Self::UndeclaredScene => "project.scene.undeclared-scene",
            Self::NotDeferredScene => "project.scene.deferred-scene-unavailable",
            Self::TaskUnavailable => "project.scene.preparation-unavailable",
            Self::IdExhausted => "project.scene.request-id-exhausted",
            Self::Stale => "project.scene.request-stale",
            #[cfg(feature = "runtime-2d")]
            Self::Content(_) => "project.scene.content-rejected",
            Self::RuntimeContent => "project.scene.runtime-content-rejected",
            Self::CommitRejected => "project.scene.commit-rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSceneRequestState {
    Preparing,
    /// Interest was revoked, but worker captures have not physically returned.
    Cancelling,
    Ready,
    /// Native domain retirement is in progress; the old Scene remains retained but gameplay must
    /// remain suspended until the managed attempt reaches reconciliation.
    Retiring,
    /// A native transition could not finish after retirement began. The request retains unpublished
    /// data or its published receipt and native progress, and may retry only unfinished phases.
    Degraded,
    Cancelled,
    Rejected(ProjectSceneRequestError),
    Committed,
}

impl ProjectSceneRequestState {
    pub(super) fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Rejected(_) | Self::Committed)
    }
}

/// Observations of the original Scene owner's coalesced source-update intent.
/// This value contains no preparation, publication or acknowledgement authority.
#[cfg(feature = "runtime-2d")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectSceneSourceUpdateStatus {
    /// The automatic request, retained through its exact terminal acknowledgement.
    pub request: Option<ProjectSceneRequestId>,
    /// A newer notification or full-rescan intent waiting for the original slot to be released.
    pub pending: bool,
    /// The last automatic admission or preparation failure, cleared on start or publication.
    pub error: Option<ProjectSceneRequestError>,
}

/// Read-only target content for preparing ordinary owned game data before commit.
///
/// This view exposes neither a snapshot/source owner nor loading or publication authority.
/// When obtained through [`ProjectScenes::scope`], it cannot leave the inspection scope. Build
/// owned game data from it, return that data, then explicitly commit after the scope returns.
pub struct PreparedProjectSceneView<'a> {
    admission: &'a Admission,
    content: &'a PreparedSceneContent,
}

impl PreparedProjectSceneView<'_> {
    /// Returns the original admitted source-content stamp for this prepared Project Scene.
    ///
    /// Processed content returns `None`. Committing source content preserves this frozen stamp.
    #[cfg(feature = "runtime-2d")]
    #[must_use]
    pub fn source_content_stamp(&self) -> Option<crate::project_content::ProjectSceneContentStamp> {
        match self.content {
            PreparedSceneContent::Source(snapshot) => Some(snapshot.content_stamp()),
            PreparedSceneContent::Processed(_) => None,
        }
    }

    /// Typed CPU data prepared for the target Scene, before its explicit commit.
    #[must_use]
    pub fn domain_assets(&self) -> &crate::project_host::ProjectDomainAssets {
        self.content.domain_assets()
    }

    /// Admitted target document for the selected replacement or additive operation.
    #[must_use]
    pub fn scene(&self) -> &CompiledSceneDocument {
        self.content.document()
    }

    /// Borrows an Immediate Scene data dependency already prepared for this request.
    ///
    /// This performs no loading and grants no travel authority. Deferred targets, unrelated
    /// catalogue members and the selected root return `None`. The borrow cannot outlive this
    /// prepared view or its actual source/processed content owner.
    #[must_use]
    pub fn scene_dependency(&self, product: AssetProductRef) -> Option<&CompiledSceneDocument> {
        self.content.scene_dependency(product)
    }

    /// Borrows the exact selection captured by this request.
    #[must_use]
    pub fn selected_scene(&self) -> &ProjectSceneSelection {
        &self.admission.selection
    }

    /// Returns the captured path relative to its source directory, for source content only.
    /// Travel uses the configured Scenes directory. Deferred targets use their bound directory.
    #[must_use]
    pub fn selected_scene_path(&self) -> Option<&ProjectPath> {
        match self.content {
            #[cfg(feature = "runtime-2d")]
            PreparedSceneContent::Source(snapshot) => Some(snapshot.selected_scene_path()),
            PreparedSceneContent::Processed(_) => None,
        }
    }

    /// Returns the caller's original Product selection in either content mode. A request made by
    /// source path returns `None`, even when its captured source has an admitted Product association.
    #[must_use]
    pub fn selected_scene_product(&self) -> Option<AssetProductRef> {
        match self.selected_scene() {
            ProjectSceneSelection::Source(_) => None,
            ProjectSceneSelection::Product(product) => Some(*product),
        }
    }

    #[must_use]
    pub fn expected_instance(&self) -> &SpawnedSceneInstance {
        &self.admission.expected
    }
}

/// Finite request, cancellation, observation and acknowledgement surface for hosted products.
///
/// A cancelled request keeps its slot until physical return. Ready content never commits itself;
/// inspect it, prepare game data, then call [`commit_prepared_project_scene`] at an exclusive
/// simulation boundary. Request and result identities must match exactly.
///
/// Scheduled systems receive this parameter directly. Exclusive callers use [`ProjectScenes::scope`]
/// with the same operations and request predicates.
#[derive(SystemParam)]
pub struct ProjectScenes<'w> {
    owner: Option<ResMut<'w, RequestOwner>>,
    current: Option<Res<'w, CurrentSceneOwner>>,
    residency: Option<Res<'w, ProjectSceneResidency>>,
    generation: Option<Res<'w, RuntimeGeneration>>,
    pools: Option<Res<'w, TaskPools>>,
    #[cfg(all(feature = "runtime-2d", feature = "asset-watch"))]
    source_watch: Option<ResMut<'w, super::super::source_watch::SourceWatchDriver>>,
}

impl ProjectScenes<'_> {
    /// Observes optional native source coverage without polling or admitting a request.
    #[cfg(all(feature = "runtime-2d", feature = "asset-watch"))]
    #[must_use]
    pub fn source_watch_status(&self) -> Option<super::super::ProjectSceneWatchStatus> {
        self.source_watch.as_deref().map(|driver| driver.status())
    }

    /// Retains read-only original-worker join counts through Runtime close and World retention.
    #[cfg(all(feature = "runtime-2d", feature = "asset-watch"))]
    #[must_use]
    pub fn source_watch_retirement(&self) -> Option<nara_asset_watch::AssetWatchRuntimeObserver> {
        self.source_watch
            .as_deref()
            .map(|driver| driver.retirement())
    }

    /// Requests one retry after native coverage failed. Native I/O occurs in TaskUpdate.
    #[cfg(all(feature = "runtime-2d", feature = "asset-watch"))]
    pub fn retry_source_watch(&mut self) -> bool {
        self.source_watch
            .as_deref_mut()
            .is_some_and(|driver| driver.retry())
    }

    /// Records a canonical source observation for the current hosted source Scene.
    ///
    /// Relevant successful/retry paths queue one successor. During an automatic Preparing/Ready/Managed
    /// request, unknown paths conservatively queue a successor before its new dependency cut is
    /// known. No I/O, asset-state mutation, commit or acknowledgement occurs here. The returned
    /// flag reports queued interest, not exclusive consumption of a shared source event.
    #[cfg(feature = "runtime-2d")]
    pub fn notify_source_change(
        &mut self,
        change: &nara_asset::AssetSourceChange,
    ) -> Result<bool, ProjectSceneRequestError> {
        self.validate_source_update_authority()?;
        let owner = self.owner.as_deref_mut().unwrap();
        let automatic_in_flight = owner
            .source_updates
            .as_ref()
            .unwrap()
            .status()
            .request
            .is_some_and(|request| {
                owner.slot.as_ref().is_some_and(|slot| {
                    slot.id() == request
                        && matches!(
                            slot,
                            Slot::Preparing { .. } | Slot::Ready { .. } | Slot::Managed(_)
                        )
                })
            });
        Ok(owner
            .source_updates
            .as_mut()
            .unwrap()
            .notify(change, automatic_in_flight))
    }

    /// Queues a complete source revalidation after observation overflow or uncertain scope.
    /// The original worker builds its own candidate; the current Scene stays usable until commit.
    #[cfg(feature = "runtime-2d")]
    pub fn request_source_rescan(&mut self) -> Result<(), ProjectSceneRequestError> {
        self.validate_source_update_authority()?;
        self.owner
            .as_deref_mut()
            .unwrap()
            .source_updates
            .as_mut()
            .unwrap()
            .rescan();
        Ok(())
    }

    /// Observes source intent without advancing work or consuming its terminal request.
    /// A stale current instance or generation does not hide a retained automatic request ID.
    #[cfg(feature = "runtime-2d")]
    #[must_use]
    pub fn source_update_status(&self) -> Option<ProjectSceneSourceUpdateStatus> {
        self.owner
            .as_deref()?
            .source_updates
            .as_ref()
            .map(|updates| updates.status())
    }

    #[cfg(feature = "runtime-2d")]
    fn validate_source_update_authority(&self) -> Result<(), ProjectSceneRequestError> {
        let owner = self
            .owner
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        let current = self
            .current
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        if owner.closing.load(Ordering::Acquire)
            || self.generation.is_none()
            || owner.source_updates.is_none()
            || !owner.context.matches_current(current)
        {
            return Err(ProjectSceneRequestError::Unavailable);
        }
        if !owner
            .source_updates
            .as_ref()
            .unwrap()
            .matches_current(**self.generation.as_ref().unwrap(), current.receipt())
        {
            return Err(ProjectSceneRequestError::Stale);
        }
        Ok(())
    }

    #[cfg(feature = "runtime-2d")]
    pub(super) fn start_source_update(&mut self) {
        let Some(owner) = self.owner.as_deref() else {
            return;
        };
        if owner.closing.load(Ordering::Acquire)
            || owner.slot.is_some()
            || self
                .residency
                .as_deref()
                .is_some_and(ProjectSceneResidency::has_addition)
            || owner
                .source_updates
                .as_ref()
                .is_none_or(|updates| !updates.status().pending)
        {
            return;
        }
        let selection = owner
            .source_updates
            .as_ref()
            .unwrap()
            .selected_scene()
            .clone();
        let result = self
            .validate_source_update_authority()
            .and_then(|()| self.request(ProjectSceneSelection::Source(selection)));
        let updates = self
            .owner
            .as_deref_mut()
            .unwrap()
            .source_updates
            .as_mut()
            .unwrap();
        match result {
            Ok(request) => updates.started(request),
            Err(error) => updates.refused(error),
        }
    }

    /// Runs a finite Scene request operation while exclusively borrowing the World.
    ///
    /// The callback receives the same request, inspection, cancellation, retry and acknowledgement
    /// interface as a scheduled system. Missing resources remain optional: [`Self::request`] returns
    /// [`ProjectSceneRequestError::Unavailable`] or [`ProjectSceneRequestError::TaskUnavailable`]
    /// according to the existing admission checks, and observations may return `None`.
    ///
    /// Resources stay installed throughout the callback. Entering or leaving this scope does not
    /// flush the World, poll preparation, commit a Scene, acknowledge a result or release a
    /// cancelling request's slot. Request changes require explicit domain operations; commit
    /// remains a separate exclusive call after the scope returns.
    ///
    /// Construct owned game data while inspecting [`Self::prepared`], then commit outside the scope:
    ///
    /// ```no_run
    /// use nara::{
    ///     advanced_prelude::commit_prepared_project_scene,
    ///     core::ItemLimit,
    ///     ecs::World,
    ///     project_host::{ProjectSceneRequestId, ProjectScenes},
    ///     scene::advanced::SceneProductTransactionLimits,
    /// };
    ///
    /// fn inspect_and_commit(world: &mut World, request: ProjectSceneRequestId) {
    ///     let roots = ProjectScenes::scope(world, |scenes| {
    ///         scenes.prepared(request).map(|view| view.scene().roots().to_vec())
    ///     });
    ///     if let Some(roots) = roots {
    ///         let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE);
    ///         let report = commit_prepared_project_scene(world, request, limits, &[], |_| {});
    ///         // Publish owned game data only after accepting the commit result.
    ///         if !report.diagnostics.has_errors() && report.instance.is_some() {
    ///             let _ = roots;
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// A prepared view cannot escape the callback:
    ///
    /// ```compile_fail
    /// use nara::{
    ///     ecs::World,
    ///     project_host::{PreparedProjectSceneView, ProjectSceneRequestId, ProjectScenes},
    /// };
    ///
    /// fn escape(world: &mut World, request: ProjectSceneRequestId)
    ///     -> Option<PreparedProjectSceneView<'_>>
    /// {
    ///     ProjectScenes::scope(world, |scenes| scenes.prepared(request))
    /// }
    /// ```
    pub fn scope<R>(world: &mut World, operation: impl FnOnce(&mut ProjectScenes<'_>) -> R) -> R {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access
            .get_mut(world)
            .expect("ProjectScenes contains only optional, non-conflicting resources");
        operation(&mut scenes)
    }

    /// Returns the retained participant's progress while a managed travel occupies the slot.
    #[must_use]
    pub fn native_domain_phase(
        &self,
        request: ProjectSceneRequestId,
        domain: domains::ManagedSceneDomainId,
    ) -> Option<domains::ManagedSceneDomainPhase> {
        match self.owner.as_deref()?.slot.as_ref()? {
            Slot::Managed(travel) if travel.admission.id == request => travel
                .attempt
                .domain_phases()
                .find_map(|(id, phase)| (id == domain).then_some(phase)),
            _ => None,
        }
    }

    /// Requests another owner-thread attempt at unfinished native phases.
    /// Completed retirement and reconciliation participants are never replayed.
    pub fn retry(&mut self, request: ProjectSceneRequestId) -> bool {
        let Some(owner) = self.owner.as_deref_mut() else {
            return false;
        };
        if owner.closing.load(Ordering::Acquire)
            || self.generation.as_deref().copied() != Some(request.generation)
        {
            return false;
        }
        match owner.slot.as_mut() {
            Some(Slot::Managed(travel))
                if travel.admission.id == request
                    && travel.state == ProjectSceneRequestState::Degraded
                    && !travel.retry_requested =>
            {
                travel.retry_requested = true;
                true
            }
            _ => false,
        }
    }

    /// Cancels preparation, or queues candidate disposal before native retirement starts.
    /// Once a domain retirement has begun or the new Scene has published, cancellation is rejected.
    pub fn cancel(&mut self, request: ProjectSceneRequestId) -> bool {
        self.owner
            .as_deref_mut()
            .is_some_and(|owner| owner.cancel(request))
    }

    /// Requests asynchronous preparation of a declared Scene through the original Host authority.
    ///
    /// The current source must belong to first-party hosted content. Source Products enter
    /// Preparing provisionally: the worker resolves their actual primary Scene and checks declared
    /// root membership. Dependency membership alone does not authorize travel. Processed requests
    /// select only Products declared by the original base. Missing authority or generation returns
    /// [`ProjectSceneRequestError::Unavailable`]; a missing I/O pool returns
    /// [`ProjectSceneRequestError::TaskUnavailable`]. An occupied slot remains
    /// [`ProjectSceneRequestError::Busy`] until its terminal result is explicitly acknowledged.
    pub fn request(
        &mut self,
        selection: impl Into<ProjectSceneSelection>,
    ) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        self.request_with_purpose(selection.into(), RequestPurpose::Travel)
    }

    /// Prepares a Deferred Scene Product from the current Host's admitted dependency closure.
    ///
    /// The request shares the original preparation slot and leaves primary simulation running.
    /// Once Ready, inspect the content and call [`activate_prepared_project_scene`] at an
    /// exclusive boundary. A loose pack member, travel root or resident Immediate dependency
    /// alone cannot authorize additive preparation. Committed additions may coexist. Each new
    /// activation needs its own occurrence and remains subject to the original content limits.
    pub fn request_additive(
        &mut self,
        product: AssetProductRef,
    ) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        let current = self
            .current
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        let residency = self
            .residency
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        #[cfg(feature = "runtime-2d")]
        if current.is_project_backed() {
            let path = current
                .deferred_source_scene(product)
                .ok_or(ProjectSceneRequestError::NotDeferredScene)?;
            let purpose = RequestPurpose::AdditiveSource(path.clone());
            return self.request_with_purpose(ProjectSceneSelection::Product(product), purpose);
        }
        if !residency.admits_processed_addition(current, product) {
            return Err(ProjectSceneRequestError::NotDeferredScene);
        }
        self.request_with_purpose(
            ProjectSceneSelection::Product(product),
            RequestPurpose::AdditiveProcessed,
        )
    }

    fn request_with_purpose(
        &mut self,
        selection: ProjectSceneSelection,
        purpose: RequestPurpose,
    ) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        let current = self
            .current
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        if !current.is_content_managed() {
            return Err(ProjectSceneRequestError::Unavailable);
        }
        let generation = **self
            .generation
            .as_ref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        let pools = self
            .pools
            .as_deref()
            .ok_or(ProjectSceneRequestError::TaskUnavailable)?;
        let owner = self
            .owner
            .as_deref_mut()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        if !owner.context.matches_current(current) {
            return Err(ProjectSceneRequestError::Stale);
        }
        owner.request(selection, purpose, generation, current.receipt(), pools)
    }

    /// Observes exactly this request using the current generation and expected-receipt predicates.
    ///
    /// Observation does not advance preparation or release its slot.
    #[must_use]
    pub fn state(&self, request: ProjectSceneRequestId) -> Option<ProjectSceneRequestState> {
        let owner = self.owner.as_deref()?;
        let slot = owner.slot.as_ref().filter(|slot| slot.id() == request)?;
        Some(if matches!(slot, Slot::Managed(_)) {
            slot.state()
        } else if let Some(admission) = slot.admission()
            && !owner.matches(
                admission,
                self.generation.as_deref().copied(),
                self.current.as_deref(),
            )
        {
            match slot {
                Slot::Preparing { .. } => ProjectSceneRequestState::Cancelling,
                _ => ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
            }
        } else {
            slot.state()
        })
    }

    /// Borrows ready content only while its Host, generation and expected receipt still match.
    ///
    /// Construct owned game data before ending this borrow and explicitly committing the request.
    #[must_use]
    pub fn prepared(&self, request: ProjectSceneRequestId) -> Option<PreparedProjectSceneView<'_>> {
        let owner = self.owner.as_deref()?;
        let Slot::Ready { admission, content } = owner.slot.as_ref()? else {
            return None;
        };
        (admission.id == request
            && owner.matches(
                admission,
                self.generation.as_deref().copied(),
                self.current.as_deref(),
            )
            && content_matches_admission(content, admission))
        .then_some(PreparedProjectSceneView { admission, content })
    }

    /// Safe owner-produced details for exactly this managed or terminal request.
    /// A successful retry retains publication diagnostics without resolved native failures.
    #[must_use]
    pub fn diagnostics(&self, request: ProjectSceneRequestId) -> Option<&DiagnosticReport> {
        match self.owner.as_deref()?.slot.as_ref()? {
            Slot::Managed(travel) if travel.admission.id == request => Some(&travel.diagnostics),
            Slot::Terminal {
                id, diagnostics, ..
            } if *id == request => Some(diagnostics),
            _ => None,
        }
    }

    /// Releases exactly this terminal request's slot.
    ///
    /// Returns `false` for preparing, cancelling or still-valid ready content. A stale ready
    /// request is rejected and acknowledged through the same admission checks as scheduled access.
    pub fn acknowledge(&mut self, request: ProjectSceneRequestId) -> bool {
        let generation = self.generation.as_deref().copied();
        let current = self.current.as_deref();
        let Some(owner) = self.owner.as_deref_mut() else {
            return false;
        };
        if let Some(Slot::Ready { admission, .. }) = owner.slot.as_ref()
            && admission.id == request
            && !owner.matches(admission, generation, current)
        {
            owner.reject_stale(request);
        }
        if owner
            .slot
            .as_ref()
            .is_some_and(|slot| slot.id() == request && slot.state().is_terminal())
        {
            owner.slot = None;
            #[cfg(feature = "runtime-2d")]
            if let Some(updates) = &mut owner.source_updates {
                updates.acknowledge(request);
            }
            true
        } else {
            false
        }
    }
}

/// Commits exactly one ready request using the original Host's content-residency transaction.
/// Predictable rejection leaves the current Scene unchanged and publishes no automatic successor.
pub fn commit_prepared_project_scene<F>(
    world: &mut World,
    request: ProjectSceneRequestId,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: F,
) -> SceneSpawnReport
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    if world
        .get_resource::<domains::ManagedSceneDomains>()
        .is_some_and(|domains| !domains.is_empty())
    {
        return rejected(
            "project.scene.managed-travel-required",
            "Installed native domains require begin_prepared_project_scene_travel",
        );
    }
    commit_ready_project_scene(world, request, false, |world, admission, content| {
        super::super::content_residency::replace_scene(
            world,
            &admission.expected,
            content,
            limits,
            additional_retirements,
            configure,
        )
    })
}

/// Activates exactly one ready additive request alongside the primary Scene.
///
/// The occurrence names the new Scene's lifetime and the returned receipt selects its later
/// unload. The Host keeps its original primary receipt and resource publications. Invoke from an
/// exclusive simulation boundary. Additive activation has the ordinary Image residency contract.
pub fn activate_prepared_project_scene(
    world: &mut World,
    request: ProjectSceneRequestId,
    occurrence: &SceneOccurrenceKey,
) -> SceneSpawnReport {
    commit_ready_project_scene(world, request, true, |world, admission, content| {
        super::super::content_residency::activate_addition(
            world,
            &admission.expected,
            content,
            occurrence,
        )
        .unwrap_or_else(|report| report)
    })
}

fn commit_ready_project_scene(
    world: &mut World,
    request: ProjectSceneRequestId,
    additive: bool,
    publish: impl FnOnce(&mut World, &Admission, &PreparedSceneContent) -> SceneSpawnReport,
) -> SceneSpawnReport {
    if !world.contains_resource::<RequestOwner>() {
        return rejected(
            "project.scene.request-unavailable",
            "Project Scene request authority is unavailable",
        );
    }
    if nara_ecs::__private::validate_resource_scope::<RequestOwner>(world).is_err() {
        return rejected(
            "project.scene.request-lifecycle-unsupported",
            "Scene request ownership cannot run lifecycle effects during commit",
        );
    }
    world.resource_scope(|world, mut owner: nara_ecs::Mut<RequestOwner>| {
        if owner.slot.as_ref().is_none_or(|slot| slot.id() != request) {
            return rejected(
                "project.scene.request-mismatch",
                "Project Scene request identity does not match",
            );
        }
        let valid = owner
            .slot
            .as_ref()
            .and_then(Slot::admission)
            .is_some_and(|admission| {
                owner.matches(
                    admission,
                    world.get_resource::<RuntimeGeneration>().copied(),
                    world.get_resource::<CurrentSceneOwner>(),
                )
            });
        if !valid {
            owner.reject_stale(request);
            return rejected(
                "project.scene.request-stale",
                "Project Scene request inputs changed before commit",
            );
        }
        if !matches!(owner.slot, Some(Slot::Ready { .. })) {
            return rejected(
                "project.scene.request-not-ready",
                "Project Scene preparation has not returned ready content",
            );
        }
        if owner
            .slot
            .as_ref()
            .and_then(Slot::admission)
            .is_some_and(|admission| admission.purpose.is_additive() != additive)
        {
            return wrong_publication();
        }
        let Some(Slot::Ready { admission, content }) = owner.slot.take() else {
            unreachable!()
        };
        let report = if !content_matches_admission(&content, &admission) {
            rejected(
                "project.scene.prepared-input-mismatch",
                "Prepared Scene does not match its captured Host authority",
            )
        } else {
            publish(world, &admission, &content)
        };
        let state = if !report.diagnostics.has_errors() && report.instance.is_some() {
            #[cfg(feature = "runtime-2d")]
            if !additive
                && let (Some(updates), PreparedSceneContent::Source(snapshot), Some(instance)) = (
                    &mut owner.source_updates,
                    &content,
                    report.instance.as_ref(),
                )
            {
                updates.publish(snapshot.content_closure(), instance);
            }
            ProjectSceneRequestState::Committed
        } else {
            ProjectSceneRequestState::Rejected(ProjectSceneRequestError::CommitRejected)
        };
        owner.slot = Some(Slot::Terminal {
            id: request,
            state,
            diagnostics: report.diagnostics.clone(),
        });
        report
    })
}

fn wrong_publication() -> SceneSpawnReport {
    rejected(
        "project.scene.request-purpose-mismatch",
        "Scene request must be published through the operation that admitted it",
    )
}

/// Prepares the complete candidate and queues managed Scene travel at the next TaskUpdate boundary.
///
/// The configuration callback runs once during this call. Native operations begin only after all
/// World-dependent preparation has succeeded. Observe the request state until Committed; after a
/// Degraded result, retry unfinished phases with [`ProjectScenes::retry`] or Stop the runtime.
pub fn begin_prepared_project_scene_travel<F>(
    world: &mut World,
    request: ProjectSceneRequestId,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: F,
) -> Result<(), SceneSpawnReport>
where
    F: FnOnce(&mut SceneProductOverlayWriter<'_>),
{
    if !world.contains_resource::<RequestOwner>() {
        return Err(rejected(
            "project.scene.request-unavailable",
            "Project Scene request authority is unavailable",
        ));
    }
    if nara_ecs::__private::validate_resource_scope::<RequestOwner>(world).is_err() {
        return Err(rejected(
            "project.scene.request-lifecycle-unsupported",
            "Scene request ownership cannot run lifecycle effects during preparation",
        ));
    }
    world.resource_scope(|world, mut owner: nara_ecs::Mut<RequestOwner>| {
        let Some(slot) = owner.slot.as_ref().filter(|slot| slot.id() == request) else {
            return Err(rejected(
                "project.scene.request-mismatch",
                "Project Scene request identity does not match",
            ));
        };
        if !matches!(slot, Slot::Ready { .. }) {
            return Err(rejected(
                "project.scene.request-not-ready",
                "Project Scene preparation has not returned ready content",
            ));
        }
        if slot
            .admission()
            .is_some_and(|admission| admission.purpose.is_additive())
        {
            return Err(wrong_publication());
        }
        let valid = slot.admission().is_some_and(|admission| {
            owner.matches(
                admission,
                world.get_resource::<RuntimeGeneration>().copied(),
                world.get_resource::<CurrentSceneOwner>(),
            )
        });
        if !valid {
            owner.reject_stale(request);
            return Err(rejected(
                "project.scene.request-stale",
                "Project Scene request inputs changed before preparation",
            ));
        }
        let Some(Slot::Ready { admission, content }) = owner.slot.take() else {
            unreachable!()
        };
        let prepared = (|| {
            if !content_matches_admission(&content, &admission) {
                return Err(rejected(
                    "project.scene.prepared-input-mismatch",
                    "Prepared Scene does not match its captured Host authority",
                ));
            }
            let registry = world
                .get_resource::<domains::ManagedSceneDomains>()
                .ok_or_else(|| {
                    rejected(
                        "project.scene.domains-unavailable",
                        "Managed Scene domain registry is unavailable",
                    )
                })?;
            let view = PreparedProjectSceneView {
                admission: &admission,
                content: &content,
            };
            let attempt =
                ManagedSceneTravelAttempt::prepare(registry, world, &view).map_err(|error| {
                    rejected_with_code(error.code(), "Native Scene domain preparation was rejected")
                })?;
            let candidate = super::super::content_residency::StagedProjectSceneReplacement::stage(
                world,
                &admission.expected,
                &content,
                &attempt.combined_claims(),
                limits,
                additional_retirements,
                configure,
            )?;
            Ok((attempt, candidate))
        })();
        match prepared {
            Ok((attempt, prepared)) => {
                owner.slot = Some(Slot::Managed(ManagedTravelState {
                    admission,
                    attempt,
                    prepared: Some(prepared),
                    published: None,
                    retry_requested: true,
                    native_started: false,
                    cancel_requested: false,
                    cleanup_result: None,
                    queued_frame: world
                        .get_resource::<nara_app::RealTime>()
                        .map_or(0, |time| time.frame),
                    state: ProjectSceneRequestState::Retiring,
                    publication_diagnostics: DiagnosticReport::default(),
                    diagnostics: DiagnosticReport::default(),
                    #[cfg(feature = "runtime-2d")]
                    source_closure: match &content {
                        PreparedSceneContent::Source(snapshot) => Some(snapshot.content_closure()),
                        PreparedSceneContent::Processed(_) => None,
                    },
                }));
                Ok(())
            }
            Err(report) => {
                owner.slot = Some(Slot::Terminal {
                    id: request,
                    state: ProjectSceneRequestState::Rejected(
                        ProjectSceneRequestError::CommitRejected,
                    ),
                    diagnostics: report.diagnostics.clone(),
                });
                Err(report)
            }
        }
    })
}

pub(super) fn content_matches_admission(
    content: &PreparedSceneContent,
    admission: &Admission,
) -> bool {
    match (content, admission.context.as_ref(), &admission.selection) {
        #[cfg(feature = "runtime-2d")]
        (
            PreparedSceneContent::Source(snapshot),
            Context::Source {
                candidate,
                plan,
                input,
                ..
            },
            selection,
        ) => {
            let selection_matches = match selection {
                ProjectSceneSelection::Source(path) => snapshot.selected_scene_path() == path,
                ProjectSceneSelection::Product(product) => {
                    product.product() == nara_asset::ImportedProductId::PRIMARY
                        && snapshot.selected_scene_product() == Some(*product)
                }
            };
            let input_matches = match (snapshot.input_action_map(), input) {
                (Some(captured), Some(original)) => captured.shares_owner_with(original),
                (None, None) => true,
                _ => false,
            };
            selection_matches
                && input_matches
                && snapshot.lineage() == candidate.lineage()
                && plan.lineage() == candidate.lineage()
                && snapshot.content_closure().manifest_digest() == candidate.manifest_digest()
                && match &admission.purpose {
                    RequestPurpose::Travel => admission
                        .context
                        .declares_source_path(snapshot.selected_scene_path()),
                    RequestPurpose::AdditiveSource(path) => {
                        snapshot.selected_scene_product().is_some_and(|product| {
                            snapshot
                                .asset_database()
                                .record_for_stable_id(product.source())
                                .is_some_and(|record| record.path() == path)
                        })
                    }
                    RequestPurpose::AdditiveProcessed => false,
                }
                && snapshot.shares_component_bindings(plan.schema_validation().bindings())
                && snapshot.matches_asset_domains(plan)
        }
        (
            PreparedSceneContent::Processed(content),
            Context::Processed { base },
            ProjectSceneSelection::Product(product),
        ) => {
            base.same_base(content.base())
                && content.product() == *product
                && match admission.purpose {
                    RequestPurpose::Travel => {
                        admission.context.admits_request(&admission.selection)
                    }
                    RequestPurpose::AdditiveProcessed => true,
                    #[cfg(feature = "runtime-2d")]
                    RequestPurpose::AdditiveSource(_) => false,
                }
                && content
                    .scene
                    .candidate()
                    .definitions()
                    .matches(base.plan().schema_validation().bindings())
        }
        _ => false,
    }
}
