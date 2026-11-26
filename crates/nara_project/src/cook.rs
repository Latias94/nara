//! Assembly of cooked Scene and asset members for one selected project delivery.
//!
//! The caller supplies the executable's selected Schema, codecs and strict runtime admission.
//! Assembly keeps the original verified group bytes until admission and validation finish.
//! Build input freshness and package publication are separate operations.

mod budget;

use std::{fmt::Write as _, mem::size_of};

use nara_asset::{
    AssetDomain, AssetProductRef, AssetRef, AssetReferenceDemand, AssetReferenceResolver,
    AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec, artifact::VerifiedArtifactGroup,
};
use nara_core::vfs::capabilities::ContentDigest;
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_reflect::{
    ComponentTypeId, DeclaredAssetReference, DeclaredAssetReferenceError,
    asset_reference::{
        AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
    },
};
use nara_scene::retention::runtime_startup_scene_retained_bytes;
use nara_scene::runtime_catalog::{
    RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogError,
    RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
    RuntimeContentExpectation, RuntimeContentMember, RuntimeContentMemberKind,
    RuntimeContentTarget,
};
use nara_scene::runtime_configuration::RuntimeConfiguration;
use nara_scene::{
    CompiledSceneCandidate, RuntimeSceneCandidate, RuntimeSceneError, RuntimeSceneLimits,
    SceneComponentBindings,
};
use thiserror::Error;

use budget::{add, check, member_heap_bytes, mul, request_heap_bytes};

/// The complete selected delivery roots. `scenes` includes `startup_scene`.
///
/// Scene dependencies may supply additional Scene data without granting public travel authority.
/// Required and Reference dependencies both belong to delivery, including Deferred destinations.
#[derive(Debug, Clone)]
pub struct CookedContentRequest {
    pub target: RuntimeContentTarget,
    pub configuration: RuntimeConfiguration,
    pub startup_scene: AssetProductRef,
    pub scenes: Vec<AssetProductRef>,
    pub addressable_roots: Vec<AssetProductRef>,
}

/// Logical assembly bounds and the caller's executable admission ceilings.
#[derive(Debug, Clone, Copy)]
pub struct CookedContentAssemblyLimits {
    pub catalog: RuntimeContentCatalogLimits,
    pub scene: RuntimeSceneLimits,
    pub member_encoded_bytes: ByteLimit,
    pub total_encoded_bytes: ByteLimit,
    pub runtime_retained_bytes: ByteLimit,
    pub runtime_work_bytes: ByteLimit,
    pub work_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
}

/// Fingerprints of the selected executable composition, copied into the canonical catalog.
#[derive(Debug, Clone, Copy)]
pub struct CookedContentFingerprints {
    pub plugin_plan: [u8; 32],
    pub schema: [u8; 32],
    pub runtime_codecs: [u8; 32],
}

#[derive(Debug, Error)]
pub enum CookedContentAssemblyError {
    #[error(transparent)]
    Catalog(#[from] RuntimeContentCatalogError),
    #[error(transparent)]
    Scene(#[from] RuntimeSceneError),
    #[error(transparent)]
    SceneReference(#[from] DeclaredAssetReferenceError),
    #[error(transparent)]
    Asset(#[from] AssetRuntimeError),
    #[error("duplicate cooked Product {0}")]
    DuplicateProduct(AssetProductRef),
    #[error("duplicate {root} root {product}")]
    DuplicateRoot {
        root: &'static str,
        product: AssetProductRef,
    },
    #[error("startup Scene must be a declared Scene root")]
    UndeclaredStartupScene,
    #[error("cooked content requires complete Product references")]
    IncompleteProductReference,
    #[error("verified group has inconsistent retained membership")]
    InvalidGroup,
    #[error("the Runtime codec has an invalid format declaration")]
    InvalidCodec,
    #[error("only {reachable} of {total} cooked members belong to the selected delivery closure")]
    UnreachableMembers { reachable: usize, total: usize },
    #[error("cooked content {kind} requires {required} bytes/items, limit {limit}")]
    Budget {
        kind: &'static str,
        required: usize,
        limit: usize,
    },
    #[error("cooked content ownership accounting overflowed")]
    Overflow,
}

#[derive(Debug)]
enum MemberBytes {
    Owned(Vec<u8>),
    Group { group: usize, member: usize },
}

#[derive(Debug)]
struct PendingMember {
    member: RuntimeContentMember,
    bytes: MemberBytes,
}

/// Move-only assembly using one caller-selected executable configuration.
///
/// Each `add_*` is transactional: errors leave all prior members and reservations unchanged.
/// A complete group is added atomically. `finish` consumes the builder, so a failed final check
/// drops the complete unpublished candidate and cannot leave a partially prepared result.
#[derive(Debug)]
pub struct CookedContentAssembly {
    request: CookedContentRequest,
    limits: CookedContentAssemblyLimits,
    members: Vec<PendingMember>,
    groups: Vec<VerifiedArtifactGroup>,
    retained_bytes: usize,
    encoded_bytes: usize,
    consumer_bytes: usize,
    consumer_work_bytes: usize,
}

impl CookedContentAssembly {
    /// Counts the inline executable state held beside the assembly in the caller's builder.
    pub fn new(
        mut request: CookedContentRequest,
        limits: CookedContentAssemblyLimits,
        executable_bytes: usize,
    ) -> Result<Self, CookedContentAssemblyError> {
        let catalog = limits.catalog;
        check("Scene-roots", request.scenes.len(), catalog.scenes().get())?;
        check(
            "addressable-roots",
            request.addressable_roots.len(),
            catalog.addressable_roots().get(),
        )?;
        check(
            "configuration-capabilities",
            request.configuration.capabilities.requested.len(),
            catalog.shape().container_items().get(),
        )?;
        let retained_bytes = add(
            add(size_of::<Self>(), executable_bytes)?,
            request_heap_bytes(&request)?,
        )?;
        check(
            "retained-bytes",
            retained_bytes,
            limits.retained_bytes.get(),
        )?;
        // Effective settings borrow this bounded record. Recipe/Schema construction has the
        // same independent composition limits as the receiving Runtime owner.
        check(
            "work-bytes",
            add(retained_bytes, request_heap_bytes(&request)?)?,
            limits.work_bytes.get(),
        )?;
        for (root, products) in [
            ("scenes", &mut request.scenes),
            ("addressable-roots", &mut request.addressable_roots),
        ] {
            products.sort_unstable();
            for pair in products.windows(2) {
                if pair[0] == pair[1] {
                    return Err(CookedContentAssemblyError::DuplicateRoot {
                        root,
                        product: pair[0],
                    });
                }
            }
        }
        if request
            .scenes
            .binary_search(&request.startup_scene)
            .is_err()
        {
            return Err(CookedContentAssemblyError::UndeclaredStartupScene);
        }
        Ok(Self {
            request,
            limits,
            members: Vec::new(),
            groups: Vec::new(),
            retained_bytes,
            encoded_bytes: 0,
            consumer_bytes: 0,
            consumer_work_bytes: 0,
        })
    }

    #[must_use]
    pub const fn configuration(&self) -> &RuntimeConfiguration {
        &self.request.configuration
    }

    /// Current logical retention, including the caller's reserved metadata.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Declares an admitted Scene root during sequential source traversal.
    pub fn declare_scene_root(
        &mut self,
        product: AssetProductRef,
    ) -> Result<(), CookedContentAssemblyError> {
        let index = match self.request.scenes.binary_search(&product) {
            Ok(_) => return Ok(()),
            Err(index) => index,
        };
        let count = add(self.request.scenes.len(), 1)?;
        check("Scene-roots", count, self.limits.catalog.scenes().get())?;
        let retained_bytes = add(self.retained_bytes, size_of::<AssetProductRef>())?;
        check(
            "retained-bytes",
            retained_bytes,
            self.limits.retained_bytes.get(),
        )?;
        self.admit_work(mul(count, size_of::<AssetProductRef>())?)?;
        let mut scenes = Vec::with_capacity(count);
        scenes.extend_from_slice(&self.request.scenes[..index]);
        scenes.push(product);
        scenes.extend_from_slice(&self.request.scenes[index..]);
        self.request.scenes = scenes;
        self.retained_bytes = retained_bytes;
        Ok(())
    }

    /// Borrows the actual dependencies in assembly order, which is stable until `finish`.
    pub fn pending_dependencies(&self, index: usize) -> Option<&[RuntimeContentDependency]> {
        self.members
            .get(index)
            .map(|pending| pending.member.dependencies())
    }

    pub fn add_scene(
        &mut self,
        product: AssetProductRef,
        compiled: &CompiledSceneCandidate,
        bindings: &SceneComponentBindings,
        asset_resolution: AssetReferenceResolver<'_>,
        omitted: &[ComponentTypeId],
    ) -> Result<(), CookedContentAssemblyError> {
        self.require_new_product(product)?;
        self.check_member_path(product)?;
        let scene_limits = self.limits.scene;
        self.admit_work(scene_limits.working_bytes().get())?;
        let candidate = RuntimeSceneCandidate::cook_with_component_omissions(
            compiled,
            bindings,
            asset_resolution,
            omitted,
            scene_limits,
        )?;
        let candidate_bytes = runtime_startup_scene_retained_bytes(&candidate)
            .map_err(|_| CookedContentAssemblyError::Overflow)?;
        // Encoding and the effective reference inventory overlap the actual cooked candidate.
        self.admit_work(add(candidate_bytes, scene_limits.working_bytes().get())?)?;
        let bytes = candidate.to_json_bytes()?;
        self.check_member_bytes(bytes.len())?;
        let fixed = add(candidate_bytes, bytes.len())?;
        let components = candidate
            .document()
            .entities()
            .iter()
            .try_fold(0, |total, entity| add(total, entity.components().len()))?;
        let mut work = add(
            fixed,
            mul(components, size_of::<Vec<DeclaredAssetReference>>())?,
        )?;
        self.admit_work(work)?;
        let mut inventories = Vec::with_capacity(components);
        let mut references_count = 0;
        for entity in candidate.document().entities() {
            for (component, record) in entity.components() {
                let schema = bindings
                    .schema(component)
                    .expect("cooked Scene uses this builder's original admitted definitions");
                let references = collect_declared_effective_asset_references_with_admission(
                    schema,
                    record.version(),
                    record.value(),
                    scene_limits.reference_traversal_limits(),
                    |_, _, allocation| {
                        work = add(work, allocation)?;
                        self.admit_work(work)
                    },
                )
                .map_err(|error| match error {
                    AssetReferenceCollectionError::Admission(error) => error,
                    AssetReferenceCollectionError::Reference(error) => {
                        CookedContentAssemblyError::SceneReference(error)
                    }
                })?;
                references_count = add(references_count, references.len())?;
                inventories.push(references);
            }
        }
        // One table for actual effective uses, followed by allocation-free normalization. There
        // is no second selector walker and no per-reference table growth or guessed capacity.
        work = add(
            work,
            mul(references_count, size_of::<RuntimeContentDependency>())?,
        )?;
        self.admit_work(work)?;
        let mut dependencies = Vec::with_capacity(references_count);
        for references in inventories {
            for reference in references {
                let AssetRef::Product(product) = reference.asset_ref() else {
                    return Err(CookedContentAssemblyError::IncompleteProductReference);
                };
                dependencies.push(RuntimeContentDependency::new(
                    *product,
                    dependency_role(reference.demand()),
                ));
            }
        }
        dependencies.sort_unstable_by_key(|dependency| (dependency.product, dependency.role));
        dependencies.dedup_by_key(|dependency| dependency.product);
        check(
            "dependencies",
            dependencies.len(),
            self.limits.catalog.dependencies().get(),
        )?;
        if dependencies.len() != references_count {
            // Shrinking may allocate a replacement. Admit both tables before the old one drops.
            work = add(
                work,
                mul(dependencies.len(), size_of::<RuntimeContentDependency>())?,
            )?;
            self.admit_work(work)?;
            dependencies = dependencies.into_boxed_slice().into_vec();
        }
        let metadata = add(
            size_of::<PendingMember>(),
            add(
                size_of::<AssetProductRef>(),
                add(
                    MAX_MEMBER_PATH_BYTES,
                    candidate.source_key().retained_bytes(),
                )?,
            )?,
        )?;
        self.admit_work(add(work, metadata)?)?;
        let digest = ContentDigest::of_bytes(&bytes);
        let member = RuntimeContentMember::new(
            member_path(product, digest),
            digest,
            RuntimeContentMemberKind::Scene {
                source: candidate.source_key().clone(),
            },
            vec![product],
            dependencies,
        )?;
        drop(candidate);
        self.commit(
            vec![PendingMember {
                member,
                bytes: MemberBytes::Owned(bytes),
            }],
            None,
        )
    }

    /// Retains the original complete group through capture and deep validation during `finish`.
    /// Member names from the generated store never become Runtime member paths.
    pub fn add_group<E: From<CookedContentAssemblyError>>(
        &mut self,
        group: &VerifiedArtifactGroup,
        mut validate_member: impl FnMut(&RuntimeContentMember) -> Result<(), E>,
    ) -> Result<(), E> {
        check(
            "members",
            add(self.members.len(), group.members().len())?,
            self.limits.catalog.members().get(),
        )?;
        let mut encoded = self.encoded_bytes;
        let mut allocation = add(
            group.retained_bytes(),
            mul(group.members().len(), size_of::<PendingMember>())?,
        )?;
        for member in group.members() {
            let first = member
                .products
                .iter()
                .min()
                .copied()
                .ok_or(CookedContentAssemblyError::InvalidGroup)?;
            self.check_member_path(first)?;
            for product in &member.products {
                self.require_new_product(*product)?;
            }
            let bytes = group
                .member_bytes(&member.name)
                .ok_or(CookedContentAssemblyError::InvalidGroup)?;
            if ContentDigest::of_bytes(bytes) != member.digest.content() {
                return Err(CookedContentAssemblyError::InvalidGroup.into());
            }
            self.check_member_bytes(bytes.len())?;
            encoded = add(encoded, bytes.len())?;
            check(
                "total-encoded-bytes",
                encoded,
                self.limits.total_encoded_bytes.get(),
            )?;
            allocation = add(
                allocation,
                add(MAX_MEMBER_PATH_BYTES, member.format.as_str().len())?,
            )?;
            allocation = add(
                allocation,
                mul(member.products.len(), size_of::<AssetProductRef>())?,
            )?;
            allocation = add(
                allocation,
                mul(
                    member.dependencies.len(),
                    size_of::<RuntimeContentDependency>(),
                )?,
            )?;
        }
        self.admit_work(allocation)?;
        let mut incoming = Vec::with_capacity(group.members().len());
        for (index, original) in group.members().iter().enumerate() {
            let first = original
                .products
                .iter()
                .min()
                .copied()
                .ok_or(CookedContentAssemblyError::InvalidGroup)?;
            let digest = original.digest.content();
            let member = RuntimeContentMember::new(
                member_path(first, digest),
                digest,
                RuntimeContentMemberKind::Asset {
                    format: original.format.clone(),
                    version: original.format_version,
                },
                original.products.clone(),
                original
                    .dependencies
                    .iter()
                    .map(|(product, demand)| {
                        RuntimeContentDependency::new(*product, dependency_role(*demand))
                    })
                    .collect(),
            )
            .map_err(CookedContentAssemblyError::from)?;
            validate_member(&member)?;
            incoming.push(PendingMember {
                member,
                bytes: MemberBytes::Group {
                    group: self.groups.len(),
                    member: index,
                },
            });
        }
        self.commit(incoming, Some(group))?;
        Ok(())
    }

    /// Encodes one CPU product using the codec and limits selected by the executable.
    pub fn add_asset<D, C, E>(
        &mut self,
        product: AssetProductRef,
        value: &D::Product,
        dependencies: &[(AssetProductRef, AssetReferenceDemand)],
        select_limits: impl FnOnce(&RuntimeContentMember) -> Result<AssetRuntimeLimits, E>,
    ) -> Result<(), E>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
        E: From<CookedContentAssemblyError>,
    {
        self.require_new_product(product)?;
        self.check_member_path(product)?;
        let metadata = add(
            add(
                size_of::<PendingMember>(),
                add(MAX_MEMBER_PATH_BYTES, C::RUNTIME_KIND.len())?,
            )?,
            add(
                size_of::<AssetProductRef>(),
                mul(dependencies.len(), size_of::<RuntimeContentDependency>())?,
            )?,
        )?;
        self.admit_work(metadata)?;
        let make_member = |digest| -> Result<_, CookedContentAssemblyError> {
            Ok(RuntimeContentMember::new(
                member_path(product, digest),
                digest,
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(C::RUNTIME_KIND)
                        .map_err(|_| CookedContentAssemblyError::InvalidCodec)?,
                    version: FormatVersion::new(C::RUNTIME_VERSION)
                        .ok_or(CookedContentAssemblyError::InvalidCodec)?,
                },
                vec![product],
                dependencies
                    .iter()
                    .map(|(product, demand)| {
                        RuntimeContentDependency::new(*product, dependency_role(*demand))
                    })
                    .collect(),
            )?)
        };
        let member = make_member(ContentDigest::of_bytes(&[]))?;
        let selected = select_limits(&member)?;
        drop(member);
        self.admit_work(add(
            metadata,
            add(selected.encoded_bytes().get(), selected.work_bytes().get())?,
        )?)?;
        let bytes = C::encode_runtime(value)
            .map_err(CookedContentAssemblyError::from)?
            .into_vec();
        self.check_member_bytes(bytes.len())?;
        let digest = ContentDigest::of_bytes(&bytes);
        self.admit_work(add(metadata, bytes.len())?)?;
        let member = make_member(digest)?;
        self.commit(
            vec![PendingMember {
                member,
                bytes: MemberBytes::Owned(bytes),
            }],
            None,
        )?;
        Ok(())
    }

    /// Assembles the canonical delivery, then admits it with the original executable.
    ///
    /// `capture` receives the catalog and member buffers in catalog order, the producer's
    /// expectation, and the residual Runtime retention limit. It returns the admitted base and
    /// its logical retention. `validate` deeply validates that base and returns the number of
    /// unique members visited. All supplied members must belong to that closure.
    pub fn finish<T, E: From<CookedContentAssemblyError>>(
        mut self,
        fingerprints: CookedContentFingerprints,
        prepared_bytes: usize,
        capture: impl FnOnce(
            Vec<u8>,
            Vec<Vec<u8>>,
            &RuntimeContentExpectation,
            ByteLimit,
        ) -> Result<(T, usize), E>,
        validate: impl FnOnce(&T) -> Result<usize, E>,
    ) -> Result<(T, RuntimeContentExpectation, usize), E> {
        let prepared_overhead = add(
            prepared_bytes,
            add(
                self.request.target.triple().len(),
                self.request.target.profile().len(),
            )?,
        )?;
        let group_retained = self.groups.iter().try_fold(
            mul(self.groups.len(), size_of::<VerifiedArtifactGroup>())?,
            |total, group| add(total, group.retained_bytes()),
        )?;
        let retained_outside_base =
            add(add(prepared_overhead, group_retained)?, self.consumer_bytes)?;
        let residual = self
            .limits
            .retained_bytes
            .get()
            .checked_sub(retained_outside_base)
            .and_then(ByteLimit::new)
            .ok_or(CookedContentAssemblyError::Budget {
                kind: "retained-bytes",
                required: add(retained_outside_base, 1)?,
                limit: self.limits.retained_bytes.get(),
            })?;
        let runtime_retained = residual.min(self.limits.runtime_retained_bytes);
        let catalog_limits = self.limits.catalog;
        let mut copies = 0;
        for member in &self.members {
            if matches!(member.bytes, MemberBytes::Group { .. }) {
                copies = add(
                    copies,
                    usize::try_from(member.member.digest().length())
                        .map_err(|_| CookedContentAssemblyError::Overflow)?,
                )?;
            }
        }
        let tables = mul(
            self.members.len(),
            add(size_of::<RuntimeContentMember>(), size_of::<Vec<u8>>())?,
        )?;
        self.admit_work(add(
            copies,
            add(
                tables,
                add(
                    catalog_limits.retained_bytes().get(),
                    catalog_limits.working_bytes().get(),
                )?,
            )?,
        )?)?;
        // Original groups, copied base, decoder output and decoder work are simultaneously live.
        // Runtime retention already includes the base; equal digests never merge group custody.
        check(
            "work-bytes",
            add(
                add(retained_outside_base, self.consumer_work_bytes)?,
                add(runtime_retained.get(), self.limits.runtime_work_bytes.get())?,
            )?,
            self.limits.work_bytes.get(),
        )?;
        self.members
            .sort_unstable_by(|left, right| left.member.path().cmp(right.member.path()));
        let Self {
            request,
            limits,
            members,
            groups,
            consumer_bytes,
            ..
        } = self;
        let mut declarations = Vec::with_capacity(members.len());
        let mut captured = Vec::with_capacity(members.len());
        for pending in members {
            let bytes = match pending.bytes {
                MemberBytes::Owned(bytes) => bytes,
                MemberBytes::Group { group, member } => {
                    let original = &groups[group].members()[member];
                    groups[group]
                        .member_bytes(&original.name)
                        .ok_or(CookedContentAssemblyError::InvalidGroup)?
                        .to_vec()
                }
            };
            declarations.push(pending.member);
            captured.push(bytes);
        }
        let definition = RuntimeContentCatalogDefinition {
            target: request.target,
            configuration: request.configuration,
            plugin_plan_fingerprint: fingerprints.plugin_plan,
            schema_fingerprint: fingerprints.schema,
            runtime_codecs_fingerprint: fingerprints.runtime_codecs,
            startup_scene: request.startup_scene,
            scenes: request.scenes,
            addressable_roots: request.addressable_roots,
            members: declarations,
        };
        let catalog = RuntimeContentCatalog::new(definition, catalog_limits)
            .map_err(CookedContentAssemblyError::from)?;
        let encoded = catalog
            .to_json_bytes()
            .map_err(CookedContentAssemblyError::from)?;
        let total = catalog.members().len();
        let expectation = RuntimeContentExpectation::new(
            ContentDigest::of_bytes(&encoded),
            catalog.target().clone(),
        );
        drop(catalog);
        let (base, base_retained) = capture(encoded, captured, &expectation, runtime_retained)?;
        let retained_bytes = add(base_retained, prepared_overhead)?;
        check(
            "retained-bytes",
            add(add(retained_bytes, group_retained)?, consumer_bytes)?,
            limits.retained_bytes.get(),
        )?;
        let reachable = validate(&base)?;
        if reachable != total {
            return Err(CookedContentAssemblyError::UnreachableMembers { reachable, total }.into());
        }
        // Release group custody only after the base and all validation outputs have passed.
        drop(groups);
        Ok((base, expectation, retained_bytes))
    }

    fn require_new_product(
        &self,
        product: AssetProductRef,
    ) -> Result<(), CookedContentAssemblyError> {
        if self
            .members
            .iter()
            .any(|pending| pending.member.products().contains(&product))
        {
            Err(CookedContentAssemblyError::DuplicateProduct(product))
        } else {
            Ok(())
        }
    }

    fn check_member_bytes(&self, bytes: usize) -> Result<(), CookedContentAssemblyError> {
        check(
            "member-encoded-bytes",
            bytes,
            self.limits.member_encoded_bytes.get(),
        )?;
        check(
            "total-encoded-bytes",
            add(self.encoded_bytes, bytes)?,
            self.limits.total_encoded_bytes.get(),
        )
    }

    fn check_member_path(
        &self,
        product: AssetProductRef,
    ) -> Result<(), CookedContentAssemblyError> {
        check(
            "path-bytes",
            member_path_bytes(product),
            self.limits.catalog.path_bytes().get(),
        )
    }

    fn admit_work(&self, additional: usize) -> Result<(), CookedContentAssemblyError> {
        check(
            "work-bytes",
            add(
                add(self.retained_bytes, self.consumer_work_bytes)?,
                additional,
            )?,
            self.limits.work_bytes.get(),
        )
    }

    /// Reserves logical retention for source selection metadata the caller keeps live.
    pub fn reserve_consumer_metadata(
        &mut self,
        bytes: usize,
    ) -> Result<(), CookedContentAssemblyError> {
        let retained = add(self.retained_bytes, bytes)?;
        let consumer = add(self.consumer_bytes, bytes)?;
        check("retained-bytes", retained, self.limits.retained_bytes.get())?;
        self.admit_work(bytes)?;
        self.retained_bytes = retained;
        self.consumer_bytes = consumer;
        Ok(())
    }

    /// Checks transient work that the caller completes before its next assembly operation.
    pub fn admit_consumer_work(&self, bytes: usize) -> Result<(), CookedContentAssemblyError> {
        self.admit_work(bytes)
    }

    /// Releases the charge for caller-owned metadata that has already been dropped.
    pub fn release_consumer_metadata(
        &mut self,
        bytes: usize,
    ) -> Result<(), CookedContentAssemblyError> {
        self.consumer_bytes = self
            .consumer_bytes
            .checked_sub(bytes)
            .ok_or(CookedContentAssemblyError::Overflow)?;
        self.retained_bytes -= bytes;
        Ok(())
    }

    /// Charges visitor scratch that stays live during subsequent assembly operations.
    pub fn reserve_consumer_work(
        &mut self,
        bytes: usize,
    ) -> Result<(), CookedContentAssemblyError> {
        self.admit_work(bytes)?;
        self.consumer_work_bytes = add(self.consumer_work_bytes, bytes)?;
        Ok(())
    }

    /// Releases caller scratch that has already been dropped.
    pub fn release_consumer_work(
        &mut self,
        bytes: usize,
    ) -> Result<(), CookedContentAssemblyError> {
        self.consumer_work_bytes = self
            .consumer_work_bytes
            .checked_sub(bytes)
            .ok_or(CookedContentAssemblyError::Overflow)?;
        Ok(())
    }

    fn commit(
        &mut self,
        mut incoming: Vec<PendingMember>,
        group: Option<&VerifiedArtifactGroup>,
    ) -> Result<(), CookedContentAssemblyError> {
        let count = add(self.members.len(), incoming.len())?;
        let catalog = self.limits.catalog;
        check("members", count, catalog.members().get())?;
        let mut products = self.members.iter().try_fold(0, |total, member| {
            add(total, member.member.products().len())
        })?;
        let mut dependencies = self.members.iter().try_fold(0, |total, member| {
            add(total, member.member.dependencies().len())
        })?;
        let mut retained = self.retained_bytes;
        let mut encoded = self.encoded_bytes;
        for (index, pending) in incoming.iter().enumerate() {
            for product in pending.member.products() {
                self.require_new_product(*product)?;
                if incoming[..index]
                    .iter()
                    .any(|other| other.member.products().contains(product))
                {
                    return Err(CookedContentAssemblyError::DuplicateProduct(*product));
                }
            }
            products = add(products, pending.member.products().len())?;
            dependencies = add(dependencies, pending.member.dependencies().len())?;
            retained = add(
                retained,
                add(
                    size_of::<PendingMember>(),
                    member_heap_bytes(&pending.member)?,
                )?,
            )?;
            if let MemberBytes::Owned(bytes) = &pending.bytes {
                retained = add(retained, bytes.len())?;
            }
            encoded = add(
                encoded,
                usize::try_from(pending.member.digest().length())
                    .map_err(|_| CookedContentAssemblyError::Overflow)?,
            )?;
        }
        check("products", products, catalog.products().get())?;
        check("dependencies", dependencies, catalog.dependencies().get())?;
        check(
            "total-encoded-bytes",
            encoded,
            self.limits.total_encoded_bytes.get(),
        )?;
        if let Some(group) = group {
            retained = add(
                retained,
                add(size_of::<VerifiedArtifactGroup>(), group.retained_bytes())?,
            )?;
        }
        check("retained-bytes", retained, self.limits.retained_bytes.get())?;
        let old_tables = add(
            mul(count, size_of::<PendingMember>())?,
            mul(self.groups.len(), size_of::<VerifiedArtifactGroup>())?,
        )?;
        check(
            "work-bytes",
            add(add(retained, self.consumer_work_bytes)?, old_tables)?,
            self.limits.work_bytes.get(),
        )?;
        let mut members = Vec::with_capacity(count);
        let mut groups = Vec::with_capacity(self.groups.len() + usize::from(group.is_some()));
        members.append(&mut self.members);
        members.append(&mut incoming);
        groups.append(&mut self.groups);
        if let Some(group) = group {
            groups.push(group.clone());
        }
        self.members = members;
        self.groups = groups;
        drop(incoming);
        self.retained_bytes = retained;
        self.encoded_bytes = encoded;
        Ok(())
    }
}

const MAX_MEMBER_PATH_BYTES: usize = 8 + 36 + 1 + 36 + 1 + 64 + 4;

fn member_path(product: AssetProductRef, digest: ContentDigest) -> String {
    let mut path = String::with_capacity(member_path_bytes(product));
    write!(
        &mut path,
        "members/{}_{}_",
        product.source(),
        product.product()
    )
    .expect("String writer");
    for byte in digest.as_bytes() {
        write!(&mut path, "{byte:02x}").expect("String writer");
    }
    path.push_str(".bin");
    path
}

fn member_path_bytes(product: AssetProductRef) -> usize {
    let product_bytes = if product.product().as_subproduct().is_some() {
        36
    } else {
        7
    };
    8 + 36 + 1 + product_bytes + 1 + 64 + 4
}

fn dependency_role(demand: AssetReferenceDemand) -> RuntimeContentDependencyRole {
    match demand {
        AssetReferenceDemand::Immediate => RuntimeContentDependencyRole::Required,
        AssetReferenceDemand::Deferred => RuntimeContentDependencyRole::Reference,
    }
}
