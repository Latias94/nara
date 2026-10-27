//! Schema admission shared by native and data-only Prefab compilation.

use nara_reflect::{
    ComponentCapability, ComponentMigrationError, ComponentRegistry, ComponentSchema,
    ComponentTypeId, ComponentValue, MigratedComponentValue,
};

use crate::{PrefabExpansionFailureKind, SceneComponentRecord};

#[derive(Clone, Copy)]
pub(crate) enum SchemaAuthority<'a> {
    Native(&'a ComponentRegistry),
    #[cfg(feature = "serde")]
    Authoring(&'a crate::AuthoringSchemaBasis),
}

impl<'a> SchemaAuthority<'a> {
    pub(crate) fn native_registry(self) -> Option<&'a ComponentRegistry> {
        match self {
            Self::Native(registry) => Some(registry),
            #[cfg(feature = "serde")]
            Self::Authoring(_) => None,
        }
    }

    pub(crate) fn schema(self, component: &ComponentTypeId) -> Option<&'a ComponentSchema> {
        match self {
            Self::Native(registry) => registry.schema(component),
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis.schema(component),
        }
    }

    pub(crate) fn owner_matches(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> bool {
        let Some(declared) = record.owner() else {
            return true;
        };
        match self {
            Self::Native(registry) => registry
                .component_schema_owner(component)
                .is_some_and(|owner| declared.matches_executable(owner).unwrap_or(false)),
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis.component_schema_owner(component) == Some(declared),
        }
    }

    /// A complete slot can be removed or replaced without interpreting its previous payload.
    /// Opaque slots retain their record and cost until those layer operations have run.
    pub(crate) fn can_interpret(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> bool {
        let Some(schema) = self.schema(component) else {
            return false;
        };
        if record.version != schema.version() || !self.owner_matches(component, record) {
            return false;
        }
        match self {
            Self::Native(_) => true,
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis.has_authoring_adapter(component),
        }
    }

    pub(crate) fn current_schema(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<&'a ComponentSchema, PrefabExpansionFailureKind> {
        let schema = self
            .schema(component)
            .ok_or(PrefabExpansionFailureKind::Component)?;
        if !self.owner_matches(component, record) {
            return Err(PrefabExpansionFailureKind::OwnerAdmission);
        }
        if !self.can_interpret(component, record) {
            return Err(PrefabExpansionFailureKind::Component);
        }
        match self {
            Self::Native(registry) => {
                if registry.native_rust_type_path(component).is_none() {
                    return Err(PrefabExpansionFailureKind::Component);
                }
                registry
                    .validate_whole_value_capabilities(component, [ComponentCapability::Scene])
                    .map_err(|_| PrefabExpansionFailureKind::Component)?;
            }
            #[cfg(feature = "serde")]
            Self::Authoring(_) => {
                nara_reflect::__private::validate_schema_capabilities(
                    schema,
                    [ComponentCapability::Scene],
                )
                .map_err(|_| PrefabExpansionFailureKind::Component)?;
            }
        }
        Ok(schema)
    }

    pub(crate) fn migrate_owned(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
        value: ComponentValue,
    ) -> Result<MigratedComponentValue, ComponentMigrationError> {
        match self {
            Self::Native(registry) => {
                registry.migrate_component_value_owned(component, record.version, value)
            }
            #[cfg(feature = "serde")]
            Self::Authoring(_) => {
                let schema = self.schema(component).ok_or_else(|| {
                    ComponentMigrationError::UnknownComponentId {
                        component_id: component.clone(),
                    }
                })?;
                if record.version != schema.version() {
                    return Err(ComponentMigrationError::UnsupportedVersion {
                        component_id: component.clone(),
                        from_version: record.version,
                        target_version: schema.version(),
                    });
                }
                Ok(MigratedComponentValue {
                    version: record.version,
                    value,
                })
            }
        }
    }

    #[cfg(feature = "serde")]
    pub(crate) fn unavailable_reason(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Option<crate::AuthoringRecordReadiness> {
        use crate::AuthoringRecordReadiness;
        let Self::Authoring(basis) = self else {
            return None;
        };
        let Some(schema) = basis
            .schema(component)
            .filter(|_| self.owner_matches(component, record))
        else {
            return Some(AuthoringRecordReadiness::UnknownSchema);
        };
        if record.version > schema.version() {
            Some(AuthoringRecordReadiness::FutureSchema)
        } else if record.version < schema.version() {
            Some(AuthoringRecordReadiness::Unmigratable)
        } else if !basis.has_authoring_adapter(component) {
            Some(AuthoringRecordReadiness::KnownUnbound)
        } else {
            None
        }
    }
}

impl<'a> From<&'a ComponentRegistry> for SchemaAuthority<'a> {
    fn from(registry: &'a ComponentRegistry) -> Self {
        Self::Native(registry)
    }
}
