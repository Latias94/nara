//! The global source Input owner is independent of Scene residency and collection replacement.

use nara_input::{ActionMap, ActionMapBindings, ActionMapDefinition, ActionMapFileCandidate};

use super::*;

#[derive(Clone)]
pub struct ProjectInputContent {
    inner: Arc<SourceInputOwner>,
}

struct SourceInputOwner {
    definition: ActionMapDefinition,
    product: AssetProductRef,
    path: AssetSourcePath,
    digest: ContentDigest,
    metadata_digest: Option<ContentDigest>,
    _lease: ProjectContentLease,
}

impl AsRef<ActionMapDefinition> for SourceInputOwner {
    fn as_ref(&self) -> &ActionMapDefinition {
        &self.definition
    }
}

impl fmt::Debug for ProjectInputContent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectInputContent")
            .field("product", &self.product())
            .field("path", self.path())
            .field("digest", &self.source_digest())
            .finish_non_exhaustive()
    }
}

impl ProjectInputContent {
    #[must_use]
    pub fn product(&self) -> AssetProductRef {
        self.inner.product
    }
    #[must_use]
    pub fn path(&self) -> &AssetSourcePath {
        &self.inner.path
    }
    #[must_use]
    pub fn source_digest(&self) -> ContentDigest {
        self.inner.digest
    }
    #[must_use]
    pub fn definition(&self) -> &ActionMapDefinition {
        &self.inner.definition
    }
    #[must_use]
    pub fn bindings(&self) -> ActionMapBindings {
        ActionMapBindings::from_owner(Arc::clone(&self.inner))
    }
    #[must_use]
    pub fn shares_owner_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub(crate) fn prepare_state(&self) -> Result<ActionMap, ProjectContentError> {
        let bytes = ActionMap::required_state_bytes();
        let lease = self
            .inner
            ._lease
            .reserve_retained(bytes)
            .map_err(ProjectContentError::budget)?;
        ActionMap::new_with_retention_owner(
            self.bindings(),
            ByteLimit::new(bytes).expect("state cost is nonzero"),
            Arc::new(lease),
        )
        .map_err(|_| invalid_input())
    }
}

impl ProjectContentLoader {
    /// Scene travel verifies the captured global selection and shares its original compiled owner.
    pub(crate) fn load_scene_reusing_input(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        scene: &nara_project::ProjectPath,
        input: Option<&ProjectInputContent>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_travel_with_input(candidate, plan, SourceSceneSelection::Path(scene), input)
    }

    pub(crate) fn load_scene_product_reusing_input(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        product: AssetProductRef,
        input: Option<&ProjectInputContent>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_travel_with_input(
            candidate,
            plan,
            SourceSceneSelection::Product(product),
            input,
        )
    }

    fn load_travel_with_input(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        scene: SourceSceneSelection<'_>,
        input: Option<&ProjectInputContent>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        if candidate.input_requirement().is_some() != input.is_some() {
            return Err(input_changed());
        }
        self.load_inner_with_delivery_catalogue(
            candidate,
            plan,
            Some(scene),
            None,
            BTreeMap::new(),
            None,
            None,
            false,
            input,
        )
        .map(|(snapshot, _)| snapshot)
    }
}

impl LoadContext<'_> {
    pub(super) fn load_input(
        &mut self,
        candidate: &ProjectSettingsCandidate,
        reuse: Option<&ProjectInputContent>,
    ) -> Result<Option<ProjectInputContent>, ProjectContentError> {
        let Some(selected) = candidate.input_requirement() else {
            return if reuse.is_some() {
                Err(input_changed())
            } else {
                Ok(None)
            };
        };
        let limits = self.loader.limits.input_file();
        // Exact project-relative input reads never infer an Assets alias scope.
        let root = self.loader.root.share();
        let (bytes, digest) =
            self.read_native_file(&root, selected.as_str(), limits.encoded_bytes().get())?;
        let encoded = bytes.len();
        if let Some(original) = reuse {
            if original.path().as_str() != selected.as_str() || original.source_digest() != digest {
                return Err(input_changed());
            }
            drop(bytes);
            self.subtract_budget(ProjectContentBudgetKind::EncodedBytes, encoded);
            let metadata = self.admit_input_identity(original.product(), original.path())?;
            if metadata != original.inner.metadata_digest {
                return Err(input_changed());
            }
            self.record_project_member(
                ProjectContentMemberKind::InputActionMap,
                selected.as_str(),
                digest,
            )?;
            return Ok(Some(original.clone()));
        }

        let decode_work = limits
            .predecode_work_bytes()
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let mut budget = self.loader.budget_host.reserve();
        let owner_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<SourceInputOwner>(),
                ARC_CONTROL_BLOCK_BYTES,
                string_retained_bytes(selected.as_str())?,
                budget::CONTENT_LEASE_RETAINED_BYTES,
            ],
        )?;
        budget
            .set(
                ProjectContentBudgetKind::WorkBytes,
                checked_plan_sum(
                    ProjectContentBudgetKind::WorkBytes,
                    [decode_work, owner_work],
                )?,
            )
            .map_err(ProjectContentError::budget)?;
        let file = ActionMapFileCandidate::decode_json_bytes_with_limits(&bytes, limits)
            .map_err(|_| invalid_input())?;
        drop(bytes);
        self.subtract_budget(ProjectContentBudgetKind::EncodedBytes, encoded);
        let (product, definition) = file.into_parts();
        let path = AssetSourcePath::new(selected.as_str()).map_err(|_| invalid_input())?;
        let metadata_digest = self.admit_input_identity(product, &path)?;
        self.record_project_member(
            ProjectContentMemberKind::InputActionMap,
            selected.as_str(),
            digest,
        )?;
        let retained = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [definition.cost().retained_bytes(), owner_work],
        )?;
        budget
            .set_many(&[
                (ProjectContentBudgetKind::WorkBytes, 0),
                (ProjectContentBudgetKind::RetainedBytes, retained),
            ])
            .map_err(ProjectContentError::budget)?;
        let lease = budget.into_lease().map_err(ProjectContentError::budget)?;
        Ok(Some(ProjectInputContent {
            inner: Arc::new(SourceInputOwner {
                definition,
                product,
                path,
                digest,
                metadata_digest,
                _lease: lease,
            }),
        }))
    }

    fn admit_input_identity(
        &mut self,
        product: AssetProductRef,
        path: &AssetSourcePath,
    ) -> Result<Option<ContentDigest>, ProjectContentError> {
        // Intrinsic identity joins the same catalogue used by Scene, Prefab and resource capture.
        self.ensure_reference_catalogue()?;
        let scratch = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<AssetRecord>(),
                string_retained_bytes(path.as_str())?,
                string_retained_bytes("input-action-map")?,
            ],
        )?)?;
        let record = AssetRecord::new(
            product.source(),
            path.clone(),
            AssetSourceKind::Other("input-action-map".to_owned()),
        );
        let metadata_digest = self.validate_input_sidecar(&record)?;
        if let Some(existing) = self.asset_database.record_for_path(path) {
            if existing != &record {
                return Err(catalogue::identity_conflict(path));
            }
        } else {
            self.asset_database
                .validate_insert(&record)
                .map_err(|_| catalogue::identity_conflict(path))?;
            self.reserve_asset_database_work(&record)?;
            self.asset_database
                .insert(record)
                .map_err(|_| catalogue::identity_conflict(path))?;
        }
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, scratch.reserved);
        Ok(metadata_digest)
    }

    fn validate_input_sidecar(
        &mut self,
        record: &AssetRecord,
    ) -> Result<Option<ContentDigest>, ProjectContentError> {
        let path_work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                string_retained_bytes(record.path().as_str())?,
                ".meta".len(),
            ],
        )?)?;
        let meta_path = format!("{}.meta", record.path().as_str());
        let root = self.loader.root.share();
        let mut metadata_digest = None;
        if let Some((bytes, digest)) = self.read_optional_input_metadata(&root, &meta_path)? {
            metadata_digest = Some(digest);
            let encoded = bytes.len();
            let decode_work =
                self.reserve_work(AssetMetaFileLimits::default().encoded_bytes().get())?;
            let identity =
                AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
                    .map_err(|_| invalid_input())?
                    .into_identity();
            if identity.stable_id() != record.stable_id()
                || identity.source_kind() != record.source_kind()
                || ![
                    &self.asset_scope,
                    &self.prefab_scope,
                    &self.scene_scope,
                    &AssetPathScope::ROOT,
                ]
                .into_iter()
                .any(|scope| scope.relative_path(record.path()) == Some(identity.path().as_str()))
            {
                return Err(catalogue::identity_conflict(record.path()));
            }
            if self
                .catalogue
                .as_ref()
                .and_then(|catalogue| catalogue.metadata.get(record.path()))
                .is_some_and(|entry| entry.digest != digest)
            {
                return Err(input_changed());
            }
            drop(identity);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, decode_work.reserved);
            self.subtract_budget(ProjectContentBudgetKind::EncodedBytes, encoded);
            self.record_project_member(
                ProjectContentMemberKind::AssetMetadata,
                &meta_path,
                digest,
            )?;
        }
        drop(meta_path);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, path_work.reserved);
        Ok(metadata_digest)
    }

    fn read_optional_input_metadata(
        &mut self,
        root: &DirectoryCapability,
        path: &str,
    ) -> Result<Option<(Vec<u8>, ContentDigest)>, ProjectContentError> {
        let relative = self.observe_path(path)?;
        self.add_budget(ProjectContentBudgetKind::Files, 1)?;
        self.add_budget(
            ProjectContentBudgetKind::OpenHandles,
            EXACT_RELATIVE_OPEN_HANDLE_PEAK,
        )?;
        let name_work = self.reserve_work(EXACT_RELATIVE_OPEN_WORK_BYTES)?;
        let opened = root.open_file_exact(&relative);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, name_work.reserved);
        let file = match opened {
            Ok(file) => file,
            Err(error) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    EXACT_RELATIVE_OPEN_HANDLE_PEAK,
                );
                return if fs_error_is_not_found(&error) {
                    Ok(None)
                } else {
                    Err(map_fs_error(error))
                };
            }
        };
        let ceiling = AssetMetaFileLimits::default().encoded_bytes().get();
        self.add_budget(ProjectContentBudgetKind::EncodedBytes, ceiling)?;
        let result = file.read_to_end_bounded(ceiling as u64);
        drop(file);
        self.subtract_budget(
            ProjectContentBudgetKind::OpenHandles,
            EXACT_RELATIVE_OPEN_HANDLE_PEAK,
        );
        match result {
            Ok(bytes) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::EncodedBytes,
                    ceiling - bytes.len(),
                );
                let digest = ContentDigest::of_bytes(&bytes);
                Ok(Some((bytes, digest)))
            }
            Err(error) => {
                self.subtract_budget(ProjectContentBudgetKind::EncodedBytes, ceiling);
                Err(map_fs_error(error))
            }
        }
    }
}

fn invalid_input() -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::InputDefinition,
        "project.content.input-definition-invalid",
        "Selected Input source is not an admitted finite action map",
    )
}

fn input_changed() -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::HostAuthorityRejected,
        "project.content.global-input-source-changed",
        "Global Input source changed; restart the Runtime before Scene travel",
    )
}
