//! Compiles authored scenes to the NSCN runtime format.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt::Display,
};

use nara_core::{AssetId, EntityUuid};
use nara_reflect::{
    __macro_support::write_postcard_field, ComponentCodecError, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    PostcardReference, Schema,
};
use thiserror::Error;

use crate::{
    Name,
    authoring::{
        SceneDocument, SceneDocumentError, builtin_component_schemas,
        prefab::{PrefabError, expand_prefabs},
    },
    cooked::wire,
};

/// A cooked scene and its stable dependency inventory.
#[derive(Debug)]
pub struct CookedSceneOutput {
    pub bytes: Vec<u8>,
    /// Runtime asset references used by the flattened components, in UUID order.
    pub asset_dependencies: Vec<AssetId>,
    /// Source prefabs read during expansion, in UUID order.
    pub prefab_dependencies: Vec<AssetId>,
}

/// Expands prefab instances and sparse overrides before compiling the scene.
pub fn cook_scene<E: Display>(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
    mut load_prefab: impl FnMut(AssetId) -> Result<SceneDocument, E>,
) -> Result<CookedSceneOutput, CookSceneError> {
    let schemas = with_builtins(schemas)?;
    let mut prefabs = BTreeSet::new();
    let expanded = expand_prefabs(source, &schemas, |asset| {
        let document = load_prefab(asset)?;
        prefabs.insert(asset);
        Ok::<_, E>(document)
    })?;
    let mut output = encode_flattened(&expanded.document, &schemas)?;
    output.prefab_dependencies = prefabs.into_iter().collect();
    Ok(output)
}

/// Compiles a document whose prefab instances have already been expanded.
pub fn cook_flattened_scene(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
) -> Result<CookedSceneOutput, CookSceneError> {
    encode_flattened(source, &with_builtins(schemas)?)
}

fn with_builtins(
    schemas: &ComponentSchemaCatalog,
) -> Result<ComponentSchemaCatalog, CookSceneError> {
    let builtins = builtin_component_schemas()?;
    let mut components: Vec<_> = schemas
        .components()
        .iter()
        .filter(|schema| builtins.iter().all(|builtin| builtin.id() != schema.id()))
        .cloned()
        .collect();
    components.extend(builtins);
    Ok(ComponentSchemaCatalog::from_components(components)?)
}

fn encode_flattened(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
) -> Result<CookedSceneOutput, CookSceneError> {
    source.validate()?;
    let entity_count =
        i32::try_from(source.entities.len()).map_err(|_| CookSceneError::Length("entity table"))?;
    let mut entities: Vec<_> = source.entities.iter().collect();
    entities.sort_unstable_by_key(|row| row.id);

    let names = Name::type_schema()?;
    let mut name_column = EncodedColumn::new(&names);
    let mut columns = BTreeMap::<&ComponentTypeId, EncodedColumn<'_>>::new();
    let mut assets = BTreeSet::new();
    for (index, entity) in (0..entity_count as u32).zip(&entities) {
        if entity.prefab.is_some() {
            return Err(CookSceneError::UnexpandedPrefab(entity.id));
        }
        let mut name = Vec::new();
        write_postcard_field(&mut name, &entity.name, "value")?;
        name_column.push(index, &name)?;
        for (id, component) in &entity.components {
            if id == names.id() {
                return Err(CookSceneError::ReservedComponent(id.clone()));
            }
            let value = component.value().ok_or_else(|| CookSceneError::Opaque {
                entity: entity.id,
                component: id.clone(),
                version: component.version(),
            })?;
            let column = match columns.entry(id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let schema = resolve_schema(schemas, id).ok_or_else(|| {
                        CookSceneError::MissingSchema {
                            entity: entity.id,
                            component: id.clone(),
                        }
                    })?;
                    if !schema.supports_postcard() {
                        return Err(CookSceneError::Component {
                            entity: entity.id,
                            component: id.clone(),
                            source: ComponentCodecError::PostcardUnavailable,
                        });
                    }
                    entry.insert(EncodedColumn::new(schema))
                }
            };
            if component.version() != column.schema.version() {
                return Err(CookSceneError::VersionMismatch {
                    entity: entity.id,
                    component: id.clone(),
                    stored: component.version(),
                    expected: column.schema.version(),
                });
            }

            let mut missing = None;
            let payload = column
                .schema
                .encode_postcard_value_with_references(value, |reference| match reference {
                    PostcardReference::Asset(asset) => {
                        assets.insert(asset);
                    }
                    PostcardReference::Entity(target) => {
                        if missing.is_none()
                            && entities
                                .binary_search_by_key(&target, |row| row.id)
                                .is_err()
                        {
                            missing = Some(target);
                        }
                    }
                })
                .map_err(|source| CookSceneError::Component {
                    entity: entity.id,
                    component: id.clone(),
                    source,
                })?;
            if let Some(target) = missing {
                return Err(CookSceneError::MissingEntityReference {
                    entity: entity.id,
                    component: id.clone(),
                    target,
                });
            }
            column.push(index, &payload)?;
        }
    }

    let mut columns: Vec<_> = columns.into_values().collect();
    if entity_count != 0 {
        columns.push(name_column);
    }
    columns.sort_unstable_by_key(|column| column.type_hash);
    for pair in columns.windows(2) {
        if pair[0].type_hash == pair[1].type_hash {
            return Err(CookSceneError::TypeHashCollision {
                first: pair[0].schema.id().clone(),
                second: pair[1].schema.id().clone(),
            });
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(wire::MAGIC);
    bytes.extend_from_slice(&wire::VERSION.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&(entity_count as u32).to_le_bytes());
    bytes.extend_from_slice(&length(columns.len(), "component columns")?.to_le_bytes());
    bytes.extend_from_slice(&wire::schema_digest(
        columns
            .iter()
            .map(|column| (column.type_hash, &column.schema_hash)),
    ));

    let assets: Vec<_> = assets.into_iter().collect();
    bytes.extend_from_slice(&length(assets.len(), "asset dependencies")?.to_le_bytes());
    for id in &assets {
        bytes.extend_from_slice(id.as_uuid().as_bytes());
    }
    for entity in &entities {
        let parent = match entity.parent {
            Some(parent) => entities
                .binary_search_by_key(&parent, |row| row.id)
                .map_err(|_| SceneDocumentError::MissingParent {
                    entity: entity.id,
                    parent,
                })? as i32,
            None => wire::ROOT_PARENT,
        };
        bytes.extend_from_slice(entity.id.as_uuid().as_bytes());
        bytes.extend_from_slice(&parent.to_le_bytes());
    }
    for column in columns {
        bytes.extend_from_slice(&column.type_hash.to_le_bytes());
        bytes.extend_from_slice(&column.schema_hash);
        bytes.extend_from_slice(&column.rows.to_le_bytes());
        bytes.extend_from_slice(&length(column.bytes.len(), "component column")?.to_le_bytes());
        bytes.extend_from_slice(&column.bytes);
    }
    Ok(CookedSceneOutput {
        bytes,
        asset_dependencies: assets,
        prefab_dependencies: Vec::new(),
    })
}

fn resolve_schema<'a>(
    catalog: &'a ComponentSchemaCatalog,
    id: &ComponentTypeId,
) -> Option<&'a ComponentSchema> {
    let schemas = catalog.components();
    let index = schemas
        .binary_search_by(|schema| schema.id().cmp(id))
        .ok()?;
    schemas.get(index)
}

fn length(value: usize, section: &'static str) -> Result<u32, CookSceneError> {
    u32::try_from(value).map_err(|_| CookSceneError::Length(section))
}

struct EncodedColumn<'a> {
    schema: &'a ComponentSchema,
    type_hash: u64,
    schema_hash: [u8; 32],
    rows: u32,
    bytes: Vec<u8>,
}

impl<'a> EncodedColumn<'a> {
    fn new(schema: &'a ComponentSchema) -> Self {
        Self {
            schema,
            type_hash: wire::type_hash(schema.id().as_str()),
            schema_hash: *schema.postcard_schema_hash().as_bytes(),
            rows: 0,
            bytes: Vec::new(),
        }
    }

    fn push(&mut self, index: u32, payload: &[u8]) -> Result<(), CookSceneError> {
        self.bytes.extend_from_slice(&index.to_le_bytes());
        self.bytes
            .extend_from_slice(&length(payload.len(), "component payload")?.to_le_bytes());
        self.bytes.extend_from_slice(payload);
        self.rows += 1;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CookSceneError {
    #[error(transparent)]
    Document(#[from] SceneDocumentError),
    #[error(transparent)]
    Prefab(#[from] PrefabError),
    #[error(transparent)]
    Schema(#[from] ComponentCodecError),
    #[error(transparent)]
    InvalidSchema(#[from] ComponentRegistryError),
    #[error("NSCN {0} exceeds the format length limit")]
    Length(&'static str),
    #[error("NSCN input still contains a prefab instance at entity {0}")]
    UnexpandedPrefab(EntityUuid),
    #[error("NSCN reserves '{0}' for the entity name column")]
    ReservedComponent(ComponentTypeId),
    #[error("NSCN component type hash collides for '{first}' and '{second}'")]
    TypeHashCollision {
        first: ComponentTypeId,
        second: ComponentTypeId,
    },
    #[error("no Schema for component '{component}' on {entity}")]
    MissingSchema {
        entity: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("opaque component '{component}' v{version:?} on {entity} cannot be cooked")]
    Opaque {
        entity: EntityUuid,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
    },
    #[error(
        "component '{component}' on {entity} has incompatible version {stored:?}, expected {expected:?}"
    )]
    VersionMismatch {
        entity: EntityUuid,
        component: ComponentTypeId,
        stored: ComponentSchemaVersion,
        expected: ComponentSchemaVersion,
    },
    #[error("could not encode NSCN component '{component}' on {entity}: {source}")]
    Component {
        entity: EntityUuid,
        component: ComponentTypeId,
        #[source]
        source: ComponentCodecError,
    },
    #[error("component '{component}' on {entity} refers to missing entity {target}")]
    MissingEntityReference {
        entity: EntityUuid,
        component: ComponentTypeId,
        target: EntityUuid,
    },
}
