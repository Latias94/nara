//! Schema admission shared by native and data-only Prefab compilation.

use nara_reflect::{
    ComponentCapability, ComponentMigrationError, ComponentRegistrySnapshot, ComponentSchema,
    ComponentTypeId, ComponentValue, MigratedComponentValue,
};

use crate::{SceneCompilationFailureKind, SceneComponentBindings, SceneComponentRecord};

#[derive(Clone, Copy)]
pub(crate) enum SchemaAuthority<'a> {
    Native(&'a ComponentRegistrySnapshot),
    Mixed(&'a SceneComponentBindings),
    #[cfg(feature = "serde")]
    Authoring(&'a crate::AuthoringSchemaBasis),
}

impl<'a> SchemaAuthority<'a> {
    pub(crate) fn native_snapshot(self) -> Option<&'a ComponentRegistrySnapshot> {
        match self {
            Self::Native(registry) => Some(registry),
            Self::Mixed(bindings) => Some(bindings.native_snapshot()),
            #[cfg(feature = "serde")]
            Self::Authoring(_) => None,
        }
    }

    pub(crate) fn schema(self, component: &ComponentTypeId) -> Option<&'a ComponentSchema> {
        match self {
            Self::Native(registry) => registry.schema(component),
            Self::Mixed(bindings) => bindings.schema(component),
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis.schema(component),
        }
    }

    /// The actual selected owner, independent of an omitted authored claim.
    /// Callers first require the component Schema; `None` then means an unowned definition.
    pub(crate) fn selected_owner_name(self, component: &ComponentTypeId) -> Option<&'a str> {
        match self {
            Self::Native(registry) => registry
                .component_schema_owner(component)
                .map(|owner| owner.as_str()),
            Self::Mixed(bindings) => bindings.data_owner(component).map_or_else(
                || {
                    bindings
                        .native_snapshot()
                        .component_schema_owner(component)
                        .map(|owner| owner.as_str())
                },
                |owner| Some(owner.owner_id().as_str()),
            ),
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis
                .component_schema_owner(component)
                .map(|owner| owner.as_str()),
        }
    }

    pub(crate) fn owner_matches(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> bool {
        self.owner_claim_matches(component, record.owner(), false)
    }

    /// Compiled absence identifies an actually unowned definition; source absence may omit a
    /// native owner claim. Neither policy can omit the selected owner of a data component.
    pub(crate) fn owner_claim_matches(
        self,
        component: &ComponentTypeId,
        declared: Option<&nara_reflect::PersistentComponentSchemaOwnerId>,
        exact: bool,
    ) -> bool {
        if exact {
            return declared.map(|owner| owner.as_str()) == self.selected_owner_name(component);
        }
        if let Self::Mixed(bindings) = self
            && let Some(owner) = bindings.data_owner(component)
        {
            return declared == Some(owner.owner_id());
        }
        let Some(declared) = declared else {
            return true;
        };
        match self {
            Self::Native(registry) => registry
                .component_schema_owner(component)
                .is_some_and(|owner| declared.matches_executable(owner).unwrap_or(false)),
            Self::Mixed(bindings) => bindings
                .native_snapshot()
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
            Self::Native(_) | Self::Mixed(_) => true,
            #[cfg(feature = "serde")]
            Self::Authoring(basis) => basis.has_authoring_adapter(component),
        }
    }

    pub(crate) fn current_schema(
        self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<&'a ComponentSchema, SceneCompilationFailureKind> {
        let schema = self
            .schema(component)
            .ok_or(SceneCompilationFailureKind::Component)?;
        if !self.owner_matches(component, record) {
            return Err(SceneCompilationFailureKind::OwnerAdmission);
        }
        if !self.can_interpret(component, record) {
            return Err(SceneCompilationFailureKind::Component);
        }
        self.validate_current_schema_capabilities(component, schema)?;
        Ok(schema)
    }

    pub(crate) fn current_flat_schema(
        self,
        component: &ComponentTypeId,
        declared: Option<&nara_reflect::PersistentComponentSchemaOwnerId>,
        version: nara_reflect::ComponentSchemaVersion,
    ) -> Result<&'a ComponentSchema, SceneCompilationFailureKind> {
        let schema = self
            .schema(component)
            .ok_or(SceneCompilationFailureKind::Component)?;
        if !self.owner_claim_matches(component, declared, true) {
            return Err(SceneCompilationFailureKind::OwnerAdmission);
        }
        if schema.version() != version {
            return Err(SceneCompilationFailureKind::Component);
        }
        #[cfg(feature = "serde")]
        if let Self::Authoring(basis) = self
            && !basis.has_authoring_adapter(component)
        {
            return Err(SceneCompilationFailureKind::Component);
        }
        self.validate_current_schema_capabilities(component, schema)?;
        Ok(schema)
    }

    pub(crate) fn validate_current_schema_capabilities(
        self,
        component: &ComponentTypeId,
        schema: &ComponentSchema,
    ) -> Result<(), SceneCompilationFailureKind> {
        match self {
            Self::Native(registry) => {
                if registry.native_rust_type_path(component).is_none() {
                    return Err(SceneCompilationFailureKind::Component);
                }
                registry
                    .validate_whole_value_capabilities(component, [ComponentCapability::Scene])
                    .map_err(|_| SceneCompilationFailureKind::Component)?;
            }
            Self::Mixed(bindings) => {
                if bindings.data_owner(component).is_none() {
                    bindings
                        .native_snapshot()
                        .validate_whole_value_capabilities(component, [ComponentCapability::Scene])
                        .map_err(|_| SceneCompilationFailureKind::Component)?;
                } else {
                    nara_reflect::__private::validate_schema_capabilities(
                        schema,
                        [ComponentCapability::Scene],
                    )
                    .map_err(|_| SceneCompilationFailureKind::Component)?;
                }
            }
            #[cfg(feature = "serde")]
            Self::Authoring(_) => {
                nara_reflect::__private::validate_schema_capabilities(
                    schema,
                    [ComponentCapability::Scene],
                )
                .map_err(|_| SceneCompilationFailureKind::Component)?;
            }
        }
        Ok(())
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
            Self::Mixed(bindings) if bindings.data_owner(component).is_none() => bindings
                .native_snapshot()
                .migrate_component_value_owned(component, record.version, value),
            Self::Mixed(_) => current_version_value(self, component, record, value),
            #[cfg(feature = "serde")]
            Self::Authoring(_) => current_version_value(self, component, record, value),
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

fn current_version_value(
    authority: SchemaAuthority<'_>,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    value: ComponentValue,
) -> Result<MigratedComponentValue, ComponentMigrationError> {
    let schema =
        authority
            .schema(component)
            .ok_or_else(|| ComponentMigrationError::UnknownComponentId {
                component_id: component.clone(),
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

impl<'a> From<&'a ComponentRegistrySnapshot> for SchemaAuthority<'a> {
    fn from(registry: &'a ComponentRegistrySnapshot) -> Self {
        Self::Native(registry)
    }
}
