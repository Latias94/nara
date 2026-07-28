//! Component catalog validation, native bindings, codecs, and migrations.

use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::{Arc, Weak},
};

use bevy_reflect::{GetTypeRegistration, TypeRegistry};
use nara_ecs::{
    __private::{
        PersistentComponentMetadataError, register_persistent_component,
        validate_persistent_component_registration, validate_registered_persistent_component_apply,
    },
    Component, Entity, World,
    component::ComponentId,
};

use crate::{
    ComponentFieldPath, ComponentFieldPathSegment, ComponentValue, PersistentComponentProvider,
    asset_reference::is_asset_reference_value,
    codec::{
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        FnComponentCodec,
    },
    migration::{ComponentMigration, ComponentMigrationError, MigratedComponentValue},
    persistent_apply::{PreparedComponent, PreparedComponentCandidate},
    provider::{
        ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
        ComponentSchemaOwnerId, ComponentSchemaProviderDefinition, ComponentSchemaProviderReceipt,
        ComponentSchemaProviderSourceError, ExecutableRegistryFingerprint,
        ResolvedComponentSchemaProvider, SchemaCompositionFingerprint,
    },
    schema::{
        AliasError, CatalogFingerprint, ComponentCapability, ComponentFieldId,
        ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
        ComponentTypeId, ComponentValueKind, validate_alias,
    },
};

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
    MissingSceneComponentFields {
        component_id: ComponentTypeId,
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
    InvalidComponentFieldDefault {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        expected: ComponentValueKind,
        actual: ComponentValueKind,
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
            Self::MissingSceneComponentFields { component_id } => write!(
                formatter,
                "scene component ID '{component_id}' requires explicit schema fields"
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
            Self::InvalidComponentFieldDefault {
                component_id,
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "component ID '{component_id}' field '{path}' default has kind {actual:?}, expected {expected:?}"
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

impl Error for ComponentRegistryError {}

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

struct NativeComponentBinding {
    rust_type_path: &'static str,
    rust_type_id: TypeId,
    register_component: fn(&mut World) -> ComponentId,
    validate_registration: fn() -> Result<(), PersistentComponentMetadataError>,
    validate_apply: fn(&World, Option<Entity>) -> Result<(), PersistentComponentMetadataError>,
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
            move |world, entity, _context| {
                let Some(component) = world.get::<T>(entity) else {
                    return Ok(None);
                };
                encode(component).map(Some)
            },
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
            move |world, entity, _context| {
                let Some(component) = world.get::<T>(entity) else {
                    return Ok(None);
                };
                encode(component).map(Some)
            },
        )
    }

    pub fn register_persistent_component_codec<T, Preflight, Encode>(
        &mut self,
        schema: ComponentSchema,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponentCandidate, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&World, Entity) -> Result<Option<ComponentValue>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        self.register_persistent_component_codec_with_context::<T, _, _>(
            schema,
            move |value, _context| preflight(value),
            move |world, entity, _context| encode(world, entity),
        )
    }

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
            ) -> Result<PreparedComponentCandidate, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: for<'a> Fn(
                &World,
                Entity,
                &ComponentEncodeContext<'a>,
            ) -> Result<Option<ComponentValue>, ComponentCodecError>
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
                rust_type_id: TypeId::of::<T>(),
                register_component: register_persistent_component::<T>,
                validate_registration: validate_persistent_component_registration::<T>,
                validate_apply: validate_registered_persistent_component_apply::<T>,
                codec: Box::new(FnComponentCodec { preflight, encode }),
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
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponentCandidate, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&World, Entity) -> Result<Option<ComponentValue>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        self.register_native_component_codec_with_context::<T, _, _>(
            id,
            move |value, _context| preflight(value),
            move |world, entity, _context| encode(world, entity),
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
            ) -> Result<PreparedComponentCandidate, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: for<'a> Fn(
                &World,
                Entity,
                &ComponentEncodeContext<'a>,
            ) -> Result<Option<ComponentValue>, ComponentCodecError>
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
                rust_type_id: TypeId::of::<T>(),
                register_component: register_persistent_component::<T>,
                validate_registration: validate_persistent_component_registration::<T>,
                validate_apply: validate_registered_persistent_component_apply::<T>,
                codec: Box::new(FnComponentCodec { preflight, encode }),
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
            RegistryState::Frozen(data) => data
                .type_index
                .get(id)
                .and_then(|index| data.catalog.components.get(*index)),
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
            RegistryState::Frozen(data) => data
                .field_index
                .get(&(component_id.clone(), field_id.clone()))
                .and_then(|(component_index, field_index)| {
                    data.catalog
                        .components
                        .get(*component_index)
                        .and_then(|schema| schema.fields.get(*field_index))
                }),
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
        for capability in capabilities {
            if !schema.has_capability(capability) {
                return Err(ComponentProjectionError::MissingComponentCapability {
                    component_id: id.clone(),
                    capability,
                });
            }
            if let Some(field) = schema
                .fields
                .iter()
                .find(|field| !field.has_capability(capability))
            {
                return Err(ComponentProjectionError::ProjectionRequired {
                    component_id: id.clone(),
                    field_id: field.id.clone(),
                    capability,
                });
            }
        }
        Ok(())
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

    pub fn preflight_component(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
    ) -> Option<Result<PreparedComponent, ComponentCodecError>> {
        let mut context = ComponentDecodeContext::new();
        self.preflight_component_with_context(id, value, &mut context)
    }

    pub fn preflight_component_with_context(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Option<Result<PreparedComponent, ComponentCodecError>> {
        let data = match &self.state {
            RegistryState::Frozen(data) => data,
            RegistryState::Building(_) => return None,
            RegistryState::Transitioning => {
                unreachable!("component registry transition is synchronous")
            }
        };
        let binding = data.bindings.get(id)?;
        let schema = data
            .type_index
            .get(id)
            .and_then(|index| data.catalog.components.get(*index))?;
        let path_index = data.path_indexes.get(id);
        Some(
            self.validate_whole_value_capabilities(id, [ComponentCapability::Scene])
                .map_err(|error| ComponentCodecError::Message(error.to_string()))
                .and_then(|()| {
                    let path_index = path_index.ok_or_else(missing_schema_path_index_error)?;
                    validate_component_value_coverage(schema, path_index, value)
                })
                .and_then(|()| binding.codec.preflight_with_context(value, context))
                .and_then(|prepared| {
                    prepared.bind(
                        id.clone(),
                        binding.rust_type_id,
                        binding.rust_type_path,
                        binding.register_component,
                        binding.validate_apply,
                    )
                }),
        )
    }

    pub fn encode_component(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
    ) -> Option<Result<Option<ComponentValue>, ComponentCodecError>> {
        let context = ComponentEncodeContext::new();
        self.encode_component_with_context(id, world, entity, &context)
    }

    pub fn encode_component_with_context(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Option<Result<Option<ComponentValue>, ComponentCodecError>> {
        let binding = self.data().bindings.get(id)?;
        let schema = self.schema(id)?;
        let path_index = self.data().path_indexes.get(id);
        Some(
            self.validate_whole_value_capabilities(id, [ComponentCapability::Scene])
                .map_err(|error| ComponentCodecError::Message(error.to_string()))
                .and_then(|()| binding.codec.encode_with_context(world, entity, context))
                .and_then(|encoded| {
                    if let Some(value) = &encoded {
                        let path_index = path_index.ok_or_else(missing_schema_path_index_error)?;
                        validate_component_value_coverage(schema, path_index, value)?;
                    }
                    Ok(encoded)
                }),
        )
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
    for registration in type_registry.iter().cloned() {
        aggregate.type_registry.add_registration(registration);
    }
    aggregate.rust_type_ids.extend(rust_type_ids);
    aggregate.bindings.extend(bindings);
    aggregate.migrations.extend(migrations);
    aggregate.path_indexes.extend(path_indexes);
    aggregate.type_tombstone_index.extend(type_tombstone_index);
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
    catalog.canonicalize();
    validate_catalog(&catalog, previous)?;
    Ok(catalog)
}

pub(crate) fn prepare_owner_catalogs(
    mut current: ComponentSchemaCatalog,
    mut predecessor: Option<ComponentSchemaCatalog>,
) -> Result<(ComponentSchemaCatalog, Option<ComponentSchemaCatalog>), ComponentRegistryError> {
    if let Some(predecessor) = predecessor.as_mut() {
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

fn validate_catalog_contents(
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
    if schema.has_capability(ComponentCapability::Scene) && schema.fields.is_empty() {
        return Err(ComponentRegistryError::MissingSceneComponentFields {
            component_id: schema.id.clone(),
        });
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
        validate_component_field_default(&schema.id, field)?;
    }
    SchemaPathIndex::from_schema(schema).validate(schema)?;

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
    Ok(())
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
        if (field.value_kind == kind) != field.has_capability(capability) {
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
                    && current_field.value_kind == previous_field.value_kind
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
    if !field.required && matches!(default_value, ComponentValue::Null) {
        return Ok(());
    }
    if component_value_matches_kind(default_value, field.value_kind) {
        return Ok(());
    }
    Err(ComponentRegistryError::InvalidComponentFieldDefault {
        component_id: component_id.clone(),
        path: field.path.clone(),
        expected: field.value_kind,
        actual: default_value.kind(),
    })
}

fn component_value_matches_kind(value: &ComponentValue, expected: ComponentValueKind) -> bool {
    match expected {
        ComponentValueKind::AssetRef => is_asset_reference_value(value),
        expected => value.kind() == expected,
    }
}

fn validate_component_value_coverage(
    schema: &ComponentSchema,
    path_index: &SchemaPathIndex,
    value: &ComponentValue,
) -> Result<(), ComponentCodecError> {
    for field in &schema.fields {
        match value.get_path(&field.path) {
            Ok(value) => {
                if !component_field_accepts_value(field, value) {
                    return Err(ComponentCodecError::invalid_field(
                        field.path.to_string(),
                        format!("schema kind {:?}", field.value_kind),
                    ));
                }
            }
            Err(error) if !field.required || field.default_value.is_some() => {
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

fn component_field_accepts_value(field: &ComponentFieldSchema, value: &ComponentValue) -> bool {
    if !field.required && matches!(value, ComponentValue::Null) {
        return true;
    }
    component_value_matches_kind(value, field.value_kind)
}

fn validate_value_node_coverage(
    schema: &ComponentSchema,
    path_index: &SchemaPathIndex,
    value: &ComponentValue,
    path: &mut Vec<ComponentFieldPathSegment>,
) -> Result<(), ComponentCodecError> {
    let relation = path_index.relation(schema, path);
    if relation == SchemaPathRelation::Exact {
        return Ok(());
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
