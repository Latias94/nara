//! Public finite request surface and explicit owner-thread commit.

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
pub struct PreparedProjectSceneView<'a> {
    admission: &'a Admission,
    snapshot: &'a ProjectContentSnapshot,
}

impl PreparedProjectSceneView<'_> {
    /// Typed CPU data prepared for the target Scene, before its explicit commit.
    #[must_use]
    pub fn domain_assets(&self) -> &crate::project_host::ProjectDomainAssets {
        self.snapshot.domain_assets()
    }

    /// Expanded target document, matching the document that becomes `CurrentScene::source`.
    #[must_use]
    pub fn scene(&self) -> &SceneDocument {
        self.snapshot.expanded_scene()
    }

    #[must_use]
    pub fn selected_scene_path(&self) -> &ProjectPath {
        &self.admission.path
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
#[derive(SystemParam)]
pub struct ProjectScenes<'w> {
    owner: Option<ResMut<'w, RequestOwner>>,
    current: Option<Res<'w, CurrentSceneOwner>>,
    generation: Option<Res<'w, RuntimeGeneration>>,
    pools: Option<Res<'w, TaskPools>>,
}

impl ProjectScenes<'_> {
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

    pub fn request(
        &mut self,
        path: ProjectPath,
    ) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        let current = self
            .current
            .as_deref()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        if !current.is_project_backed() {
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
        self.owner
            .as_deref_mut()
            .ok_or(ProjectSceneRequestError::Unavailable)?
            .request(path, generation, current.receipt(), pools)
    }

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

    #[must_use]
    pub fn prepared(&self, request: ProjectSceneRequestId) -> Option<PreparedProjectSceneView<'_>> {
        let owner = self.owner.as_deref()?;
        let Slot::Ready {
            admission,
            snapshot,
        } = owner.slot.as_ref()?
        else {
            return None;
        };
        (admission.id == request
            && owner.matches(
                admission,
                self.generation.as_deref().copied(),
                self.current.as_deref(),
            ))
        .then_some(PreparedProjectSceneView {
            admission,
            snapshot,
        })
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
        let Some(Slot::Ready {
            admission,
            snapshot,
        }) = owner.slot.take()
        else {
            unreachable!()
        };
        let report = if !snapshot_matches_admission(&snapshot, &admission) {
            rejected(
                "project.scene.prepared-input-mismatch",
                "Prepared Scene does not match its captured Host authority",
            )
        } else {
            super::super::content_residency::replace_project_scene(
                world,
                &admission.expected,
                &snapshot,
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
        let Some(Slot::Ready {
            admission,
            snapshot,
        }) = owner.slot.take()
        else {
            unreachable!()
        };
        let prepared = (|| {
            if !snapshot_matches_admission(&snapshot, &admission) {
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
                snapshot: &snapshot,
            };
            let attempt =
                ManagedSceneTravelAttempt::prepare(registry, world, &view).map_err(|error| {
                    rejected_with_code(error.code(), "Native Scene domain preparation was rejected")
                })?;
            let candidate =
                super::super::content_residency::PreparedProjectSceneReplacement::prepare(
                    world,
                    &admission.expected,
                    &snapshot,
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

fn snapshot_matches_admission(snapshot: &ProjectContentSnapshot, admission: &Admission) -> bool {
    snapshot.lineage() == admission.context.candidate.lineage()
        && snapshot.content_closure().manifest_digest()
            == admission.context.candidate.manifest_digest()
        && snapshot.selected_scene_path() == &admission.path
        && snapshot.shares_schema_snapshot(admission.context.plan.schema_validation().snapshot())
        && snapshot.matches_asset_domains(&admission.context.plan)
}
