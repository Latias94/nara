use std::{collections::BTreeMap, fmt, mem::size_of, sync::Arc};

use nara_asset::StableAssetId;
use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity};
use nara_project::ProjectPath;
use nara_reflect::SchemaCompositionFingerprint;

use super::catalogue::DeliveryCatalogue;
use super::{
    ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD, ProjectContentBudgetKind,
    ProjectContentClosureReceipt, ProjectContentError, ProjectContentErrorKind,
    ProjectContentLease, ProjectContentLoader, ProjectContentMemberReceipt, ProjectContentSnapshot,
    ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan, StartupContentClosureReceipt,
    allocation_failed, append_frame, checked_plan_product, checked_plan_sum, string_retained_bytes,
};

/// Exact source delivery for startup and the project's explicitly declared Scene roots.
///
/// This receipt retains bounded source facts and read authority. It does not retain decoded
/// Scenes, Images, running instances, or runtime residency. Startup attestation remains a
/// separate receipt; adding another delivery root does not change what startup materializes.
#[derive(Clone)]
pub struct ProjectContentDeliveryReceipt {
    inner: Arc<DeliveryInner>,
}

struct DeliveryInner {
    startup: StartupContentClosureReceipt,
    roots: Box<[ProjectPath]>,
    members: Box<[ProjectContentMemberReceipt]>,
    digest: ContentDigest,
    _lease: ProjectContentLease,
}

impl ProjectContentDeliveryReceipt {
    #[must_use]
    pub fn startup_content(&self) -> &StartupContentClosureReceipt {
        &self.inner.startup
    }

    /// Sorted Scene paths relative to the effective `paths.scenes`, including startup.
    #[must_use]
    pub fn scene_roots(&self) -> &[ProjectPath] {
        &self.inner.roots
    }

    /// Sorted, deduplicated project-relative source members for all declared roots.
    #[must_use]
    pub fn members(&self) -> &[ProjectContentMemberReceipt] {
        &self.inner.members
    }

    #[must_use]
    pub fn delivery_digest(&self) -> ContentDigest {
        self.inner.digest
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.inner.startup.project_root_identity()
    }

    #[must_use]
    pub fn settings_lineage(&self) -> ProjectSettingsLineage {
        self.inner.startup.settings_lineage()
    }

    #[must_use]
    pub fn manifest_digest(&self) -> ContentDigest {
        self.inner.startup.manifest_digest()
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.inner.startup.schema_fingerprint()
    }

    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.inner.startup.project_root()
    }

    pub(crate) fn runtime_directories(&self) -> &[ProjectPath] {
        self.inner.startup.runtime_directories()
    }
}

impl fmt::Debug for ProjectContentDeliveryReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectContentDeliveryReceipt")
            .field("project_root_identity", &self.project_root_identity())
            .field("settings_lineage", &self.settings_lineage())
            .field("scene_roots", &self.scene_roots())
            .field("member_count", &self.members().len())
            .field("delivery_digest", &self.delivery_digest())
            .finish()
    }
}

impl ProjectContentLoader {
    /// Validates startup and every declared Scene using the same bounded content loader.
    ///
    /// Decoded snapshots retire between roots. The resulting source union is independently
    /// budgeted and grants no runtime residency. A missing or invalid declared root rejects the
    /// complete delivery plan; undeclared files are not automatically included.
    /// Stable identities use one lazily captured catalogue for this operation. Referenced
    /// metadata is revalidated on use; unrelated additions are discovered by the next operation.
    pub fn load_delivery(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
    ) -> Result<ProjectContentDeliveryReceipt, ProjectContentError> {
        self.validate_inputs(candidate, plan)?;
        let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
            &self.root, candidate, plan,
        )?;
        let (startup, catalogue) =
            self.load_startup_for_delivery(candidate, plan, None, agreement)?;
        self.load_delivery_with_startup_and_catalogue(candidate, plan, startup, catalogue)
    }

    pub(crate) fn load_delivery_with_startup_and_catalogue(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        snapshot: ProjectContentSnapshot,
        mut catalogue: Option<DeliveryCatalogue>,
    ) -> Result<ProjectContentDeliveryReceipt, ProjectContentError> {
        self.validate_inputs(candidate, plan)?;
        let startup = snapshot.startup_content_closure().ok_or_else(|| {
            delivery_error(
                "project.delivery.startup-mismatch",
                "Delivery requires the admitted startup Scene",
            )
        })?;
        let content = snapshot.content_closure();
        if !snapshot.shares_component_bindings(plan.schema_validation().bindings())
            || !snapshot.matches_asset_domains(plan)
            || !snapshot.has_runtime_schema_agreement()
            || content.settings_lineage() != candidate.lineage()
            || content.project_root_identity() != self.root.identity()
            || content.manifest_digest() != candidate.manifest_digest()
            || content.schema_fingerprint() != plan.schema_validation().composition_fingerprint()
        {
            return Err(delivery_error(
                "project.delivery.authority-mismatch",
                "Delivery source inputs do not share one project and schema authority",
            ));
        }

        let mut budget = self.budget_host.reserve();
        let root_capacity = candidate.settings().content.scenes.len().saturating_add(1);
        let roots_work =
            checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    checked_plan_product(
                        ProjectContentBudgetKind::WorkBytes,
                        root_capacity,
                        size_of::<ProjectPath>(),
                    )?,
                    string_retained_bytes(content.selected_scene_path().as_str())?,
                    candidate.settings().content.scenes.iter().try_fold(
                        0usize,
                        |bytes, root| {
                            checked_plan_sum(
                                ProjectContentBudgetKind::WorkBytes,
                                [bytes, string_retained_bytes(root.as_str())?],
                            )
                        },
                    )?,
                ],
            )?;
        budget
            .set(ProjectContentBudgetKind::WorkBytes, roots_work)
            .map_err(ProjectContentError::budget)?;
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(root_capacity)
            .map_err(|_| allocation_failed("project.delivery.roots-allocation"))?;
        roots.push(content.selected_scene_path().clone());
        roots.extend(candidate.settings().content.scenes.iter().cloned());
        roots.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        roots.dedup();

        let mut source_identities = BTreeMap::new();
        merge_delivery_source_identities(&mut source_identities, &snapshot, &mut budget)?;
        let mut members = BTreeMap::<String, ProjectContentMemberReceipt>::new();
        self.merge_delivery_members(
            &mut members,
            &content,
            startup.content_closure(),
            &mut budget,
        )?;
        drop(snapshot);
        for root in &roots {
            if root == startup.content_closure().selected_scene_path() {
                continue;
            }
            let (snapshot, next_catalogue) = self.load_scene_for_delivery(
                candidate,
                plan,
                root,
                catalogue.as_ref(),
                catalogue.is_none(),
            )?;
            catalogue = next_catalogue.or(catalogue);
            merge_delivery_source_identities(&mut source_identities, &snapshot, &mut budget)?;
            self.merge_delivery_members(
                &mut members,
                &snapshot.content_closure(),
                startup.content_closure(),
                &mut budget,
            )?;
            // No decoded Scene/Image or generation lease escapes into the delivery receipt.
            drop(snapshot);
        }
        drop(source_identities);

        let array_work = checked_plan_product(
            ProjectContentBudgetKind::WorkBytes,
            members.len(),
            size_of::<ProjectContentMemberReceipt>(),
        )?;
        budget
            .add(ProjectContentBudgetKind::WorkBytes, array_work)
            .map_err(ProjectContentError::budget)?;
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(members.len())
            .map_err(|_| allocation_failed("project.delivery.members-allocation"))?;
        ordered.extend(members.into_values());
        let digest_bytes = delivery_digest_bytes(startup.runtime_directories(), &roots, &ordered)?;
        budget
            .add(ProjectContentBudgetKind::WorkBytes, digest_bytes)
            .map_err(ProjectContentError::budget)?;
        let digest = delivery_digest(&startup, &roots, &ordered, digest_bytes)?;
        let retained = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                size_of::<DeliveryInner>(),
                ARC_CONTROL_BLOCK_BYTES,
                checked_plan_product(
                    ProjectContentBudgetKind::RetainedBytes,
                    roots.len(),
                    size_of::<ProjectPath>(),
                )?,
                array_work,
                roots.iter().try_fold(0usize, |bytes, root| {
                    checked_plan_sum(
                        ProjectContentBudgetKind::RetainedBytes,
                        [bytes, string_retained_bytes(root.as_str())?],
                    )
                })?,
                ordered.iter().try_fold(0usize, |bytes, member| {
                    checked_plan_sum(
                        ProjectContentBudgetKind::RetainedBytes,
                        [bytes, string_retained_bytes(member.path().as_str())?],
                    )
                })?,
            ],
        )?;
        budget
            .set_many(&[
                (ProjectContentBudgetKind::RetainedBytes, retained),
                (ProjectContentBudgetKind::WorkBytes, 0),
            ])
            .map_err(ProjectContentError::budget)?;
        let lease = budget.into_lease().map_err(ProjectContentError::budget)?;
        Ok(ProjectContentDeliveryReceipt {
            inner: Arc::new(DeliveryInner {
                startup,
                roots: roots.into_boxed_slice(),
                members: ordered.into_boxed_slice(),
                digest,
                _lease: lease,
            }),
        })
    }

    fn merge_delivery_members(
        &self,
        members: &mut BTreeMap<String, ProjectContentMemberReceipt>,
        content: &ProjectContentClosureReceipt,
        startup: &ProjectContentClosureReceipt,
        budget: &mut super::BudgetTicket,
    ) -> Result<(), ProjectContentError> {
        if content.project_root_identity() != startup.project_root_identity()
            || content.settings_lineage() != startup.settings_lineage()
            || content.manifest_digest() != startup.manifest_digest()
            || content.schema_fingerprint() != startup.schema_fingerprint()
            || content.runtime_directories() != startup.runtime_directories()
        {
            return Err(delivery_error(
                "project.delivery.authority-mismatch",
                "Delivery roots do not share the same source authority",
            ));
        }
        for member in content.members() {
            if let Some(previous) = members.get(member.path().as_str()) {
                if previous != member {
                    return Err(delivery_error(
                        "project.delivery.member-changed",
                        "A shared delivery source changed between Scene loads",
                    ));
                }
                continue;
            }
            if members.len() >= self.limits.limit(ProjectContentBudgetKind::Files) {
                return Err(delivery_error(
                    "project.delivery.member-limit",
                    "Delivery exceeds the project content file limit",
                ));
            }
            let work = checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    size_of::<ProjectPath>(),
                    size_of::<ProjectContentMemberReceipt>(),
                    BTREE_ENTRY_OVERHEAD,
                    string_retained_bytes(member.path().as_str())?,
                    string_retained_bytes(member.path().as_str())?,
                ],
            )?;
            budget
                .add(ProjectContentBudgetKind::WorkBytes, work)
                .map_err(ProjectContentError::budget)?;
            members.insert(member.path().as_str().to_owned(), member.clone());
        }
        Ok(())
    }
}

fn merge_delivery_source_identities(
    identities: &mut BTreeMap<StableAssetId, ProjectPath>,
    snapshot: &ProjectContentSnapshot,
    budget: &mut super::BudgetTicket,
) -> Result<(), ProjectContentError> {
    // Source content retains every actually used Scene association in the same canonical index.
    // Delivery neither reconstructs a directory prefix nor rediscovers dependency identities.
    for record in snapshot.asset_database().records() {
        admit_delivery_source_identity(
            identities,
            record.stable_id(),
            record.path().as_str(),
            budget,
        )?;
    }
    Ok(())
}

fn admit_delivery_source_identity(
    identities: &mut BTreeMap<StableAssetId, ProjectPath>,
    stable_id: StableAssetId,
    source_path: &str,
    budget: &mut super::BudgetTicket,
) -> Result<(), ProjectContentError> {
    if let Some(existing) = identities.get(&stable_id) {
        if existing.as_str() == source_path {
            return Ok(());
        }
        return Err(ProjectContentError::single(
            ProjectContentErrorKind::AssetMetaMismatch,
            "project.delivery.source-identity-conflict",
            "Different delivery source paths claim the same stable identity",
        ));
    }
    let work = checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<(StableAssetId, ProjectPath)>(),
            BTREE_ENTRY_OVERHEAD,
            source_path.len(),
        ],
    )?;
    budget
        .add(ProjectContentBudgetKind::WorkBytes, work)
        .map_err(ProjectContentError::budget)?;
    let path = ProjectPath::new(source_path).map_err(|_| {
        delivery_error(
            "project.delivery.source-path-invalid",
            "Delivery source has an invalid project-relative path",
        )
    })?;
    identities.insert(stable_id, path);
    Ok(())
}

fn delivery_error(code: &'static str, summary: &'static str) -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::HostAuthorityRejected,
        code,
        summary,
    )
}

fn delivery_digest_bytes(
    directories: &[ProjectPath],
    roots: &[ProjectPath],
    members: &[ProjectContentMemberReceipt],
) -> Result<usize, ProjectContentError> {
    let mut total = b"nara.project-content-delivery.v1\0".len() + 32 + 8 + 64 + 8 + 8 + 8;
    for directory in directories {
        total = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [total, 8, directory.as_str().len()],
        )?;
    }
    for root in roots {
        total = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [total, 8, root.as_str().len()],
        )?;
    }
    for member in members {
        total = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                total,
                24,
                member.path().as_str().len(),
                member.kind().digest_role().len(),
                32,
            ],
        )?;
    }
    Ok(total)
}

fn delivery_digest(
    startup: &StartupContentClosureReceipt,
    roots: &[ProjectPath],
    members: &[ProjectContentMemberReceipt],
    capacity: usize,
) -> Result<ContentDigest, ProjectContentError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| allocation_failed("project.delivery.digest-allocation"))?;
    bytes.extend_from_slice(b"nara.project-content-delivery.v1\0");
    bytes.extend_from_slice(startup.manifest_digest().as_bytes());
    append_frame(&mut bytes, startup.schema_fingerprint().to_hex().as_bytes())?;
    bytes.extend_from_slice(&(startup.runtime_directories().len() as u64).to_le_bytes());
    for directory in startup.runtime_directories() {
        append_frame(&mut bytes, directory.as_str().as_bytes())?;
    }
    bytes.extend_from_slice(&(roots.len() as u64).to_le_bytes());
    for root in roots {
        append_frame(&mut bytes, root.as_str().as_bytes())?;
    }
    bytes.extend_from_slice(&(members.len() as u64).to_le_bytes());
    for member in members {
        append_frame(&mut bytes, member.kind().digest_role().as_bytes())?;
        append_frame(&mut bytes, member.path().as_str().as_bytes())?;
        bytes.extend_from_slice(&member.digest().length().to_le_bytes());
        bytes.extend_from_slice(member.digest().as_bytes());
    }
    debug_assert_eq!(
        bytes.len(),
        capacity,
        "delivery framing must match its reserved work"
    );
    Ok(ContentDigest::of_bytes(&bytes))
}
