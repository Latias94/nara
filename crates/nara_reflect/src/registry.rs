//! Component catalog validation, native bindings, codecs, and migrations.

use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    sync::{Arc, Weak},
};

use bevy_reflect::{GetTypeRegistration, TypeRegistry};
use nara_diagnostic::{DiagnosticCode, SafeSummary};
use nara_ecs::{
    __private::{PersistentComponentMetadataError, validate_persistent_component_registration},
    Component, Entity, World,
};

use crate::{
    ComponentDomainError, ComponentFieldPath, ComponentFieldPathSegment, ComponentMemberPath,
    ComponentValue, PersistentComponentProvider,
    codec::{ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext},
    effective::{ComponentValueMaterializationLimits, effective_component_value},
    migration::{ComponentMigration, ComponentMigrationError, MigratedComponentValue},
    persistent_apply::{
        ErasedPreparedComponentCandidate, PreparedComponent, PreparedComponentCandidate,
    },
    provider::{
        ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
        ComponentSchemaOwnerId, ComponentSchemaProviderDefinition, ComponentSchemaProviderReceipt,
        ComponentSchemaProviderSourceError, ExecutableRegistryFingerprint,
        ResolvedComponentSchemaProvider, SchemaCompositionFingerprint,
    },
    schema::{
        AliasError, CatalogFingerprint, ComponentCapability, ComponentFieldId,
        ComponentFieldSchema, ComponentNumericDomain, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentShapeError, ComponentShapeErrorKind, ComponentTypeId,
        ComponentValueKind, MAX_SHAPE_DEPTH, validate_alias, validate_component_shape_budget,
        validate_component_value_budget,
    },
};

mod member_lineage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRegistryError {
    Frozen,
    NotFrozen,
    InvalidSchemaOwnerId {
        owner: ComponentSchemaOwnerId,
    },
    SchemaProviderSourceRejected {
        provider: nara_app::PluginSchemaProviderId,
        source: ComponentSchemaProviderSourceError,
    },
    SchemaProviderSourcePanicked {
        provider: nara_app::PluginSchemaProviderId,
    },
    SchemaProviderCatalogMismatch {
        provider: nara_app::PluginSchemaProviderId,
    },
    SchemaProviderCandidateAuthorityChanged {
        provider: nara_app::PluginSchemaProviderId,
    },
    NestedSchemaProviderRegistration {
        provider: nara_app::PluginSchemaProviderId,
    },
    MixedSchemaRegistrationModes {
        provider: nara_app::PluginSchemaProviderId,
    },
    RawSchemaRegistrationInProviderComposition,
    SchemaCompositionUnavailable,
    DivergentSchemaOwnerReceipt {
        owner: ComponentSchemaOwnerId,
    },
    MissingSchemaProviderReceipt {
        provider: nara_app::PluginSchemaProviderId,
    },
    UndeclaredSchemaProviderReceipt {
        provider: nara_app::PluginSchemaProviderId,
    },
    DivergentSchemaProviderReceipt {
        provider: nara_app::PluginSchemaProviderId,
    },
    DuplicateComponentId(ComponentTypeId),
    DuplicateNativeBinding(ComponentTypeId),
    DuplicateComponentRustType {
        rust_type_path: String,
        existing_component_id: ComponentTypeId,
        requested_component_id: ComponentTypeId,
    },
    UnknownComponentId(ComponentTypeId),
    MissingNativeBinding {
        component_id: ComponentTypeId,
    },
    PersistentComponentRequiresImplicitComponents {
        component_id: ComponentTypeId,
    },
    PersistentComponentHasLifecycleHook {
        component_id: ComponentTypeId,
    },
    InvalidComponentTypeId {
        component_id: ComponentTypeId,
    },
    InvalidComponentFieldId {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    InvalidAlias {
        component_id: ComponentTypeId,
        field_id: Option<ComponentFieldId>,
        error: AliasError,
    },
    DuplicateAlias {
        component_id: ComponentTypeId,
        field_id: Option<ComponentFieldId>,
        alias: String,
    },
    InvalidComponentSchemaVersion {
        component_id: ComponentTypeId,
    },
    InvalidComponentCapability {
        component_id: ComponentTypeId,
        capability: ComponentCapability,
    },
    DuplicateComponentFieldId {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    DuplicateComponentFieldPath {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
    },
    OverlappingComponentFieldPaths {
        component_id: ComponentTypeId,
        first: ComponentFieldPath,
        second: ComponentFieldPath,
    },
    DuplicateTypeTombstone(ComponentTypeId),
    DuplicateFieldTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    ActiveTypeIsTombstoned(ComponentTypeId),
    ActiveFieldIsTombstoned {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    DuplicateMemberTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    ActiveMemberIsTombstoned {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    MissingMemberTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    ReactivatedMember {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    InvalidComponentFieldDefault {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        expected: ComponentValueKind,
        actual: ComponentValueKind,
    },
    InvalidComponentFieldShape {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        source: ComponentShapeError,
    },
    InvalidComponentFieldDefaultShape {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        source: ComponentShapeError,
    },
    InvalidComponentFieldNumericDomain {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        numeric_domain: ComponentNumericDomain,
        value_kind: ComponentValueKind,
    },
    ComponentFieldDefaultOutsideNumericDomain {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        numeric_domain: ComponentNumericDomain,
    },
    InvalidComponentAuthoringInitialValue {
        component_id: ComponentTypeId,
        source: ComponentCodecError,
    },
    InvalidFieldCapability {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        capability: ComponentCapability,
    },
    InvalidCatalogGeneration {
        expected: u64,
        actual: u64,
    },
    CatalogGenerationExhausted {
        generation: u64,
    },
    InvalidCatalogPredecessor {
        expected: Option<CatalogFingerprint>,
        actual: Option<CatalogFingerprint>,
    },
    MissingCatalogPredecessor {
        generation: u64,
    },
    MissingTypeTombstone {
        component_id: ComponentTypeId,
    },
    MissingFieldTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    ReactivatedTypeId {
        component_id: ComponentTypeId,
    },
    ReactivatedFieldId {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    ComponentSchemaVersionRegressed {
        component_id: ComponentTypeId,
        previous: ComponentSchemaVersion,
        current: ComponentSchemaVersion,
    },
    ComponentSchemaChangedWithoutVersionBump {
        component_id: ComponentTypeId,
    },
    DuplicateComponentMigration {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
    },
    InvalidComponentMigration {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
        to_version: ComponentSchemaVersion,
    },
    MissingComponentMigrationChain {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
        target_version: ComponentSchemaVersion,
    },
}

impl Display for ComponentRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frozen => formatter.write_str("component registry is frozen"),
            Self::NotFrozen => formatter.write_str("component registry is not frozen"),
            Self::InvalidSchemaOwnerId { owner } => {
                write!(formatter, "component schema owner ID '{owner}' is invalid")
            }
            Self::SchemaProviderSourceRejected { provider, source } => write!(
                formatter,
                "component schema provider '{provider}' source was rejected: {source}"
            ),
            Self::SchemaProviderSourcePanicked { provider } => write!(
                formatter,
                "component schema provider '{provider}' source panicked"
            ),
            Self::SchemaProviderCatalogMismatch { provider } => write!(
                formatter,
                "component schema provider '{provider}' executable candidate differs from its declared owner head"
            ),
            Self::SchemaProviderCandidateAuthorityChanged { provider } => write!(
                formatter,
                "component schema provider '{provider}' replaced its owner-local registry candidate"
            ),
            Self::NestedSchemaProviderRegistration { provider } => write!(
                formatter,
                "component schema provider '{provider}' cannot register inside an owner-local candidate"
            ),
            Self::MixedSchemaRegistrationModes { provider } => write!(
                formatter,
                "component schema provider '{provider}' cannot be mixed with ownerless raw registry registration"
            ),
            Self::RawSchemaRegistrationInProviderComposition => formatter.write_str(
                "ownerless raw schema registration cannot mutate a provider-owned composition",
            ),
            Self::SchemaCompositionUnavailable => formatter.write_str(
                "component registry does not contain a provider-owned Runtime composition",
            ),
            Self::DivergentSchemaOwnerReceipt { owner } => write!(
                formatter,
                "component schema owner '{owner}' has a different lineage receipt"
            ),
            Self::MissingSchemaProviderReceipt { provider } => write!(
                formatter,
                "component schema provider '{provider}' has no executable behavior receipt"
            ),
            Self::UndeclaredSchemaProviderReceipt { provider } => write!(
                formatter,
                "component schema provider '{provider}' is not declared by the installed plugins"
            ),
            Self::DivergentSchemaProviderReceipt { provider } => write!(
                formatter,
                "component schema provider '{provider}' has a different executable behavior receipt"
            ),
            Self::DuplicateComponentId(id) => {
                write!(formatter, "component ID '{id}' is already registered")
            }
            Self::DuplicateNativeBinding(id) => {
                write!(
                    formatter,
                    "component ID '{id}' already has a native binding"
                )
            }
            Self::DuplicateComponentRustType {
                rust_type_path,
                existing_component_id,
                requested_component_id,
            } => write!(
                formatter,
                "Rust component type '{rust_type_path}' is already bound to '{existing_component_id}', not '{requested_component_id}'"
            ),
            Self::UnknownComponentId(id) => {
                write!(formatter, "component ID '{id}' is not registered")
            }
            Self::MissingNativeBinding { component_id } => {
                write!(
                    formatter,
                    "component ID '{component_id}' has no native binding"
                )
            }
            Self::PersistentComponentRequiresImplicitComponents { component_id } => write!(
                formatter,
                "persistent component ID '{component_id}' declares implicit required components"
            ),
            Self::PersistentComponentHasLifecycleHook { component_id } => write!(
                formatter,
                "persistent component ID '{component_id}' declares an intrinsic lifecycle hook"
            ),
            Self::InvalidComponentTypeId { component_id } => {
                write!(formatter, "component ID '{component_id}' is invalid")
            }
            Self::InvalidComponentFieldId {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' has invalid field ID '{field_id}'"
            ),
            Self::InvalidAlias {
                component_id,
                field_id,
                error,
            } => match field_id {
                Some(field_id) => write!(
                    formatter,
                    "component ID '{component_id}' field ID '{field_id}' has invalid alias: {error}"
                ),
                None => write!(
                    formatter,
                    "component ID '{component_id}' has invalid alias: {error}"
                ),
            },
            Self::DuplicateAlias {
                component_id,
                field_id,
                alias,
            } => match field_id {
                Some(field_id) => write!(
                    formatter,
                    "component ID '{component_id}' field ID '{field_id}' repeats alias '{alias}'"
                ),
                None => write!(
                    formatter,
                    "component ID '{component_id}' repeats alias '{alias}'"
                ),
            },
            Self::InvalidComponentSchemaVersion { component_id } => write!(
                formatter,
                "component ID '{component_id}' has schema version zero"
            ),
            Self::InvalidComponentCapability {
                component_id,
                capability,
            } => write!(
                formatter,
                "component ID '{component_id}' cannot use field-only {capability:?} capability"
            ),
            Self::DuplicateComponentFieldId {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' repeats field ID '{field_id}'"
            ),
            Self::DuplicateComponentFieldPath { component_id, path } => write!(
                formatter,
                "component ID '{component_id}' repeats field path '{path}'"
            ),
            Self::OverlappingComponentFieldPaths {
                component_id,
                first,
                second,
            } => write!(
                formatter,
                "component ID '{component_id}' has overlapping field paths '{first}' and '{second}'"
            ),
            Self::DuplicateTypeTombstone(id) => {
                write!(formatter, "component type tombstone '{id}' is duplicated")
            }
            Self::DuplicateFieldTombstone {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' repeats field tombstone '{field_id}'"
            ),
            Self::ActiveTypeIsTombstoned(id) => write!(
                formatter,
                "component ID '{id}' is both active and tombstoned"
            ),
            Self::ActiveFieldIsTombstoned {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' is both active and tombstoned"
            ),
            Self::DuplicateMemberTombstone {
                component_id,
                field_id,
                path,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' repeats member tombstone {path:?}"
            ),
            Self::ActiveMemberIsTombstoned {
                component_id,
                field_id,
                path,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' has an active member in retired scope {path:?}"
            ),
            Self::MissingMemberTombstone {
                component_id,
                field_id,
                path,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' lacks member tombstone {path:?}"
            ),
            Self::ReactivatedMember {
                component_id,
                field_id,
                path,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' reactivated retired member scope {path:?}"
            ),
            Self::InvalidComponentFieldDefault {
                component_id,
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "component ID '{component_id}' field '{path}' default has kind {actual:?}, expected {expected:?}"
            ),
            Self::InvalidComponentFieldShape {
                component_id,
                field_id,
                source,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' has invalid shape: {source}"
            ),
            Self::InvalidComponentFieldDefaultShape {
                component_id,
                path,
                source,
            } => write!(
                formatter,
                "component ID '{component_id}' field '{path}' default has invalid shape: {source}"
            ),
            Self::InvalidComponentFieldNumericDomain {
                component_id,
                field_id,
                numeric_domain,
                value_kind,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' numeric domain {numeric_domain:?} is incompatible with {value_kind:?}"
            ),
            Self::ComponentFieldDefaultOutsideNumericDomain {
                component_id,
                path,
                numeric_domain,
            } => write!(
                formatter,
                "component ID '{component_id}' field '{path}' default is outside numeric domain {numeric_domain:?}"
            ),
            Self::InvalidComponentAuthoringInitialValue {
                component_id,
                source,
            } => write!(
                formatter,
                "component ID '{component_id}' authoring initial value is invalid: {source}"
            ),
            Self::InvalidFieldCapability {
                component_id,
                field_id,
                capability,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' has invalid {capability:?} capability"
            ),
            Self::InvalidCatalogGeneration { expected, actual } => write!(
                formatter,
                "component catalog generation is {actual}, expected {expected}"
            ),
            Self::CatalogGenerationExhausted { generation } => write!(
                formatter,
                "component catalog generation {generation} has no successor"
            ),
            Self::InvalidCatalogPredecessor { .. } => {
                formatter.write_str("component catalog predecessor fingerprint is invalid")
            }
            Self::MissingCatalogPredecessor { generation } => write!(
                formatter,
                "component catalog generation {generation} must identify its predecessor"
            ),
            Self::MissingTypeTombstone { component_id } => write!(
                formatter,
                "removed component ID '{component_id}' is missing a tombstone"
            ),
            Self::MissingFieldTombstone {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' removed field ID '{field_id}' without a tombstone"
            ),
            Self::ReactivatedTypeId { component_id } => write!(
                formatter,
                "tombstoned component ID '{component_id}' was reactivated"
            ),
            Self::ReactivatedFieldId {
                component_id,
                field_id,
            } => write!(
                formatter,
                "component ID '{component_id}' reactivated tombstoned field ID '{field_id}'"
            ),
            Self::ComponentSchemaVersionRegressed {
                component_id,
                previous,
                current,
            } => write!(
                formatter,
                "component ID '{component_id}' schema version regressed from {} to {}",
                previous.get(),
                current.get()
            ),
            Self::ComponentSchemaChangedWithoutVersionBump { component_id } => write!(
                formatter,
                "component ID '{component_id}' changed versioned schema semantics without a version bump"
            ),
            Self::DuplicateComponentMigration {
                component_id,
                from_version,
            } => write!(
                formatter,
                "component ID '{component_id}' already has a migration from version {}",
                from_version.get()
            ),
            Self::InvalidComponentMigration {
                component_id,
                from_version,
                to_version,
            } => write!(
                formatter,
                "component ID '{component_id}' has invalid migration {} -> {}",
                from_version.get(),
                to_version.get()
            ),
            Self::MissingComponentMigrationChain {
                component_id,
                from_version,
                target_version,
            } => write!(
                formatter,
                "component ID '{component_id}' has no complete migration chain from version {} to {}",
                from_version.get(),
                target_version.get()
            ),
        }
    }
}

impl Error for ComponentRegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComponentAuthoringInitialValue { source, .. } => Some(source),
            Self::InvalidComponentFieldShape { source, .. }
            | Self::InvalidComponentFieldDefaultShape { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentProjectionError {
    RegistryNotFrozen,
    UnknownComponentId(ComponentTypeId),
    MissingComponentCapability {
        component_id: ComponentTypeId,
        capability: ComponentCapability,
    },
    ProjectionRequired {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        capability: ComponentCapability,
    },
}

impl Display for ComponentProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryNotFrozen => formatter.write_str("component registry is not frozen"),
            Self::UnknownComponentId(id) => write!(formatter, "component ID '{id}' is unknown"),
            Self::MissingComponentCapability {
                component_id,
                capability,
            } => write!(
                formatter,
                "component ID '{component_id}' lacks {capability:?} capability"
            ),
            Self::ProjectionRequired {
                component_id,
                field_id,
                capability,
            } => write!(
                formatter,
                "component ID '{component_id}' field ID '{field_id}' lacks {capability:?} capability; an explicit projection is required"
            ),
        }
    }
}

impl Error for ComponentProjectionError {}

trait ComponentCodec: Send + Sync {
    fn preflight_with_context(
        &self,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<ErasedPreparedComponentCandidate, ComponentCodecError>;

    fn encode_with_context(
        &self,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Option<ComponentValue>, ComponentCodecError>;
}

struct FnComponentCodec<T, Preflight, Encode> {
    preflight: Preflight,
    encode: Encode,
    component: PhantomData<fn() -> T>,
}

impl<T, Preflight, Encode> ComponentCodec for FnComponentCodec<T, Preflight, Encode>
where
    T: Component,
    Preflight: for<'a> Fn(
            &ComponentValue,
            &mut ComponentDecodeContext<'a>,
        ) -> Result<PreparedComponentCandidate<T>, ComponentCodecError>
        + Send
        + Sync
        + 'static,
    Encode: for<'a> Fn(
            &T,
            &World,
            &ComponentEncodeContext<'a>,
        ) -> Result<ComponentValue, ComponentCodecError>
        + Send
        + Sync
        + 'static,
{
    fn preflight_with_context(
        &self,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<ErasedPreparedComponentCandidate, ComponentCodecError> {
        (self.preflight)(value, context).map(PreparedComponentCandidate::erase)
    }

    fn encode_with_context(
        &self,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        world
            .get::<T>(entity)
            .map(|component| (self.encode)(component, world, context))
            .transpose()
    }
}

struct NativeComponentBinding {
    rust_type_path: &'static str,
    validate_registration: fn() -> Result<(), PersistentComponentMetadataError>,
    codec: Box<dyn ComponentCodec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryBuildMode {
    Undecided,
    RawOwnerLocal,
    ProviderComposition,
    ProviderCallback,
}

struct RegistryData {
    catalog: ComponentSchemaCatalog,
    previous_catalog: Option<ComponentSchemaCatalog>,
    raw_owner: Option<ComponentSchemaOwnerId>,
    build_mode: RegistryBuildMode,
    contributions: BTreeMap<ComponentSchemaOwnerId, ComponentSchemaContributionReceipt>,
    provider_owners: BTreeMap<nara_app::PluginSchemaProviderId, ComponentSchemaOwnerId>,
    component_owners: BTreeMap<ComponentTypeId, ComponentSchemaOwnerId>,
    type_registry: TypeRegistry,
    rust_type_ids: HashMap<TypeId, ComponentTypeId>,
    bindings: BTreeMap<ComponentTypeId, NativeComponentBinding>,
    migrations: BTreeMap<(ComponentTypeId, ComponentSchemaVersion), ComponentMigration>,
    type_index: BTreeMap<ComponentTypeId, usize>,
    field_index: BTreeMap<(ComponentTypeId, ComponentFieldId), (usize, usize)>,
    path_indexes: BTreeMap<ComponentTypeId, SchemaPathIndex>,
    type_tombstone_index: BTreeSet<ComponentTypeId>,
}

impl RegistryData {
    fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.type_index
            .get(id)
            .and_then(|index| self.catalog.components.get(*index))
    }

    fn resolve_field(
        &self,
        component_id: &ComponentTypeId,
        field_id: &ComponentFieldId,
    ) -> Option<&ComponentFieldSchema> {
        self.field_index
            .get(&(component_id.clone(), field_id.clone()))
            .and_then(|(component_index, field_index)| {
                self.catalog
                    .components
                    .get(*component_index)
                    .and_then(|schema| schema.fields.get(*field_index))
            })
    }

    fn first_generation() -> Self {
        Self::with_catalog(ComponentSchemaCatalog::default(), None)
    }

    fn successor_of(
        owner: ComponentSchemaOwnerId,
        previous_catalog: ComponentSchemaCatalog,
    ) -> Result<Self, ComponentRegistryError> {
        let catalog = ComponentSchemaCatalog::successor_of(&previous_catalog).map_err(|error| {
            ComponentRegistryError::CatalogGenerationExhausted {
                generation: error.generation(),
            }
        })?;
        let mut data = Self::with_catalog(catalog, Some(previous_catalog));
        data.raw_owner = Some(owner);
        data.build_mode = RegistryBuildMode::RawOwnerLocal;
        Ok(data)
    }

    fn with_catalog(
        catalog: ComponentSchemaCatalog,
        previous_catalog: Option<ComponentSchemaCatalog>,
    ) -> Self {
        let path_indexes = build_schema_path_indexes(&catalog);
        let type_tombstone_index = catalog.type_tombstones.iter().cloned().collect();
        Self {
            catalog,
            previous_catalog,
            raw_owner: None,
            build_mode: RegistryBuildMode::Undecided,
            contributions: BTreeMap::new(),
            provider_owners: BTreeMap::new(),
            component_owners: BTreeMap::new(),
            type_registry: TypeRegistry::default(),
            rust_type_ids: HashMap::new(),
            bindings: BTreeMap::new(),
            migrations: BTreeMap::new(),
            type_index: BTreeMap::new(),
            field_index: BTreeMap::new(),
            path_indexes,
            type_tombstone_index,
        }
    }
}

fn contribution_for_provider(
    data: &RegistryData,
    provider: nara_app::PluginSchemaProviderId,
) -> Option<&ComponentSchemaContributionReceipt> {
    data.provider_owners
        .get(&provider)
        .and_then(|owner| data.contributions.get(owner))
}

fn insert_contribution(data: &mut RegistryData, receipt: ComponentSchemaContributionReceipt) {
    let owner = receipt.owner().owner();
    let provider = receipt.provider().provider();
    let previous_owner = data.provider_owners.insert(provider, owner);
    let previous_receipt = data.contributions.insert(owner, receipt);
    debug_assert!(previous_owner.is_none());
    debug_assert!(previous_receipt.is_none());
}

enum RegistryState {
    Building(Box<RegistryData>),
    Frozen(Arc<RegistryData>),
    Transitioning,
}

pub struct ComponentRegistry {
    state: RegistryState,
    instance_token: Arc<()>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable schema and native codec execution authority published by a successful freeze.
///
/// A snapshot retains its exact definition independently of the registry that published it.
/// Target-World and asset admission still happen when a prepared component is applied.
///
/// Codec execution is unavailable on a building registry:
///
/// ```compile_fail,E0599
/// use nara_reflect::{ComponentRegistry, ComponentTypeId, ComponentValue};
/// let registry = ComponentRegistry::new();
/// let _ = registry.preflight_component(
///     &ComponentTypeId::new("example.Health"),
///     &ComponentValue::I64(42),
/// );
/// ```
///
/// ```compile_fail,E0599
/// use nara_ecs::World;
/// use nara_reflect::{ComponentRegistry, ComponentTypeId};
/// let registry = ComponentRegistry::new();
/// let mut world = World::new();
/// let entity = world.spawn_empty().id();
/// let _ = registry.encode_component(&ComponentTypeId::new("example.Health"), &world, entity);
/// ```
#[derive(Clone)]
pub struct ComponentRegistrySnapshot(Arc<RegistryData>);

/// Transient identity witness for one exact frozen registry snapshot.
///
/// The witness neither retains nor exposes schema bindings, codecs, or migrations.
#[doc(hidden)]
#[derive(Clone)]
pub struct ComponentRegistrySnapshotWitness(Weak<RegistryData>);

impl ComponentRegistrySnapshotWitness {
    pub(crate) fn matches(&self, snapshot: &ComponentRegistrySnapshot) -> bool {
        Weak::ptr_eq(&self.0, &Arc::downgrade(&snapshot.0))
    }
}

impl ComponentRegistrySnapshot {
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    #[must_use]
    pub fn catalog(&self) -> &ComponentSchemaCatalog {
        &self.0.catalog
    }

    #[must_use]
    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.0.schema(id)
    }

    #[must_use]
    pub fn resolve_field(
        &self,
        component_id: &ComponentTypeId,
        field_id: &ComponentFieldId,
    ) -> Option<&ComponentFieldSchema> {
        self.0.resolve_field(component_id, field_id)
    }

    pub fn contribution_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaContributionReceipt> + '_ {
        self.0.contributions.values().copied()
    }

    pub fn provider_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaProviderReceipt> + '_ {
        self.0.provider_owners.values().map(|owner| {
            self.0
                .contributions
                .get(owner)
                .unwrap_or_else(|| unreachable!("provider index must reference one contribution"))
                .provider()
        })
    }

    pub fn owner_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaOwnerContributionReceipt> + '_ {
        self.0.contributions.values().map(|receipt| receipt.owner())
    }

    #[must_use]
    pub fn owner_receipt(
        &self,
        owner: ComponentSchemaOwnerId,
    ) -> Option<ComponentSchemaOwnerContributionReceipt> {
        self.0
            .contributions
            .get(&owner)
            .map(|receipt| receipt.owner())
    }

    #[must_use]
    pub fn component_schema_owner(
        &self,
        component: &ComponentTypeId,
    ) -> Option<ComponentSchemaOwnerId> {
        self.0.component_owners.get(component).copied()
    }

    #[must_use]
    pub fn provider_receipt(
        &self,
        provider: nara_app::PluginSchemaProviderId,
    ) -> Option<ComponentSchemaProviderReceipt> {
        contribution_for_provider(&self.0, provider).map(|receipt| receipt.provider())
    }

    pub fn schema_composition_fingerprint(
        &self,
    ) -> Result<SchemaCompositionFingerprint, ComponentRegistryError> {
        if self.0.build_mode != RegistryBuildMode::ProviderComposition {
            return Err(ComponentRegistryError::SchemaCompositionUnavailable);
        }
        Ok(SchemaCompositionFingerprint::from_owner_receipts(
            self.owner_receipts(),
        ))
    }

    pub fn executable_registry_fingerprint(
        &self,
    ) -> Result<ExecutableRegistryFingerprint, ComponentRegistryError> {
        Ok(ExecutableRegistryFingerprint::from_contributions(
            self.schema_composition_fingerprint()?,
            self.contribution_receipts(),
        ))
    }

    pub(crate) fn witness(&self) -> ComponentRegistrySnapshotWitness {
        ComponentRegistrySnapshotWitness(Arc::downgrade(&self.0))
    }

    /// Decodes and binds one component using this exact frozen definition.
    pub fn preflight_component(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
    ) -> Result<PreparedComponent, ComponentCodecError> {
        let mut context = ComponentDecodeContext::new();
        self.preflight_component_with_context(id, value, &mut context)
    }

    /// Shares the caller's preparation context across a complete component batch.
    pub fn preflight_component_with_context(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<PreparedComponent, ComponentCodecError> {
        let (schema, binding) = self.execution_binding(id)?;
        validate_schema_capabilities(schema, [ComponentCapability::Scene])
            .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
        let path_index = self
            .0
            .path_indexes
            .get(id)
            .ok_or_else(missing_schema_path_index_error)?;
        let effective = effective_component_value(schema, value, context.materialization_limits())?;
        validate_component_value_coverage(schema, path_index, effective.as_ref())?;
        binding
            .codec
            .preflight_with_context(effective.as_ref(), context)
            .map(|prepared| prepared.bind(id.clone()))
    }

    /// Encodes the selected component, returning `None` only for entity/component absence.
    pub fn encode_component(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        let context = ComponentEncodeContext::new();
        self.encode_component_with_context(id, world, entity, &context)
    }

    pub fn encode_component_with_context(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        let (schema, binding) = self.execution_binding(id)?;
        validate_schema_capabilities(schema, [ComponentCapability::Scene])
            .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
        let encoded = binding.codec.encode_with_context(world, entity, context)?;
        if let Some(value) = &encoded {
            let path_index = self
                .0
                .path_indexes
                .get(id)
                .ok_or_else(missing_schema_path_index_error)?;
            validate_component_value_coverage(schema, path_index, value)?;
        }
        Ok(encoded)
    }

    fn execution_binding(
        &self,
        id: &ComponentTypeId,
    ) -> Result<(&ComponentSchema, &NativeComponentBinding), ComponentCodecError> {
        let schema = self
            .schema(id)
            .ok_or_else(|| ComponentCodecError::UnknownComponent {
                component_id: id.clone(),
            })?;
        let binding =
            self.0
                .bindings
                .get(id)
                .ok_or_else(|| ComponentCodecError::MissingNativeBinding {
                    component_id: id.clone(),
                })?;
        Ok((schema, binding))
    }
}

impl ComponentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RegistryState::Building(Box::new(RegistryData::first_generation())),
            instance_token: Arc::new(()),
        }
    }

    fn owner_local_successor(
        owner: ComponentSchemaOwnerId,
        previous_catalog: ComponentSchemaCatalog,
    ) -> Result<Self, ComponentRegistryError> {
        if !owner.is_valid() {
            return Err(ComponentRegistryError::InvalidSchemaOwnerId { owner });
        }
        Ok(Self {
            state: RegistryState::Building(Box::new(RegistryData::successor_of(
                owner,
                previous_catalog,
            )?)),
            instance_token: Arc::new(()),
        })
    }

    /// Starts a building registry from one trusted single-owner catalog declaration.
    ///
    /// `current` and `predecessor` must both belong to `owner`. A flattened Runtime composition has
    /// no owner-attribution proof and must never be supplied here. This lower-level source tool
    /// cannot publish product composition or executable fingerprints; normal plugin and product
    /// paths use [`ComponentSchemaProviderDefinition`] instead.
    pub fn from_owner_catalog_candidate(
        owner: ComponentSchemaOwnerId,
        catalog: ComponentSchemaCatalog,
        predecessor: Option<ComponentSchemaCatalog>,
    ) -> Result<Self, ComponentRegistryError> {
        if !owner.is_valid() {
            return Err(ComponentRegistryError::InvalidSchemaOwnerId { owner });
        }
        let (catalog, predecessor) = prepare_owner_catalogs(catalog, predecessor)?;
        let mut data = RegistryData::with_catalog(catalog, predecessor);
        data.raw_owner = Some(owner);
        data.build_mode = RegistryBuildMode::RawOwnerLocal;
        Ok(Self {
            state: RegistryState::Building(Box::new(data)),
            instance_token: Arc::new(()),
        })
    }

    /// Constructs another frozen registry view over the exact same executable behavior snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: ComponentRegistrySnapshot) -> Self {
        Self {
            state: RegistryState::Frozen(snapshot.0),
            instance_token: Arc::new(()),
        }
    }

    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        matches!(self.state, RegistryState::Frozen(_))
    }

    pub(crate) fn validate_declared_providers(
        &self,
        declared: &BTreeSet<nara_app::PluginSchemaProviderId>,
    ) -> Result<(), ComponentRegistryError> {
        let actual = &self.data().provider_owners;
        if let Some(provider) = declared
            .iter()
            .find(|provider| !actual.contains_key(provider))
        {
            return Err(ComponentRegistryError::MissingSchemaProviderReceipt {
                provider: *provider,
            });
        }
        if let Some(provider) = actual.keys().find(|provider| !declared.contains(provider)) {
            return Err(ComponentRegistryError::UndeclaredSchemaProviderReceipt {
                provider: *provider,
            });
        }
        Ok(())
    }

    pub fn freeze(&mut self) -> Result<&mut Self, ComponentRegistryError> {
        if self.is_frozen() {
            return Ok(self);
        }

        let RegistryState::Building(building) = &self.state else {
            unreachable!("component registry transition is synchronous")
        };
        let publish_mode = match building.build_mode {
            RegistryBuildMode::Undecided => RegistryBuildMode::ProviderComposition,
            mode => mode,
        };
        let (catalog, type_index, field_index, path_indexes) = validate_registry(building)?;

        let state = std::mem::replace(&mut self.state, RegistryState::Transitioning);
        let RegistryState::Building(mut data) = state else {
            unreachable!("validated component registry must still be building")
        };
        data.build_mode = publish_mode;
        data.catalog = catalog;
        if let Some(owner) = data.raw_owner {
            data.component_owners.extend(
                data.catalog
                    .components
                    .iter()
                    .map(|schema| (schema.id().clone(), owner)),
            );
        }
        data.type_index = type_index;
        data.field_index = field_index;
        data.path_indexes = path_indexes;
        self.state = RegistryState::Frozen(Arc::from(data));
        Ok(self)
    }

    pub fn snapshot(&self) -> Result<ComponentRegistrySnapshot, ComponentRegistryError> {
        match &self.state {
            RegistryState::Frozen(snapshot) => Ok(ComponentRegistrySnapshot(Arc::clone(snapshot))),
            RegistryState::Building(_) => Err(ComponentRegistryError::NotFrozen),
            RegistryState::Transitioning => {
                unreachable!("component registry transition is synchronous")
            }
        }
    }

    #[must_use]
    pub fn shares_snapshot(&self, snapshot: &ComponentRegistrySnapshot) -> bool {
        matches!(
            &self.state,
            RegistryState::Frozen(current) if Arc::ptr_eq(current, &snapshot.0)
        )
    }

    #[doc(hidden)]
    #[must_use]
    pub(crate) fn shares_instance_token(&self, token: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.instance_token, token)
    }

    #[doc(hidden)]
    #[must_use]
    pub(crate) fn instance_token(&self) -> &Arc<()> {
        &self.instance_token
    }

    fn validate_provider_registration_mode(
        &self,
        provider: nara_app::PluginSchemaProviderId,
    ) -> Result<(), ComponentRegistryError> {
        match self.data().build_mode {
            RegistryBuildMode::RawOwnerLocal => {
                Err(ComponentRegistryError::MixedSchemaRegistrationModes { provider })
            }
            RegistryBuildMode::ProviderCallback => {
                Err(ComponentRegistryError::NestedSchemaProviderRegistration { provider })
            }
            RegistryBuildMode::Undecided | RegistryBuildMode::ProviderComposition => Ok(()),
        }
    }

    /// Registers a provider into a building registry or validates its stable behavior receipt
    /// against an already frozen snapshot.
    pub(crate) fn register_or_validate_schema_provider(
        &mut self,
        provider: ComponentSchemaProviderDefinition,
    ) -> Result<&mut Self, ComponentRegistryError> {
        self.validate_provider_registration_mode(provider.id())?;
        let resolved = provider.resolve()?;
        self.register_or_validate_resolved_schema_provider(resolved)
    }

    #[doc(hidden)]
    pub(crate) fn register_or_validate_resolved_schema_provider(
        &mut self,
        resolved: ResolvedComponentSchemaProvider,
    ) -> Result<&mut Self, ComponentRegistryError> {
        self.validate_provider_registration_mode(resolved.definition().id())?;
        if self.is_frozen() {
            self.validate_resolved_schema_provider(&resolved)?;
            return Ok(self);
        }

        let provider = resolved.definition().id();
        if let Some(existing) = contribution_for_provider(self.data(), provider) {
            if *existing == resolved.contribution_receipt() {
                return Ok(self);
            }
            return Err(ComponentRegistryError::DivergentSchemaProviderReceipt { provider });
        }

        let owner = resolved.definition().owner();
        if self.data().contributions.contains_key(&owner) {
            return Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner });
        }
        preflight_owner_catalog_merge(self.data(), resolved.current().catalog())?;

        let candidate = build_owner_candidate(&resolved)?;
        preflight_owner_candidate_merge(self.data(), &candidate)?;
        let aggregate = self.building_mut()?;
        aggregate.build_mode = RegistryBuildMode::ProviderComposition;
        merge_owner_candidate(aggregate, candidate);
        Ok(self)
    }

    pub(crate) fn validate_schema_provider(
        &self,
        provider: ComponentSchemaProviderDefinition,
    ) -> Result<(), ComponentRegistryError> {
        self.validate_provider_registration_mode(provider.id())?;
        let resolved = provider.resolve()?;
        if self.is_frozen() || contribution_for_provider(self.data(), provider.id()).is_some() {
            return self.validate_resolved_schema_provider(&resolved);
        }
        if self.data().contributions.contains_key(&provider.owner()) {
            return Err(ComponentRegistryError::DivergentSchemaOwnerReceipt {
                owner: provider.owner(),
            });
        }
        preflight_owner_catalog_merge(self.data(), resolved.current().catalog())
    }

    fn validate_resolved_schema_provider(
        &self,
        resolved: &ResolvedComponentSchemaProvider,
    ) -> Result<(), ComponentRegistryError> {
        let provider = resolved.definition().id();
        let Some(existing) = contribution_for_provider(self.data(), provider) else {
            let owner = resolved.definition().owner();
            if self.data().contributions.contains_key(&owner) {
                return Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner });
            }
            return Err(ComponentRegistryError::MissingSchemaProviderReceipt { provider });
        };
        if existing.provider() != resolved.provider_receipt() {
            return Err(ComponentRegistryError::DivergentSchemaProviderReceipt { provider });
        }

        let owner = resolved.definition().owner();
        if existing.owner() != resolved.owner_receipt() {
            return Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner });
        }
        Ok(())
    }

    #[must_use]
    pub fn catalog_candidate(&self) -> &ComponentSchemaCatalog {
        &self.data().catalog
    }

    pub fn catalog(&self) -> Result<&ComponentSchemaCatalog, ComponentRegistryError> {
        match &self.state {
            RegistryState::Frozen(data) => Ok(&data.catalog),
            RegistryState::Building(_) => Err(ComponentRegistryError::NotFrozen),
            RegistryState::Transitioning => {
                unreachable!("component registry transition is synchronous")
            }
        }
    }

    pub fn register_component_schema(
        &mut self,
        schema: ComponentSchema,
    ) -> Result<&mut Self, ComponentRegistryError> {
        let data = self.raw_building_mut()?;
        if data.path_indexes.contains_key(&schema.id) {
            return Err(ComponentRegistryError::DuplicateComponentId(schema.id));
        }
        let id = schema.id.clone();
        let path_index = SchemaPathIndex::from_schema(&schema);
        data.catalog.components.push(schema);
        data.path_indexes.insert(id, path_index);
        self.commit_raw_registration();
        Ok(self)
    }

    pub fn declare_type_tombstone(
        &mut self,
        component_id: ComponentTypeId,
    ) -> Result<&mut Self, ComponentRegistryError> {
        let data = self.raw_building_mut()?;
        data.catalog.type_tombstones.push(component_id.clone());
        data.type_tombstone_index.insert(component_id);
        self.commit_raw_registration();
        Ok(self)
    }

    pub fn validate_component_registration<T>(
        &self,
        id: &ComponentTypeId,
    ) -> Result<(), ComponentRegistryError>
    where
        T: Component,
    {
        if self.is_frozen() {
            return Err(ComponentRegistryError::Frozen);
        }
        let data = self.data();
        if data.path_indexes.contains_key(id) {
            return Err(ComponentRegistryError::DuplicateComponentId(id.clone()));
        }
        validate_native_binding::<T>(data, id, false)
    }

    pub fn validate_persistent_component<T>(&self) -> Result<(), ComponentRegistryError>
    where
        T: PersistentComponentProvider,
    {
        let schema = T::persistent_component_schema();
        if self.is_frozen() {
            return Err(ComponentRegistryError::Frozen);
        }
        validate_persistent_registration::<T>(self.data(), &schema)
    }

    pub fn register_persistent_component<T>(&mut self) -> Result<&mut Self, ComponentRegistryError>
    where
        T: PersistentComponentProvider,
    {
        self.register_persistent_component_with_codec::<T, _, _>(
            T::persistent_component_schema(),
            T::__decode_persistent_component,
            T::__encode_persistent_component,
        )
    }

    pub fn register_native_component_with_codec<T, Decode, Encode>(
        &mut self,
        id: &ComponentTypeId,
        decode: Decode,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Decode: Fn(&ComponentValue) -> Result<T, ComponentCodecError> + Send + Sync + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        self.register_native_component_codec_with_context::<T, _, _>(
            id,
            move |value, _context| {
                let component = decode(value)?;
                Ok(PreparedComponentCandidate::insert(component))
            },
            move |component, _world, _context| encode(component),
        )
    }

    pub fn register_persistent_component_with_codec<T, Decode, Encode>(
        &mut self,
        schema: ComponentSchema,
        decode: Decode,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Decode: Fn(&ComponentValue) -> Result<T, ComponentCodecError> + Send + Sync + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        self.register_persistent_component_codec_with_context::<T, _, _>(
            schema,
            move |value, _context| {
                let component = decode(value)?;
                Ok(PreparedComponentCandidate::insert(component))
            },
            move |component, _world, _context| encode(component),
        )
    }

    /// Registers deferred preparation and encoding for exactly one component type.
    ///
    /// A candidate for a different component is rejected by Rust, before registration or decode:
    ///
    /// ```compile_fail
    /// use nara_ecs::Component;
    /// use nara_reflect::{
    ///     ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ///     ComponentValue, PreparedComponentCandidate,
    /// };
    ///
    /// #[derive(Component)]
    /// struct Registered;
    /// #[derive(Component)]
    /// struct Other;
    ///
    /// let mut registry = ComponentRegistry::new();
    /// let schema = ComponentSchema::new(
    ///     ComponentTypeId::new("example.Registered"), "Registered", ComponentSchemaVersion::ONE,
    /// );
    /// registry.register_persistent_component_codec::<Registered, _, _>(
    ///     schema,
    ///     |_| Ok(PreparedComponentCandidate::insert(Other)),
    ///     |_: &Registered| Ok(ComponentValue::Null),
    /// );
    /// ```
    pub fn register_persistent_component_codec<T, Preflight, Encode>(
        &mut self,
        schema: ComponentSchema,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponentCandidate<T>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        self.register_persistent_component_codec_with_context::<T, _, _>(
            schema,
            move |value, _context| preflight(value),
            move |component, _world, _context| encode(component),
        )
    }

    /// Registers a typed codec with asset decode and read-only encode context.
    ///
    /// The registry selects `T` from the target entity. The encoder only runs when it is present;
    /// `World` supplies read-only resource context, rather than component selection or absence.
    /// Deferred candidates own their inputs and share the apply batch's scratch asset server.
    ///
    /// ```compile_fail
    /// use nara_ecs::Component;
    /// use nara_reflect::{
    ///     ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ///     ComponentValue, PreparedComponentCandidate,
    /// };
    ///
    /// #[derive(Component)]
    /// struct Registered;
    /// #[derive(Component)]
    /// struct Other;
    ///
    /// let mut registry = ComponentRegistry::new();
    /// let schema = ComponentSchema::new(
    ///     ComponentTypeId::new("example.Registered"), "Registered", ComponentSchemaVersion::ONE,
    /// );
    /// registry.register_persistent_component_codec_with_context::<Registered, _, _>(
    ///     schema,
    ///     |_, _| Ok(PreparedComponentCandidate::with_asset_server(move |_| Ok(Other))),
    ///     |_: &Registered, _, _| Ok(ComponentValue::Null),
    /// );
    /// ```
    pub fn register_persistent_component_codec_with_context<T, Preflight, Encode>(
        &mut self,
        schema: ComponentSchema,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: for<'a> Fn(
                &ComponentValue,
                &mut ComponentDecodeContext<'a>,
            ) -> Result<PreparedComponentCandidate<T>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: for<'a> Fn(
                &T,
                &World,
                &ComponentEncodeContext<'a>,
            ) -> Result<ComponentValue, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        let id = schema.id.clone();
        let data = self.raw_building_mut()?;
        validate_persistent_registration::<T>(data, &schema)?;

        let rust_type_id = TypeId::of::<T>();
        let path_index = SchemaPathIndex::from_schema(&schema);
        data.catalog.components.push(schema);
        data.path_indexes.insert(id.clone(), path_index);
        data.rust_type_ids.insert(rust_type_id, id.clone());
        data.bindings.insert(
            id,
            NativeComponentBinding {
                rust_type_path: std::any::type_name::<T>(),
                validate_registration: validate_persistent_component_registration::<T>,
                codec: Box::new(FnComponentCodec::<T, _, _> {
                    preflight,
                    encode,
                    component: PhantomData,
                }),
            },
        );
        self.commit_raw_registration();
        Ok(self)
    }

    pub fn register_native_component_codec<T, Preflight, Encode>(
        &mut self,
        id: &ComponentTypeId,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponentCandidate<T>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        self.register_native_component_codec_with_context::<T, _, _>(
            id,
            move |value, _context| preflight(value),
            move |component, _world, _context| encode(component),
        )
    }

    pub fn register_native_component_codec_with_context<T, Preflight, Encode>(
        &mut self,
        id: &ComponentTypeId,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: for<'a> Fn(
                &ComponentValue,
                &mut ComponentDecodeContext<'a>,
            ) -> Result<PreparedComponentCandidate<T>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: for<'a> Fn(
                &T,
                &World,
                &ComponentEncodeContext<'a>,
            ) -> Result<ComponentValue, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        let data = self.raw_building_mut()?;
        if !data.path_indexes.contains_key(id) {
            return Err(ComponentRegistryError::UnknownComponentId(id.clone()));
        }
        let persistent_eligible = data
            .catalog
            .components
            .iter()
            .find(|schema| schema.id() == id)
            .is_some_and(|schema| schema.has_capability(ComponentCapability::Scene));
        validate_native_binding::<T>(data, id, persistent_eligible)?;
        let rust_type_id = TypeId::of::<T>();
        data.rust_type_ids.insert(rust_type_id, id.clone());
        data.bindings.insert(
            id.clone(),
            NativeComponentBinding {
                rust_type_path: std::any::type_name::<T>(),
                validate_registration: validate_persistent_component_registration::<T>,
                codec: Box::new(FnComponentCodec::<T, _, _> {
                    preflight,
                    encode,
                    component: PhantomData,
                }),
            },
        );
        self.commit_raw_registration();
        Ok(self)
    }

    pub fn register_reflected_type<T>(&mut self) -> Result<&mut Self, ComponentRegistryError>
    where
        T: GetTypeRegistration,
    {
        self.raw_building_mut()?.type_registry.register::<T>();
        self.commit_raw_registration();
        Ok(self)
    }

    pub fn register_component_migration<Migrate>(
        &mut self,
        id: &ComponentTypeId,
        from_version: ComponentSchemaVersion,
        to_version: ComponentSchemaVersion,
        migrate: Migrate,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        Migrate: Fn(ComponentValue) -> Result<ComponentValue, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        let data = self.raw_building_mut()?;
        if !data.path_indexes.contains_key(id) {
            return Err(ComponentRegistryError::UnknownComponentId(id.clone()));
        }
        if from_version.get() == 0 || to_version <= from_version {
            return Err(ComponentRegistryError::InvalidComponentMigration {
                component_id: id.clone(),
                from_version,
                to_version,
            });
        }
        let key = (id.clone(), from_version);
        if data.migrations.contains_key(&key) {
            return Err(ComponentRegistryError::DuplicateComponentMigration {
                component_id: id.clone(),
                from_version,
            });
        }
        data.migrations.insert(
            key,
            ComponentMigration {
                to_version,
                migrate: Box::new(migrate),
            },
        );
        self.commit_raw_registration();
        Ok(self)
    }

    #[must_use]
    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        match &self.state {
            RegistryState::Frozen(data) => data.schema(id),
            RegistryState::Building(_) => None,
            RegistryState::Transitioning => unreachable!(),
        }
    }

    #[must_use]
    pub fn component_schema_owner(
        &self,
        component: &ComponentTypeId,
    ) -> Option<ComponentSchemaOwnerId> {
        match &self.state {
            RegistryState::Frozen(data) => data.component_owners.get(component).copied(),
            RegistryState::Building(_) => None,
            RegistryState::Transitioning => unreachable!(),
        }
    }

    pub fn schemas(
        &self,
    ) -> Result<impl Iterator<Item = &ComponentSchema>, ComponentRegistryError> {
        let components = match &self.state {
            RegistryState::Frozen(data) => data.catalog.components.as_slice(),
            RegistryState::Building(_) => return Err(ComponentRegistryError::NotFrozen),
            RegistryState::Transitioning => unreachable!(),
        };
        Ok(components.iter())
    }

    #[must_use]
    pub fn resolve_field(
        &self,
        component_id: &ComponentTypeId,
        field_id: &ComponentFieldId,
    ) -> Option<&ComponentFieldSchema> {
        match &self.state {
            RegistryState::Frozen(data) => data.resolve_field(component_id, field_id),
            RegistryState::Building(_) => None,
            RegistryState::Transitioning => unreachable!(),
        }
    }

    #[must_use]
    pub fn schema_for_type<T: 'static>(&self) -> Option<&ComponentSchema> {
        self.data()
            .rust_type_ids
            .get(&TypeId::of::<T>())
            .and_then(|id| self.schema(id))
    }

    #[must_use]
    pub fn native_rust_type_path(&self, id: &ComponentTypeId) -> Option<&'static str> {
        match &self.state {
            RegistryState::Frozen(data) => {
                data.bindings.get(id).map(|binding| binding.rust_type_path)
            }
            RegistryState::Building(_) => None,
            RegistryState::Transitioning => unreachable!(),
        }
    }

    #[must_use]
    pub fn type_registry(&self) -> Result<&TypeRegistry, ComponentRegistryError> {
        match &self.state {
            RegistryState::Frozen(data) => Ok(&data.type_registry),
            RegistryState::Building(_) => Err(ComponentRegistryError::NotFrozen),
            RegistryState::Transitioning => unreachable!(),
        }
    }

    pub fn validate_whole_value_capabilities(
        &self,
        id: &ComponentTypeId,
        capabilities: impl IntoIterator<Item = ComponentCapability>,
    ) -> Result<(), ComponentProjectionError> {
        if !self.is_frozen() {
            return Err(ComponentProjectionError::RegistryNotFrozen);
        }
        let Some(schema) = self.schema(id) else {
            return Err(ComponentProjectionError::UnknownComponentId(id.clone()));
        };
        validate_schema_capabilities(schema, capabilities)
    }

    pub fn migrate_component_value(
        &self,
        id: &ComponentTypeId,
        version: ComponentSchemaVersion,
        value: &ComponentValue,
    ) -> Result<MigratedComponentValue, ComponentMigrationError> {
        self.migrate_component_value_owned(id, version, value.clone())
    }

    /// Migrates an owned component value without cloning the current-version value.
    pub fn migrate_component_value_owned(
        &self,
        id: &ComponentTypeId,
        version: ComponentSchemaVersion,
        value: ComponentValue,
    ) -> Result<MigratedComponentValue, ComponentMigrationError> {
        let Some(schema) = self.schema(id) else {
            return Err(ComponentMigrationError::UnknownComponentId {
                component_id: id.clone(),
            });
        };
        if version == schema.version {
            return Ok(MigratedComponentValue { version, value });
        }
        if version > schema.version {
            return Err(ComponentMigrationError::UnsupportedVersion {
                component_id: id.clone(),
                from_version: version,
                target_version: schema.version,
            });
        }

        let data = self.data();
        let mut current_version = version;
        let mut current_value = value;
        let mut seen_versions = BTreeSet::from([current_version]);
        while current_version != schema.version {
            let Some(migration) = data.migrations.get(&(id.clone(), current_version)) else {
                return Err(ComponentMigrationError::MissingMigration {
                    component_id: id.clone(),
                    from_version: current_version,
                    target_version: schema.version,
                });
            };
            if migration.to_version <= current_version || migration.to_version > schema.version {
                return Err(ComponentMigrationError::UnsupportedVersion {
                    component_id: id.clone(),
                    from_version: current_version,
                    target_version: schema.version,
                });
            }
            let from_version = current_version;
            current_value = (migration.migrate)(current_value).map_err(|error| {
                ComponentMigrationError::MigrationFailed {
                    component_id: id.clone(),
                    from_version,
                    to_version: migration.to_version,
                    error,
                }
            })?;
            current_version = migration.to_version;
            if !seen_versions.insert(current_version) {
                return Err(ComponentMigrationError::UnsupportedVersion {
                    component_id: id.clone(),
                    from_version: current_version,
                    target_version: schema.version,
                });
            }
        }
        Ok(MigratedComponentValue {
            version: current_version,
            value: current_value,
        })
    }

    fn data(&self) -> &RegistryData {
        match &self.state {
            RegistryState::Building(data) => data,
            RegistryState::Frozen(data) => data,
            RegistryState::Transitioning => {
                unreachable!("component registry transition is synchronous")
            }
        }
    }

    fn building_mut(&mut self) -> Result<&mut RegistryData, ComponentRegistryError> {
        match &mut self.state {
            RegistryState::Building(data) => Ok(data),
            RegistryState::Frozen(_) => Err(ComponentRegistryError::Frozen),
            RegistryState::Transitioning => {
                unreachable!("component registry transition is synchronous")
            }
        }
    }

    fn raw_building_mut(&mut self) -> Result<&mut RegistryData, ComponentRegistryError> {
        let data = self.building_mut()?;
        match data.build_mode {
            RegistryBuildMode::Undecided
            | RegistryBuildMode::RawOwnerLocal
            | RegistryBuildMode::ProviderCallback => {}
            RegistryBuildMode::ProviderComposition => {
                return Err(ComponentRegistryError::RawSchemaRegistrationInProviderComposition);
            }
        }
        Ok(data)
    }

    fn commit_raw_registration(&mut self) {
        let data = self
            .building_mut()
            .unwrap_or_else(|_| unreachable!("a successful raw registration remains building"));
        if data.build_mode == RegistryBuildMode::Undecided {
            data.build_mode = RegistryBuildMode::RawOwnerLocal;
        }
    }

    fn into_frozen_data(self) -> RegistryData {
        let RegistryState::Frozen(data) = self.state else {
            unreachable!("only a frozen owner candidate can be merged")
        };
        Arc::try_unwrap(data)
            .unwrap_or_else(|_| unreachable!("private owner candidate snapshot cannot escape"))
    }
}

fn owner_candidate_base(
    resolved: &ResolvedComponentSchemaProvider,
) -> Result<ComponentRegistry, ComponentRegistryError> {
    let mut candidate = resolved.predecessor().map_or_else(
        || {
            let mut candidate = ComponentRegistry::new();
            candidate.building_mut()?.raw_owner = Some(resolved.definition().owner());
            Ok(candidate)
        },
        |predecessor| {
            ComponentRegistry::owner_local_successor(
                resolved.definition().owner(),
                predecessor.catalog().clone(),
            )
        },
    )?;
    candidate.building_mut()?.build_mode = RegistryBuildMode::ProviderCallback;
    Ok(candidate)
}

fn build_owner_candidate(
    resolved: &ResolvedComponentSchemaProvider,
) -> Result<RegistryData, ComponentRegistryError> {
    let mut candidate = owner_candidate_base(resolved)?;
    let instance_token = Arc::clone(candidate.instance_token());
    resolved.definition().validate_into(&candidate)?;
    resolved.definition().register_into(&mut candidate)?;
    if !candidate.shares_instance_token(&instance_token)
        || candidate.data().build_mode != RegistryBuildMode::ProviderCallback
        || candidate.data().raw_owner != Some(resolved.definition().owner())
    {
        return Err(
            ComponentRegistryError::SchemaProviderCandidateAuthorityChanged {
                provider: resolved.definition().id(),
            },
        );
    }
    {
        let data = candidate.building_mut()?;
        insert_contribution(data, resolved.contribution_receipt());
    }
    candidate.freeze()?;
    if candidate.catalog()? != resolved.current().catalog() {
        return Err(ComponentRegistryError::SchemaProviderCatalogMismatch {
            provider: resolved.definition().id(),
        });
    }
    Ok(candidate.into_frozen_data())
}

fn preflight_owner_candidate_merge(
    aggregate: &RegistryData,
    candidate: &RegistryData,
) -> Result<(), ComponentRegistryError> {
    for (owner, receipt) in &candidate.contributions {
        if aggregate.contributions.contains_key(owner) {
            return Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner: *owner });
        }
        let provider = receipt.provider().provider();
        if let Some(existing) = contribution_for_provider(aggregate, provider) {
            if existing == receipt {
                continue;
            }
            return Err(ComponentRegistryError::DivergentSchemaProviderReceipt { provider });
        }
    }

    for (rust_type_id, requested_component_id) in &candidate.rust_type_ids {
        let Some(existing_component_id) = aggregate.rust_type_ids.get(rust_type_id) else {
            continue;
        };
        let rust_type_path = candidate
            .bindings
            .get(requested_component_id)
            .map_or("<unknown>", |binding| binding.rust_type_path);
        return Err(ComponentRegistryError::DuplicateComponentRustType {
            rust_type_path: rust_type_path.to_owned(),
            existing_component_id: existing_component_id.clone(),
            requested_component_id: requested_component_id.clone(),
        });
    }
    Ok(())
}

fn preflight_owner_catalog_merge(
    aggregate: &RegistryData,
    catalog: &ComponentSchemaCatalog,
) -> Result<(), ComponentRegistryError> {
    for schema in &catalog.components {
        if aggregate.path_indexes.contains_key(schema.id()) {
            return Err(ComponentRegistryError::DuplicateComponentId(
                schema.id().clone(),
            ));
        }
        if aggregate.type_tombstone_index.contains(schema.id()) {
            return Err(ComponentRegistryError::ActiveTypeIsTombstoned(
                schema.id().clone(),
            ));
        }
    }
    for tombstone in &catalog.type_tombstones {
        if aggregate.path_indexes.contains_key(tombstone) {
            return Err(ComponentRegistryError::ActiveTypeIsTombstoned(
                tombstone.clone(),
            ));
        }
        if aggregate.type_tombstone_index.contains(tombstone) {
            return Err(ComponentRegistryError::DuplicateTypeTombstone(
                tombstone.clone(),
            ));
        }
    }
    Ok(())
}

fn merge_owner_candidate(aggregate: &mut RegistryData, candidate: RegistryData) {
    let RegistryData {
        catalog,
        previous_catalog: _,
        raw_owner: _,
        build_mode: _,
        contributions,
        provider_owners,
        component_owners,
        type_registry,
        rust_type_ids,
        bindings,
        migrations,
        type_index: _,
        field_index: _,
        path_indexes,
        type_tombstone_index,
    } = candidate;

    aggregate.catalog.components.extend(catalog.components);
    aggregate
        .catalog
        .type_tombstones
        .extend(catalog.type_tombstones);
    debug_assert_eq!(provider_owners.len(), contributions.len());
    for receipt in contributions.into_values() {
        insert_contribution(aggregate, receipt);
    }
    aggregate.component_owners.extend(component_owners);
    for registration in type_registry.iter().cloned() {
        aggregate.type_registry.add_registration(registration);
    }
    aggregate.rust_type_ids.extend(rust_type_ids);
    aggregate.bindings.extend(bindings);
    aggregate.migrations.extend(migrations);
    aggregate.path_indexes.extend(path_indexes);
    aggregate.type_tombstone_index.extend(type_tombstone_index);
}

fn validate_schema_capabilities(
    schema: &ComponentSchema,
    capabilities: impl IntoIterator<Item = ComponentCapability>,
) -> Result<(), ComponentProjectionError> {
    for capability in capabilities {
        if !schema.has_capability(capability) {
            return Err(ComponentProjectionError::MissingComponentCapability {
                component_id: schema.id.clone(),
                capability,
            });
        }
        if let Some(field) = schema
            .fields
            .iter()
            .find(|field| !field.has_capability(capability))
        {
            return Err(ComponentProjectionError::ProjectionRequired {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
                capability,
            });
        }
    }
    Ok(())
}

fn validate_native_binding<T: Component>(
    data: &RegistryData,
    id: &ComponentTypeId,
    persistent_eligible: bool,
) -> Result<(), ComponentRegistryError> {
    if data.bindings.contains_key(id) {
        return Err(ComponentRegistryError::DuplicateNativeBinding(id.clone()));
    }
    if let Some(existing_component_id) = data.rust_type_ids.get(&TypeId::of::<T>()) {
        return Err(ComponentRegistryError::DuplicateComponentRustType {
            rust_type_path: std::any::type_name::<T>().to_owned(),
            existing_component_id: existing_component_id.clone(),
            requested_component_id: id.clone(),
        });
    }
    if persistent_eligible {
        validate_persistent_metadata::<T>(id)?;
    }
    Ok(())
}

fn validate_persistent_metadata<T: Component>(
    id: &ComponentTypeId,
) -> Result<(), ComponentRegistryError> {
    validate_persistent_component_registration::<T>()
        .map_err(|error| map_persistent_metadata_error(id, error))
}

fn map_persistent_metadata_error(
    id: &ComponentTypeId,
    error: PersistentComponentMetadataError,
) -> ComponentRegistryError {
    match error {
        PersistentComponentMetadataError::RequiredComponents => {
            ComponentRegistryError::PersistentComponentRequiresImplicitComponents {
                component_id: id.clone(),
            }
        }
        PersistentComponentMetadataError::LifecycleHook(_) => {
            ComponentRegistryError::PersistentComponentHasLifecycleHook {
                component_id: id.clone(),
            }
        }
        PersistentComponentMetadataError::ComponentMissing
        | PersistentComponentMetadataError::Observer { .. } => {
            unreachable!(
                "isolated component registration cannot contain observers or lose metadata"
            )
        }
    }
}

fn validate_persistent_registration<T: Component>(
    data: &RegistryData,
    schema: &ComponentSchema,
) -> Result<(), ComponentRegistryError> {
    if data.path_indexes.contains_key(schema.id()) {
        return Err(ComponentRegistryError::DuplicateComponentId(
            schema.id().clone(),
        ));
    }
    validate_schema(schema)?;
    validate_native_binding::<T>(data, schema.id(), true)
}

type TypeIndex = BTreeMap<ComponentTypeId, usize>;
type FieldIndex = BTreeMap<(ComponentTypeId, ComponentFieldId), (usize, usize)>;
type PathIndexes = BTreeMap<ComponentTypeId, SchemaPathIndex>;

fn validate_registry(
    data: &RegistryData,
) -> Result<(ComponentSchemaCatalog, TypeIndex, FieldIndex, PathIndexes), ComponentRegistryError> {
    let catalog = prepare_catalog_candidate(data.catalog.clone(), data.previous_catalog.as_ref())?;

    for schema in &catalog.components {
        let Some(binding) = data.bindings.get(&schema.id) else {
            return Err(ComponentRegistryError::MissingNativeBinding {
                component_id: schema.id.clone(),
            });
        };
        if schema.has_capability(ComponentCapability::Scene) {
            (binding.validate_registration)()
                .map_err(|error| map_persistent_metadata_error(&schema.id, error))?;
        }
    }

    for ((component_id, from_version), migration) in &data.migrations {
        let Some(schema) = catalog
            .components
            .iter()
            .find(|schema| &schema.id == component_id)
        else {
            return Err(ComponentRegistryError::UnknownComponentId(
                component_id.clone(),
            ));
        };
        if from_version.get() == 0
            || migration.to_version <= *from_version
            || migration.to_version > schema.version
        {
            return Err(ComponentRegistryError::InvalidComponentMigration {
                component_id: component_id.clone(),
                from_version: *from_version,
                to_version: migration.to_version,
            });
        }
    }
    validate_predecessor_migration_chains(data, &catalog)?;

    let mut type_index = BTreeMap::new();
    let mut field_index = BTreeMap::new();
    for (component_index, schema) in catalog.components.iter().enumerate() {
        type_index.insert(schema.id.clone(), component_index);
        for (field_offset, field) in schema.fields.iter().enumerate() {
            field_index.insert(
                (schema.id.clone(), field.id.clone()),
                (component_index, field_offset),
            );
        }
    }
    let path_indexes = build_schema_path_indexes(&catalog);
    Ok((catalog, type_index, field_index, path_indexes))
}

pub(crate) fn prepare_catalog_candidate(
    mut catalog: ComponentSchemaCatalog,
    previous: Option<&ComponentSchemaCatalog>,
) -> Result<ComponentSchemaCatalog, ComponentRegistryError> {
    validate_catalog_shapes(&catalog)?;
    catalog.canonicalize();
    let previous = previous
        .map(|previous| {
            validate_catalog_contents(previous)?;
            let mut previous = previous.clone();
            previous.canonicalize();
            Ok::<_, ComponentRegistryError>(previous)
        })
        .transpose()?;
    validate_catalog(&catalog, previous.as_ref())?;
    Ok(catalog)
}

pub(crate) fn prepare_owner_catalogs(
    mut current: ComponentSchemaCatalog,
    mut predecessor: Option<ComponentSchemaCatalog>,
) -> Result<(ComponentSchemaCatalog, Option<ComponentSchemaCatalog>), ComponentRegistryError> {
    if let Some(predecessor) = predecessor.as_mut() {
        validate_catalog_shapes(predecessor)?;
        predecessor.canonicalize();
        validate_catalog_contents(predecessor)?;
        match predecessor.generation() {
            0 => {
                return Err(ComponentRegistryError::InvalidCatalogGeneration {
                    expected: 1,
                    actual: 0,
                });
            }
            1 if predecessor.predecessor().is_some() => {
                return Err(ComponentRegistryError::InvalidCatalogPredecessor {
                    expected: None,
                    actual: predecessor.predecessor().copied(),
                });
            }
            generation if generation > 1 && predecessor.predecessor().is_none() => {
                return Err(ComponentRegistryError::MissingCatalogPredecessor { generation });
            }
            _ => {}
        }
    }

    validate_catalog_shapes(&current)?;
    current.canonicalize();
    validate_catalog(&current, predecessor.as_ref())?;
    Ok((current, predecessor))
}

fn validate_catalog(
    catalog: &ComponentSchemaCatalog,
    previous: Option<&ComponentSchemaCatalog>,
) -> Result<(), ComponentRegistryError> {
    let expected_generation = match previous {
        Some(catalog) => catalog.generation.checked_add(1).ok_or(
            ComponentRegistryError::CatalogGenerationExhausted {
                generation: catalog.generation,
            },
        )?,
        None => 1,
    };
    if catalog.generation != expected_generation {
        return Err(ComponentRegistryError::InvalidCatalogGeneration {
            expected: expected_generation,
            actual: catalog.generation,
        });
    }
    let expected_predecessor = previous.map(ComponentSchemaCatalog::fingerprint);
    if catalog.predecessor != expected_predecessor {
        return Err(ComponentRegistryError::InvalidCatalogPredecessor {
            expected: expected_predecessor,
            actual: catalog.predecessor,
        });
    }

    validate_catalog_contents(catalog)?;
    if let Some(previous) = previous {
        validate_catalog_lineage(catalog, previous)?;
    }
    Ok(())
}

pub(crate) fn validate_catalog_contents(
    catalog: &ComponentSchemaCatalog,
) -> Result<(), ComponentRegistryError> {
    let mut active_types = BTreeSet::new();
    for schema in &catalog.components {
        if !active_types.insert(schema.id.clone()) {
            return Err(ComponentRegistryError::DuplicateComponentId(
                schema.id.clone(),
            ));
        }
        validate_schema(schema)?;
    }
    let mut type_tombstones = BTreeSet::new();
    for tombstone in &catalog.type_tombstones {
        tombstone
            .validate()
            .map_err(|_| ComponentRegistryError::InvalidComponentTypeId {
                component_id: tombstone.clone(),
            })?;
        if !type_tombstones.insert(tombstone.clone()) {
            return Err(ComponentRegistryError::DuplicateTypeTombstone(
                tombstone.clone(),
            ));
        }
        if active_types.contains(tombstone) {
            return Err(ComponentRegistryError::ActiveTypeIsTombstoned(
                tombstone.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_schema(schema: &ComponentSchema) -> Result<(), ComponentRegistryError> {
    validate_schema_shapes(schema)?;
    schema
        .id
        .validate()
        .map_err(|_| ComponentRegistryError::InvalidComponentTypeId {
            component_id: schema.id.clone(),
        })?;
    validate_aliases(&schema.id, None, &schema.aliases)?;
    if schema.version.get() == 0 {
        return Err(ComponentRegistryError::InvalidComponentSchemaVersion {
            component_id: schema.id.clone(),
        });
    }
    for capability in [
        ComponentCapability::AssetRef,
        ComponentCapability::EntityRef,
    ] {
        if schema.has_capability(capability) {
            return Err(ComponentRegistryError::InvalidComponentCapability {
                component_id: schema.id.clone(),
                capability,
            });
        }
    }
    let mut field_ids = BTreeSet::new();
    for field in &schema.fields {
        field
            .id
            .validate()
            .map_err(|_| ComponentRegistryError::InvalidComponentFieldId {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
            })?;
        if !field_ids.insert(field.id.clone()) {
            return Err(ComponentRegistryError::DuplicateComponentFieldId {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
            });
        }
        validate_aliases(&schema.id, Some(&field.id), &field.aliases)?;
        validate_field_capabilities(schema, field)?;
        if let Some(numeric_domain) = field.numeric_domain() {
            if numeric_domain.value_kind() != field.value_kind() {
                return Err(ComponentRegistryError::InvalidComponentFieldNumericDomain {
                    component_id: schema.id.clone(),
                    field_id: field.id.clone(),
                    numeric_domain,
                    value_kind: field.value_kind(),
                });
            }
        }
        validate_component_field_default(&schema.id, field)?;
        member_lineage::validate(schema.id(), field)?;
    }
    let path_index = SchemaPathIndex::from_schema(schema);
    path_index.validate(schema)?;

    let mut tombstones = BTreeSet::new();
    for tombstone in &schema.field_tombstones {
        tombstone
            .validate()
            .map_err(|_| ComponentRegistryError::InvalidComponentFieldId {
                component_id: schema.id.clone(),
                field_id: tombstone.clone(),
            })?;
        if !tombstones.insert(tombstone.clone()) {
            return Err(ComponentRegistryError::DuplicateFieldTombstone {
                component_id: schema.id.clone(),
                field_id: tombstone.clone(),
            });
        }
        if field_ids.contains(tombstone) {
            return Err(ComponentRegistryError::ActiveFieldIsTombstoned {
                component_id: schema.id.clone(),
                field_id: tombstone.clone(),
            });
        }
    }
    if let Some(value) = &schema.authoring_initial_value {
        validate_component_value_budget(value).map_err(|source| {
            ComponentRegistryError::InvalidComponentAuthoringInitialValue {
                component_id: schema.id.clone(),
                source: ComponentCodecError::Message(source.to_string()),
            }
        })?;
        validate_component_value_coverage_with_presence(schema, &path_index, value, true).map_err(
            |source| ComponentRegistryError::InvalidComponentAuthoringInitialValue {
                component_id: schema.id.clone(),
                source,
            },
        )?;
    }
    Ok(())
}

fn validate_catalog_shapes(catalog: &ComponentSchemaCatalog) -> Result<(), ComponentRegistryError> {
    for schema in &catalog.components {
        validate_schema_shapes(schema)?;
    }
    Ok(())
}

fn validate_schema_shapes(schema: &ComponentSchema) -> Result<(), ComponentRegistryError> {
    validate_component_shape_budget(schema).map_err(|(field_id, source)| {
        if let ComponentShapeErrorKind::InvalidNumericDomain { kind, domain } = &source.kind {
            ComponentRegistryError::InvalidComponentFieldNumericDomain {
                component_id: schema.id.clone(),
                field_id,
                numeric_domain: *domain,
                value_kind: *kind,
            }
        } else {
            ComponentRegistryError::InvalidComponentFieldShape {
                component_id: schema.id.clone(),
                field_id,
                source,
            }
        }
    })
}

fn validate_aliases(
    component_id: &ComponentTypeId,
    field_id: Option<&ComponentFieldId>,
    aliases: &[String],
) -> Result<(), ComponentRegistryError> {
    let mut unique = BTreeSet::new();
    for alias in aliases {
        validate_alias(alias).map_err(|error| ComponentRegistryError::InvalidAlias {
            component_id: component_id.clone(),
            field_id: field_id.cloned(),
            error,
        })?;
        if !unique.insert(alias) {
            return Err(ComponentRegistryError::DuplicateAlias {
                component_id: component_id.clone(),
                field_id: field_id.cloned(),
                alias: alias.clone(),
            });
        }
    }
    if aliases.is_empty() {
        return Err(ComponentRegistryError::InvalidAlias {
            component_id: component_id.clone(),
            field_id: field_id.cloned(),
            error: AliasError::Empty,
        });
    }
    Ok(())
}

fn validate_field_capabilities(
    schema: &ComponentSchema,
    field: &ComponentFieldSchema,
) -> Result<(), ComponentRegistryError> {
    for capability in [
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::Edit,
    ] {
        if field.has_capability(capability) && !schema.has_capability(capability) {
            return Err(ComponentRegistryError::InvalidFieldCapability {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
                capability,
            });
        }
    }
    for (kind, capability) in [
        (ComponentValueKind::AssetRef, ComponentCapability::AssetRef),
        (
            ComponentValueKind::EntityRef,
            ComponentCapability::EntityRef,
        ),
    ] {
        if field.shape.contains_kind(kind) != field.has_capability(capability) {
            return Err(ComponentRegistryError::InvalidFieldCapability {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
                capability,
            });
        }
    }
    Ok(())
}

fn validate_catalog_lineage(
    catalog: &ComponentSchemaCatalog,
    previous: &ComponentSchemaCatalog,
) -> Result<(), ComponentRegistryError> {
    let current_types = catalog
        .components
        .iter()
        .map(|schema| (&schema.id, schema))
        .collect::<BTreeMap<_, _>>();
    let current_type_tombstones = catalog.type_tombstones.iter().collect::<BTreeSet<_>>();

    for tombstone in &previous.type_tombstones {
        if current_types.contains_key(tombstone) {
            return Err(ComponentRegistryError::ReactivatedTypeId {
                component_id: tombstone.clone(),
            });
        }
        if !current_type_tombstones.contains(tombstone) {
            return Err(ComponentRegistryError::MissingTypeTombstone {
                component_id: tombstone.clone(),
            });
        }
    }

    for previous_schema in &previous.components {
        let Some(current_schema) = current_types.get(&previous_schema.id) else {
            if !current_type_tombstones.contains(&previous_schema.id) {
                return Err(ComponentRegistryError::MissingTypeTombstone {
                    component_id: previous_schema.id.clone(),
                });
            }
            continue;
        };
        validate_field_lineage(current_schema, previous_schema)?;
    }
    Ok(())
}

fn validate_field_lineage(
    current: &ComponentSchema,
    previous: &ComponentSchema,
) -> Result<(), ComponentRegistryError> {
    let current_fields = current
        .fields
        .iter()
        .map(|field| &field.id)
        .collect::<BTreeSet<_>>();
    let current_tombstones = current.field_tombstones.iter().collect::<BTreeSet<_>>();

    for tombstone in &previous.field_tombstones {
        if current_fields.contains(tombstone) {
            return Err(ComponentRegistryError::ReactivatedFieldId {
                component_id: current.id.clone(),
                field_id: tombstone.clone(),
            });
        }
        if !current_tombstones.contains(tombstone) {
            return Err(ComponentRegistryError::MissingFieldTombstone {
                component_id: current.id.clone(),
                field_id: tombstone.clone(),
            });
        }
    }
    for field in &previous.fields {
        if !current_fields.contains(&field.id) && !current_tombstones.contains(&field.id) {
            return Err(ComponentRegistryError::MissingFieldTombstone {
                component_id: current.id.clone(),
                field_id: field.id.clone(),
            });
        }
        if let Some(current_field) = current
            .fields
            .iter()
            .find(|candidate| candidate.id == field.id)
        {
            member_lineage::validate_successor(current.id(), current_field, field)?;
        }
    }
    if current.version < previous.version {
        return Err(ComponentRegistryError::ComponentSchemaVersionRegressed {
            component_id: current.id.clone(),
            previous: previous.version,
            current: current.version,
        });
    }
    if current.version == previous.version && !same_versioned_schema_semantics(current, previous) {
        return Err(
            ComponentRegistryError::ComponentSchemaChangedWithoutVersionBump {
                component_id: current.id.clone(),
            },
        );
    }
    Ok(())
}

fn same_versioned_schema_semantics(current: &ComponentSchema, previous: &ComponentSchema) -> bool {
    // Creation-only templates affect catalog identity, not stored component decoding.
    if current.capabilities != previous.capabilities
        || current.fields.len() != previous.fields.len()
    {
        return false;
    }
    let current_fields = current
        .fields
        .iter()
        .map(|field| (&field.id, field))
        .collect::<BTreeMap<_, _>>();
    previous.fields.iter().all(|previous_field| {
        current_fields
            .get(&previous_field.id)
            .is_some_and(|current_field| {
                current_field.path == previous_field.path
                    && current_field.shape == previous_field.shape
                    && current_field.required == previous_field.required
                    && current_field.capabilities == previous_field.capabilities
                    && current_field.default_value == previous_field.default_value
            })
    })
}

fn validate_predecessor_migration_chains(
    data: &RegistryData,
    catalog: &ComponentSchemaCatalog,
) -> Result<(), ComponentRegistryError> {
    let Some(previous) = &data.previous_catalog else {
        return Ok(());
    };
    let current = catalog
        .components
        .iter()
        .map(|schema| (&schema.id, schema))
        .collect::<BTreeMap<_, _>>();

    for previous_schema in &previous.components {
        let Some(current_schema) = current.get(&previous_schema.id) else {
            continue;
        };
        if current_schema.version <= previous_schema.version {
            continue;
        }

        let mut version = previous_schema.version;
        while version < current_schema.version {
            let Some(migration) = data.migrations.get(&(previous_schema.id.clone(), version))
            else {
                return Err(ComponentRegistryError::MissingComponentMigrationChain {
                    component_id: previous_schema.id.clone(),
                    from_version: version,
                    target_version: current_schema.version,
                });
            };
            version = migration.to_version;
        }
    }
    Ok(())
}

fn validate_component_field_default(
    component_id: &ComponentTypeId,
    field: &ComponentFieldSchema,
) -> Result<(), ComponentRegistryError> {
    let Some(default_value) = &field.default_value else {
        return Ok(());
    };
    validate_component_value_budget(default_value).map_err(|source| {
        ComponentRegistryError::InvalidComponentFieldDefaultShape {
            component_id: component_id.clone(),
            path: field.path.clone(),
            source,
        }
    })?;
    let error = match field.shape.validate_value(default_value) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    if default_value.kind() == field.value_kind() {
        if let ComponentShapeErrorKind::NumericDomain(numeric_domain) = error.kind {
            return Err(
                ComponentRegistryError::ComponentFieldDefaultOutsideNumericDomain {
                    component_id: component_id.clone(),
                    path: field.path.clone(),
                    numeric_domain,
                },
            );
        }
    }
    if default_value.kind() == field.value_kind() {
        return Err(ComponentRegistryError::InvalidComponentFieldDefaultShape {
            component_id: component_id.clone(),
            path: field.path.clone(),
            source: error,
        });
    }
    Err(ComponentRegistryError::InvalidComponentFieldDefault {
        component_id: component_id.clone(),
        path: field.path.clone(),
        expected: field.value_kind(),
        actual: default_value.kind(),
    })
}

/// Validates canonical field coverage, required paths, wire kinds and numeric domains.
///
/// Missing defaults use the native materialization policy. This performs no native codec calls
/// or World operations. The caller owns raw payload admission and aggregate work; persistent
/// files, Prefab expansion, patches and reference traversal retain their owner budgets.
pub fn validate_component_value(
    schema: &ComponentSchema,
    value: &ComponentValue,
) -> Result<(), ComponentCodecError> {
    validate_component_value_with_limits(
        schema,
        value,
        ComponentValueMaterializationLimits::default(),
    )
}

/// Validates one effective value using its owner's explicit default-expansion limits.
///
/// The schema must already have been admitted by its catalog owner. Missing defaults are checked
/// before allocation, then the same effective candidate supplies presence and shape validation.
/// No-insertion values retain their caller-owned raw admission and are not rescanned for cost.
pub fn validate_component_value_with_limits(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), ComponentCodecError> {
    validate_component_shape_budget(schema)
        .map_err(|(_, error)| ComponentCodecError::Message(error.to_string()))?;
    let effective = effective_component_value(schema, value, limits)?;
    validate_component_value_coverage(
        schema,
        &SchemaPathIndex::from_schema(schema),
        effective.as_ref(),
    )
}

fn validate_component_value_coverage(
    schema: &ComponentSchema,
    path_index: &SchemaPathIndex,
    value: &ComponentValue,
) -> Result<(), ComponentCodecError> {
    validate_component_value_coverage_with_presence(schema, path_index, value, false)
}

fn validate_component_value_coverage_with_presence(
    schema: &ComponentSchema,
    path_index: &SchemaPathIndex,
    value: &ComponentValue,
    require_all_fields: bool,
) -> Result<(), ComponentCodecError> {
    if schema.fields.is_empty() && matches!(value, ComponentValue::Map(fields) if fields.is_empty())
    {
        return Ok(());
    }

    for field in &schema.fields {
        match value.get_path(&field.path) {
            Ok(value) => {
                if let Err(error) = field.shape.validate_value(value) {
                    if error.kind == ComponentShapeErrorKind::NumericInterval {
                        return Err(numeric_range_error(&field.id));
                    }
                    let path = ComponentFieldPath::new(
                        field
                            .path
                            .segments()
                            .iter()
                            .chain(error.path.segments())
                            .cloned(),
                    );
                    return Err(ComponentCodecError::invalid_field(
                        path.to_string(),
                        error.to_string(),
                    ));
                }
            }
            Err(error)
                if !require_all_fields && (!field.required || field.default_value.is_some()) =>
            {
                if !matches!(
                    error,
                    crate::ComponentFieldPathError::MissingField { .. }
                        | crate::ComponentFieldPathError::IndexOutOfBounds { .. }
                ) {
                    return Err(ComponentCodecError::invalid_field(
                        field.path.to_string(),
                        "a path compatible with the component schema",
                    ));
                }
            }
            Err(_) => return Err(ComponentCodecError::missing_field(field.path.to_string())),
        }
    }

    validate_value_node_coverage(schema, path_index, value, &mut Vec::new())
}

pub(crate) fn numeric_range_error(field: &ComponentFieldId) -> ComponentCodecError {
    ComponentCodecError::domain(
        ComponentDomainError::new(
            DiagnosticCode::new("nara.reflect.numeric-range")
                .expect("numeric interval diagnostic code is valid"),
            SafeSummary::new("Component value is outside its declared numeric interval")
                .expect("numeric interval summary is safe static text"),
        )
        .with_field(field.clone()),
    )
}

fn validate_value_node_coverage(
    schema: &ComponentSchema,
    path_index: &SchemaPathIndex,
    value: &ComponentValue,
    path: &mut Vec<ComponentFieldPathSegment>,
) -> Result<(), ComponentCodecError> {
    if path.len() > MAX_SHAPE_DEPTH {
        return Err(ComponentCodecError::Message(
            "component value coverage exceeds the schema path depth limit".to_owned(),
        ));
    }
    let relation = path_index.relation(schema, path);
    if relation == SchemaPathRelation::Exact {
        return Ok(());
    }
    if path.len() == MAX_SHAPE_DEPTH {
        return Err(component_value_not_declared_error(
            &ComponentFieldPath::new(path.iter().cloned()),
        ));
    }

    match value {
        ComponentValue::Map(fields) if !fields.is_empty() => {
            for (key, value) in fields {
                path.push(ComponentFieldPathSegment::field(key));
                validate_value_node_coverage(schema, path_index, value, path)?;
                path.pop();
            }
            Ok(())
        }
        ComponentValue::List(values) if !values.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                let index = u32::try_from(index).map_err(|_| {
                    component_value_not_declared_error(&ComponentFieldPath::new(
                        path.iter().cloned(),
                    ))
                })?;
                path.push(ComponentFieldPathSegment::index(index));
                validate_value_node_coverage(schema, path_index, value, path)?;
                path.pop();
            }
            Ok(())
        }
        ComponentValue::Map(_) | ComponentValue::List(_)
            if relation == SchemaPathRelation::Descendant =>
        {
            Ok(())
        }
        _ => Err(component_value_not_declared_error(
            &ComponentFieldPath::new(path.iter().cloned()),
        )),
    }
}

fn component_value_not_declared_error(path: &ComponentFieldPath) -> ComponentCodecError {
    ComponentCodecError::Message(format!(
        "component value path '{path}' is not declared by schema"
    ))
}

fn missing_schema_path_index_error() -> ComponentCodecError {
    ComponentCodecError::Message("component schema path index is unavailable".to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaPathRelation {
    Exact,
    Descendant,
    None,
}

struct SchemaPathIndex {
    field_offsets_by_path: Vec<usize>,
}

impl SchemaPathIndex {
    fn from_schema(schema: &ComponentSchema) -> Self {
        let mut field_offsets_by_path = (0..schema.fields.len()).collect::<Vec<_>>();
        field_offsets_by_path.sort_unstable_by(|left, right| {
            schema.fields[*left]
                .path
                .cmp(&schema.fields[*right].path)
                .then_with(|| left.cmp(right))
        });
        Self {
            field_offsets_by_path,
        }
    }

    fn validate(&self, schema: &ComponentSchema) -> Result<(), ComponentRegistryError> {
        for offsets in self.field_offsets_by_path.windows(2) {
            let first = &schema.fields[offsets[0]].path;
            let second = &schema.fields[offsets[1]].path;
            if first == second {
                return Err(ComponentRegistryError::DuplicateComponentFieldPath {
                    component_id: schema.id.clone(),
                    path: first.clone(),
                });
            }
            if second.segments().starts_with(first.segments()) {
                return Err(ComponentRegistryError::OverlappingComponentFieldPaths {
                    component_id: schema.id.clone(),
                    first: first.clone(),
                    second: second.clone(),
                });
            }
        }
        Ok(())
    }

    fn relation(
        &self,
        schema: &ComponentSchema,
        path: &[ComponentFieldPathSegment],
    ) -> SchemaPathRelation {
        let offset = self
            .field_offsets_by_path
            .partition_point(|field_offset| schema.fields[*field_offset].path.segments() < path);
        let Some(field_offset) = self.field_offsets_by_path.get(offset) else {
            return SchemaPathRelation::None;
        };
        let candidate = schema.fields[*field_offset].path.segments();
        if candidate == path {
            SchemaPathRelation::Exact
        } else if candidate.starts_with(path) {
            SchemaPathRelation::Descendant
        } else {
            SchemaPathRelation::None
        }
    }
}

fn build_schema_path_indexes(catalog: &ComponentSchemaCatalog) -> PathIndexes {
    catalog
        .components
        .iter()
        .map(|schema| (schema.id.clone(), SchemaPathIndex::from_schema(schema)))
        .collect()
}

#[cfg(test)]
mod owner_composition_tests {
    use std::{
        any::TypeId,
        collections::{BTreeMap, BTreeSet},
        panic::{AssertUnwindSafe, catch_unwind},
        sync::Arc,
    };

    use bevy_reflect::Reflect;
    use nara_app::PluginSchemaProviderId;
    use nara_ecs::Component;

    use super::*;
    use crate::{ComponentSchemaProviderBindingId, ComponentSchemaProviderSourceError};

    const BASELINE_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("nara.test.atomic-baseline-owner");
    const BASELINE_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("nara.test.atomic-baseline-provider");
    const FAILURE_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("nara.test.atomic-failure-owner");
    const FAILURE_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("nara.test.atomic-failure-provider");

    #[derive(Component, Reflect)]
    struct FailureProbe;

    #[derive(Debug, PartialEq)]
    struct BuildingProbe {
        instance_token: usize,
        catalog: ComponentSchemaCatalog,
        previous_catalog: Option<ComponentSchemaCatalog>,
        raw_owner: Option<ComponentSchemaOwnerId>,
        build_mode: RegistryBuildMode,
        contributions: BTreeMap<ComponentSchemaOwnerId, ComponentSchemaContributionReceipt>,
        provider_owners: BTreeMap<PluginSchemaProviderId, ComponentSchemaOwnerId>,
        reflected_types: BTreeSet<TypeId>,
        rust_type_ids: BTreeMap<TypeId, ComponentTypeId>,
        binding_ids: BTreeSet<ComponentTypeId>,
        migration_ids: BTreeSet<(ComponentTypeId, ComponentSchemaVersion)>,
        type_index: BTreeMap<ComponentTypeId, usize>,
        field_index: BTreeMap<(ComponentTypeId, ComponentFieldId), (usize, usize)>,
        path_index_ids: BTreeSet<ComponentTypeId>,
        type_tombstone_index: BTreeSet<ComponentTypeId>,
    }

    fn building_probe(registry: &ComponentRegistry) -> BuildingProbe {
        let data = registry.data();
        BuildingProbe {
            instance_token: Arc::as_ptr(registry.instance_token()) as usize,
            catalog: data.catalog.clone(),
            previous_catalog: data.previous_catalog.clone(),
            raw_owner: data.raw_owner,
            build_mode: data.build_mode,
            contributions: data.contributions.clone(),
            provider_owners: data.provider_owners.clone(),
            reflected_types: data
                .type_registry
                .iter()
                .map(|registration| registration.type_id())
                .collect(),
            rust_type_ids: data
                .rust_type_ids
                .iter()
                .map(|(type_id, component_id)| (*type_id, component_id.clone()))
                .collect(),
            binding_ids: data.bindings.keys().cloned().collect(),
            migration_ids: data.migrations.keys().cloned().collect(),
            type_index: data.type_index.clone(),
            field_index: data.field_index.clone(),
            path_index_ids: data.path_indexes.keys().cloned().collect(),
            type_tombstone_index: data.type_tombstone_index.clone(),
        }
    }

    fn empty_source() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
        Ok(ComponentSchemaCatalog::default())
    }

    fn register_empty(_: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
        Ok(())
    }

    fn failure_schema(version: ComponentSchemaVersion) -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("nara.test.AtomicFailureProbe"),
            "Failure probe",
            version,
        )
    }

    fn failure_predecessor_source()
    -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
        Ok(ComponentSchemaCatalog {
            components: vec![failure_schema(ComponentSchemaVersion::ONE)],
            ..ComponentSchemaCatalog::default()
        })
    }

    fn failure_current_source() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
    {
        let predecessor = failure_predecessor_source()?;
        let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
            .map_err(|_| ComponentSchemaProviderSourceError::new("atomic-successor-exhausted"))?;
        current.components.push(failure_schema(
            ComponentSchemaVersion::new(2).expect("version two is non-zero"),
        ));
        Ok(current)
    }

    fn mutate_failure_candidate(
        registry: &mut ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        let id = ComponentTypeId::new("nara.test.AtomicFailureProbe");
        let version_two = ComponentSchemaVersion::new(2).expect("version two is non-zero");
        registry.register_component_schema(failure_schema(version_two))?;
        registry.register_native_component_with_codec::<FailureProbe, _, _>(
            &id,
            |_| Ok(FailureProbe),
            |_| Ok(ComponentValue::Null),
        )?;
        registry.register_component_migration(&id, ComponentSchemaVersion::ONE, version_two, Ok)?;
        registry.register_reflected_type::<FailureProbe>()?;
        Ok(())
    }

    fn mutate_then_reject(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
        mutate_failure_candidate(registry)?;
        Err(ComponentRegistryError::Frozen)
    }

    fn mutate_then_panic(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
        mutate_failure_candidate(registry)?;
        panic!("owner candidate panic after complete private mutation")
    }

    fn baseline_provider() -> ComponentSchemaProviderDefinition {
        ComponentSchemaProviderDefinition::new(
            BASELINE_OWNER,
            BASELINE_PROVIDER,
            ComponentSchemaProviderBindingId::new("nara.test.atomic-baseline.native", 1),
            empty_source,
            register_empty,
        )
    }

    fn failure_provider(
        register: fn(&mut ComponentRegistry) -> Result<(), ComponentRegistryError>,
    ) -> ComponentSchemaProviderDefinition {
        ComponentSchemaProviderDefinition::new(
            FAILURE_OWNER,
            FAILURE_PROVIDER,
            ComponentSchemaProviderBindingId::new("nara.test.atomic-failure.native", 2),
            failure_current_source,
            register,
        )
        .with_predecessor(failure_predecessor_source)
    }

    fn assert_failed_candidate_is_atomic(
        register: fn(&mut ComponentRegistry) -> Result<(), ComponentRegistryError>,
        should_panic: bool,
    ) {
        let mut registry = ComponentRegistry::new();
        baseline_provider()
            .register_or_validate_into(&mut registry)
            .unwrap();
        let before = building_probe(&registry);

        let result = catch_unwind(AssertUnwindSafe(|| {
            failure_provider(register).register_or_validate_into(&mut registry)
        }));
        if should_panic {
            assert!(result.is_err());
        } else {
            assert!(matches!(result, Ok(Err(ComponentRegistryError::Frozen))));
        }
        assert_eq!(building_probe(&registry), before);

        registry.freeze().unwrap();
        let snapshot = registry.snapshot().unwrap();
        let mut control = ComponentRegistry::new();
        baseline_provider()
            .register_or_validate_into(&mut control)
            .unwrap();
        control.freeze().unwrap();
        let control_snapshot = control.snapshot().unwrap();

        assert_eq!(snapshot.catalog(), control_snapshot.catalog());
        assert_eq!(
            snapshot.contribution_receipts().collect::<Vec<_>>(),
            control_snapshot.contribution_receipts().collect::<Vec<_>>()
        );
        assert_eq!(
            snapshot.schema_composition_fingerprint().unwrap(),
            control_snapshot.schema_composition_fingerprint().unwrap()
        );
        assert_eq!(
            snapshot.executable_registry_fingerprint().unwrap(),
            control_snapshot.executable_registry_fingerprint().unwrap()
        );
        assert_eq!(snapshot.provider_receipt(FAILURE_PROVIDER), None);
        assert_eq!(snapshot.owner_receipt(FAILURE_OWNER), None);
        assert!(registry.schema_for_type::<FailureProbe>().is_none());
        assert!(
            registry
                .type_registry()
                .unwrap()
                .get(TypeId::of::<FailureProbe>())
                .is_none()
        );
    }

    #[test]
    fn rejected_private_owner_candidate_preserves_every_aggregate_index() {
        assert_failed_candidate_is_atomic(mutate_then_reject, false);
    }

    #[test]
    fn panicked_private_owner_candidate_preserves_every_aggregate_index() {
        assert_failed_candidate_is_atomic(mutate_then_panic, true);
    }
}
