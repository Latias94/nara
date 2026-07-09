//! Component value migration contracts between schema versions.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::{ComponentCodecError, ComponentSchemaVersion, ComponentTypeId, ComponentValue};

#[derive(Debug, Clone, PartialEq)]
pub struct MigratedComponentValue {
    pub version: ComponentSchemaVersion,
    pub value: ComponentValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentMigrationError {
    UnknownComponentId {
        component_id: ComponentTypeId,
    },
    UnsupportedVersion {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
        target_version: ComponentSchemaVersion,
    },
    MissingMigration {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
        target_version: ComponentSchemaVersion,
    },
    MigrationFailed {
        component_id: ComponentTypeId,
        from_version: ComponentSchemaVersion,
        to_version: ComponentSchemaVersion,
        error: ComponentCodecError,
    },
}

impl Display for ComponentMigrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownComponentId { component_id } => write!(
                formatter,
                "component id '{}' is not registered",
                component_id.as_str()
            ),
            Self::UnsupportedVersion {
                component_id,
                from_version,
                target_version,
            } => write!(
                formatter,
                "component id '{}' version {} cannot migrate to current version {}",
                component_id.as_str(),
                from_version.0,
                target_version.0
            ),
            Self::MissingMigration {
                component_id,
                from_version,
                target_version,
            } => write!(
                formatter,
                "component id '{}' is missing a migration from version {} toward current version {}",
                component_id.as_str(),
                from_version.0,
                target_version.0
            ),
            Self::MigrationFailed {
                component_id,
                from_version,
                to_version,
                error,
            } => write!(
                formatter,
                "component id '{}' migration {} -> {} failed: {error}",
                component_id.as_str(),
                from_version.0,
                to_version.0
            ),
        }
    }
}

impl Error for ComponentMigrationError {}

pub(crate) type ComponentMigrationFn =
    dyn Fn(ComponentValue) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static;

pub(crate) struct ComponentMigration {
    pub(crate) to_version: ComponentSchemaVersion,
    pub(crate) migrate: Box<ComponentMigrationFn>,
}
