use std::sync::Arc;

use crate::{
    ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog, ComponentTypeId,
    PersistentComponentSchemaOwnerId, registry::prepare_owner_catalogs,
};

/// An immutable, data-only admission of one owner's current catalog and lineage boundary.
///
/// This contains no native bindings or executable callbacks and does not require serialization.
/// It proves metadata validity, not that a representation or language module is installed.
/// Clones share authority; separately admitted equivalent catalogs do not.
#[derive(Debug, Clone)]
pub struct ComponentSchemaOwnerSnapshot(Arc<OwnerCatalogs>);

#[derive(Debug)]
struct OwnerCatalogs {
    owner: PersistentComponentSchemaOwnerId,
    current: ComponentSchemaCatalog,
    predecessor: Option<ComponentSchemaCatalog>,
}

impl ComponentSchemaOwnerSnapshot {
    /// Admits already-owned catalog data using the ordinary Schema and lineage validators.
    ///
    /// File/transport consumers must apply their input/work limits before calling this method.
    /// This does not decode input, admit a multi-owner composition or register native types.
    pub fn admit(
        owner: PersistentComponentSchemaOwnerId,
        current: ComponentSchemaCatalog,
        predecessor: Option<ComponentSchemaCatalog>,
    ) -> Result<Self, ComponentRegistryError> {
        let (current, predecessor) = prepare_owner_catalogs(current, predecessor)?;
        Ok(Self(Arc::new(OwnerCatalogs {
            owner,
            current,
            predecessor,
        })))
    }

    #[must_use]
    pub fn owner_id(&self) -> &PersistentComponentSchemaOwnerId {
        &self.0.owner
    }

    #[must_use]
    pub fn current(&self) -> &ComponentSchemaCatalog {
        &self.0.current
    }

    #[must_use]
    pub fn predecessor(&self) -> Option<&ComponentSchemaCatalog> {
        self.0.predecessor.as_ref()
    }

    #[must_use]
    pub fn schema(&self, component: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.current()
            .components()
            .binary_search_by(|schema| schema.id().cmp(component))
            .ok()
            .map(|index| &self.current().components()[index])
    }

    /// Tests exact admission identity, not catalog-content or fingerprint equality.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
