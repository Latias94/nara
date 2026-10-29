//! Verified immutable Runtime content, independent of authoring source and import caches.
//!
//! The Host supplies the expected catalog digest and target independently of the directory.
//! Opening a base captures every declared member's verified bytes once. Subsequent acquisition
//! retains that exact base and its original executable composition; it never reopens a mutable
//! member or searches a source database. Integrity, typed decode and native residency are distinct
//! operations. This module does not install decoded products into a World or replace a resolver.

mod budget;
mod catalog;
mod codecs;
mod prepared;

pub use codecs::{RuntimeCodecDefinitionId, RuntimeCodecSelectionError, RuntimeCodecsFingerprint};
pub(crate) use codecs::{RuntimeCodecRole, RuntimeCodecs};
pub(crate) use prepared::ProcessedSceneContent;

use std::{
    error::Error,
    fmt,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_asset::{AssetDomain, AssetProductRef, AssetRef, AssetRuntimeError, RuntimeAssetCodec};
use nara_core::ByteLimit;
use nara_fs::{ContentDigest, DirectoryCapability, FsError, RelativePath};
use nara_project::RuntimeConfigurationError;
use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
    },
    DeclaredAssetReferenceError,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneError, RuntimeSceneLimits,
};

use crate::{
    ProductRecipe,
    project_host::{
        RuntimeConfigurationPlan, RuntimeConfigurationPlanError, resolve_runtime_configuration,
    },
    startup_scene::SceneSource,
};
use budget::{DecodeBudget, DecodeLease};

pub use catalog::{
    RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogError,
    RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
    RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
};

/// The one catalog entry read relative to the Host-issued base directory.
pub const RUNTIME_CONTENT_CATALOG_FILE: &str = "catalog.json";

/// Independent Host expectations for one trusted local base.
///
/// These values must come from the selected build/output authority, not from the catalog being
/// opened. They establish local integrity and compatibility, not remote authenticity or freshness.
#[derive(Debug, Clone)]
pub struct RuntimeContentExpectation {
    catalog_digest: ContentDigest,
    target: RuntimeContentTarget,
}

impl RuntimeContentExpectation {
    #[must_use]
    pub const fn new(catalog_digest: ContentDigest, target: RuntimeContentTarget) -> Self {
        Self {
            catalog_digest,
            target,
        }
    }

    #[must_use]
    pub const fn catalog_digest(&self) -> ContentDigest {
        self.catalog_digest
    }

    #[must_use]
    pub const fn target(&self) -> &RuntimeContentTarget {
        &self.target
    }
}

/// Bounds owned by content capture and concurrent typed acquisition.
///
/// Encoded bytes include the catalog and all retained members. Retained bytes additionally count
/// the catalog's logical model, the base's indices and admitted decoder output. Work reservations
/// cover trusted codec promises and the Scene owner's allocation model. Compiled recipe/plugin
/// and Schema construction retain their existing owner limits; this is not allocator/RSS or an
/// interception of arbitrary native code allocations.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeContentLimits {
    catalog: RuntimeContentCatalogLimits,
    member_encoded_bytes: ByteLimit,
    total_encoded_bytes: ByteLimit,
    retained_bytes: ByteLimit,
    work_bytes: ByteLimit,
    scene: RuntimeSceneLimits,
}

impl Default for RuntimeContentLimits {
    fn default() -> Self {
        let bytes = |value| ByteLimit::new(value).expect("nonzero Runtime content limit");
        Self {
            catalog: RuntimeContentCatalogLimits::default(),
            member_encoded_bytes: bytes(128 * 1024 * 1024),
            total_encoded_bytes: bytes(512 * 1024 * 1024),
            retained_bytes: bytes(1024 * 1024 * 1024),
            work_bytes: bytes(512 * 1024 * 1024),
            scene: RuntimeSceneLimits::default(),
        }
    }
}

impl RuntimeContentLimits {
    #[must_use]
    pub const fn catalog(self) -> RuntimeContentCatalogLimits {
        self.catalog
    }
    #[must_use]
    pub const fn member_encoded_bytes(self) -> ByteLimit {
        self.member_encoded_bytes
    }
    #[must_use]
    pub const fn total_encoded_bytes(self) -> ByteLimit {
        self.total_encoded_bytes
    }
    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
    #[must_use]
    pub const fn work_bytes(self) -> ByteLimit {
        self.work_bytes
    }
    #[must_use]
    pub const fn scene(self) -> RuntimeSceneLimits {
        self.scene
    }
    #[must_use]
    pub const fn with_catalog(mut self, value: RuntimeContentCatalogLimits) -> Self {
        self.catalog = value;
        self
    }
    #[must_use]
    pub const fn with_member_encoded_bytes(mut self, value: ByteLimit) -> Self {
        self.member_encoded_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_total_encoded_bytes(mut self, value: ByteLimit) -> Self {
        self.total_encoded_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_retained_bytes(mut self, value: ByteLimit) -> Self {
        self.retained_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_work_bytes(mut self, value: ByteLimit) -> Self {
        self.work_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_scene(mut self, value: RuntimeSceneLimits) -> Self {
        self.scene = value;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeContentBudgetKind {
    MemberEncodedBytes,
    TotalEncodedBytes,
    RetainedBytes,
    WorkBytes,
}

#[derive(Debug)]
pub enum RuntimeContentError {
    Host(FsError),
    Catalog(RuntimeContentCatalogError),
    Configuration(RuntimeConfigurationError),
    Composition(RuntimeConfigurationPlanError),
    CatalogDigestMismatch,
    TargetMismatch,
    RecipeMismatch,
    SchemaMismatch,
    CodecsMismatch,
    MemberDigestMismatch,
    UndeclaredProduct,
    UndeclaredScene,
    UndeclaredDependency,
    NotScene,
    NotAsset,
    CodecNotSelected,
    CodecMismatch,
    CodecPanicked,
    ProductIdentityMismatch,
    NativeResourceUnavailable,
    TileSet(nara_tilemap::TileSetDescriptorError),
    Tilemap(nara_tilemap::TilemapMembershipError),
    Scene(RuntimeSceneError),
    SceneReference(DeclaredAssetReferenceError),
    Asset(AssetRuntimeError),
    Budget {
        kind: RuntimeContentBudgetKind,
        required: usize,
        limit: usize,
    },
    BudgetOverflow {
        kind: RuntimeContentBudgetKind,
    },
}

impl fmt::Display for RuntimeContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.summary())
    }
}

impl RuntimeContentError {
    pub(crate) const fn summary(&self) -> &'static str {
        match self {
            Self::Host(_) => "Runtime content capability read failed",
            Self::Catalog(_) => "Runtime content catalog was rejected",
            Self::Configuration(_) => "Runtime configuration was rejected",
            Self::Composition(_) => "Runtime executable composition was rejected",
            Self::CatalogDigestMismatch => "Runtime catalog differs from the Host expectation",
            Self::TargetMismatch => "Runtime catalog target differs from the Host expectation",
            Self::RecipeMismatch => "Runtime catalog differs from the executable recipe",
            Self::SchemaMismatch => "Runtime catalog differs from the executable Schema",
            Self::CodecsMismatch => "Runtime catalog differs from the executable codec selection",
            Self::MemberDigestMismatch => "Runtime member differs from its admitted catalog digest",
            Self::UndeclaredProduct => "Runtime product is absent from the admitted catalog",
            Self::UndeclaredScene => "Runtime Scene is not a declared acquisition root",
            Self::UndeclaredDependency => {
                "Runtime payload requires a Product absent from its required dependency declarations"
            }
            Self::NotScene => "Runtime product is not a Scene",
            Self::NotAsset => "Runtime product is not an asset member",
            Self::CodecNotSelected => {
                "Runtime codec is absent from the original executable selection"
            }
            Self::CodecMismatch => "Runtime member requires another compiled codec",
            Self::CodecPanicked => "Runtime codec panicked",
            Self::ProductIdentityMismatch => "Runtime product identity differs from its catalog",
            Self::NativeResourceUnavailable => {
                "Runtime resource requires an unavailable native product family"
            }
            Self::TileSet(_) => "Runtime TileSet descriptor does not match its actual Image",
            Self::Tilemap(_) => "Runtime Tilemap cells do not match their admitted TileSet",
            Self::Scene(_) => "Runtime Scene admission failed",
            Self::SceneReference(_) => "Runtime Scene asset-reference inventory failed",
            Self::Asset(_) => "Runtime asset decode failed",
            Self::Budget { .. } => "Runtime content budget exceeded",
            Self::BudgetOverflow { .. } => "Runtime content budget calculation overflowed",
        }
    }
}

impl Error for RuntimeContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::Configuration(error) => Some(error),
            Self::Composition(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::SceneReference(error) => Some(error),
            Self::Asset(error) => Some(error),
            Self::TileSet(error) => Some(error),
            Self::Tilemap(error) => Some(error),
            _ => None,
        }
    }
}

/// A complete immutable encoded base and its original executable composition.
///
/// Clones share captured bytes. Replacing files, opening another directory, or admitting another
/// base cannot redirect an existing acquisition. No filesystem capability remains in this owner.
#[derive(Clone)]
pub struct RuntimeContentBase {
    inner: Arc<RuntimeContentBaseInner>,
}

struct RuntimeContentBaseInner {
    catalog_digest: ContentDigest,
    catalog: RuntimeContentCatalog,
    plan: RuntimeConfigurationPlan,
    members: Box<[Vec<u8>]>,
    budget: Arc<DecodeBudget>,
    limits: RuntimeContentLimits,
}

/// Retention charged to the original base for first-party collection and residency metadata.
/// Values retain their own decode leases; this owner covers only the enclosing Host storage.
pub(crate) struct RuntimeContentLease {
    _lease: DecodeLease,
}

impl fmt::Debug for RuntimeContentBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeContentBase")
            .field("catalog_digest", &self.inner.catalog_digest)
            .field("members", &self.inner.members.len())
            .finish_non_exhaustive()
    }
}

impl RuntimeContentBase {
    /// Captures and verifies a trusted local base before any domain decode or App construction.
    ///
    /// The supplied Rust recipe is executable authority. The catalog only declares compatible
    /// configuration and fingerprints; it cannot select code or supply its own expected digest.
    pub fn open(
        directory: &DirectoryCapability,
        expected: &RuntimeContentExpectation,
        recipe: ProductRecipe,
        limits: RuntimeContentLimits,
    ) -> Result<Self, RuntimeContentError> {
        let catalog_limit = limits.catalog.encoded_bytes().get();
        let expected_length = usize::try_from(expected.catalog_digest.length()).map_err(|_| {
            budget_error(
                RuntimeContentBudgetKind::TotalEncodedBytes,
                usize::MAX,
                limits.total_encoded_bytes.get(),
            )
        })?;
        check(
            RuntimeContentBudgetKind::TotalEncodedBytes,
            expected_length,
            catalog_limit.min(limits.total_encoded_bytes.get()),
        )?;
        // Bound the raw input before opening it and pass the actual root residual into the
        // catalog owner. A small Host cap must constrain allocation, not merely reject a fully
        // decoded catalog afterwards. Recipe/Schema construction has its separate owner limits.
        check(
            RuntimeContentBudgetKind::RetainedBytes,
            expected_length,
            limits.retained_bytes.get(),
        )?;
        let catalog_retained = limits
            .retained_bytes
            .get()
            .checked_sub(expected_length)
            .and_then(ByteLimit::new)
            .ok_or_else(|| {
                budget_error(
                    RuntimeContentBudgetKind::RetainedBytes,
                    expected_length.saturating_add(1),
                    limits.retained_bytes.get(),
                )
            })?;
        let catalog_limits = limits
            .catalog
            .with_retained_bytes(
                ByteLimit::new(
                    catalog_retained
                        .get()
                        .min(limits.catalog.retained_bytes().get()),
                )
                .expect("nonzero admitted catalog residual"),
            )
            .with_working_bytes(
                ByteLimit::new(
                    limits
                        .work_bytes
                        .get()
                        .min(limits.catalog.working_bytes().get()),
                )
                .expect("nonzero catalog work bound"),
            );
        let path =
            RelativePath::new(RUNTIME_CONTENT_CATALOG_FILE).expect("fixed valid catalog path");
        let catalog_bytes = directory
            .open_file(&path)
            .map_err(RuntimeContentError::Host)?
            .read_to_end_bounded(expected.catalog_digest.length())
            .map_err(RuntimeContentError::Host)?;
        if ContentDigest::of_bytes(&catalog_bytes) != expected.catalog_digest {
            return Err(RuntimeContentError::CatalogDigestMismatch);
        }
        let catalog = RuntimeContentCatalog::from_json_bytes(&catalog_bytes, catalog_limits)
            .map_err(RuntimeContentError::Catalog)?;
        if catalog.target() != &expected.target {
            return Err(RuntimeContentError::TargetMismatch);
        }
        let configuration = catalog
            .configuration()
            .clone()
            .resolve()
            .map_err(RuntimeContentError::Configuration)?;
        let plan = resolve_runtime_configuration(&configuration, recipe)
            .map_err(RuntimeContentError::Composition)?;
        if plan.plugin_plan().fingerprint().as_bytes() != catalog.plugin_plan_fingerprint() {
            return Err(RuntimeContentError::RecipeMismatch);
        }
        if plan
            .schema_validation()
            .composition_fingerprint()
            .as_bytes()
            != catalog.schema_fingerprint()
        {
            return Err(RuntimeContentError::SchemaMismatch);
        }
        if plan.runtime_codecs_fingerprint().as_bytes() != catalog.runtime_codecs_fingerprint() {
            return Err(RuntimeContentError::CodecsMismatch);
        }
        for member in catalog.members() {
            plan.runtime_codecs().validate_member(member)?;
        }

        let mut encoded = catalog_bytes.len();
        for member in catalog.members() {
            let size = usize::try_from(member.digest().length()).map_err(|_| {
                budget_error(
                    RuntimeContentBudgetKind::MemberEncodedBytes,
                    usize::MAX,
                    limits.member_encoded_bytes.get(),
                )
            })?;
            check(
                RuntimeContentBudgetKind::MemberEncodedBytes,
                size,
                limits.member_encoded_bytes.get(),
            )?;
            encoded = checked_add(RuntimeContentBudgetKind::TotalEncodedBytes, encoded, size)?;
        }
        check(
            RuntimeContentBudgetKind::TotalEncodedBytes,
            encoded,
            limits.total_encoded_bytes.get(),
        )?;
        let member_bytes = catalog
            .members()
            .len()
            .checked_mul(size_of::<Vec<u8>>())
            .ok_or_else(|| {
                budget_error(
                    RuntimeContentBudgetKind::RetainedBytes,
                    usize::MAX,
                    limits.retained_bytes.get(),
                )
            })?;
        let retained = [
            encoded,
            catalog.retained_bytes(),
            member_bytes,
            size_of::<RuntimeContentBaseInner>(),
            2 * crate::scene_retention::ARC_CONTROL_BLOCK_BYTES,
        ]
        .into_iter()
        .try_fold(0, |total, bytes| {
            checked_add(RuntimeContentBudgetKind::RetainedBytes, total, bytes)
        })?;
        check(
            RuntimeContentBudgetKind::RetainedBytes,
            retained,
            limits.retained_bytes.get(),
        )?;

        let mut members = Vec::with_capacity(catalog.members().len());
        for member in catalog.members() {
            let path = RelativePath::new(member.path())
                .expect("catalog admits portable relative member paths");
            let bytes = directory
                .open_file(&path)
                .map_err(RuntimeContentError::Host)?
                .read_to_end_bounded(member.digest().length())
                .map_err(RuntimeContentError::Host)?;
            if ContentDigest::of_bytes(&bytes) != member.digest() {
                return Err(RuntimeContentError::MemberDigestMismatch);
            }
            members.push(bytes);
        }
        // The retained plan conservatively includes the raw catalog input. It is released here;
        // the parsed catalog and member snapshot remain the only content owners.
        drop(catalog_bytes);
        Ok(Self {
            inner: Arc::new(RuntimeContentBaseInner {
                catalog_digest: expected.catalog_digest,
                catalog,
                plan,
                members: members.into_boxed_slice(),
                budget: DecodeBudget::new(
                    retained,
                    limits.retained_bytes.get(),
                    limits.work_bytes.get(),
                ),
                limits,
            }),
        })
    }

    #[must_use]
    pub fn catalog_digest(&self) -> ContentDigest {
        self.inner.catalog_digest
    }
    #[must_use]
    pub fn catalog(&self) -> &RuntimeContentCatalog {
        &self.inner.catalog
    }
    #[must_use]
    pub fn plan(&self) -> &RuntimeConfigurationPlan {
        &self.inner.plan
    }

    pub(crate) fn same_base(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Admit Host-owned indices before allocating them, against this base's shared limit.
    pub(crate) fn reserve_retained(
        &self,
        bytes: usize,
    ) -> Result<RuntimeContentLease, RuntimeContentError> {
        Ok(RuntimeContentLease {
            _lease: self.inner.budget.reserve(bytes, 0)?,
        })
    }

    /// Inspects admitted membership and dependency metadata without decoding or making it resident.
    #[must_use]
    pub fn member(&self, product: AssetProductRef) -> Option<&RuntimeContentMember> {
        self.inner.catalog.member_for_product(product)
    }

    pub fn acquire_startup_scene(&self) -> Result<RuntimeContentScene, RuntimeContentError> {
        self.acquire_scene(self.inner.catalog.startup_scene())
    }

    /// Admits a declared startup or later Scene against this base's original exact definitions.
    pub fn acquire_scene(
        &self,
        product: AssetProductRef,
    ) -> Result<RuntimeContentScene, RuntimeContentError> {
        if !self.inner.catalog.scenes().contains(&product) {
            return Err(RuntimeContentError::UndeclaredScene);
        }
        self.acquire_scene_member(product)
    }

    /// Data membership does not grant the separate public Scene-root acquisition authority.
    fn acquire_scene_member(
        &self,
        product: AssetProductRef,
    ) -> Result<RuntimeContentScene, RuntimeContentError> {
        let index = self.member_index(product)?;
        let member = &self.inner.catalog.members()[index];
        let RuntimeContentMemberKind::Scene { source } = member.kind() else {
            return Err(RuntimeContentError::NotScene);
        };
        let scene_limits = self.inner.limits.scene;
        let work = scene_limits.working_bytes().get();
        // Scene's bounded allocation model includes its decoded candidate. Reserve that complete
        // promise before decode, then replace it with the actual checked retention charge.
        let metadata = size_of::<RuntimeContentSceneInner>()
            + 2 * crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;
        let promised = checked_add(RuntimeContentBudgetKind::RetainedBytes, work, metadata)?;
        let mut lease = self.inner.budget.reserve(promised, work)?;
        let candidate = RuntimeSceneDocumentCandidate::from_json_bytes(
            &self.inner.members[index],
            scene_limits,
        )
        .and_then(|candidate| {
            candidate.admit(source, self.inner.plan.schema_validation().bindings())
        })
        .map_err(RuntimeContentError::Scene)?;
        let retained = crate::scene_retention::runtime_startup_scene_retained_bytes(&candidate)
            .map_err(|_| {
                budget_error(
                    RuntimeContentBudgetKind::RetainedBytes,
                    usize::MAX,
                    self.inner.limits.retained_bytes.get(),
                )
            })?;
        let retained = checked_add(RuntimeContentBudgetKind::RetainedBytes, retained, metadata)?;
        lease.finish_decode(retained)?;
        self.validate_scene_dependencies(&candidate, member)?;
        Ok(RuntimeContentScene {
            inner: Arc::new(RuntimeContentSceneInner {
                candidate: Arc::new(candidate),
                _lease: lease,
                base: self.clone(),
                product,
                retained_bytes: retained,
            }),
        })
    }

    /// Decodes a concrete compiled format and verifies every Product mapped to that member.
    ///
    /// This operation grants no native-store publication. The resulting typed product retains
    /// its exact base and checked actual output reservation until its last shared owner is dropped.
    /// The complete maximum output/work promise is admitted before decode; unused output and
    /// temporary work are refunded only after the decoded value and its cost are validated.
    pub fn acquire_asset<D, C>(
        &self,
        product: AssetProductRef,
    ) -> Result<RuntimeContentAsset<D::Product>, RuntimeContentError>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
    {
        let index = self.member_index(product)?;
        let member = &self.inner.catalog.members()[index];
        let RuntimeContentMemberKind::Asset { format, version } = member.kind() else {
            return Err(RuntimeContentError::NotAsset);
        };
        if format.as_str() != C::RUNTIME_KIND || version.get() != C::RUNTIME_VERSION {
            return Err(RuntimeContentError::CodecMismatch);
        }
        let limits = self
            .inner
            .plan
            .runtime_codecs()
            .selected_limits::<D, C>(member)?;
        catch_unwind(AssertUnwindSafe(|| {
            check(
                RuntimeContentBudgetKind::MemberEncodedBytes,
                self.inner.members[index].len(),
                limits.encoded_bytes().get(),
            )?;
            check(
                RuntimeContentBudgetKind::RetainedBytes,
                size_of::<D::Product>(),
                limits.retained_bytes().get(),
            )?;
            // The codec accounts for the complete product. Only the enclosing wrapper and its
            // Arc control block are additional Host storage; do not count the inline value twice.
            let metadata = size_of::<RuntimeContentAssetInner<D::Product>>()
                - size_of::<D::Product>()
                + crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;
            let promised = checked_add(
                RuntimeContentBudgetKind::RetainedBytes,
                limits.retained_bytes().get(),
                metadata,
            )?;
            let mut lease = self
                .inner
                .budget
                .reserve(promised, limits.work_bytes().get())?;
            let value = C::decode_runtime(&self.inner.members[index])
                .map_err(RuntimeContentError::Asset)?;
            for reference in member.products() {
                if !D::contains_product(&value, reference.product())
                    || !C::contains_runtime_product(&value, *reference)
                {
                    return Err(RuntimeContentError::ProductIdentityMismatch);
                }
            }
            C::visit_required_runtime_products(&value, |dependency| {
                self.validate_required_dependency(member, dependency)
            })?;
            let product_retained =
                C::runtime_retained_bytes(&value).map_err(RuntimeContentError::Asset)?;
            check(
                RuntimeContentBudgetKind::RetainedBytes,
                size_of::<D::Product>(),
                product_retained,
            )?;
            check(
                RuntimeContentBudgetKind::RetainedBytes,
                product_retained,
                limits.retained_bytes().get(),
            )?;
            let retained = checked_add(
                RuntimeContentBudgetKind::RetainedBytes,
                product_retained,
                metadata,
            )?;
            lease.finish_decode(retained)?;
            Ok(RuntimeContentAsset {
                inner: Arc::new(RuntimeContentAssetInner {
                    product: value,
                    _lease: lease,
                    base: self.clone(),
                    reference: product,
                    retained_bytes: retained,
                }),
            })
        }))
        .unwrap_or(Err(RuntimeContentError::CodecPanicked))
    }

    fn member_index(&self, product: AssetProductRef) -> Result<usize, RuntimeContentError> {
        self.inner
            .catalog
            .member_index(product)
            .ok_or(RuntimeContentError::UndeclaredProduct)
    }

    fn validate_required_dependency(
        &self,
        member: &RuntimeContentMember,
        product: AssetProductRef,
    ) -> Result<(), RuntimeContentError> {
        self.member_index(product)?;
        if !member
            .dependencies()
            .binary_search_by_key(&product, RuntimeContentDependency::product)
            .is_ok_and(|index| {
                member.dependencies()[index].role() == RuntimeContentDependencyRole::Required
            })
        {
            return Err(RuntimeContentError::UndeclaredDependency);
        }
        Ok(())
    }

    fn validate_scene_dependencies(
        &self,
        candidate: &RuntimeSceneCandidate,
        member: &RuntimeContentMember,
    ) -> Result<(), RuntimeContentError> {
        let bindings = self.inner.plan.schema_validation().bindings();
        let mut work = self.inner.budget.reserve(0, 0)?;
        for entity in &candidate.document().entities {
            for (component, record) in &entity.components {
                // Runtime Scene admission has already proved this exact immutable selection.
                let schema = bindings
                    .schema(component)
                    .expect("admitted Runtime Scene uses its original definitions");
                let references = collect_declared_effective_asset_references_with_admission(
                    schema,
                    record.version,
                    &record.value,
                    candidate.limits().reference_traversal_limits(),
                    |_, _, allocation| work.admit_work(allocation),
                )
                .map_err(|error| match error {
                    AssetReferenceCollectionError::Reference(error) => {
                        RuntimeContentError::SceneReference(error)
                    }
                    AssetReferenceCollectionError::Admission(error) => error,
                })?;
                for reference in references {
                    let AssetRef::Product(product) = reference.asset_ref() else {
                        return Err(RuntimeContentError::ProductIdentityMismatch);
                    };
                    self.validate_required_dependency(member, *product)?;
                }
            }
        }
        Ok(())
    }
}

/// One typed decoded member, shared with its origin base and retained-output reservation.
pub struct RuntimeContentAsset<P> {
    inner: Arc<RuntimeContentAssetInner<P>>,
}
struct RuntimeContentAssetInner<P> {
    // Drop the actual product before releasing its reservation or immutable encoded base.
    product: P,
    _lease: DecodeLease,
    base: RuntimeContentBase,
    reference: AssetProductRef,
    retained_bytes: usize,
}
impl<P> Clone for RuntimeContentAsset<P> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<P> RuntimeContentAsset<P> {
    #[must_use]
    pub fn product(&self) -> &P {
        &self.inner.product
    }
    #[must_use]
    pub fn reference(&self) -> AssetProductRef {
        self.inner.reference
    }
    #[must_use]
    pub fn base(&self) -> &RuntimeContentBase {
        &self.inner.base
    }

    /// Complete modeled product and wrapper charge, shared by all clones of this acquisition.
    ///
    /// The immutable base has its own shared charge. This value describes the admitted retention
    /// model, not allocator capacity or process memory.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.inner.retained_bytes
    }
}
impl<P> fmt::Debug for RuntimeContentAsset<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeContentAsset")
            .field("reference", &self.inner.reference)
            .field("catalog_digest", &self.inner.base.catalog_digest())
            .finish_non_exhaustive()
    }
}

/// One admitted processed Scene, retaining its exact original base and definitions.
#[derive(Clone)]
pub struct RuntimeContentScene {
    inner: Arc<RuntimeContentSceneInner>,
}
struct RuntimeContentSceneInner {
    candidate: Arc<RuntimeSceneCandidate>,
    _lease: DecodeLease,
    base: RuntimeContentBase,
    product: AssetProductRef,
    retained_bytes: usize,
}
impl RuntimeContentScene {
    #[must_use]
    pub fn candidate(&self) -> &RuntimeSceneCandidate {
        &self.inner.candidate
    }
    #[must_use]
    pub fn product(&self) -> AssetProductRef {
        self.inner.product
    }
    #[must_use]
    pub fn base(&self) -> &RuntimeContentBase {
        &self.inner.base
    }
    /// Shares this complete retained owner with ordinary startup/current-Scene materialization.
    #[must_use]
    pub fn scene_source(&self) -> SceneSource {
        SceneSource::from_runtime_content(self.clone())
    }
    pub(crate) fn candidate_arc(&self) -> Arc<RuntimeSceneCandidate> {
        Arc::clone(&self.inner.candidate)
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.inner.retained_bytes
    }
}
impl fmt::Debug for RuntimeContentScene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeContentScene")
            .field("product", &self.inner.product)
            .field("catalog_digest", &self.inner.base.catalog_digest())
            .finish_non_exhaustive()
    }
}

fn checked_add(
    kind: RuntimeContentBudgetKind,
    left: usize,
    right: usize,
) -> Result<usize, RuntimeContentError> {
    left.checked_add(right)
        .ok_or(RuntimeContentError::BudgetOverflow { kind })
}
fn check(
    kind: RuntimeContentBudgetKind,
    required: usize,
    limit: usize,
) -> Result<(), RuntimeContentError> {
    if required > limit {
        Err(budget_error(kind, required, limit))
    } else {
        Ok(())
    }
}
fn budget_error(
    kind: RuntimeContentBudgetKind,
    required: usize,
    limit: usize,
) -> RuntimeContentError {
    RuntimeContentError::Budget {
        kind,
        required,
        limit,
    }
}
