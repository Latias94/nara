//! Flat RON source documents for scenes and prefabs.
//!
//! Both `.nara.scene.ron` and `.nara.prefab.ron` store local entity UUIDs and versioned Reflect
//! values. Reading and saving a document does not require the game's component catalog.

mod builtins;
mod wire;

pub use builtins::{ColliderSource, SpriteSource, builtin_component_schemas};

use std::collections::{BTreeMap, HashMap};
#[cfg(any(unix, windows))]
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use nara_core::{DocumentId, EntityUuid};
use nara_reflect::{
    ComponentCodecError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentTypeIdError, ComponentValue, PersistentComponentProvider, validate_component_value,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A source scene or prefab. Entity order has no meaning in the file format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDocument {
    #[serde(with = "wire::uuid_text")]
    pub id: DocumentId,
    #[serde(default, serialize_with = "wire::serialize_entities")]
    pub entities: Vec<SceneEntity>,
}

impl SceneDocument {
    #[must_use]
    pub fn new(id: DocumentId) -> Self {
        Self {
            id,
            entities: Vec::new(),
        }
    }

    /// Parses RON and validates the resulting source document.
    pub fn parse(source: &str) -> Result<Self, SceneDocumentError> {
        let document: Self = ron::from_str(source)?;
        document.validate()?;
        Ok(document)
    }

    #[cfg(any(unix, windows))]
    pub fn read(path: impl AsRef<Path>) -> Result<Self, SceneDocumentError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| SceneDocumentError::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::parse(&source)
    }

    /// Produces canonical RON with entities sorted by UUID and component keys sorted by ID.
    pub fn to_ron(&self) -> Result<String, SceneDocumentError> {
        self.validate()?;
        let config = ron::ser::PrettyConfig::new()
            .new_line("\n".into())
            .depth_limit(7);
        let mut source =
            ron::ser::to_string_pretty(self, config).map_err(SceneDocumentError::Serialize)?;
        source.push('\n');
        Ok(source)
    }

    /// Writes a complete document using a same-directory atomic replacement on native platforms.
    #[cfg(any(unix, windows))]
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), SceneDocumentError> {
        let path = path.as_ref();
        let source = self.to_ron()?;
        nara_core::vfs::write_atomic(path, source.as_bytes()).map_err(|source| {
            SceneDocumentError::Io {
                path: path.to_owned(),
                source,
            }
        })
    }

    /// Checks document-local identities, parent links, and built-in source path syntax.
    ///
    /// External component values and versions remain opaque until requested through a provider.
    pub fn validate(&self) -> Result<(), SceneDocumentError> {
        let mut indices = HashMap::with_capacity(self.entities.len());
        for (index, entity) in self.entities.iter().enumerate() {
            if indices.insert(entity.id, index).is_some() {
                return Err(SceneDocumentError::DuplicateEntity { entity: entity.id });
            }
            for (component, record) in &entity.components {
                ComponentTypeId::try_new(component.as_str()).map_err(|source| {
                    SceneDocumentError::InvalidComponentId {
                        entity: entity.id,
                        source,
                    }
                })?;
                if record.version.get() == 0 {
                    return Err(SceneDocumentError::InvalidComponentVersion {
                        entity: entity.id,
                        component: component.clone(),
                    });
                }
                record
                    .validate_source_path(component.as_str())
                    .map_err(|source| SceneDocumentError::InvalidComponentValue {
                        entity: entity.id,
                        component: component.clone(),
                        source,
                    })?;
            }
        }

        let mut visited_by = vec![None; self.entities.len()];
        for origin in 0..self.entities.len() {
            let mut next = Some(origin);
            while let Some(index) = next {
                let entity = &self.entities[index];
                match visited_by[index] {
                    Some(visitor) if visitor == origin => {
                        return Err(SceneDocumentError::HierarchyCycle { entity: entity.id });
                    }
                    Some(_) => break,
                    None => visited_by[index] = Some(origin),
                }
                next = match entity.parent {
                    Some(parent) => Some(*indices.get(&parent).ok_or(
                        SceneDocumentError::MissingParent {
                            entity: entity.id,
                            parent,
                        },
                    )?),
                    None => None,
                };
            }
        }
        Ok(())
    }
}

/// One entity in the flat source table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEntity {
    #[serde(with = "wire::uuid_text")]
    pub id: EntityUuid,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "wire::optional_uuid_text"
    )]
    pub parent: Option<EntityUuid>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "wire::deserialize_components"
    )]
    pub components: BTreeMap<ComponentTypeId, SceneComponent>,
}

impl SceneEntity {
    #[must_use]
    pub fn new(id: EntityUuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            parent: None,
            components: BTreeMap::new(),
        }
    }

    /// Encodes a typed component at its provider's current schema version.
    pub fn set<T: PersistentComponentProvider>(
        &mut self,
        component: &T,
    ) -> Result<(), SceneComponentError> {
        let schema = provider_schema::<T>()?;
        let value = component.encode_persistent_component()?;
        validate_component_value(&schema, &value)?;
        let record = SceneComponent {
            version: schema.version(),
            value,
        };
        record.validate_source_path(T::COMPONENT_ID)?;
        self.components.insert(schema.id().clone(), record);
        Ok(())
    }

    /// Reads the component selected by the provider's ID. An absent component returns `None`.
    pub fn typed<T: PersistentComponentProvider>(&self) -> Result<Option<T>, SceneComponentError> {
        let id = ComponentTypeId::try_new(T::COMPONENT_ID)?;
        self.components
            .get(&id)
            .map(SceneComponent::typed::<T>)
            .transpose()
    }
}

/// One versioned component payload, retained as a Reflect value tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneComponent {
    pub version: ComponentSchemaVersion,
    pub value: ComponentValue,
}

impl SceneComponent {
    /// Decodes a record already selected by its component ID using the provider's current schema.
    pub fn typed<T: PersistentComponentProvider>(&self) -> Result<T, SceneComponentError> {
        let schema = provider_schema::<T>()?;
        if self.version != schema.version() {
            return Err(SceneComponentError::VersionMismatch {
                component: schema.id().clone(),
                stored: self.version,
                current: schema.version(),
            });
        }
        validate_component_value(&schema, &self.value)?;
        self.validate_source_path(T::COMPONENT_ID)?;
        Ok(T::decode_persistent_component((&self.value).into())?)
    }

    fn validate_source_path(&self, component: &str) -> Result<(), ComponentCodecError> {
        if component == SpriteSource::COMPONENT_ID
            && self.version == ComponentSchemaVersion::ONE
            && let Some(path) = self.value.get("image").and_then(ComponentValue::as_str)
        {
            builtins::png_asset_path(path)?;
        }
        Ok(())
    }
}

fn provider_schema<T: PersistentComponentProvider>() -> Result<ComponentSchema, SceneComponentError>
{
    let id = ComponentTypeId::try_new(T::COMPONENT_ID)?;
    let schema = T::persistent_component_schema();
    if schema.id() != &id {
        return Err(SceneComponentError::ProviderIdMismatch {
            expected: T::COMPONENT_ID,
            actual: schema.id().clone(),
        });
    }
    if schema.version().get() == 0 {
        return Err(SceneComponentError::InvalidProviderVersion { component: id });
    }
    Ok(schema)
}

#[derive(Debug, Error)]
pub enum SceneDocumentError {
    #[cfg(any(unix, windows))]
    #[error("could not access scene document {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid source scene RON: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("could not serialize source scene: {0}")]
    Serialize(#[source] ron::Error),
    #[error("duplicate entity UUID {entity}")]
    DuplicateEntity { entity: EntityUuid },
    #[error("entity {entity} has missing parent {parent}")]
    MissingParent {
        entity: EntityUuid,
        parent: EntityUuid,
    },
    #[error("entity {entity} participates in a parent cycle")]
    HierarchyCycle { entity: EntityUuid },
    #[error("entity {entity} has an invalid component ID: {source}")]
    InvalidComponentId {
        entity: EntityUuid,
        #[source]
        source: ComponentTypeIdError,
    },
    #[error("entity {entity} has schema version zero for {component}")]
    InvalidComponentVersion {
        entity: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("entity {entity} has an invalid {component} value: {source}")]
    InvalidComponentValue {
        entity: EntityUuid,
        component: ComponentTypeId,
        #[source]
        source: ComponentCodecError,
    },
}

#[derive(Debug, Error)]
pub enum SceneComponentError {
    #[error(transparent)]
    InvalidId(#[from] ComponentTypeIdError),
    #[error("component provider declares '{expected}' but provided schema '{actual}'")]
    ProviderIdMismatch {
        expected: &'static str,
        actual: ComponentTypeId,
    },
    #[error("component provider for '{component}' has schema version zero")]
    InvalidProviderVersion { component: ComponentTypeId },
    #[error("component '{component}' has schema version {}, provider uses {}", .stored.get(), .current.get())]
    VersionMismatch {
        component: ComponentTypeId,
        stored: ComponentSchemaVersion,
        current: ComponentSchemaVersion,
    },
    #[error(transparent)]
    Codec(#[from] ComponentCodecError),
}
