//! Finite production assembly of processed Scenes, verified groups, Input and TileSets.
//!
//! Callers select and revalidate their current build inputs through the concrete build owners.
//! A verified historical group alone does not prove freshness. This owner consumes its retained
//! bytes, never a cache pathname, and publishes no directory, executable or native resource.

mod budget;

#[cfg(test)]
mod tests;

use std::{error::Error, fmt, fmt::Write as _, mem::size_of};

use nara_asset::{
    AssetDomain, AssetProductRef, AssetRef, AssetReferenceDemand, AssetReferenceResolver,
    RuntimeAssetCodec,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_fs::ContentDigest;
use nara_input::{ActionMapAsset, ActionMapDomain, ActionMapRuntimeCodec};
use nara_project::RuntimeConfiguration;
use nara_reflect::__private::{
    AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
};
use nara_scene::{ExpandedSceneCandidate, RuntimeSceneCandidate};
use nara_tilemap::{TileSetDescriptor, TileSetDomain, TileSetRuntimeCodec};

use crate::{
    ProductRecipe,
    asset_build::VerifiedArtifactGroup,
    project_host::{RuntimeConfigurationPlan, resolve_runtime_configuration},
    runtime_content::{
        RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogError, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};
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

/// Logical assembly bounds, in addition to each production owner's frozen limits.
///
/// Retention counts this owner's live request, exact member tables/bytes and complete retained
/// groups, then the prepared captured base and expectation. Work bounds their overlap with new
/// tables, group copies and conservative Scene/codec/catalog/deep-validation promises. Tables
/// request exact capacity; lengths describe logical ownership, not allocator capacity or RSS.
/// Borrowed source candidates and CPU inputs remain their caller's custody. Compiled recipe,
/// plugin and Schema composition retain their existing owner limits, as in Runtime admission.
/// Deep validation reserves the Runtime retained/work ceilings together before decoding; callers
/// can select smaller Runtime limits when qualifying a smaller assembly allowance.
#[derive(Debug, Clone, Copy)]
pub struct CookedContentLimits {
    pub runtime: RuntimeContentLimits,
    pub work_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
}

impl Default for CookedContentLimits {
    fn default() -> Self {
        Self {
            runtime: RuntimeContentLimits::default(),
            work_bytes: ByteLimit::new(2 * 1024 * 1024 * 1024).expect("nonzero assembly limit"),
            retained_bytes: ByteLimit::new(1024 * 1024 * 1024).expect("nonzero assembly limit"),
        }
    }
}

#[derive(Debug)]
pub enum CookedContentError {
    Runtime(RuntimeContentError),
    DuplicateProduct(AssetProductRef),
    DuplicateRoot {
        root: &'static str,
        product: AssetProductRef,
    },
    UndeclaredStartupScene,
    IncompleteProductReference,
    InvalidGroup,
    UnreachableMembers {
        reachable: usize,
        total: usize,
    },
    Budget {
        kind: &'static str,
        required: usize,
        limit: usize,
    },
    Overflow,
}

impl fmt::Display for CookedContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "cooked content was rejected: {error}"),
            Self::DuplicateProduct(product) => {
                write!(formatter, "duplicate cooked Product {product}")
            }
            Self::DuplicateRoot { root, product } => {
                write!(formatter, "duplicate {root} root {product}")
            }
            Self::UndeclaredStartupScene => {
                formatter.write_str("startup Scene must be a declared Scene root")
            }
            Self::IncompleteProductReference => {
                formatter.write_str("cooked Scene references require complete Products")
            }
            Self::InvalidGroup => {
                formatter.write_str("verified group has inconsistent retained membership")
            }
            Self::UnreachableMembers { reachable, total } => {
                write!(
                    formatter,
                    "only {reachable} of {total} cooked members belong to the selected delivery closure"
                )
            }
            Self::Budget {
                kind,
                required,
                limit,
            } => {
                write!(
                    formatter,
                    "cooked content {kind} requires {required} bytes/items, limit {limit}"
                )
            }
            Self::Overflow => formatter.write_str("cooked content ownership accounting overflowed"),
        }
    }
}

impl Error for CookedContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RuntimeContentError> for CookedContentError {
    fn from(value: RuntimeContentError) -> Self {
        Self::Runtime(value)
    }
}

impl From<RuntimeContentCatalogError> for CookedContentError {
    fn from(value: RuntimeContentCatalogError) -> Self {
        RuntimeContentError::Catalog(value).into()
    }
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

/// Move-only unpublished assembly using one original executable configuration plan.
///
/// Each `add_*` is transactional: errors leave all prior members and reservations unchanged.
/// A complete group is added atomically. `finish` consumes the builder, so a failed final check
/// drops the complete unpublished candidate and cannot leave a partially prepared result.
#[derive(Debug)]
pub struct CookedContentBuilder {
    request: CookedContentRequest,
    recipe: ProductRecipe,
    plan: RuntimeConfigurationPlan,
    limits: CookedContentLimits,
    members: Vec<PendingMember>,
    groups: Vec<VerifiedArtifactGroup>,
    retained_bytes: usize,
    encoded_bytes: usize,
}

impl CookedContentBuilder {
    pub fn new(
        mut request: CookedContentRequest,
        recipe: ProductRecipe,
        limits: CookedContentLimits,
    ) -> Result<Self, CookedContentError> {
        let catalog = limits.runtime.catalog();
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
        let retained_bytes = add(size_of::<Self>(), request_heap_bytes(&request)?)?;
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
                    return Err(CookedContentError::DuplicateRoot {
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
            return Err(CookedContentError::UndeclaredStartupScene);
        }
        let configuration = request
            .configuration
            .resolve()
            .map_err(RuntimeContentError::Configuration)?;
        let plan = resolve_runtime_configuration(&configuration, recipe.clone())
            .map_err(RuntimeContentError::Composition)?;
        Ok(Self {
            request,
            recipe,
            plan,
            limits,
            members: Vec::new(),
            groups: Vec::new(),
            retained_bytes,
            encoded_bytes: 0,
        })
    }

    pub fn add_scene(
        &mut self,
        product: AssetProductRef,
        expanded: &ExpandedSceneCandidate,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> Result<(), CookedContentError> {
        self.require_new_product(product)?;
        self.check_member_path(product)?;
        let scene_limits = self.limits.runtime.scene();
        self.admit_work(scene_limits.working_bytes().get())?;
        let candidate = RuntimeSceneCandidate::cook(
            expanded,
            self.plan.schema_validation().bindings(),
            asset_resolution,
            scene_limits,
        )
        .map_err(RuntimeContentError::Scene)?;
        let candidate_bytes =
            crate::scene_retention::runtime_startup_scene_retained_bytes(&candidate)
                .map_err(|_| CookedContentError::Overflow)?;
        // Encoding and the effective reference inventory overlap the actual cooked candidate.
        self.admit_work(add(candidate_bytes, scene_limits.working_bytes().get())?)?;
        let bytes = candidate
            .to_json_bytes()
            .map_err(RuntimeContentError::Scene)?;
        self.check_member_bytes(bytes.len())?;
        let fixed = add(candidate_bytes, bytes.len())?;
        let components = candidate
            .document()
            .entities
            .iter()
            .try_fold(0, |total, entity| add(total, entity.components.len()))?;
        let mut work = add(
            fixed,
            mul(
                components,
                size_of::<Vec<nara_reflect::DeclaredAssetReference>>(),
            )?,
        )?;
        self.admit_work(work)?;
        let mut inventories = Vec::with_capacity(components);
        let mut references_count = 0;
        for entity in &candidate.document().entities {
            for (component, record) in &entity.components {
                let schema = self
                    .plan
                    .schema_validation()
                    .bindings()
                    .schema(component)
                    .expect("cooked Scene uses this builder's original admitted definitions");
                let references = collect_declared_effective_asset_references_with_admission(
                    schema,
                    record.version,
                    &record.value,
                    scene_limits.reference_traversal_limits(),
                    |_, _, allocation| {
                        work = add(work, allocation)?;
                        self.admit_work(work)
                    },
                )
                .map_err(|error| match error {
                    AssetReferenceCollectionError::Admission(error) => error,
                    AssetReferenceCollectionError::Reference(error) => {
                        RuntimeContentError::SceneReference(error).into()
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
                    return Err(CookedContentError::IncompleteProductReference);
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
            self.limits.runtime.catalog().dependencies().get(),
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
    pub fn add_group(&mut self, group: &VerifiedArtifactGroup) -> Result<(), CookedContentError> {
        check(
            "members",
            add(self.members.len(), group.members().len())?,
            self.limits.runtime.catalog().members().get(),
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
                .ok_or(CookedContentError::InvalidGroup)?;
            self.check_member_path(first)?;
            for product in &member.products {
                self.require_new_product(*product)?;
            }
            let bytes = group
                .member_bytes(&member.name)
                .ok_or(CookedContentError::InvalidGroup)?;
            if ContentDigest::of_bytes(bytes) != member.digest.content() {
                return Err(CookedContentError::InvalidGroup);
            }
            self.check_member_bytes(bytes.len())?;
            encoded = add(encoded, bytes.len())?;
            check(
                "total-encoded-bytes",
                encoded,
                self.limits.runtime.total_encoded_bytes().get(),
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
                .ok_or(CookedContentError::InvalidGroup)?;
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
            )?;
            self.plan.runtime_codecs().validate_member(&member)?;
            incoming.push(PendingMember {
                member,
                bytes: MemberBytes::Group {
                    group: self.groups.len(),
                    member: index,
                },
            });
        }
        self.commit(incoming, Some(group))
    }

    pub fn add_input(&mut self, input: &ActionMapAsset) -> Result<(), CookedContentError> {
        self.add_cpu::<ActionMapDomain, ActionMapRuntimeCodec>(input.product(), input, &[])
    }

    pub fn add_tileset(&mut self, tileset: &TileSetDescriptor) -> Result<(), CookedContentError> {
        self.add_cpu::<TileSetDomain, TileSetRuntimeCodec>(
            tileset.product(),
            tileset,
            &[(tileset.image_product(), AssetReferenceDemand::Immediate)],
        )
    }

    // Only the two concrete public operations above use this encoding/accounting helper.
    fn add_cpu<D, C>(
        &mut self,
        product: AssetProductRef,
        value: &D::Product,
        dependencies: &[(AssetProductRef, AssetReferenceDemand)],
    ) -> Result<(), CookedContentError>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
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
        let make_member = |digest| {
            RuntimeContentMember::new(
                member_path(product, digest),
                digest,
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(C::RUNTIME_KIND)
                        .expect("selected production codec has a valid kind"),
                    version: FormatVersion::new(C::RUNTIME_VERSION)
                        .expect("selected production codec has a nonzero version"),
                },
                vec![product],
                dependencies
                    .iter()
                    .map(|(product, demand)| {
                        RuntimeContentDependency::new(*product, dependency_role(*demand))
                    })
                    .collect(),
            )
        };
        let member = make_member(ContentDigest::of_bytes(&[]))?;
        let selected = self
            .plan
            .runtime_codecs()
            .selected_limits::<D, C>(&member)?;
        drop(member);
        self.admit_work(add(
            metadata,
            add(selected.encoded_bytes().get(), selected.work_bytes().get())?,
        )?)?;
        let bytes = C::encode_runtime(value)
            .map_err(RuntimeContentError::Asset)?
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
        )
    }

    /// Captures exact member bytes and deep-validates the complete selected delivery closure.
    /// No member may be silently stripped, including a Deferred Scene or a compound sibling.
    pub fn finish(mut self) -> Result<PreparedRuntimeContent, CookedContentError> {
        let prepared_overhead = add(
            size_of::<PreparedRuntimeContent>(),
            add(
                self.request.target.triple().len(),
                self.request.target.profile().len(),
            )?,
        )?;
        let group_retained = self.groups.iter().try_fold(
            mul(self.groups.len(), size_of::<VerifiedArtifactGroup>())?,
            |total, group| add(total, group.retained_bytes()),
        )?;
        let retained_outside_base = add(prepared_overhead, group_retained)?;
        let residual = self
            .limits
            .retained_bytes
            .get()
            .checked_sub(retained_outside_base)
            .and_then(ByteLimit::new)
            .ok_or(CookedContentError::Budget {
                kind: "retained-bytes",
                required: add(retained_outside_base, 1)?,
                limit: self.limits.retained_bytes.get(),
            })?;
        let runtime = self.limits.runtime.with_retained_bytes(
            ByteLimit::new(
                residual
                    .get()
                    .min(self.limits.runtime.retained_bytes().get()),
            )
            .expect("nonzero captured Runtime residual"),
        );
        let catalog_limits = runtime.catalog();
        let mut copies = 0;
        for member in &self.members {
            if matches!(member.bytes, MemberBytes::Group { .. }) {
                copies = add(
                    copies,
                    usize::try_from(member.member.digest().length())
                        .map_err(|_| CookedContentError::Overflow)?,
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
                retained_outside_base,
                add(runtime.retained_bytes().get(), runtime.work_bytes().get())?,
            )?,
            self.limits.work_bytes.get(),
        )?;
        self.members
            .sort_unstable_by(|left, right| left.member.path().cmp(right.member.path()));
        let Self {
            request,
            recipe,
            plan,
            limits,
            members,
            groups,
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
                        .ok_or(CookedContentError::InvalidGroup)?
                        .to_vec()
                }
            };
            declarations.push(pending.member);
            captured.push(bytes);
        }
        let definition = RuntimeContentCatalogDefinition {
            target: request.target,
            configuration: request.configuration,
            plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
            schema_fingerprint: plan
                .schema_validation()
                .composition_fingerprint()
                .as_bytes(),
            runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
            startup_scene: request.startup_scene,
            scenes: request.scenes,
            addressable_roots: request.addressable_roots,
            members: declarations,
        };
        drop(plan);
        let catalog = RuntimeContentCatalog::new(definition, catalog_limits)?;
        let encoded = catalog.to_json_bytes()?;
        let expectation = RuntimeContentExpectation::new(
            ContentDigest::of_bytes(&encoded),
            catalog.target().clone(),
        );
        drop(catalog);
        let (base, base_retained) =
            RuntimeContentBase::capture_members(encoded, captured, &expectation, recipe, runtime)?;
        let retained_bytes = add(base_retained, prepared_overhead)?;
        check(
            "retained-bytes",
            add(retained_bytes, group_retained)?,
            limits.retained_bytes.get(),
        )?;
        let report = base.validate_deep()?;
        if report.members() != base.catalog().members().len() {
            return Err(CookedContentError::UnreachableMembers {
                reachable: report.members(),
                total: base.catalog().members().len(),
            });
        }
        // Release group custody only after the base and all validation outputs have passed.
        drop(groups);
        Ok(PreparedRuntimeContent {
            base,
            expectation,
            retained_bytes,
        })
    }

    fn require_new_product(&self, product: AssetProductRef) -> Result<(), CookedContentError> {
        if self
            .members
            .iter()
            .any(|pending| pending.member.products().contains(&product))
        {
            Err(CookedContentError::DuplicateProduct(product))
        } else {
            Ok(())
        }
    }

    fn check_member_bytes(&self, bytes: usize) -> Result<(), CookedContentError> {
        check(
            "member-encoded-bytes",
            bytes,
            self.limits.runtime.member_encoded_bytes().get(),
        )?;
        check(
            "total-encoded-bytes",
            add(self.encoded_bytes, bytes)?,
            self.limits.runtime.total_encoded_bytes().get(),
        )
    }

    fn check_member_path(&self, product: AssetProductRef) -> Result<(), CookedContentError> {
        check(
            "path-bytes",
            member_path_bytes(product),
            self.limits.runtime.catalog().path_bytes().get(),
        )
    }

    fn admit_work(&self, additional: usize) -> Result<(), CookedContentError> {
        check(
            "work-bytes",
            add(self.retained_bytes, additional)?,
            self.limits.work_bytes.get(),
        )
    }

    fn commit(
        &mut self,
        mut incoming: Vec<PendingMember>,
        group: Option<&VerifiedArtifactGroup>,
    ) -> Result<(), CookedContentError> {
        let count = add(self.members.len(), incoming.len())?;
        let catalog = self.limits.runtime.catalog();
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
                    return Err(CookedContentError::DuplicateProduct(*product));
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
                    .map_err(|_| CookedContentError::Overflow)?,
            )?;
        }
        check("products", products, catalog.products().get())?;
        check("dependencies", dependencies, catalog.dependencies().get())?;
        check(
            "total-encoded-bytes",
            encoded,
            self.limits.runtime.total_encoded_bytes().get(),
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
            add(retained, old_tables)?,
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

/// Complete captured content with independent producer expectation and no decoded residency.
#[derive(Debug)]
pub struct PreparedRuntimeContent {
    base: RuntimeContentBase,
    expectation: RuntimeContentExpectation,
    retained_bytes: usize,
}

impl PreparedRuntimeContent {
    #[must_use]
    pub fn base(&self) -> &RuntimeContentBase {
        &self.base
    }
    #[must_use]
    pub fn expectation(&self) -> &RuntimeContentExpectation {
        &self.expectation
    }
    /// The captured base and this wrapper's logical ownership. Later independent acquisitions
    /// retain their own Runtime charges; they do not change this preparation receipt.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    #[must_use]
    pub fn into_base(self) -> RuntimeContentBase {
        self.base
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
