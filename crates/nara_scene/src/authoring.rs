//! Compact RON source documents for scenes and prefabs.

pub(crate) mod compact;
pub mod prefab;
mod wire;

pub use crate::components::{
    ColliderSource, SpriteSource, TilemapSource, builtin_component_schemas,
};
pub use compact::CompactValueError;

use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(any(unix, windows))]
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    ComponentCodecError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentTypeIdError, ComponentValue, Schema,
};
use ron::value::RawValue;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A source scene or prefab. Entity order has no meaning in the file format.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDocument {
    pub id: DocumentId,
    pub assets: BTreeMap<String, SceneAsset>,
    pub entities: Vec<SceneEntity>,
}

impl SceneDocument {
    #[must_use]
    pub fn new(id: DocumentId) -> Self {
        Self {
            id,
            assets: BTreeMap::new(),
            entities: Vec::new(),
        }
    }

    /// Parses with built-in schemas. Other component payloads remain opaque.
    pub fn parse(source: &str) -> Result<Self, SceneDocumentError> {
        Self::parse_with(source, &[])
    }

    /// Parses using built-in and project schemas. Components without a matching schema version
    /// retain their original RON payload.
    pub fn parse_with(
        source: &str,
        schemas: &[ComponentSchema],
    ) -> Result<Self, SceneDocumentError> {
        let schemas = compact::Schemas::new(schemas)?;
        let document = wire::decode(source, &schemas)?;
        document.validate()?;
        Ok(document)
    }

    #[cfg(any(unix, windows))]
    pub fn read(path: impl AsRef<Path>) -> Result<Self, SceneDocumentError> {
        Self::read_with(path, &[])
    }

    #[cfg(any(unix, windows))]
    pub fn read_with(
        path: impl AsRef<Path>,
        schemas: &[ComponentSchema],
    ) -> Result<Self, SceneDocumentError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| SceneDocumentError::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::parse_with(&source, schemas)
    }

    /// Produces canonical scene RON using built-in schemas.
    pub fn to_ron(&self) -> Result<String, SceneDocumentError> {
        self.to_ron_with(&[])
    }

    /// Produces canonical scene RON with known fields in schema declaration order.
    pub fn to_ron_with(&self, schemas: &[ComponentSchema]) -> Result<String, SceneDocumentError> {
        self.encode(schemas, wire::DocumentKind::Scene)
    }

    /// Atomically replaces a native file using built-in schemas.
    #[cfg(any(unix, windows))]
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), SceneDocumentError> {
        self.write_with(path, &[])
    }

    /// Atomically replaces a native file. A `.prefab.ron` path uses the `Prefab` document name.
    #[cfg(any(unix, windows))]
    pub fn write_with(
        &self,
        path: impl AsRef<Path>,
        schemas: &[ComponentSchema],
    ) -> Result<(), SceneDocumentError> {
        let path = path.as_ref();
        let kind = if path.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".prefab.ron")
        }) {
            wire::DocumentKind::Prefab
        } else {
            wire::DocumentKind::Scene
        };
        let source = self.encode(schemas, kind)?;
        nara_core::vfs::write_atomic(path, source.as_bytes()).map_err(|source| {
            SceneDocumentError::Io {
                path: path.to_owned(),
                source,
            }
        })
    }

    fn encode(
        &self,
        schemas: &[ComponentSchema],
        kind: wire::DocumentKind,
    ) -> Result<String, SceneDocumentError> {
        self.validate()?;
        wire::encode(self, &compact::Schemas::new(schemas)?, kind)
    }

    /// Materializes opaque records newly covered by the supplied schemas.
    ///
    /// Each record is replaced only after it decodes and validates in full. Failed matching
    /// records are reported and retain their original bytes. Missing schemas and mismatched
    /// versions leave records untouched. Invalid document or schema setup makes no changes.
    pub fn resolve_unknown_components(
        &mut self,
        schemas: &[ComponentSchema],
    ) -> Result<Vec<SceneDocumentError>, SceneDocumentError> {
        self.validate()?;
        let schemas = compact::Schemas::new(schemas)?;
        rebind_rows(&mut self.entities, &self.assets, None, &schemas)
    }

    /// Checks document-local identities, parent links, asset aliases, and record metadata.
    pub fn validate(&self) -> Result<(), SceneDocumentError> {
        for (alias, asset) in &self.assets {
            if alias.is_empty() || alias.chars().any(char::is_control) {
                return Err(SceneDocumentError::InvalidAssetAlias {
                    alias: alias.clone(),
                });
            }
            if asset.hint == AssetPath::ROOT {
                return Err(SceneDocumentError::EmptyAssetHint {
                    alias: alias.clone(),
                });
            }
        }

        let mut indices = HashMap::with_capacity(self.entities.len());
        for (index, entity) in self.entities.iter().enumerate() {
            if indices.insert(entity.id, index).is_some() {
                return Err(SceneDocumentError::DuplicateEntity { entity: entity.id });
            }
            let added = entity
                .prefab
                .iter()
                .flat_map(|prefab| &prefab.added_components);
            for (component, record) in entity.components.iter().chain(added) {
                validate_component_id(entity.id, component)?;
                if record.version().get() == 0 {
                    return Err(SceneDocumentError::InvalidComponentVersion {
                        entity: entity.id,
                        component: component.clone(),
                    });
                }
                if let SceneComponent::Unknown(unknown) = record
                    && &unknown.type_id != component
                {
                    return Err(SceneDocumentError::UnknownComponentIdMismatch {
                        entity: entity.id,
                        component: component.clone(),
                        payload: unknown.type_id.clone(),
                    });
                }
            }
            if let Some(prefab) = &entity.prefab {
                for component in &prefab.removed_components {
                    validate_component_id(entity.id, component)?;
                }
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

/// Prepares a change of schemas for authored documents and partial editor history snapshots.
pub struct SceneSchemaRebind<'a> {
    previous: compact::Schemas<'a>,
    current: compact::Schemas<'a>,
}

impl<'a> SceneSchemaRebind<'a> {
    pub fn new(
        previous: &'a [ComponentSchema],
        current: &'a [ComponentSchema],
    ) -> Result<Self, SceneDocumentError> {
        Ok(Self {
            previous: compact::Schemas::new(previous)?,
            current: compact::Schemas::new(current)?,
        })
    }

    /// Rebinds validated rows using their original document's asset aliases.
    ///
    /// A known record the current schema cannot encode becomes opaque at its original version.
    /// Previously opaque records that still cannot decode retain their exact payload, with
    /// diagnostics for matching schemas. If a known record cannot be preserved, no rows change.
    pub fn rebind_rows(
        &self,
        entities: &mut [SceneEntity],
        assets: &BTreeMap<String, SceneAsset>,
    ) -> Result<Vec<SceneDocumentError>, SceneDocumentError> {
        rebind_rows(entities, assets, Some(&self.previous), &self.current)
    }
}

fn rebind_rows(
    entities: &mut [SceneEntity],
    assets: &BTreeMap<String, SceneAsset>,
    previous: Option<&compact::Schemas<'_>>,
    current: &compact::Schemas<'_>,
) -> Result<Vec<SceneDocumentError>, SceneDocumentError> {
    let mut replacements = Vec::new();
    let mut diagnostics = Vec::new();
    for entity in entities {
        let added = entity
            .prefab
            .iter_mut()
            .flat_map(|prefab| &mut prefab.added_components);
        for (component, record) in entity.components.iter_mut().chain(added) {
            let entity = entity.id;
            let version = record.version();
            let invalid = |source| SceneDocumentError::InvalidComponentValue {
                entity,
                component: component.clone(),
                source,
            };
            let replacement = match record {
                SceneComponent::Unknown(unknown) => {
                    let Some(schema) = current.matching(component, version) else {
                        continue;
                    };
                    match compact::decode_component(&unknown.raw, schema, assets) {
                        Ok(value) => SceneComponent::known(version, value),
                        Err(source) => {
                            diagnostics.push(invalid(source));
                            continue;
                        }
                    }
                }
                SceneComponent::Known { value, .. } => {
                    let Some(previous) = previous else {
                        continue;
                    };
                    if let Some(schema) = current.matching(component, version) {
                        match compact::encode_component(value, schema, assets) {
                            Ok(_) => continue,
                            Err(source) => diagnostics.push(invalid(source)),
                        }
                    }
                    let schema = previous.matching(component, version).ok_or_else(|| {
                        SceneDocumentError::MissingComponentSchema {
                            entity,
                            component: component.clone(),
                            version,
                        }
                    })?;
                    let raw = preserve_component(value, schema, assets).map_err(invalid)?;
                    SceneComponent::Unknown(UnknownComponent {
                        type_id: component.clone(),
                        version,
                        raw,
                    })
                }
            };
            replacements.push((record, replacement));
        }
    }
    for (record, replacement) in replacements {
        *record = replacement;
    }
    Ok(diagnostics)
}

fn preserve_component(
    value: &ComponentValue,
    schema: &ComponentSchema,
    assets: &BTreeMap<String, SceneAsset>,
) -> Result<RawRon, CompactValueError> {
    if schema
        .fields()
        .iter()
        .all(|field| field.default_value().is_none())
    {
        return compact::encode_component(value, schema, assets);
    }
    let value = schema.with_defaults(value.clone())?;
    let mut schema = schema.clone();
    for field in &mut schema.fields {
        field.default_value = None;
    }
    compact::encode_component(&value, &schema, assets)
}

fn validate_component_id(
    entity: EntityUuid,
    component: &ComponentTypeId,
) -> Result<(), SceneDocumentError> {
    ComponentTypeId::try_new(component.as_str())
        .map(drop)
        .map_err(|source| SceneDocumentError::InvalidComponentId { entity, source })
}

/// An asset alias binds a short source name to an ID and an asset-root-relative path hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAsset {
    pub id: AssetId,
    pub hint: AssetPath,
}

/// One entity in the flat source table.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneEntity {
    pub id: EntityUuid,
    pub name: String,
    pub parent: Option<EntityUuid>,
    pub components: BTreeMap<ComponentTypeId, SceneComponent>,
    pub prefab: Option<PrefabInstance>,
}

impl SceneEntity {
    #[must_use]
    pub fn new(id: EntityUuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            parent: None,
            components: BTreeMap::new(),
            prefab: None,
        }
    }

    /// Encodes a typed component at its current schema version.
    pub fn set<T: Schema>(&mut self, component: &T) -> Result<(), SceneComponentError> {
        let schema = provider_schema::<T>()?;
        let value = schema.with_defaults(component.encode_component()?)?;
        self.components.insert(
            schema.id().clone(),
            SceneComponent::known(schema.version(), value),
        );
        Ok(())
    }

    /// Reads the component selected by the schema's ID. An absent component returns `None`.
    pub fn typed<T: Schema>(&self) -> Result<Option<T>, SceneComponentError> {
        let id = ComponentTypeId::try_new(T::COMPONENT_ID)?;
        self.components
            .get(&id)
            .map(SceneComponent::typed::<T>)
            .transpose()
    }
}

/// An instance's source reference and sparse edits.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabInstance {
    pub asset: AssetId,
    pub overrides: BTreeMap<String, RawRon>,
    pub added_components: BTreeMap<ComponentTypeId, SceneComponent>,
    pub removed_components: BTreeSet<ComponentTypeId>,
}

impl PrefabInstance {
    #[must_use]
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            overrides: BTreeMap::new(),
            added_components: BTreeMap::new(),
            removed_components: BTreeSet::new(),
        }
    }
}

/// One versioned component payload, decoded only when its schema is available.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneComponent {
    Known {
        version: ComponentSchemaVersion,
        value: ComponentValue,
    },
    Unknown(UnknownComponent),
}

impl SceneComponent {
    #[must_use]
    pub const fn known(version: ComponentSchemaVersion, value: ComponentValue) -> Self {
        Self::Known { version, value }
    }

    #[must_use]
    pub const fn version(&self) -> ComponentSchemaVersion {
        match self {
            Self::Known { version, .. } => *version,
            Self::Unknown(unknown) => unknown.version,
        }
    }

    #[must_use]
    pub const fn value(&self) -> Option<&ComponentValue> {
        match self {
            Self::Known { value, .. } => Some(value),
            Self::Unknown(_) => None,
        }
    }

    #[must_use]
    pub const fn value_mut(&mut self) -> Option<&mut ComponentValue> {
        match self {
            Self::Known { value, .. } => Some(value),
            Self::Unknown(_) => None,
        }
    }

    /// Decodes a known record already selected by the schema's component ID.
    pub fn typed<T: Schema>(&self) -> Result<T, SceneComponentError> {
        let schema = provider_schema::<T>()?;
        let stored = self.version();
        if stored != schema.version() {
            return Err(SceneComponentError::VersionMismatch {
                component: schema.id().clone(),
                stored,
                current: schema.version(),
            });
        }
        let value = self.value().ok_or_else(|| SceneComponentError::Opaque {
            component: schema.id().clone(),
        })?;
        nara_reflect::validate_component_value(&schema, value)?;
        Ok(T::decode_component(value.into())?)
    }
}

/// A component whose payload cannot be bound to the available schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownComponent {
    pub type_id: ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub raw: RawRon,
}

/// One validated RON value, retaining token spelling and comments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawRon(Box<RawValue>);

impl RawRon {
    pub fn from_ron(source: &str) -> Result<Self, ron::error::SpannedError> {
        RawValue::from_ron(source).map(|raw| Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_ron(&self) -> &str {
        self.0.get_ron()
    }
}

fn provider_schema<T: Schema>() -> Result<ComponentSchema, SceneComponentError> {
    let id = ComponentTypeId::try_new(T::COMPONENT_ID)?;
    let schema = T::type_schema()?;
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
    #[error("unsupported scene document version {0}; expected version 1")]
    UnsupportedVersion(u32),
    #[error("scene documents use the standard RON syntax without extension directives")]
    Extensions,
    #[error("could not load built-in scene schemas: {0}")]
    BuiltinSchemas(#[source] ComponentCodecError),
    #[error("duplicate project schema for {component}")]
    DuplicateSchema { component: ComponentTypeId },
    #[error("invalid asset alias {alias:?}")]
    InvalidAssetAlias { alias: String },
    #[error("asset alias {alias:?} has an empty path hint")]
    EmptyAssetHint { alias: String },
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
    #[error("entity {entity} supplies a version for absent component {component}")]
    OrphanComponentVersion {
        entity: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("entity {entity} has opaque payload for {payload} under component key {component}")]
    UnknownComponentIdMismatch {
        entity: EntityUuid,
        component: ComponentTypeId,
        payload: ComponentTypeId,
    },
    #[error(
        "entity {entity} cannot save known {component} without schema version {}",
        .version.get()
    )]
    MissingComponentSchema {
        entity: EntityUuid,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
    },
    #[error("entity {entity} has an invalid {component} value: {source}")]
    InvalidComponentValue {
        entity: EntityUuid,
        component: ComponentTypeId,
        #[source]
        source: CompactValueError,
    },
    #[error("entity {entity} has prefab edits without a prefab reference")]
    OrphanPrefabEdits { entity: EntityUuid },
    #[error("entity {entity} removes component {component} more than once")]
    DuplicateRemovedComponent {
        entity: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("entity {entity} has an invalid prefab reference: {source}")]
    InvalidPrefabReference {
        entity: EntityUuid,
        #[source]
        source: CompactValueError,
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
    #[error("component '{component}' is opaque; refresh the scene's schema before reading it")]
    Opaque { component: ComponentTypeId },
    #[error(transparent)]
    Codec(#[from] ComponentCodecError),
}
