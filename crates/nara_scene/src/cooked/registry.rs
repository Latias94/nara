use std::collections::BTreeMap;

use bevy_ecs::{component::Component, entity::Entity, world::World};
use nara_reflect::{
    __macro_support::{finish_postcard, read_postcard_field, write_postcard_field},
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueRef, PostcardReference, Schema, TypeSchema,
};

use crate::{
    Name,
    components::{ColliderSource, SpriteSource, Transform2d, Visibility},
};

use super::{
    CookedScene, CookedSceneError,
    wire::{self, Column},
};

/// Native component decoders for a particular game. Extra registrations may be unused by a scene.
#[derive(Default)]
pub struct CookedRegistry {
    entries: BTreeMap<u64, Entry>,
}

impl CookedRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one native schema with a declared Postcard codec under its stable component ID.
    pub fn register<T: Component + Schema>(&mut self) -> Result<&mut Self, CookedSceneError> {
        let component = T::COMPONENT_ID;
        let type_hash = wire::type_hash(component);
        self.check_available(type_hash, component)?;
        let schema =
            T::type_schema().map_err(|source| CookedSceneError::Schema { component, source })?;
        if schema.id().as_str() != component {
            return Err(CookedSceneError::SchemaIdMismatch {
                expected: component,
                actual: schema.id,
            });
        }
        if !schema.supports_postcard() {
            return Err(CookedSceneError::Schema {
                component,
                source: ComponentCodecError::PostcardUnavailable,
            });
        }
        let catalog = ComponentSchemaCatalog::from_components([schema])?;
        let schema = &catalog.components()[0];
        self.entries.insert(
            type_hash,
            Entry {
                component,
                schema_hash: *schema.postcard_schema_hash().as_bytes(),
                has_references: schema.fields().iter().any(|field| {
                    field.has_capability(ComponentCapability::AssetRef)
                        || field.has_capability(ComponentCapability::EntityRef)
                }),
                decode: decode_column::<T>,
            },
        );
        Ok(self)
    }

    /// Registers `Name`, `Transform2d`, `Visibility`, `SpriteSource`, and `ColliderSource`.
    pub fn register_builtins(&mut self) -> Result<&mut Self, CookedSceneError> {
        let mut builtins = Self::new();
        builtins
            .register::<Name>()?
            .register::<Transform2d>()?
            .register::<Visibility>()?
            .register::<SpriteSource>()?
            .register::<ColliderSource>()?;
        for (&type_hash, entry) in &builtins.entries {
            self.check_available(type_hash, entry.component)?;
        }
        self.entries.extend(builtins.entries);
        Ok(self)
    }

    fn check_available(
        &self,
        type_hash: u64,
        candidate: &'static str,
    ) -> Result<(), CookedSceneError> {
        if let Some(entry) = self.entries.get(&type_hash) {
            return Err(CookedSceneError::DuplicateType {
                type_hash,
                registered: entry.component,
                candidate,
            });
        }
        Ok(())
    }

    pub(super) fn prepare(
        &self,
        scene: &CookedScene<'_>,
    ) -> Result<Vec<Box<dyn ReadyColumn>>, CookedSceneError> {
        let entries = scene
            .columns
            .iter()
            .map(|column| {
                let entry =
                    self.entries
                        .get(&column.type_hash)
                        .ok_or(CookedSceneError::MissingSchema {
                            type_hash: column.type_hash,
                        })?;
                if entry.schema_hash != column.schema_hash {
                    return Err(CookedSceneError::SchemaMismatch {
                        component: entry.component,
                    });
                }
                Ok(entry)
            })
            .collect::<Result<Vec<_>, _>>()?;

        scene
            .columns
            .iter()
            .zip(entries)
            .map(|(column, entry)| (entry.decode)(scene, column, entry))
            .collect()
    }
}

type DecodeColumn =
    fn(&CookedScene<'_>, &Column<'_>, &Entry) -> Result<Box<dyn ReadyColumn>, CookedSceneError>;

struct Entry {
    component: &'static str,
    schema_hash: [u8; 32],
    has_references: bool,
    decode: DecodeColumn,
}

pub(super) trait ReadyColumn {
    fn insert(self: Box<Self>, world: &mut World, spawned: &[Entity]);
}

struct TypedColumn<T>(Vec<(usize, T)>);

impl<T: Component> ReadyColumn for TypedColumn<T> {
    fn insert(self: Box<Self>, world: &mut World, spawned: &[Entity]) {
        world.insert_batch(
            self.0
                .into_iter()
                .map(|(index, component)| (spawned[index], component)),
        );
    }
}

fn decode_column<T: Component + Schema>(
    scene: &CookedScene<'_>,
    column: &Column<'_>,
    entry: &Entry,
) -> Result<Box<dyn ReadyColumn>, CookedSceneError> {
    let mut values = Vec::with_capacity(column.row_count);
    column.read_rows(scene.entity_count(), |index, payload| {
        let entity = scene.entities[index].id;
        let invalid = |source| CookedSceneError::Component {
            component: entry.component,
            entity,
            source,
        };
        let component = T::decode_postcard(payload).map_err(invalid)?;
        if entry.has_references {
            let mut missing = None;
            component
                .visit_postcard_references(&mut |reference| {
                    if missing.is_some() {
                        return;
                    }
                    missing = match reference {
                        PostcardReference::Asset(asset)
                            if scene.assets.binary_search(&asset).is_err() =>
                        {
                            Some(CookedSceneError::MissingAssetReference {
                                component: entry.component,
                                entity,
                                asset,
                            })
                        }
                        PostcardReference::Entity(target) if !scene.contains_entity(target) => {
                            Some(CookedSceneError::MissingEntityReference {
                                component: entry.component,
                                entity,
                                target,
                            })
                        }
                        _ => None,
                    };
                })
                .map_err(invalid)?;
            if let Some(error) = missing {
                return Err(error);
            }
        }
        values.push((index, component));
        Ok(())
    })?;
    Ok(Box::new(TypedColumn(values)))
}

impl Schema for Name {
    const COMPONENT_ID: &'static str = "nara.Name";

    fn type_schema() -> Result<TypeSchema, ComponentCodecError> {
        Ok(TypeSchema::new(
            ComponentTypeId::new(Self::COMPONENT_ID),
            "Name",
            ComponentSchemaVersion::ONE,
        )
        .with_postcard_codec()
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::String,
        )]))
    }

    fn decode_component(value: ComponentValueRef<'_>) -> Result<Self, ComponentCodecError> {
        value
            .field("value")?
            .as_str()
            .map(Self::new)
            .ok_or_else(|| ComponentCodecError::invalid_field("value", "string"))
    }

    fn encode_component(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::map([(
            "value",
            ComponentValue::String(self.0.clone()),
        )]))
    }

    fn encode_postcard(&self) -> Result<Vec<u8>, ComponentCodecError> {
        let mut bytes = Vec::new();
        write_postcard_field(&mut bytes, &self.0, "value")?;
        Ok(bytes)
    }

    fn decode_postcard(mut bytes: &[u8]) -> Result<Self, ComponentCodecError> {
        let value = read_postcard_field(&mut bytes, "value")?;
        finish_postcard(bytes)?;
        Ok(Self(value))
    }
}
