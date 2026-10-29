//! Bounded version-one metadata for one immutable local Runtime content base.
//!
//! Catalog membership describes complete logical products, not source files or residency.
//! Several products may share one encoded member. Required edges are closed over this base;
//! soft edges are declarations only. Cycles are finite graph data and never cause traversal here.
//! Filesystem capture, trusted expected identity and concrete resource decoding belong to the Host.

use std::{
    cmp::Ordering,
    error::Error,
    fmt,
    io::{self, Write},
    mem::size_of,
};

use nara_asset::AssetProductRef;
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, FormatVersion, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeError, SerdeShapeLimits,
    SerdeShapePreflightError, decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_fs::{ContentDigest, RelativePath};
use nara_project::RuntimeConfiguration;
use nara_scene::{SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin};
use serde::{
    Deserialize, Serialize, Serializer,
    de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::SerializeSeq,
};

const FILE_KIND: &str = "runtime_content_catalog";
const RESERVED_CATALOG_PATH: &str = "catalog.json";
const MAX_MEMBER_PATH_BYTES: usize = 4096;

/// Executable target and selected build profile, independent of source project paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeContentTarget {
    triple: String,
    profile: String,
}

impl RuntimeContentTarget {
    pub const MAX_TRIPLE_BYTES: usize = 128;
    pub const MAX_PROFILE_BYTES: usize = 64;

    pub fn new(
        triple: impl Into<String>,
        profile: impl Into<String>,
    ) -> Result<Self, RuntimeContentCatalogError> {
        let value = Self {
            triple: triple.into(),
            profile: profile.into(),
        };
        value.validate()?;
        Ok(value)
    }

    #[must_use]
    pub fn triple(&self) -> &str {
        &self.triple
    }

    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    fn validate(&self) -> Result<(), RuntimeContentCatalogError> {
        for (field, value, maximum) in [
            ("triple", self.triple(), Self::MAX_TRIPLE_BYTES),
            ("profile", self.profile(), Self::MAX_PROFILE_BYTES),
        ] {
            if value.is_empty() || value.len() > maximum {
                return Err(RuntimeContentCatalogError::InvalidTarget {
                    field,
                    reason: "target coordinates must be nonempty bounded strings",
                });
            }
            if !value.as_bytes()[0].is_ascii_alphanumeric()
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(RuntimeContentCatalogError::InvalidTarget {
                    field,
                    reason: "target coordinates require ASCII identifiers without paths or whitespace",
                });
            }
        }
        Ok(())
    }
}

/// The representation selected by the producer for this complete encoded member.
///
/// A Scene member always contains exactly one processed `runtime_scene` version-one document.
/// Asset formats are admitted by the selected concrete Runtime codec, not by the catalog alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeContentMemberKind {
    Scene {
        source: SourceDocumentKey,
    },
    Asset {
        format: FormatKind,
        version: FormatVersion,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeContentDependencyRole {
    Required,
    Soft,
}

/// A member-level edge to one complete logical product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContentDependency {
    pub product: AssetProductRef,
    pub role: RuntimeContentDependencyRole,
}

impl RuntimeContentDependency {
    #[must_use]
    pub const fn new(product: AssetProductRef, role: RuntimeContentDependencyRole) -> Self {
        Self { product, role }
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub const fn role(&self) -> RuntimeContentDependencyRole {
        self.role
    }
}

/// One portable relative file and every product supplied by its concrete decoder.
#[derive(Debug, Clone)]
pub struct RuntimeContentMember {
    path: String,
    digest: ContentDigest,
    kind: RuntimeContentMemberKind,
    products: Vec<AssetProductRef>,
    dependencies: Vec<RuntimeContentDependency>,
}

impl RuntimeContentMember {
    pub fn new(
        path: impl Into<String>,
        digest: ContentDigest,
        kind: RuntimeContentMemberKind,
        products: Vec<AssetProductRef>,
        dependencies: Vec<RuntimeContentDependency>,
    ) -> Result<Self, RuntimeContentCatalogError> {
        let value = Self {
            path: path.into(),
            digest,
            kind,
            products,
            dependencies,
        };
        validate_member_path(&value.path)?;
        validate_member_kind(&value, 0)?;
        Ok(value)
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub const fn kind(&self) -> &RuntimeContentMemberKind {
        &self.kind
    }

    /// Returns complete product declarations in canonical identity order after catalog admission.
    #[must_use]
    pub fn products(&self) -> &[AssetProductRef] {
        &self.products
    }

    /// Returns dependencies sorted by complete Product identity after catalog admission.
    ///
    /// The catalog rejects a repeated Product even if its roles differ. Consumers of admitted
    /// members may use `binary_search_by_key` with [`RuntimeContentDependency::product`].
    #[must_use]
    pub fn dependencies(&self) -> &[RuntimeContentDependency] {
        &self.dependencies
    }
}

/// Admission limits, separate from the Host's encoded member and whole-base limits.
///
/// Work bytes are checked cumulative modeled ownership requests, including temporary wire and
/// index storage. Retained bytes count logical data and index ownership by length, not allocator
/// capacity or RSS. JSON syntax-adapter scratch remains bounded by encoded bytes and shape limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeContentCatalogLimits {
    encoded_bytes: ByteLimit,
    retained_bytes: ByteLimit,
    working_bytes: ByteLimit,
    members: ItemLimit,
    products: ItemLimit,
    dependencies: ItemLimit,
    scenes: ItemLimit,
    addressable_roots: ItemLimit,
    path_bytes: ByteLimit,
    shape: SerdeShapeLimits,
}

impl Default for RuntimeContentCatalogLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(16 * 1024 * 1024).expect("nonzero default"),
            retained_bytes: ByteLimit::new(128 * 1024 * 1024).expect("nonzero default"),
            working_bytes: ByteLimit::new(512 * 1024 * 1024).expect("nonzero default"),
            members: ItemLimit::new(16_384).expect("nonzero default"),
            products: ItemLimit::new(65_536).expect("nonzero default"),
            dependencies: ItemLimit::new(262_144).expect("nonzero default"),
            scenes: ItemLimit::new(16_384).expect("nonzero default"),
            addressable_roots: ItemLimit::new(65_536).expect("nonzero default"),
            path_bytes: ByteLimit::new(1024).expect("nonzero default"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(64).expect("nonzero default"),
                ItemLimit::new(2_000_000).expect("nonzero default"),
                ItemLimit::new(262_144).expect("nonzero default"),
                ByteLimit::new(16_384).expect("nonzero default"),
                ByteLimit::new(8 * 1024 * 1024).expect("nonzero default"),
            ),
        }
    }
}

macro_rules! catalog_limit {
    ($get:ident, $with:ident, $ty:ty) => {
        #[must_use]
        pub const fn $get(self) -> $ty {
            self.$get
        }

        #[must_use]
        pub const fn $with(mut self, value: $ty) -> Self {
            self.$get = value;
            self
        }
    };
}

impl RuntimeContentCatalogLimits {
    catalog_limit!(encoded_bytes, with_encoded_bytes, ByteLimit);
    catalog_limit!(retained_bytes, with_retained_bytes, ByteLimit);
    catalog_limit!(working_bytes, with_working_bytes, ByteLimit);
    catalog_limit!(members, with_members, ItemLimit);
    catalog_limit!(products, with_products, ItemLimit);
    catalog_limit!(dependencies, with_dependencies, ItemLimit);
    catalog_limit!(scenes, with_scenes, ItemLimit);
    catalog_limit!(addressable_roots, with_addressable_roots, ItemLimit);
    catalog_limit!(path_bytes, with_path_bytes, ByteLimit);
    catalog_limit!(shape, with_shape, SerdeShapeLimits);
}

#[derive(Debug)]
struct ProductIndexEntry {
    product: AssetProductRef,
    member: usize,
}

/// Untrusted named build input, without serialization or admitted-base authority.
///
/// All member and root collections are canonicalized and validated by the catalog constructor.
#[derive(Debug)]
pub struct RuntimeContentCatalogDefinition {
    pub target: RuntimeContentTarget,
    pub configuration: RuntimeConfiguration,
    pub plugin_plan_fingerprint: [u8; 32],
    pub schema_fingerprint: [u8; 32],
    pub startup_scene: AssetProductRef,
    pub scenes: Vec<AssetProductRef>,
    pub addressable_roots: Vec<AssetProductRef>,
    pub members: Vec<RuntimeContentMember>,
}

/// Fully validated immutable version-one catalog, without a public serde admission bypass.
///
/// The catalog cannot authorize its own target, digest, plugin plan or Schema. The receiving
/// Host compares these claims with trusted executable expectations before admitting the base.
/// The Runtime configuration remains a record; its owner admits it with `resolve` before planning.
#[derive(Debug)]
pub struct RuntimeContentCatalog {
    target: RuntimeContentTarget,
    configuration: RuntimeConfiguration,
    plugin_plan_fingerprint: [u8; 32],
    schema_fingerprint: [u8; 32],
    startup_scene: AssetProductRef,
    scenes: Vec<AssetProductRef>,
    addressable_roots: Vec<AssetProductRef>,
    members: Vec<RuntimeContentMember>,
    products: Vec<ProductIndexEntry>,
    limits: RuntimeContentCatalogLimits,
    retained_bytes: usize,
    modeled_work_bytes: usize,
    encoded_len: usize,
}

impl RuntimeContentCatalog {
    pub fn new(
        definition: RuntimeContentCatalogDefinition,
        limits: RuntimeContentCatalogLimits,
    ) -> Result<Self, RuntimeContentCatalogError> {
        Self::finish(definition, limits, WorkBudget::new(limits.working_bytes))
    }

    pub fn from_json_bytes(
        bytes: &[u8],
        limits: RuntimeContentCatalogLimits,
    ) -> Result<Self, RuntimeContentCatalogError> {
        let mut work = WorkBudget::new(limits.working_bytes);
        let contract = PersistentFileContract::canonical_v1(file_kind()?, engine_version()?);
        let envelope = decode_persistent_file_with_preflight(
            bytes,
            limits.encoded_bytes,
            &contract,
            |bytes| preflight(bytes, limits, &mut work),
            |bytes| serde_json::from_slice::<PersistentFileHeader>(bytes).map_err(json_error),
            |bytes| {
                serde_json::from_slice::<PersistentFileEnvelope<PayloadWire>>(bytes)
                    .map_err(json_error)
            },
        )
        .map_err(|error| match error {
            PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
                RuntimeContentCatalogError::BudgetExceeded {
                    budget: "encoded-bytes",
                    observed,
                    maximum,
                }
            }
            PersistentFileDecodeError::Shape(error)
            | PersistentFileDecodeError::Header(error)
            | PersistentFileDecodeError::Payload(error) => error,
            PersistentFileDecodeError::Contract(error) => {
                RuntimeContentCatalogError::Contract(error)
            }
        })?;
        let raw = envelope.into_payload();
        // The raw Vec can remain alive during conversion. Charge the destination request first.
        work.reserve(
            raw.members
                .len()
                .checked_mul(size_of::<RuntimeContentMember>()),
        )?;
        let members = raw
            .members
            .into_iter()
            .map(|member| RuntimeContentMember {
                path: member.path,
                digest: ContentDigest::from_parts(member.digest.length, member.digest.hash),
                kind: member.kind,
                products: member.products,
                dependencies: member.dependencies,
            })
            .collect();
        Self::finish(
            RuntimeContentCatalogDefinition {
                target: RuntimeContentTarget::new(raw.target.triple, raw.target.profile)?,
                configuration: raw.configuration,
                plugin_plan_fingerprint: raw.plugin_plan_fingerprint,
                schema_fingerprint: raw.schema_fingerprint,
                startup_scene: raw.startup_scene,
                scenes: raw.scenes,
                addressable_roots: raw.addressable_roots,
                members,
            },
            limits,
            work,
        )
    }

    /// Writes deterministic canonical metadata without copying its semantic payload.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, RuntimeContentCatalogError> {
        let mut work = WorkBudget::new(self.limits.working_bytes);
        work.reserve(Some(self.retained_bytes))?;
        self.encode(&mut work)
    }

    fn finish(
        definition: RuntimeContentCatalogDefinition,
        limits: RuntimeContentCatalogLimits,
        mut work: WorkBudget,
    ) -> Result<Self, RuntimeContentCatalogError> {
        let RuntimeContentCatalogDefinition {
            target,
            mut configuration,
            plugin_plan_fingerprint,
            schema_fingerprint,
            startup_scene,
            mut scenes,
            mut addressable_roots,
            mut members,
        } = definition;
        target.validate()?;
        check("members", members.len(), limits.members.get())?;
        check("scenes", scenes.len(), limits.scenes.get())?;
        check(
            "addressable-roots",
            addressable_roots.len(),
            limits.addressable_roots.get(),
        )?;
        check(
            "configuration-capabilities",
            configuration.capabilities.requested.len(),
            limits.shape.container_items().get(),
        )?;
        // Nonfinite native inputs would serialize as JSON null and could not round-trip as floats.
        // All remaining Runtime configuration semantics stay with its owning resolve boundary.
        if !configuration.runtime.time_scale.is_finite()
            || !configuration.window.scale_factor.is_finite()
        {
            return Err(RuntimeContentCatalogError::NonfiniteConfiguration);
        }

        let mut count = Counts::default();
        let mut retained = size_of::<Self>();
        let mut strings = 0_usize;
        for value in [
            target.triple(),
            target.profile(),
            configuration.window.title.as_str(),
        ] {
            admit_string(value, limits, &mut strings)?;
            add_bytes(&mut retained, Some(value.len()))?;
        }
        add_bytes(
            &mut retained,
            configuration
                .capabilities
                .requested
                .len()
                .checked_mul(size_of::<nara_project::ProductCapability>()),
        )?;
        add_bytes(
            &mut retained,
            scenes.len().checked_mul(size_of::<AssetProductRef>()),
        )?;
        add_bytes(
            &mut retained,
            addressable_roots
                .len()
                .checked_mul(size_of::<AssetProductRef>()),
        )?;
        add_bytes(
            &mut retained,
            members.len().checked_mul(size_of::<RuntimeContentMember>()),
        )?;
        for (index, member) in members.iter().enumerate() {
            validate_member_path(member.path())?;
            check("path-bytes", member.path.len(), limits.path_bytes.get())?;
            validate_member_kind(member, index)?;
            increment(
                &mut count.products,
                member.products.len(),
                "products",
                limits.products.get(),
            )?;
            increment(
                &mut count.dependencies,
                member.dependencies.len(),
                "dependencies",
                limits.dependencies.get(),
            )?;
            admit_string(member.path(), limits, &mut strings)?;
            add_bytes(&mut retained, Some(member.path.len()))?;
            add_bytes(
                &mut retained,
                member
                    .products
                    .len()
                    .checked_mul(size_of::<AssetProductRef>()),
            )?;
            add_bytes(
                &mut retained,
                member
                    .dependencies
                    .len()
                    .checked_mul(size_of::<RuntimeContentDependency>()),
            )?;
            match member.kind() {
                RuntimeContentMemberKind::Asset { format, .. } => {
                    admit_string(format.as_str(), limits, &mut strings)?;
                    add_bytes(&mut retained, Some(format.as_str().len()))?;
                }
                RuntimeContentMemberKind::Scene { source } => {
                    match source.origin() {
                        SourceDocumentOrigin::Named { name, .. } => {
                            admit_string(name, limits, &mut strings)?
                        }
                        SourceDocumentOrigin::ProjectLocation { path, .. } => {
                            admit_string(path.as_str(), limits, &mut strings)?
                        }
                        SourceDocumentOrigin::StableAsset { .. } => {}
                    }
                    add_bytes(
                        &mut retained,
                        source
                            .retained_bytes()
                            .checked_sub(size_of::<SourceDocumentKey>()),
                    )?;
                }
            }
        }
        // This is the only secondary semantic collection. The complete request and retained model
        // are admitted before allocating it. All canonicalization below uses allocation-free sorts.
        add_bytes(
            &mut retained,
            count.products.checked_mul(size_of::<ProductIndexEntry>()),
        )?;
        check("retained-bytes", retained, limits.retained_bytes.get())?;
        work.reserve(Some(retained))?;

        members.sort_unstable_by(|left, right| portable_path_cmp(left.path(), right.path()));
        scenes.sort_unstable();
        addressable_roots.sort_unstable();
        configuration.capabilities.requested.sort_unstable();
        let mut products = Vec::with_capacity(count.products);
        for (index, member) in members.iter_mut().enumerate() {
            member.products.sort_unstable();
            member
                .dependencies
                .sort_unstable_by_key(|dependency| dependency.product);
            for product in &member.products {
                products.push(ProductIndexEntry {
                    product: *product,
                    member: index,
                });
            }
        }
        products.sort_unstable_by_key(|entry| entry.product);
        let mut catalog = Self {
            target,
            configuration,
            plugin_plan_fingerprint,
            schema_fingerprint,
            startup_scene,
            scenes,
            addressable_roots,
            members,
            products,
            limits,
            retained_bytes: retained,
            modeled_work_bytes: 0,
            encoded_len: 0,
        };
        catalog.validate()?;
        // Native constructors also meet the wire limits, including configuration defaults and
        // identity records. Temporary output is charged before allocation and dropped at return.
        let bytes = catalog.encode(&mut work)?;
        preflight(&bytes, limits, &mut work)?;
        catalog.encoded_len = bytes.len();
        catalog.modeled_work_bytes = work.used;
        Ok(catalog)
    }

    /// Rechecks cross-member invariants without building a second graph or traversing cycles.
    pub(super) fn validate(&self) -> Result<(), RuntimeContentCatalogError> {
        for pair in self.products.windows(2) {
            if pair[0].product == pair[1].product {
                return Err(RuntimeContentCatalogError::DuplicateProduct {
                    product: pair[0].product,
                });
            }
        }
        for (index, member) in self.members.iter().enumerate() {
            if index > 0
                && portable_path_cmp(self.members[index - 1].path(), member.path())
                    == Ordering::Equal
            {
                return Err(RuntimeContentCatalogError::DuplicateMemberPath {
                    first: index - 1,
                    second: index,
                });
            }
            // An ancestor need not be adjacent: `a-b` sorts between `a` and `a/child`.
            for (offset, _) in member.path().match_indices('/') {
                if let Ok(ancestor) = self.members.binary_search_by(|candidate| {
                    portable_path_cmp(candidate.path(), &member.path()[..offset])
                }) {
                    return Err(RuntimeContentCatalogError::AncestorMemberPath {
                        ancestor,
                        descendant: index,
                    });
                }
            }
            for pair in member.dependencies.windows(2) {
                if pair[0].product == pair[1].product {
                    return Err(RuntimeContentCatalogError::DuplicateDependency {
                        member: index,
                        product: pair[0].product,
                    });
                }
            }
            for dependency in &member.dependencies {
                if dependency.role == RuntimeContentDependencyRole::Required
                    && self.member_for_product(dependency.product).is_none()
                {
                    return Err(RuntimeContentCatalogError::MissingRequiredProduct {
                        member: index,
                        product: dependency.product,
                    });
                }
            }
        }
        validate_unique_roots(&self.scenes, "scenes")?;
        validate_unique_roots(&self.addressable_roots, "addressable-roots")?;
        for product in &self.scenes {
            if !matches!(
                self.member_for_product(*product)
                    .map(RuntimeContentMember::kind),
                Some(RuntimeContentMemberKind::Scene { .. })
            ) {
                return Err(RuntimeContentCatalogError::InvalidSceneRoot { product: *product });
            }
        }
        if self.scenes.binary_search(&self.startup_scene).is_err() {
            return Err(RuntimeContentCatalogError::UndeclaredStartupScene {
                product: self.startup_scene,
            });
        }
        for product in &self.addressable_roots {
            if self.member_for_product(*product).is_none() {
                return Err(RuntimeContentCatalogError::MissingAddressableProduct {
                    product: *product,
                });
            }
        }
        Ok(())
    }

    fn encode(&self, work: &mut WorkBudget) -> Result<Vec<u8>, RuntimeContentCatalogError> {
        let envelope = PersistentFileEnvelope::canonical_v1(
            file_kind()?,
            engine_version()?,
            FormatGenerator::new("nara", engine_version()?)
                .map_err(|_| RuntimeContentCatalogError::Metadata)?,
            PayloadRef {
                target: &self.target,
                configuration: &self.configuration,
                plugin_plan_fingerprint: &self.plugin_plan_fingerprint,
                schema_fingerprint: &self.schema_fingerprint,
                startup_scene: &self.startup_scene,
                scenes: &self.scenes,
                addressable_roots: &self.addressable_roots,
                members: MembersRef(&self.members),
            },
        );
        let maximum = self.limits.encoded_bytes.get();
        let length = if self.encoded_len == 0 {
            let mut counter = EncodedWriter::new(None, maximum);
            serde_json::to_writer(&mut counter, &envelope)
                .map_err(|error| counter.failure(error))?;
            counter.written
        } else {
            self.encoded_len
        };
        work.reserve(Some(length))?;
        let mut writer = EncodedWriter::new(Some(Vec::with_capacity(length)), maximum);
        serde_json::to_writer(&mut writer, &envelope).map_err(|error| writer.failure(error))?;
        Ok(writer.bytes.unwrap_or_default())
    }

    #[must_use]
    pub const fn target(&self) -> &RuntimeContentTarget {
        &self.target
    }
    #[must_use]
    pub const fn configuration(&self) -> &RuntimeConfiguration {
        &self.configuration
    }
    #[must_use]
    pub const fn plugin_plan_fingerprint(&self) -> [u8; 32] {
        self.plugin_plan_fingerprint
    }
    #[must_use]
    pub const fn schema_fingerprint(&self) -> [u8; 32] {
        self.schema_fingerprint
    }
    #[must_use]
    pub const fn startup_scene(&self) -> AssetProductRef {
        self.startup_scene
    }
    #[must_use]
    pub fn scenes(&self) -> &[AssetProductRef] {
        &self.scenes
    }
    #[must_use]
    pub fn addressable_roots(&self) -> &[AssetProductRef] {
        &self.addressable_roots
    }
    #[must_use]
    pub fn members(&self) -> &[RuntimeContentMember] {
        &self.members
    }
    #[must_use]
    pub const fn limits(&self) -> RuntimeContentCatalogLimits {
        self.limits
    }
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    /// Cumulative modeled admission work, including transient wire and index requests.
    #[must_use]
    pub const fn modeled_work_bytes(&self) -> usize {
        self.modeled_work_bytes
    }
    /// Exact deterministic encoded length, already checked during construction.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    #[must_use]
    pub fn member_for_product(&self, product: AssetProductRef) -> Option<&RuntimeContentMember> {
        self.member_index(product).map(|index| &self.members[index])
    }

    /// Returns the canonical `members()` position using the catalog's sole Product index.
    pub(super) fn member_index(&self, product: AssetProductRef) -> Option<usize> {
        self.products
            .binary_search_by_key(&product, |entry| entry.product)
            .ok()
            .map(|index| self.products[index].member)
    }
}

#[derive(Debug)]
pub enum RuntimeContentCatalogError {
    BudgetExceeded {
        budget: &'static str,
        observed: usize,
        maximum: usize,
    },
    ArithmeticOverflow {
        budget: &'static str,
    },
    InvalidTarget {
        field: &'static str,
        reason: &'static str,
    },
    InvalidMemberPath {
        reason: &'static str,
    },
    InvalidSceneMember {
        member: usize,
        reason: &'static str,
    },
    EmptyMemberProducts {
        member: usize,
    },
    DuplicateMemberPath {
        first: usize,
        second: usize,
    },
    AncestorMemberPath {
        ancestor: usize,
        descendant: usize,
    },
    DuplicateProduct {
        product: AssetProductRef,
    },
    DuplicateDependency {
        member: usize,
        product: AssetProductRef,
    },
    MissingRequiredProduct {
        member: usize,
        product: AssetProductRef,
    },
    DuplicateRoot {
        root: &'static str,
        product: AssetProductRef,
    },
    InvalidSceneRoot {
        product: AssetProductRef,
    },
    UndeclaredStartupScene {
        product: AssetProductRef,
    },
    MissingAddressableProduct {
        product: AssetProductRef,
    },
    NonfiniteConfiguration,
    Shape(SerdeShapeError),
    Contract(PersistentFileContractError),
    Json(serde_json::Error),
    Metadata,
}

impl fmt::Display for RuntimeContentCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded {
                budget,
                observed,
                maximum,
            } => write!(
                formatter,
                "Runtime catalog {budget} is {observed}, maximum is {maximum}"
            ),
            Self::ArithmeticOverflow { budget } => {
                write!(formatter, "Runtime catalog {budget} arithmetic overflow")
            }
            Self::InvalidTarget { field, reason } => {
                write!(formatter, "Runtime catalog target {field}: {reason}")
            }
            Self::InvalidMemberPath { reason } => {
                write!(formatter, "Runtime catalog member path: {reason}")
            }
            Self::InvalidSceneMember { member, reason } => {
                write!(formatter, "Runtime catalog Scene member {member}: {reason}")
            }
            Self::EmptyMemberProducts { member } => write!(
                formatter,
                "Runtime catalog member {member} declares no products"
            ),
            Self::DuplicateMemberPath { first, second } => write!(
                formatter,
                "Runtime catalog members {first} and {second} alias the same portable path"
            ),
            Self::AncestorMemberPath {
                ancestor,
                descendant,
            } => write!(
                formatter,
                "Runtime catalog file {ancestor} is an ancestor of member {descendant}"
            ),
            Self::DuplicateProduct { product } => {
                write!(formatter, "Runtime catalog repeats product {product}")
            }
            Self::DuplicateDependency { member, product } => write!(
                formatter,
                "Runtime catalog member {member} repeats dependency {product}"
            ),
            Self::MissingRequiredProduct { member, product } => write!(
                formatter,
                "Runtime catalog member {member} requires absent product {product}"
            ),
            Self::DuplicateRoot { root, product } => write!(
                formatter,
                "Runtime catalog {root} repeats product {product}"
            ),
            Self::InvalidSceneRoot { product } => write!(
                formatter,
                "Runtime catalog Scene root {product} is absent or is not a Scene"
            ),
            Self::UndeclaredStartupScene { product } => write!(
                formatter,
                "Runtime catalog startup Scene {product} is not a declared Scene"
            ),
            Self::MissingAddressableProduct { product } => write!(
                formatter,
                "Runtime catalog addressable root {product} has no member"
            ),
            Self::NonfiniteConfiguration => formatter.write_str(
                "Runtime catalog configuration cannot encode nonfinite floating-point values",
            ),
            Self::Shape(error) => write!(formatter, "Runtime catalog wire shape: {error}"),
            Self::Contract(error) => {
                write!(formatter, "Runtime catalog persistent contract: {error}")
            }
            Self::Json(error) => write!(formatter, "Runtime catalog JSON: {error}"),
            Self::Metadata => {
                formatter.write_str("Runtime catalog engine format metadata is invalid")
            }
        }
    }
}

impl Error for RuntimeContentCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

fn validate_member_path(path: &str) -> Result<(), RuntimeContentCatalogError> {
    let refused = |reason| RuntimeContentCatalogError::InvalidMemberPath { reason };
    if path.is_empty() || path.len() > MAX_MEMBER_PATH_BYTES {
        return Err(refused("portable paths must contain 1..=4096 bytes"));
    }
    // A deliberately ASCII transport grammar avoids platform case and Unicode-normalization
    // aliases. Display names remain unconstrained by these encoded-member transport paths.
    if !path.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.' | b' ')
    }) {
        return Err(refused(
            "portable paths use ASCII letters, digits, spaces, '/', '_', '-' and '.' only",
        ));
    }
    for (index, segment) in path.split('/').enumerate() {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.len() > 255
            || segment.starts_with(' ')
            || segment.ends_with(' ')
            || segment.ends_with('.')
        {
            return Err(refused(
                "path segments must be nonempty portable filenames without dot traversal or trailing dots/spaces",
            ));
        }
        if index == 0 && segment.eq_ignore_ascii_case(RESERVED_CATALOG_PATH) {
            return Err(refused(
                "catalog.json is reserved and cannot be a member or member ancestor",
            ));
        }
        let stem = segment
            .split('.')
            .next()
            .unwrap_or(segment)
            .trim_end_matches(' ');
        if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
            .iter()
            .any(|reserved| stem.eq_ignore_ascii_case(reserved))
            || (stem.len() == 4
                && (stem[..3].eq_ignore_ascii_case("COM") || stem[..3].eq_ignore_ascii_case("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(refused(
                "portable paths cannot contain reserved device filenames",
            ));
        }
    }
    RelativePath::preflight(path)
        .map(|_| ())
        .map_err(|_| refused("member paths must satisfy filesystem relative-path limits"))
}

fn validate_member_kind(
    member: &RuntimeContentMember,
    index: usize,
) -> Result<(), RuntimeContentCatalogError> {
    if member.products.is_empty() {
        return Err(RuntimeContentCatalogError::EmptyMemberProducts { member: index });
    }
    if let RuntimeContentMemberKind::Scene { source } = member.kind() {
        let invalid = |reason| RuntimeContentCatalogError::InvalidSceneMember {
            member: index,
            reason,
        };
        if member.products.len() != 1 || source.kind() != SourceDocumentKind::Scene {
            return Err(invalid(
                "a processed Scene requires one product and a canonical Scene source key",
            ));
        }
        if let SourceDocumentOrigin::StableAsset { stable_id } = source.origin() {
            if stable_id.as_uuid().is_nil() || *stable_id != member.products[0].source() {
                return Err(invalid(
                    "a stable Scene source must match its product's non-nil enclosing source",
                ));
            }
        }
    }
    Ok(())
}

fn portable_path_cmp(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn validate_unique_roots(
    roots: &[AssetProductRef],
    root: &'static str,
) -> Result<(), RuntimeContentCatalogError> {
    for pair in roots.windows(2) {
        if pair[0] == pair[1] {
            return Err(RuntimeContentCatalogError::DuplicateRoot {
                root,
                product: pair[0],
            });
        }
    }
    Ok(())
}

fn check(
    budget: &'static str,
    observed: usize,
    maximum: usize,
) -> Result<(), RuntimeContentCatalogError> {
    if observed > maximum {
        Err(RuntimeContentCatalogError::BudgetExceeded {
            budget,
            observed,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn increment(
    value: &mut usize,
    amount: usize,
    budget: &'static str,
    maximum: usize,
) -> Result<(), RuntimeContentCatalogError> {
    *value = value
        .checked_add(amount)
        .ok_or(RuntimeContentCatalogError::ArithmeticOverflow { budget })?;
    check(budget, *value, maximum)
}

fn add_bytes(value: &mut usize, bytes: Option<usize>) -> Result<(), RuntimeContentCatalogError> {
    let bytes = bytes.ok_or(RuntimeContentCatalogError::ArithmeticOverflow {
        budget: "retained-bytes",
    })?;
    *value = value
        .checked_add(bytes)
        .ok_or(RuntimeContentCatalogError::ArithmeticOverflow {
            budget: "retained-bytes",
        })?;
    Ok(())
}

fn admit_string(
    value: &str,
    limits: RuntimeContentCatalogLimits,
    total: &mut usize,
) -> Result<(), RuntimeContentCatalogError> {
    check(
        "string-bytes",
        value.len(),
        limits.shape.string_bytes().get(),
    )?;
    increment(
        total,
        value.len(),
        "total-string-bytes",
        limits.shape.total_string_bytes().get(),
    )
}

struct WorkBudget {
    used: usize,
    maximum: usize,
}

impl WorkBudget {
    fn new(maximum: ByteLimit) -> Self {
        Self {
            used: 0,
            maximum: maximum.get(),
        }
    }

    fn reserve(&mut self, bytes: Option<usize>) -> Result<(), RuntimeContentCatalogError> {
        let bytes = bytes.ok_or(RuntimeContentCatalogError::ArithmeticOverflow {
            budget: "working-bytes",
        })?;
        increment(&mut self.used, bytes, "working-bytes", self.maximum)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetWire {
    triple: String,
    profile: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestWire {
    length: u64,
    hash: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberWire {
    path: String,
    digest: DigestWire,
    kind: RuntimeContentMemberKind,
    products: Vec<AssetProductRef>,
    dependencies: Vec<RuntimeContentDependency>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadWire {
    target: TargetWire,
    configuration: RuntimeConfiguration,
    plugin_plan_fingerprint: [u8; 32],
    schema_fingerprint: [u8; 32],
    startup_scene: AssetProductRef,
    scenes: Vec<AssetProductRef>,
    addressable_roots: Vec<AssetProductRef>,
    members: Vec<MemberWire>,
}

#[derive(Serialize)]
struct PayloadRef<'a> {
    target: &'a RuntimeContentTarget,
    configuration: &'a RuntimeConfiguration,
    plugin_plan_fingerprint: &'a [u8; 32],
    schema_fingerprint: &'a [u8; 32],
    startup_scene: &'a AssetProductRef,
    scenes: &'a [AssetProductRef],
    addressable_roots: &'a [AssetProductRef],
    members: MembersRef<'a>,
}

struct MembersRef<'a>(&'a [RuntimeContentMember]);

impl Serialize for MembersRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct MemberRef<'a> {
            path: &'a str,
            digest: DigestWire,
            kind: &'a RuntimeContentMemberKind,
            products: &'a [AssetProductRef],
            dependencies: &'a [RuntimeContentDependency],
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for member in self.0 {
            sequence.serialize_element(&MemberRef {
                path: member.path(),
                digest: DigestWire {
                    length: member.digest.length(),
                    hash: *member.digest.as_bytes(),
                },
                kind: member.kind(),
                products: member.products(),
                dependencies: member.dependencies(),
            })?;
        }
        sequence.end()
    }
}

struct EncodedWriter {
    bytes: Option<Vec<u8>>,
    written: usize,
    maximum: usize,
    exceeded: Option<usize>,
}

impl EncodedWriter {
    fn new(bytes: Option<Vec<u8>>, maximum: usize) -> Self {
        Self {
            bytes,
            written: 0,
            maximum,
            exceeded: None,
        }
    }

    fn failure(&self, error: serde_json::Error) -> RuntimeContentCatalogError {
        match self.exceeded {
            Some(observed) => RuntimeContentCatalogError::BudgetExceeded {
                budget: "encoded-bytes",
                observed,
                maximum: self.maximum,
            },
            None => json_error(error),
        }
    }
}

impl Write for EncodedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let observed = self.written.checked_add(bytes.len()).unwrap_or(usize::MAX);
        if observed > self.maximum {
            self.exceeded = Some(observed);
            return Err(io::Error::other(
                "Runtime catalog encoded byte limit exceeded",
            ));
        }
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(bytes);
        }
        self.written = observed;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn file_kind() -> Result<FormatKind, RuntimeContentCatalogError> {
    FormatKind::new(FILE_KIND).map_err(|_| RuntimeContentCatalogError::Metadata)
}

fn engine_version() -> Result<EngineVersion, RuntimeContentCatalogError> {
    EngineVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| RuntimeContentCatalogError::Metadata)
}

fn json_error(error: serde_json::Error) -> RuntimeContentCatalogError {
    RuntimeContentCatalogError::Json(error)
}

#[derive(Default)]
struct Counts {
    members: usize,
    products: usize,
    dependencies: usize,
    scenes: usize,
    addressable_roots: usize,
}

#[derive(Clone, Copy)]
enum Location {
    Envelope,
    Payload,
    Members,
    Member,
    Products,
    Product,
    Dependencies,
    Dependency,
    Scenes,
    Scene,
    AddressableRoots,
    AddressableRoot,
    MemberPath,
    Record,
    Other,
}

impl Location {
    fn field(self, name: &str) -> Self {
        match (self, name) {
            (Self::Envelope, "payload") => Self::Payload,
            (Self::Envelope, "generator") => Self::Record,
            (Self::Payload, "members") => Self::Members,
            (Self::Payload, "scenes") => Self::Scenes,
            (Self::Payload, "addressable_roots") => Self::AddressableRoots,
            (Self::Payload, "target" | "configuration") => Self::Record,
            (Self::Member, "products") => Self::Products,
            (Self::Member, "dependencies") => Self::Dependencies,
            (Self::Member, "path") => Self::MemberPath,
            (Self::Member, "digest" | "kind") => Self::Record,
            _ => Self::Other,
        }
    }

    fn element(self) -> Self {
        match self {
            Self::Members => Self::Member,
            Self::Products => Self::Product,
            Self::Dependencies => Self::Dependency,
            Self::Scenes => Self::Scene,
            Self::AddressableRoots => Self::AddressableRoot,
            _ => Self::Other,
        }
    }
}

fn preflight(
    bytes: &[u8],
    limits: RuntimeContentCatalogLimits,
    work: &mut WorkBudget,
) -> Result<(), RuntimeContentCatalogError> {
    // Admit modeled raw-decode and key-set work with an allocation-free walk first. The following
    // core pass then rejects duplicate keys before serde can collapse or ignore any field.
    let mut state = RawState {
        limits,
        work,
        counts: Counts::default(),
        nodes: 0,
        strings: 0,
        failure: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = RawSeed {
        state: &mut state,
        location: Location::Envelope,
        depth: 1,
    }
    .deserialize(&mut deserializer);
    if let Some(error) = state.failure {
        return Err(error);
    }
    result.map_err(json_error)?;
    deserializer.end().map_err(json_error)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits.shape).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => RuntimeContentCatalogError::Shape(error),
        SerdeShapePreflightError::Parse(error) => json_error(error),
    })?;
    deserializer.end().map_err(json_error)
}

struct RawState<'a> {
    limits: RuntimeContentCatalogLimits,
    work: &'a mut WorkBudget,
    counts: Counts,
    nodes: usize,
    strings: usize,
    failure: Option<RuntimeContentCatalogError>,
}

impl RawState<'_> {
    fn enforce<E: de::Error>(
        &mut self,
        result: Result<(), RuntimeContentCatalogError>,
    ) -> Result<(), E> {
        result.map_err(|error| {
            self.failure = Some(error);
            E::custom("Runtime catalog pre-allocation admission refused")
        })
    }

    fn node<E: de::Error>(&mut self, location: Location, depth: usize) -> Result<(), E> {
        let result = check("wire-depth", depth, self.limits.shape.depth().get());
        self.enforce(result)?;
        let result = increment(
            &mut self.nodes,
            1,
            "wire-nodes",
            self.limits.shape.nodes().get(),
        );
        self.enforce(result)?;
        let result = match location {
            Location::Member => increment(
                &mut self.counts.members,
                1,
                "members",
                self.limits.members.get(),
            ),
            Location::Product => increment(
                &mut self.counts.products,
                1,
                "products",
                self.limits.products.get(),
            ),
            Location::Dependency => increment(
                &mut self.counts.dependencies,
                1,
                "dependencies",
                self.limits.dependencies.get(),
            ),
            Location::Scene => increment(
                &mut self.counts.scenes,
                1,
                "scenes",
                self.limits.scenes.get(),
            ),
            Location::AddressableRoot => increment(
                &mut self.counts.addressable_roots,
                1,
                "addressable-roots",
                self.limits.addressable_roots.get(),
            ),
            _ => Ok(()),
        };
        self.enforce(result)?;
        // Conservative ownership request for raw nodes, Vec growth, internally tagged enum
        // buffers and core duplicate-key set nodes. This is a model, not allocator measurement.
        let result = self.work.reserve(Some(192));
        self.enforce(result)
    }

    fn string<E: de::Error>(&mut self, value: &str, location: Location) -> Result<(), E> {
        let result = admit_string(value, self.limits, &mut self.strings);
        self.enforce(result)?;
        if matches!(location, Location::MemberPath) {
            let result = check("path-bytes", value.len(), self.limits.path_bytes.get());
            self.enforce(result)?;
        }
        let result = self.work.reserve(value.len().checked_mul(2));
        self.enforce(result)
    }

    fn items<E: de::Error>(&mut self, value: &mut usize) -> Result<(), E> {
        let result = increment(
            value,
            1,
            "wire-container-items",
            self.limits.shape.container_items().get(),
        );
        self.enforce(result)
    }
}

struct RawSeed<'a, 'work> {
    state: &'a mut RawState<'work>,
    location: Location,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for RawSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.state.node(self.location, self.depth)?;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for RawSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded Runtime catalog JSON")
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.state.string(value, self.location)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        if matches!(
            self.location,
            Location::Envelope
                | Location::Payload
                | Location::Member
                | Location::Dependency
                | Location::Record
        ) {
            return Err(de::Error::custom(
                "Runtime catalog records require JSON objects, not positional arrays",
            ));
        }
        let mut items = 0;
        while sequence
            .next_element_seed(RawSeed {
                state: self.state,
                location: self.location.element(),
                depth: self.depth.saturating_add(1),
            })?
            .is_some()
        {
            self.state.items(&mut items)?;
        }
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut items = 0;
        while let Some(location) = map.next_key_seed(RawKey {
            state: self.state,
            parent: self.location,
            depth: self.depth.saturating_add(1),
        })? {
            self.state.items(&mut items)?;
            map.next_value_seed(RawSeed {
                state: self.state,
                location,
                depth: self.depth.saturating_add(1),
            })?;
        }
        Ok(())
    }
}

struct RawKey<'a, 'work> {
    state: &'a mut RawState<'work>,
    parent: Location,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for RawKey<'_, '_> {
    type Value = Location;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Location, D::Error> {
        self.state.node(Location::Other, self.depth)?;
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for RawKey<'_, '_> {
    type Value = Location;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Runtime catalog object key")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Location, E> {
        self.state.string(value, Location::Other)?;
        Ok(self.parent.field(value))
    }
}

#[cfg(test)]
mod tests;
