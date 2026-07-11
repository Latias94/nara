//! Component registry, schema validation, codecs, and migrations.

use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
};

use bevy_reflect::{GetTypeRegistration, Reflect, TypeRegistry};
use nara_ecs::{Component, Entity, Resource, World};

use crate::{
    codec::{
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        FnComponentCodec, PreparedComponent,
    },
    migration::{ComponentMigration, ComponentMigrationError, MigratedComponentValue},
    path::ComponentFieldPath,
    schema::{
        ComponentCapability, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValueKind,
    },
    value::ComponentValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRegistryError {
    DuplicateComponentId(ComponentTypeId),
    DuplicateComponentRustType {
        rust_type_path: String,
        existing_component_id: ComponentTypeId,
        requested_component_id: ComponentTypeId,
    },
    UnknownComponentId(ComponentTypeId),
    MissingSceneComponentFields {
        component_id: ComponentTypeId,
    },
    DuplicateComponentFieldPath {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
    },
    InvalidComponentFieldDefault {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        expected: ComponentValueKind,
        actual: ComponentValueKind,
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
}

impl Display for ComponentRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateComponentId(id) => {
                write!(
                    formatter,
                    "component id '{}' is already registered",
                    id.as_str()
                )
            }
            Self::DuplicateComponentRustType {
                rust_type_path,
                existing_component_id,
                requested_component_id,
            } => {
                write!(
                    formatter,
                    "rust component type '{}' is already registered as '{}', not '{}'",
                    rust_type_path,
                    existing_component_id.as_str(),
                    requested_component_id.as_str()
                )
            }
            Self::UnknownComponentId(id) => {
                write!(
                    formatter,
                    "component id '{}' is not registered",
                    id.as_str()
                )
            }
            Self::MissingSceneComponentFields { component_id } => {
                write!(
                    formatter,
                    "scene component id '{}' requires explicit schema fields",
                    component_id.as_str()
                )
            }
            Self::DuplicateComponentFieldPath { component_id, path } => {
                write!(
                    formatter,
                    "component id '{}' registered duplicate schema field path '{}'",
                    component_id.as_str(),
                    path
                )
            }
            Self::InvalidComponentFieldDefault {
                component_id,
                path,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "component id '{}' field '{}' default has kind {:?}, expected {:?}",
                    component_id.as_str(),
                    path,
                    actual,
                    expected
                )
            }
            Self::DuplicateComponentMigration {
                component_id,
                from_version,
            } => {
                write!(
                    formatter,
                    "component id '{}' already has a migration from version {}",
                    component_id.as_str(),
                    from_version.0
                )
            }
            Self::InvalidComponentMigration {
                component_id,
                from_version,
                to_version,
            } => {
                write!(
                    formatter,
                    "component id '{}' has invalid migration version range {} -> {}",
                    component_id.as_str(),
                    from_version.0,
                    to_version.0
                )
            }
        }
    }
}

impl Error for ComponentRegistryError {}

#[derive(Resource)]
pub struct ComponentRegistry {
    type_registry: TypeRegistry,
    schemas: BTreeMap<ComponentTypeId, ComponentSchema>,
    rust_type_ids: HashMap<TypeId, ComponentTypeId>,
    codecs: BTreeMap<ComponentTypeId, Box<dyn ComponentCodec>>,
    migrations: BTreeMap<(ComponentTypeId, ComponentSchemaVersion), ComponentMigration>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            type_registry: TypeRegistry::default(),
            schemas: BTreeMap::new(),
            rust_type_ids: HashMap::new(),
            codecs: BTreeMap::new(),
            migrations: BTreeMap::new(),
        }
    }

    pub fn validate_component_registration<T>(
        &self,
        id: &ComponentTypeId,
    ) -> Result<(), ComponentRegistryError>
    where
        T: Component,
    {
        if self.schemas.contains_key(id) {
            return Err(ComponentRegistryError::DuplicateComponentId(id.clone()));
        }
        if let Some(existing_component_id) = self.rust_type_ids.get(&TypeId::of::<T>()) {
            return Err(ComponentRegistryError::DuplicateComponentRustType {
                rust_type_path: std::any::type_name::<T>().to_string(),
                existing_component_id: existing_component_id.clone(),
                requested_component_id: id.clone(),
            });
        }
        Ok(())
    }

    pub fn register_component<T>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component + Reflect + GetTypeRegistration,
    {
        self.register_component_schema::<T>(id, version, BTreeSet::new(), Vec::new())?;
        self.type_registry.register::<T>();
        Ok(self)
    }

    pub fn register_scene_component_with_fields<T, Decode, Encode>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        fields: impl IntoIterator<Item = ComponentFieldSchema>,
        decode: Decode,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Decode: Fn(&ComponentValue) -> Result<T, ComponentCodecError> + Send + Sync + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        let fields = scene_component_fields(&id, fields)?;
        self.register_component_schema::<T>(
            id.clone(),
            version,
            ComponentCapability::scene_authoring(),
            fields,
        )?;
        let codec = FnComponentCodec {
            preflight: move |value: &ComponentValue, _context: &mut ComponentDecodeContext<'_>| {
                let component = decode(value)?;
                Ok(PreparedComponent::insert(component))
            },
            encode: move |world: &World, entity: Entity, _context: &ComponentEncodeContext<'_>| {
                let Some(component) = world.get::<T>(entity) else {
                    return Ok(None);
                };
                encode(component).map(Some)
            },
        };
        self.codecs.insert(id, Box::new(codec));
        Ok(self)
    }

    pub fn register_component_codec_with_fields<T, Preflight, Encode>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        fields: impl IntoIterator<Item = ComponentFieldSchema>,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponent, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&World, Entity) -> Result<Option<ComponentValue>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        self.register_component_codec_with_context_and_fields::<T, _, _>(
            id,
            version,
            fields,
            move |value, _context| preflight(value),
            move |world, entity, _context| encode(world, entity),
        )
    }

    pub fn register_component_codec_with_context_and_fields<T, Preflight, Encode>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        fields: impl IntoIterator<Item = ComponentFieldSchema>,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component,
        Preflight: for<'a> Fn(
                &ComponentValue,
                &mut ComponentDecodeContext<'a>,
            ) -> Result<PreparedComponent, ComponentCodecError>
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
        let fields = scene_component_fields(&id, fields)?;
        self.register_component_schema::<T>(
            id.clone(),
            version,
            ComponentCapability::scene_authoring(),
            fields,
        )?;
        self.codecs
            .insert(id, Box::new(FnComponentCodec { preflight, encode }));
        Ok(self)
    }

    #[must_use]
    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.schemas.get(id)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.schemas.values()
    }

    #[must_use]
    pub fn schema_catalog(&self) -> ComponentSchemaCatalog {
        ComponentSchemaCatalog {
            components: self.schemas.values().cloned().collect(),
        }
    }

    pub fn register_component_fields(
        &mut self,
        id: &ComponentTypeId,
        fields: impl IntoIterator<Item = ComponentFieldSchema>,
    ) -> Result<&mut Self, ComponentRegistryError> {
        let fields = canonical_component_fields(id, fields)?;
        let Some(schema) = self.schemas.get_mut(id) else {
            return Err(ComponentRegistryError::UnknownComponentId(id.clone()));
        };
        schema.fields = fields;
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
        if !self.schemas.contains_key(id) {
            return Err(ComponentRegistryError::UnknownComponentId(id.clone()));
        }
        if to_version <= from_version {
            return Err(ComponentRegistryError::InvalidComponentMigration {
                component_id: id.clone(),
                from_version,
                to_version,
            });
        }

        let key = (id.clone(), from_version);
        if self.migrations.contains_key(&key) {
            return Err(ComponentRegistryError::DuplicateComponentMigration {
                component_id: id.clone(),
                from_version,
            });
        }

        self.migrations.insert(
            key,
            ComponentMigration {
                to_version,
                migrate: Box::new(migrate),
            },
        );
        Ok(self)
    }

    pub fn migrate_component_value(
        &self,
        id: &ComponentTypeId,
        version: ComponentSchemaVersion,
        value: &ComponentValue,
    ) -> Result<MigratedComponentValue, ComponentMigrationError> {
        let Some(schema) = self.schema(id) else {
            return Err(ComponentMigrationError::UnknownComponentId {
                component_id: id.clone(),
            });
        };
        if version == schema.version {
            return Ok(MigratedComponentValue {
                version,
                value: value.clone(),
            });
        }
        if version > schema.version {
            return Err(ComponentMigrationError::UnsupportedVersion {
                component_id: id.clone(),
                from_version: version,
                target_version: schema.version,
            });
        }

        let mut current_version = version;
        let mut current_value = value.clone();
        let mut seen_versions = BTreeSet::from([current_version]);
        while current_version != schema.version {
            let Some(migration) = self.migrations.get(&(id.clone(), current_version)) else {
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

    #[must_use]
    pub fn schema_for_type<T: 'static>(&self) -> Option<&ComponentSchema> {
        self.rust_type_ids
            .get(&TypeId::of::<T>())
            .and_then(|id| self.schemas.get(id))
    }

    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    pub fn type_registry_mut(&mut self) -> &mut TypeRegistry {
        &mut self.type_registry
    }

    #[must_use]
    pub fn codec(&self, id: &ComponentTypeId) -> Option<&dyn ComponentCodec> {
        self.codecs.get(id).map(Box::as_ref)
    }

    pub fn preflight_component(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
    ) -> Option<Result<PreparedComponent, ComponentCodecError>> {
        self.codec(id).map(|codec| codec.preflight(value))
    }

    pub fn preflight_component_with_context(
        &self,
        id: &ComponentTypeId,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Option<Result<PreparedComponent, ComponentCodecError>> {
        self.codec(id)
            .map(|codec| codec.preflight_with_context(value, context))
    }

    pub fn encode_component(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
    ) -> Option<Result<Option<ComponentValue>, ComponentCodecError>> {
        self.codec(id).map(|codec| codec.encode(world, entity))
    }

    pub fn encode_component_with_context(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Option<Result<Option<ComponentValue>, ComponentCodecError>> {
        self.codec(id)
            .map(|codec| codec.encode_with_context(world, entity, context))
    }

    fn register_component_schema<T>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        capabilities: BTreeSet<ComponentCapability>,
        fields: Vec<ComponentFieldSchema>,
    ) -> Result<(), ComponentRegistryError>
    where
        T: Component,
    {
        self.validate_component_registration::<T>(&id)?;
        let rust_type_id = TypeId::of::<T>();

        let schema = ComponentSchema {
            id: id.clone(),
            version,
            rust_type_path: std::any::type_name::<T>().to_string(),
            capabilities,
            fields,
        };
        self.rust_type_ids.insert(rust_type_id, id.clone());
        self.schemas.insert(id, schema);
        Ok(())
    }
}

fn scene_component_fields(
    component_id: &ComponentTypeId,
    fields: impl IntoIterator<Item = ComponentFieldSchema>,
) -> Result<Vec<ComponentFieldSchema>, ComponentRegistryError> {
    let mut fields = canonical_component_fields(component_id, fields)?;
    if fields.is_empty() {
        return Err(ComponentRegistryError::MissingSceneComponentFields {
            component_id: component_id.clone(),
        });
    }
    for field in &mut fields {
        if field.capabilities.is_empty() {
            field.capabilities = ComponentCapability::scene_field_for_kind(field.value_kind);
        } else {
            match field.value_kind {
                ComponentValueKind::AssetRef => {
                    field.capabilities.insert(ComponentCapability::AssetRef);
                }
                ComponentValueKind::EntityRef => {
                    field.capabilities.insert(ComponentCapability::EntityRef);
                }
                _ => {}
            }
        }
    }
    Ok(fields)
}

fn canonical_component_fields(
    component_id: &ComponentTypeId,
    fields: impl IntoIterator<Item = ComponentFieldSchema>,
) -> Result<Vec<ComponentFieldSchema>, ComponentRegistryError> {
    let mut fields = fields.into_iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.path.cmp(&right.path));
    for pair in fields.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(ComponentRegistryError::DuplicateComponentFieldPath {
                component_id: component_id.clone(),
                path: pair[0].path.clone(),
            });
        }
    }
    for field in &fields {
        validate_component_field_default(component_id, field)?;
    }
    Ok(fields)
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
        ComponentValueKind::AssetRef => is_asset_ref_value(value),
        expected => value.kind() == expected,
    }
}

fn is_asset_ref_value(value: &ComponentValue) -> bool {
    let ComponentValue::Map(fields) = value else {
        return false;
    };
    matches!(
        (fields.get("kind"), fields.get("value")),
        (Some(ComponentValue::String(kind)), Some(ComponentValue::String(_)))
            if kind == "path" || kind == "stable_id"
    )
}
