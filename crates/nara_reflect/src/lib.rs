//! Reflection and schema metadata boundary for nara data-facing components.

use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
};

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;
use bevy_reflect::{GetTypeRegistration, TypeRegistry};
use nara_ecs::{Component, Entity, Resource, World};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentTypeId(String);

impl ComponentTypeId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchemaVersion(pub u32);

impl Default for ComponentSchemaVersion {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchema {
    pub id: ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub rust_type_path: String,
    pub serializable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentFloat(f64);

impl ComponentFloat {
    pub fn new(value: f64) -> Result<Self, ComponentValueError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(ComponentValueError::NonFiniteFloat)
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentFloat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentFloat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum ComponentValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(ComponentFloat),
    String(String),
    List(Vec<ComponentValue>),
    Map(BTreeMap<String, ComponentValue>),
}

impl ComponentValue {
    pub fn f64(value: f64) -> Result<Self, ComponentValueError> {
        Ok(Self::F64(ComponentFloat::new(value)?))
    }

    #[must_use]
    pub fn map(entries: impl IntoIterator<Item = (impl Into<String>, ComponentValue)>) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    #[must_use]
    pub fn get(&self, field: &str) -> Option<&ComponentValue> {
        match self {
            Self::Map(fields) => fields.get(field),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::I64(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(value) => Some(value.get()),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentValueError {
    NonFiniteFloat,
}

impl Display for ComponentValueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFloat => formatter.write_str("component float must be finite"),
        }
    }
}

impl Error for ComponentValueError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRegistryError {
    DuplicateComponentId(ComponentTypeId),
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
        }
    }
}

impl Error for ComponentRegistryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentCodecError {
    MissingField { field: String },
    InvalidField { field: String, expected: String },
    EntityMissing,
    Message(String),
}

impl ComponentCodecError {
    #[must_use]
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    #[must_use]
    pub fn invalid_field(field: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            expected: expected.into(),
        }
    }
}

impl Display for ComponentCodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField { field } => write!(formatter, "missing component field '{field}'"),
            Self::InvalidField { field, expected } => {
                write!(
                    formatter,
                    "invalid component field '{field}', expected {expected}"
                )
            }
            Self::EntityMissing => formatter.write_str("target entity does not exist"),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl Error for ComponentCodecError {}

impl From<ComponentValueError> for ComponentCodecError {
    fn from(error: ComponentValueError) -> Self {
        Self::Message(error.to_string())
    }
}

type ApplyComponentFn =
    dyn FnOnce(&mut World, Entity) -> Result<(), ComponentCodecError> + Send + 'static;

pub struct PreparedComponent {
    apply: Box<ApplyComponentFn>,
}

impl PreparedComponent {
    pub fn new(
        apply: impl FnOnce(&mut World, Entity) -> Result<(), ComponentCodecError> + Send + 'static,
    ) -> Self {
        Self {
            apply: Box::new(apply),
        }
    }

    pub fn insert<T>(component: T) -> Self
    where
        T: Component,
    {
        Self::new(move |world, entity| {
            let mut entity_mut = world
                .get_entity_mut(entity)
                .map_err(|_| ComponentCodecError::EntityMissing)?;
            entity_mut.insert(component);
            Ok(())
        })
    }

    pub fn apply(self, world: &mut World, entity: Entity) -> Result<(), ComponentCodecError> {
        (self.apply)(world, entity)
    }
}

pub trait ComponentCodec: Send + Sync {
    fn preflight(&self, value: &ComponentValue) -> Result<PreparedComponent, ComponentCodecError>;

    fn encode(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<Option<ComponentValue>, ComponentCodecError>;
}

struct FnComponentCodec<Preflight, Encode> {
    preflight: Preflight,
    encode: Encode,
}

impl<Preflight, Encode> ComponentCodec for FnComponentCodec<Preflight, Encode>
where
    Preflight: Fn(&ComponentValue) -> Result<PreparedComponent, ComponentCodecError>
        + Send
        + Sync
        + 'static,
    Encode: Fn(&World, Entity) -> Result<Option<ComponentValue>, ComponentCodecError>
        + Send
        + Sync
        + 'static,
{
    fn preflight(&self, value: &ComponentValue) -> Result<PreparedComponent, ComponentCodecError> {
        (self.preflight)(value)
    }

    fn encode(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        (self.encode)(world, entity)
    }
}

#[derive(Resource)]
pub struct ComponentRegistry {
    type_registry: TypeRegistry,
    schemas: BTreeMap<ComponentTypeId, ComponentSchema>,
    rust_type_ids: HashMap<TypeId, ComponentTypeId>,
    codecs: BTreeMap<ComponentTypeId, Box<dyn ComponentCodec>>,
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
        }
    }

    pub fn register_component<T>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component + Reflect + GetTypeRegistration,
    {
        self.register_component_schema::<T>(id, version, false)?;
        Ok(self)
    }

    pub fn register_serializable_component<T, Decode, Encode>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        decode: Decode,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component + Reflect + GetTypeRegistration,
        Decode: Fn(&ComponentValue) -> Result<T, ComponentCodecError> + Send + Sync + 'static,
        Encode: Fn(&T) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
    {
        self.register_component_schema::<T>(id.clone(), version, true)?;
        let codec = FnComponentCodec {
            preflight: move |value: &ComponentValue| {
                let component = decode(value)?;
                Ok(PreparedComponent::insert(component))
            },
            encode: move |world: &World, entity: Entity| {
                let Some(component) = world.get::<T>(entity) else {
                    return Ok(None);
                };
                encode(component).map(Some)
            },
        };
        self.codecs.insert(id, Box::new(codec));
        Ok(self)
    }

    pub fn register_component_codec<T, Preflight, Encode>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        preflight: Preflight,
        encode: Encode,
    ) -> Result<&mut Self, ComponentRegistryError>
    where
        T: Component + Reflect + GetTypeRegistration,
        Preflight: Fn(&ComponentValue) -> Result<PreparedComponent, ComponentCodecError>
            + Send
            + Sync
            + 'static,
        Encode: Fn(&World, Entity) -> Result<Option<ComponentValue>, ComponentCodecError>
            + Send
            + Sync
            + 'static,
    {
        self.register_component_schema::<T>(id.clone(), version, true)?;
        self.codecs
            .insert(id, Box::new(FnComponentCodec { preflight, encode }));
        Ok(self)
    }

    #[must_use]
    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.schemas.get(id)
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

    pub fn encode_component(
        &self,
        id: &ComponentTypeId,
        world: &World,
        entity: Entity,
    ) -> Option<Result<Option<ComponentValue>, ComponentCodecError>> {
        self.codec(id).map(|codec| codec.encode(world, entity))
    }

    fn register_component_schema<T>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
        serializable: bool,
    ) -> Result<(), ComponentRegistryError>
    where
        T: Component + Reflect + GetTypeRegistration,
    {
        if self.schemas.contains_key(&id) {
            return Err(ComponentRegistryError::DuplicateComponentId(id));
        }

        self.type_registry.register::<T>();
        let schema = ComponentSchema {
            id: id.clone(),
            version,
            rust_type_path: std::any::type_name::<T>().to_string(),
            serializable,
        };
        self.rust_type_ids.insert(TypeId::of::<T>(), id.clone());
        self.schemas.insert(id, schema);
        Ok(())
    }
}

pub mod prelude {
    pub use crate::{
        ComponentCodec, ComponentCodecError, ComponentFloat, ComponentRegistry,
        ComponentRegistryError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueError, PreparedComponent,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::Component;

    #[derive(Clone, Component, Reflect)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[test]
    fn registers_component_schema_by_stable_id_and_rust_type() {
        let mut registry = ComponentRegistry::new();
        let id = ComponentTypeId::new("nara.test.Position");

        registry
            .register_component::<Position>(id.clone(), ComponentSchemaVersion(1))
            .unwrap();

        assert_eq!(
            registry.schema(&id).unwrap().version,
            ComponentSchemaVersion(1)
        );
        assert!(!registry.schema(&id).unwrap().serializable);
        assert_eq!(
            registry.schema_for_type::<Position>().unwrap().id.as_str(),
            "nara.test.Position"
        );
    }

    #[test]
    fn rejects_duplicate_component_ids() {
        let mut registry = ComponentRegistry::new();
        let id = ComponentTypeId::new("nara.test.Position");

        registry
            .register_component::<Position>(id.clone(), ComponentSchemaVersion(1))
            .unwrap();
        let result = registry.register_component::<Position>(id.clone(), ComponentSchemaVersion(1));
        assert!(matches!(
            result,
            Err(ComponentRegistryError::DuplicateComponentId(duplicate)) if duplicate == id
        ));
    }

    #[test]
    fn rejects_non_finite_component_floats() {
        assert_eq!(
            ComponentValue::f64(f64::NAN),
            Err(ComponentValueError::NonFiniteFloat)
        );
    }

    #[test]
    fn preflights_applies_and_encodes_serializable_component() {
        let mut registry = ComponentRegistry::new();
        let id = ComponentTypeId::new("nara.test.Position");
        registry
            .register_serializable_component::<Position, _, _>(
                id.clone(),
                ComponentSchemaVersion(1),
                |value| {
                    let x = value
                        .get("x")
                        .and_then(ComponentValue::as_f64)
                        .ok_or_else(|| ComponentCodecError::invalid_field("x", "finite float"))?
                        as f32;
                    let y = value
                        .get("y")
                        .and_then(ComponentValue::as_f64)
                        .ok_or_else(|| ComponentCodecError::invalid_field("y", "finite float"))?
                        as f32;
                    Ok(Position { x, y })
                },
                |position| {
                    Ok(ComponentValue::map([
                        ("x", ComponentValue::f64(f64::from(position.x))?),
                        ("y", ComponentValue::f64(f64::from(position.y))?),
                    ]))
                },
            )
            .unwrap();

        assert!(registry.schema(&id).unwrap().serializable);

        let value = ComponentValue::map([
            ("x", ComponentValue::f64(2.0).unwrap()),
            ("y", ComponentValue::f64(3.0).unwrap()),
        ]);
        let prepared = registry.preflight_component(&id, &value).unwrap().unwrap();
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        prepared.apply(&mut world, entity).unwrap();

        assert_eq!(world.get::<Position>(entity).unwrap().x, 2.0);
        assert_eq!(
            registry
                .encode_component(&id, &world, entity)
                .unwrap()
                .unwrap(),
            Some(value)
        );
    }
}
