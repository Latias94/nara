use std::{collections::BTreeMap, sync::Arc};

use nara_core::ItemLimit;
use nara_reflect::{
    ComponentRegistry, ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaOwnerSnapshot,
    ComponentTypeId, PersistentComponentSchemaOwnerId,
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
#[derive(Debug)]
pub enum SceneComponentBindingError {
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
            native_registry: ComponentRegistry::from_snapshot(native.clone()),
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

    pub(crate) fn native_registry(&self) -> &ComponentRegistry {
        &self.0.native_registry
    }
}
