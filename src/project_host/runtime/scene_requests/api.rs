//! Public finite request surface and explicit owner-thread commit.

use nara_ecs::bevy_ecs::system::SystemState;

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
/// Processed Hosts require an exact Product in their original immutable base. Neither mode falls
/// back to the other, and an auxiliary selection never selects its enclosing primary Product.
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
    #[error("project Scene preparation is unavailable on the I/O task pool")]
    TaskUnavailable,
    #[error("project Scene request identities are exhausted")]
    IdExhausted,
    #[error("project Scene request inputs no longer match the runtime")]
    Stale,
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
            Self::TaskUnavailable => "project.scene.preparation-unavailable",
            Self::IdExhausted => "project.scene.request-id-exhausted",
            Self::Stale => "project.scene.request-stale",
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
    /// Typed CPU data prepared for the target Scene, before its explicit commit.
    #[must_use]
    pub fn domain_assets(&self) -> &crate::project_host::ProjectDomainAssets {
        self.content.domain_assets()
    }

    /// Admitted target document, matching the document that becomes `CurrentScene::source`.
    #[must_use]
    pub fn scene(&self) -> &SceneDocument {
        self.content.document()
    }

    /// Borrows the exact selection captured by this request.
    #[must_use]
    pub fn selected_scene(&self) -> &ProjectSceneSelection {
        &self.admission.selection
    }

    /// Returns the actual captured path relative to the configured Scenes directory for source
    /// content, including requests originally made by Product. Processed content has no source path.
    #[must_use]
    pub fn selected_scene_path(&self) -> Option<&ProjectPath> {
        match self.content {
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
    generation: Option<Res<'w, RuntimeGeneration>>,
    pools: Option<Res<'w, TaskPools>>,
}

impl ProjectScenes<'_> {
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
    ///         scenes.prepared(request).map(|view| view.scene().roots.clone())
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
        owner.request(selection.into(), generation, current.receipt(), pools)
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
    additional_retirements: &[WorldEntityToken],
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
        let Some(Slot::Ready { admission, content }) = owner.slot.take() else {
            unreachable!()
        };
        let report = if !content_matches_admission(&content, &admission) {
            rejected(
                "project.scene.prepared-input-mismatch",
                "Prepared Scene does not match its captured Host authority",
            )
        } else {
            super::super::content_residency::replace_scene(
                world,
                &admission.expected,
                &content,
                limits,
                additional_retirements,
                configure,
            )
        };
        let state = if !report.diagnostics.has_errors() && report.instance.is_some() {
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

/// Prepares the complete candidate and queues managed Scene travel at the next TaskUpdate boundary.
///
/// The configuration callback runs once during this call. Native operations begin only after all
/// World-dependent preparation has succeeded. Observe the request state until Committed; after a
/// Degraded result, retry unfinished phases with [`ProjectScenes::retry`] or Stop the runtime.
pub fn begin_prepared_project_scene_travel<F>(
    world: &mut World,
    request: ProjectSceneRequestId,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[WorldEntityToken],
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
            let candidate =
                super::super::content_residency::PreparedProjectSceneReplacement::prepare(
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
                && admission
                    .context
                    .declares_source_path(snapshot.selected_scene_path())
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
                && admission.context.admits_request(&admission.selection)
                && content
                    .scene
                    .candidate()
                    .definitions()
                    .matches(base.plan().schema_validation().bindings())
        }
        _ => false,
    }
}
