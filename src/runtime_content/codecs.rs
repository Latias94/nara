//! Executable-selected Runtime codecs, independent of plugins and source importers.

use std::{
    any::{Any, TypeId},
    fmt,
    marker::PhantomData,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_asset::{AssetDomain, AssetProductRef, AssetRuntimeLimits, RuntimeAssetCodec};
use nara_core::{FormatKind, FormatVersion};

use super::{
    DecodeLease, RuntimeContentAsset, RuntimeContentBase, RuntimeContentBudgetKind,
    RuntimeContentError, RuntimeContentMember, RuntimeContentMemberKind, checked_add,
};
use crate::project_content::{
    ErasedCollection, ProjectAssetCollection, ProjectDomainAssets, runtime_collection,
};

/// A package author's versioned declaration of one compiled family/codec binding.
///
/// This identity is distinct from the encoded format version. Change the revision when the
/// binding's executable meaning changes. It is a trusted declaration, not a Rust binary hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeCodecDefinitionId {
    name: &'static str,
    revision: u32,
}

impl RuntimeCodecDefinitionId {
    #[must_use]
    pub const fn new(name: &'static str, revision: u32) -> Self {
        Self { name, revision }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn revision(self) -> u32 {
        self.revision
    }
}

/// Stable agreement over the actually selected codec definitions, roles and frozen limits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RuntimeCodecsFingerprint([u8; 32]);

impl RuntimeCodecsFingerprint {
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for RuntimeCodecsFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeCodecSelectionError {
    #[error("Runtime codec definition requires a bounded identifier and nonzero revision")]
    InvalidDefinition,
    #[error("Runtime codec {definition:?} has an invalid format or output allowance")]
    InvalidCodec {
        definition: RuntimeCodecDefinitionId,
    },
    #[error("Runtime codec {definition:?} limits query panicked")]
    LimitsPanicked {
        definition: RuntimeCodecDefinitionId,
    },
    #[error("Runtime codec definition {definition:?} is selected more than once")]
    DuplicateDefinition {
        definition: RuntimeCodecDefinitionId,
    },
    #[error("Runtime format {format} version {version} has more than one selected codec")]
    AmbiguousFormat { format: &'static str, version: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeCodecRole {
    Cpu,
    Image,
    TileSet,
}

#[derive(Clone)]
struct CodecDefinition {
    id: RuntimeCodecDefinitionId,
    format: &'static str,
    version: u32,
    family: TypeId,
    codec: TypeId,
    role: RuntimeCodecRole,
    limits: AssetRuntimeLimits,
}

impl CodecDefinition {
    fn matches(&self, member: &RuntimeContentMember) -> bool {
        matches!(member.kind(), RuntimeContentMemberKind::Asset { format, version }
            if format.as_str() == self.format && version.get() == self.version)
    }
}

/// Host-private heterogeneous composition. It neither installs resources nor selects code from
/// package bytes. Each typed family aggregates every selected format into one CPU collection.
#[derive(Clone, Default)]
pub(crate) struct RuntimeCodecs {
    definitions: Vec<CodecDefinition>,
    families: Vec<Arc<dyn RuntimeFamily>>,
}

/// One actual acquisition kept while its verified outgoing demand is discovered. The erased
/// slot is preparation scratch; the typed acquisition retains its original output charge.
pub(super) struct DecodedAsset {
    pub(super) member: usize,
    family: TypeId,
    asset: Box<dyn Any + Send + Sync>,
    _work: DecodeLease,
}

impl DecodedAsset {
    pub(super) fn typed<D: AssetDomain>(&self) -> Option<&RuntimeContentAsset<D::Product>> {
        if self.family != TypeId::of::<D>() {
            return None;
        }
        self.asset.downcast_ref()
    }
}

impl fmt::Debug for RuntimeCodecs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.definitions.iter().map(|definition| definition.id))
            .finish()
    }
}

impl RuntimeCodecs {
    pub(crate) const fn new() -> Self {
        Self {
            definitions: Vec::new(),
            families: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub(crate) fn has_family_format<D: AssetDomain>(&self, format: &str, version: u32) -> bool {
        self.definitions.iter().any(|definition| {
            definition.family == TypeId::of::<D>()
                && definition.format == format
                && definition.version == version
        })
    }

    /// Uses the original selected decoder for this actual family and member. Product data
    /// chooses no Rust implementation; the immutable executable plan already selected it.
    pub(crate) fn acquire_selected<D: AssetDomain>(
        &self,
        base: &RuntimeContentBase,
        product: AssetProductRef,
    ) -> Result<RuntimeContentAsset<D::Product>, RuntimeContentError> {
        let member = base
            .member(product)
            .ok_or(RuntimeContentError::UndeclaredProduct)?;
        let family = self
            .families
            .iter()
            .find(|family| family.family_type() == TypeId::of::<D>())
            .and_then(|family| family.as_any().downcast_ref::<TypedFamily<D>>())
            .ok_or(RuntimeContentError::CodecNotSelected)?;
        let RuntimeContentMemberKind::Asset { format, version } = member.kind() else {
            return Err(RuntimeContentError::NotAsset);
        };
        let selected = family
            .codecs
            .iter()
            .find(|codec| codec.format == format.as_str() && codec.version == version.get())
            .ok_or(RuntimeContentError::CodecNotSelected)?;
        (selected.acquire)(base, product)
    }

    pub(crate) fn add<D, C>(
        &mut self,
        id: RuntimeCodecDefinitionId,
        role: RuntimeCodecRole,
    ) -> Result<(), RuntimeCodecSelectionError>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
    {
        if id.name.is_empty()
            || id.name.len() > 256
            || id.revision == 0
            || !id
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            || !id.name.as_bytes()[0].is_ascii_alphanumeric()
        {
            return Err(RuntimeCodecSelectionError::InvalidDefinition);
        }
        if self
            .definitions
            .iter()
            .any(|definition| definition.id == id)
        {
            return Err(RuntimeCodecSelectionError::DuplicateDefinition { definition: id });
        }
        if FormatKind::new(C::RUNTIME_KIND).is_err()
            || FormatVersion::new(C::RUNTIME_VERSION).is_none()
        {
            return Err(RuntimeCodecSelectionError::InvalidCodec { definition: id });
        }
        if self.definitions.iter().any(|definition| {
            definition.format == C::RUNTIME_KIND && definition.version == C::RUNTIME_VERSION
        }) {
            return Err(RuntimeCodecSelectionError::AmbiguousFormat {
                format: C::RUNTIME_KIND,
                version: C::RUNTIME_VERSION,
            });
        }
        let limits = catch_unwind(AssertUnwindSafe(C::runtime_limits))
            .map_err(|_| RuntimeCodecSelectionError::LimitsPanicked { definition: id })?;
        if limits.retained_bytes().get() < size_of::<D::Product>() {
            return Err(RuntimeCodecSelectionError::InvalidCodec { definition: id });
        }
        let family = self
            .families
            .iter()
            .position(|family| family.family_type() == TypeId::of::<D>());
        let mut selected = match family {
            Some(index) => self.families[index]
                .as_any()
                .downcast_ref::<TypedFamily<D>>()
                .expect("family identity is bound to its concrete typed adapter")
                .clone(),
            None => TypedFamily::<D> { codecs: Vec::new() },
        };
        selected.codecs.push(TypedCodec {
            format: C::RUNTIME_KIND,
            version: C::RUNTIME_VERSION,
            acquire: RuntimeContentBase::acquire_asset::<D, C>,
            _family: PhantomData,
        });
        match family {
            Some(index) => self.families[index] = Arc::new(selected),
            None => self.families.push(Arc::new(selected)),
        }
        self.definitions.push(CodecDefinition {
            id,
            format: C::RUNTIME_KIND,
            version: C::RUNTIME_VERSION,
            family: TypeId::of::<D>(),
            codec: TypeId::of::<C>(),
            role,
            limits,
        });
        self.definitions
            .sort_unstable_by_key(|definition| definition.id);
        let definitions = &self.definitions;
        self.families.sort_unstable_by_key(|family| {
            definitions
                .iter()
                .find(|definition| definition.family == family.family_type())
                .expect("every typed family has at least one selected definition")
                .id
        });
        Ok(())
    }

    pub(crate) fn fingerprint(&self) -> RuntimeCodecsFingerprint {
        let mut hash = blake3::Hasher::new();
        hash.update(b"nara.runtime-codecs.v1\0");
        hash.update(&(self.definitions.len() as u64).to_le_bytes());
        for definition in &self.definitions {
            for text in [definition.id.name, definition.format] {
                hash.update(&(text.len() as u64).to_le_bytes());
                hash.update(text.as_bytes());
            }
            hash.update(&definition.id.revision.to_le_bytes());
            hash.update(&definition.version.to_le_bytes());
            hash.update(&[match definition.role {
                RuntimeCodecRole::Cpu => 0,
                RuntimeCodecRole::Image => 1,
                RuntimeCodecRole::TileSet => 2,
            }]);
            for bytes in [
                definition.limits.encoded_bytes().get(),
                definition.limits.work_bytes().get(),
                definition.limits.retained_bytes().get(),
            ] {
                hash.update(&(bytes as u64).to_le_bytes());
            }
        }
        RuntimeCodecsFingerprint(*hash.finalize().as_bytes())
    }

    pub(crate) fn validate_member(
        &self,
        member: &RuntimeContentMember,
    ) -> Result<(), RuntimeContentError> {
        if matches!(member.kind(), RuntimeContentMemberKind::Asset { .. })
            && !self
                .definitions
                .iter()
                .any(|definition| definition.matches(member))
        {
            return Err(RuntimeContentError::CodecNotSelected);
        }
        Ok(())
    }

    pub(crate) fn selected_limits<D, C>(
        &self,
        member: &RuntimeContentMember,
    ) -> Result<AssetRuntimeLimits, RuntimeContentError>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
    {
        let definition = self
            .definitions
            .iter()
            .find(|definition| {
                definition.family == TypeId::of::<D>() && definition.codec == TypeId::of::<C>()
            })
            .ok_or(RuntimeContentError::CodecNotSelected)?;
        if !definition.matches(member) {
            return Err(RuntimeContentError::CodecMismatch);
        }
        Ok(definition.limits)
    }

    pub(crate) fn role(&self, member: &RuntimeContentMember) -> Option<RuntimeCodecRole> {
        self.definitions
            .iter()
            .find(|definition| definition.matches(member))
            .map(|definition| definition.role)
    }

    pub(super) fn definition(
        &self,
        member: &RuntimeContentMember,
    ) -> Result<RuntimeCodecDefinitionId, RuntimeContentError> {
        self.definitions
            .iter()
            .find(|definition| definition.matches(member))
            .map(|definition| definition.id)
            .ok_or(RuntimeContentError::CodecNotSelected)
    }

    pub(super) fn acquire_member(
        &self,
        base: &RuntimeContentBase,
        index: usize,
    ) -> Result<DecodedAsset, RuntimeContentError> {
        let member = &base.catalog().members()[index];
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.matches(member))
            .ok_or(RuntimeContentError::CodecNotSelected)?;
        self.families
            .iter()
            .find(|family| family.family_type() == definition.family)
            .expect("an admitted definition retains its typed family")
            .acquire_member(base, index)
    }

    pub(super) fn prepare_collections(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<ProjectDomainAssets, RuntimeContentError> {
        let kind = RuntimeContentBudgetKind::RetainedBytes;
        let mut bytes = ProjectDomainAssets::runtime_metadata_bytes(self.families.len())
            .ok_or(RuntimeContentError::BudgetOverflow { kind })?;
        for family in &self.families {
            bytes = checked_add(kind, bytes, family.metadata_bytes(base, assets)?)?;
        }
        let lease = base.reserve_retained(bytes)?;
        let mut collections = Vec::with_capacity(self.families.len());
        for family in &self.families {
            if let Some(collection) = family.collect(base, assets)? {
                collections.push(collection);
            }
        }
        Ok(ProjectDomainAssets::from_runtime(
            collections,
            base.clone(),
            lease,
        ))
    }
}

trait RuntimeFamily: Send + Sync {
    fn family_type(&self) -> TypeId;
    fn as_any(&self) -> &dyn Any;
    fn acquire_member(
        &self,
        base: &RuntimeContentBase,
        index: usize,
    ) -> Result<DecodedAsset, RuntimeContentError>;
    fn metadata_bytes(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<usize, RuntimeContentError>;
    fn collect(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<Option<Box<dyn ErasedCollection>>, RuntimeContentError>;
}

struct TypedFamily<D: AssetDomain> {
    codecs: Vec<TypedCodec<D>>,
}

impl<D: AssetDomain> Clone for TypedFamily<D> {
    fn clone(&self) -> Self {
        Self {
            codecs: self.codecs.clone(),
        }
    }
}

struct TypedCodec<D: AssetDomain> {
    format: &'static str,
    version: u32,
    acquire: fn(
        &RuntimeContentBase,
        AssetProductRef,
    ) -> Result<RuntimeContentAsset<D::Product>, RuntimeContentError>,
    _family: PhantomData<fn() -> D>,
}

impl<D: AssetDomain> Clone for TypedCodec<D> {
    fn clone(&self) -> Self {
        Self {
            format: self.format,
            version: self.version,
            acquire: self.acquire,
            _family: PhantomData,
        }
    }
}

impl<D: AssetDomain> TypedFamily<D> {
    fn codec(&self, member: &RuntimeContentMember) -> Option<&TypedCodec<D>> {
        self.codecs.iter().find(|codec| {
            matches!(member.kind(), RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == codec.format && version.get() == codec.version)
        })
    }

    fn counts(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<(usize, usize), RuntimeContentError> {
        let mut count = 0;
        let mut mappings = 0;
        for asset in assets {
            if asset.family == TypeId::of::<D>() {
                let member = &base.catalog().members()[asset.member];
                count += 1;
                mappings = checked_add(
                    RuntimeContentBudgetKind::RetainedBytes,
                    mappings,
                    member.products().len(),
                )?;
            }
        }
        Ok((count, mappings))
    }
}

impl<D: AssetDomain> RuntimeFamily for TypedFamily<D> {
    fn family_type(&self) -> TypeId {
        TypeId::of::<D>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire_member(
        &self,
        base: &RuntimeContentBase,
        index: usize,
    ) -> Result<DecodedAsset, RuntimeContentError> {
        let member = &base.catalog().members()[index];
        let codec = self
            .codec(member)
            .ok_or(RuntimeContentError::CodecNotSelected)?;
        let product = *member
            .products()
            .first()
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        let work = base
            .inner
            .budget
            .reserve(0, size_of::<RuntimeContentAsset<D::Product>>())?;
        let asset = (codec.acquire)(base, product)?;
        Ok(DecodedAsset {
            member: index,
            family: TypeId::of::<D>(),
            asset: Box::new(asset),
            _work: work,
        })
    }

    fn metadata_bytes(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<usize, RuntimeContentError> {
        let (count, mappings) = self.counts(base, assets)?;
        if count == 0 {
            return Ok(0);
        }
        count
            .checked_mul(size_of::<RuntimeContentAsset<D::Product>>())
            .and_then(|bytes| {
                bytes.checked_add(mappings.checked_mul(size_of::<(AssetProductRef, usize)>())?)
            })
            .and_then(|bytes| bytes.checked_add(size_of::<ProjectAssetCollection<D::Product>>()))
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::RetainedBytes,
            })
    }

    fn collect(
        &self,
        base: &RuntimeContentBase,
        assets: &[DecodedAsset],
    ) -> Result<Option<Box<dyn ErasedCollection>>, RuntimeContentError> {
        let (count, mapping_count) = self.counts(base, assets)?;
        if count == 0 {
            return Ok(None);
        }
        let mut products = Vec::with_capacity(count);
        let mut mappings = Vec::with_capacity(mapping_count);
        for asset in assets {
            if asset.family != TypeId::of::<D>() {
                continue;
            }
            let member = &base.catalog().members()[asset.member];
            let acquisition = asset
                .asset
                .downcast_ref::<RuntimeContentAsset<D::Product>>()
                .expect("the typed family created this exact acquisition")
                .clone();
            let index = products.len();
            products.push(acquisition);
            mappings.extend(member.products().iter().map(|product| (*product, index)));
        }
        runtime_collection::<D>(products, mappings).map(Some)
    }
}

#[cfg(test)]
mod tests;
