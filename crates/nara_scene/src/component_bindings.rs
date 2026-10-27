use std::{collections::BTreeMap, sync::Arc};

use nara_core::ItemLimit;
use nara_reflect::{
    __private::{
        ComponentRegistrySnapshotWitness, component_registry_snapshot_witness,
        component_registry_snapshot_witness_matches, scene_component_composition_fingerprint,
    },
    ComponentRegistry, ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaOwnerSnapshot,
    ComponentTypeId, PersistentComponentSchemaOwnerId, SchemaCompositionFingerprint,
};

/// Bounds the data-owner and declaration indexes built for one Scene binding selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneComponentBindingLimits {
    owners: ItemLimit,
    claims: ItemLimit,
}

impl Default for SceneComponentBindingLimits {
    fn default() -> Self {
        Self {
            owners: ItemLimit::new(4_096).expect("Scene binding owner limit is non-zero"),
            claims: ItemLimit::new(131_072).expect("Scene binding claim limit is non-zero"),
        }
    }
}

impl SceneComponentBindingLimits {
    #[must_use]
    pub const fn owners(self) -> ItemLimit {
        self.owners
    }

    #[must_use]
    pub const fn claims(self) -> ItemLimit {
        self.claims
    }

    #[must_use]
    pub const fn with_owners(mut self, limit: ItemLimit) -> Self {
        self.owners = limit;
        self
    }

    #[must_use]
    pub const fn with_claims(mut self, limit: ItemLimit) -> Self {
        self.claims = limit;
        self
    }
}

/// Refusal to compose disjoint native and data-only Scene definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneComponentBindingError {
    ReservedNativeComponent {
        component: ComponentTypeId,
    },
    DuplicateOwner {
        owner: PersistentComponentSchemaOwnerId,
    },
    DivergentComponentOwnership {
        component: ComponentTypeId,
    },
    OwnerLimitExceeded {
        observed: usize,
        maximum: usize,
    },
    ClaimLimitExceeded {
        observed: usize,
        maximum: usize,
    },
}

impl std::fmt::Display for SceneComponentBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReservedNativeComponent { component } => write!(
                formatter,
                "Scene-owned data carrier cannot be bound as native component '{component}'"
            ),
            Self::DuplicateOwner { owner } => write!(
                formatter,
                "Scene bindings repeat durable Schema owner '{owner}'"
            ),
            Self::DivergentComponentOwnership { component } => write!(
                formatter,
                "Scene component '{component}' is claimed by more than one representation owner"
            ),
            Self::OwnerLimitExceeded { observed, maximum } => write!(
                formatter,
                "Scene bindings contain {observed} data owners, maximum is {maximum}"
            ),
            Self::ClaimLimitExceeded { observed, maximum } => write!(
                formatter,
                "Scene bindings contain {observed} data declarations, maximum is {maximum}"
            ),
        }
    }
}

impl std::error::Error for SceneComponentBindingError {}

/// An immutable, explicit selection of native and data-only Scene representations.
///
/// Missing native bindings never implicitly choose data storage. Data owners must be separately
/// admitted; their records require their durable owner claim. This selects Schema interpretation,
/// not a gameplay callback, VM, package activation or permission to replace a managed Runtime.
#[derive(Clone)]
pub struct SceneComponentBindings(Arc<Bindings>);

/// Metadata-only witness of one exact native and data-only Scene definition selection.
///
/// The native snapshot is held weakly: retaining this witness does not keep native bindings,
/// codecs or migrations alive. Data-owner snapshots contain only admitted Schema metadata.
/// This is transient admission identity, not a persistent fingerprint or runtime authority.
#[derive(Clone)]
pub struct SceneComponentBindingsWitness {
    native: ComponentRegistrySnapshotWitness,
    owners: Vec<ComponentSchemaOwnerSnapshot>,
}

struct Bindings {
    native: ComponentRegistrySnapshot,
    native_registry: ComponentRegistry,
    owners: Vec<ComponentSchemaOwnerSnapshot>,
    claims: BTreeMap<ComponentTypeId, usize>,
}

impl std::fmt::Debug for SceneComponentBindings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SceneComponentBindings")
            .field("data_owners", &self.0.owners)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SceneComponentBindingsWitness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SceneComponentBindingsWitness")
            .field("data_owners", &self.owners)
            .finish_non_exhaustive()
    }
}

impl SceneComponentBindingsWitness {
    /// Requires the exact native snapshot and the exact owner snapshots in canonical owner order.
    ///
    /// Separately admitted equivalent catalogs do not match. Matching does not retain or upgrade
    /// the native snapshot and does not grant permission to replace a managed Runtime.
    #[must_use]
    pub fn matches(&self, bindings: &SceneComponentBindings) -> bool {
        component_registry_snapshot_witness_matches(&self.native, bindings.native_snapshot())
            && self.owners.len() == bindings.data_owners().len()
            && self
                .owners
                .iter()
                .zip(bindings.data_owners())
                .all(|(left, right)| left.owner_id() == right.owner_id() && left.ptr_eq(right))
    }
}

impl SceneComponentBindings {
    pub fn new(
        native: ComponentRegistrySnapshot,
        owners: impl IntoIterator<Item = ComponentSchemaOwnerSnapshot>,
    ) -> Result<Self, SceneComponentBindingError> {
        Self::new_with_limits(native, owners, SceneComponentBindingLimits::default())
    }

    /// Retains admitted owner snapshots, rejecting overlaps before publishing the selection.
    /// Every current/predecessor active or tombstoned ID consumes one declaration allowance.
    pub fn new_with_limits(
        native: ComponentRegistrySnapshot,
        owners: impl IntoIterator<Item = ComponentSchemaOwnerSnapshot>,
        limits: SceneComponentBindingLimits,
    ) -> Result<Self, SceneComponentBindingError> {
        let native_registry = ComponentRegistry::from_snapshot(native.clone());
        if let Some(schema) = native_registry.schema_for_type::<crate::SceneDataComponents>() {
            return Err(SceneComponentBindingError::ReservedNativeComponent {
                component: schema.id().clone(),
            });
        }
        let mut admitted = BTreeMap::new();
        let mut declared = 0usize;
        for owner in owners {
            let observed = admitted.len().saturating_add(1);
            if observed > limits.owners.get() {
                return Err(SceneComponentBindingError::OwnerLimitExceeded {
                    observed,
                    maximum: limits.owners.get(),
                });
            }
            if admitted.contains_key(owner.owner_id())
                || native
                    .schema_owners()
                    .any(|native_owner| native_owner.as_str() == owner.owner_id().as_str())
            {
                return Err(SceneComponentBindingError::DuplicateOwner {
                    owner: owner.owner_id().clone(),
                });
            }
            for catalog in std::iter::once(owner.current()).chain(owner.predecessor()) {
                declared = declared
                    .saturating_add(catalog.components().len())
                    .saturating_add(catalog.type_tombstones().len());
                if declared > limits.claims.get() {
                    return Err(SceneComponentBindingError::ClaimLimitExceeded {
                        observed: declared,
                        maximum: limits.claims.get(),
                    });
                }
            }
            admitted.insert(owner.owner_id().clone(), owner);
        }
        let owners: Vec<_> = admitted.into_values().collect();
        let mut claims = BTreeMap::new();
        for (index, owner) in owners.iter().enumerate() {
            for catalog in std::iter::once(owner.current()).chain(owner.predecessor()) {
                for component in catalog
                    .components()
                    .iter()
                    .map(ComponentSchema::id)
                    .chain(catalog.type_tombstones())
                {
                    if native.schema(component).is_some()
                        || native
                            .catalog()
                            .type_tombstones()
                            .binary_search(component)
                            .is_ok()
                        || claims
                            .get(component)
                            .is_some_and(|existing| *existing != index)
                    {
                        return Err(SceneComponentBindingError::DivergentComponentOwnership {
                            component: component.clone(),
                        });
                    }
                    if !claims.contains_key(component) {
                        claims.insert(component.clone(), index);
                    }
                }
            }
        }
        Ok(Self(Arc::new(Bindings {
            native_registry,
            native,
            owners,
            claims,
        })))
    }

    #[must_use]
    pub fn native_snapshot(&self) -> &ComponentRegistrySnapshot {
        &self.0.native
    }

    #[must_use]
    pub fn data_owners(&self) -> &[ComponentSchemaOwnerSnapshot] {
        &self.0.owners
    }

    #[must_use]
    pub fn data_owner(&self, component: &ComponentTypeId) -> Option<&ComponentSchemaOwnerSnapshot> {
        let owner = &self.0.owners[*self.0.claims.get(component)?];
        owner.schema(component).map(|_| owner)
    }

    #[must_use]
    pub fn schema(&self, component: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.0.native.schema(component).or_else(|| {
            self.data_owner(component)
                .and_then(|owner| owner.schema(component))
        })
    }

    /// Captures exact selection identity without retaining native executable definitions.
    #[must_use]
    pub fn witness(&self) -> SceneComponentBindingsWitness {
        SceneComponentBindingsWitness {
            native: component_registry_snapshot_witness(self.native_snapshot()),
            owners: self.data_owners().to_vec(),
        }
    }

    /// Returns canonical Schema identity for native owner receipts and selected data owners.
    ///
    /// Uses Reflect's existing composition fingerprint domain and canonical owner order.
    /// Provider order and native binding behavior do not affect this value; raw standalone
    /// native definitions without owner receipts contribute no owner entries. Fingerprint
    /// equality never replaces [`Self::same_definitions`] or exact managed admission.
    #[must_use]
    pub fn composition_fingerprint(&self) -> SchemaCompositionFingerprint {
        scene_component_composition_fingerprint(self.native_snapshot(), self.data_owners())
    }

    pub(crate) fn native_registry(&self) -> &ComponentRegistry {
        &self.0.native_registry
    }

    /// Tests exact immutable selection identity independently of the bindings container.
    ///
    /// The native snapshots must satisfy [`ComponentRegistrySnapshot::ptr_eq`]. Data owners are
    /// stored sorted by durable owner ID; both selections must have the same length and each
    /// corresponding owner ID and [`ComponentSchemaOwnerSnapshot::ptr_eq`] must match.
    /// Reordering the same admitted inputs matches; separately admitting equivalent catalogs
    /// does not. Catalog or composition fingerprint equality is insufficient.
    #[must_use]
    pub fn same_definitions(&self, other: &Self) -> bool {
        self.native_snapshot().ptr_eq(other.native_snapshot())
            && self.data_owners().len() == other.data_owners().len()
            && self
                .data_owners()
                .iter()
                .zip(other.data_owners())
                .all(|(left, right)| left.owner_id() == right.owner_id() && left.ptr_eq(right))
    }
}
