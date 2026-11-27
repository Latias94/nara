//! Reads a flattened NSCN scene and inserts its typed components into Bevy ECS.
//!
//! Version 1 uses little-endian integers. Its 32-byte header contains `NSCN`, the `u16` version
//! and flags, the `u32` entity and column counts, and a 16-byte schema-set digest. Flags are zero.
//! A `u32` dependency count precedes the sorted 16-byte asset IDs. Entities are sorted by UUID
//! and carry their 16-byte UUID and an `i32` parent index (`-1` for a root).
//! Columns in ascending type-hash order carry a `u64` type hash, 32-byte schema hash, `u32` row
//! count, and `u32` byte length. Each row contains a `u32` entity index, `u32` payload length, and
//! a typed Postcard payload, with rows in ascending entity-index order. Entity names use the
//! built-in `nara.Name` column. No other source-document metadata is stored.
//!
//! The type hash uses the first eight bytes of BLAKE3 over `nara.nscn.component-id.v1\0` followed
//! by the UTF-8 component ID. The schema-set digest uses the first 16 bytes of BLAKE3 over
//! `nara.nscn.schema-set.v1\0` followed by each column's type and schema hashes in wire order.

mod registry;
pub(crate) mod wire;

use std::collections::BTreeMap;

use bevy_ecs::{entity::Entity, hierarchy::ChildOf, world::World};
use nara_core::{AssetId, EntityUuid};
use nara_reflect::{ComponentCodecError, ComponentRegistryError, ComponentTypeId};
use thiserror::Error;

pub use registry::CookedRegistry;

use wire::{Column, EntityRow, Reader};

/// A structurally validated NSCN scene borrowing its component payloads.
#[derive(Debug)]
pub struct CookedScene<'a> {
    assets: Vec<AssetId>,
    entities: Vec<EntityRow>,
    columns: Vec<Column<'a>>,
}

impl<'a> CookedScene<'a> {
    /// Validates framing, identity tables, hierarchy, column ordering, and the schema-set digest.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, CookedSceneError> {
        if bytes.len() < wire::HEADER_LEN {
            return Err(CookedSceneError::Truncated);
        }
        let mut reader = Reader::new(bytes);
        if &reader.read_array()? != wire::MAGIC {
            return Err(CookedSceneError::Format("invalid NSCN signature"));
        }
        let version = reader.read_u16()?;
        if version != wire::VERSION {
            return Err(CookedSceneError::UnsupportedVersion(version));
        }
        let flags = reader.read_u16()?;
        if flags != 0 {
            return Err(CookedSceneError::UnsupportedFlags(flags));
        }
        let entity_count = reader.read_u32()? as usize;
        let column_count = reader.read_u32()? as usize;
        let digest: [u8; 16] = reader.read_array()?;
        let assets = reader.assets()?;
        let entities = reader.entities(entity_count)?;
        let columns = reader.columns(column_count, entity_count)?;
        reader.finish()?;
        if digest
            != wire::schema_digest(
                columns
                    .iter()
                    .map(|column| (column.type_hash, &column.schema_hash)),
            )
        {
            return Err(CookedSceneError::SchemaDigestMismatch);
        }
        Ok(Self {
            assets,
            entities,
            columns,
        })
    }

    #[must_use]
    pub fn asset_dependencies(&self) -> &[AssetId] {
        &self.assets
    }

    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Decodes all component payloads and validates their references before mutating `world`.
    /// After validation, normal Bevy insertion runs registered requirements, hooks, and observers.
    /// The returned map links each stored UUID to its newly spawned entity.
    pub fn instantiate(
        &self,
        world: &mut World,
        registry: &CookedRegistry,
    ) -> Result<BTreeMap<EntityUuid, Entity>, CookedSceneError> {
        let ready = registry.prepare(self)?;
        let spawned: Vec<_> = world
            .spawn_batch(std::iter::repeat_n((), self.entities.len()))
            .collect();
        world.insert_batch(self.entities.iter().enumerate().filter_map(|(index, row)| {
            row.parent
                .map(|parent| (spawned[index], ChildOf(spawned[parent])))
        }));
        for column in ready {
            column.insert(world, &spawned);
        }
        Ok(self
            .entities
            .iter()
            .map(|row| row.id)
            .zip(spawned)
            .collect())
    }

    fn contains_entity(&self, id: EntityUuid) -> bool {
        self.entities
            .binary_search_by_key(&id, |row| row.id)
            .is_ok()
    }
}

#[derive(Debug, Error)]
pub enum CookedSceneError {
    #[error("truncated NSCN scene")]
    Truncated,
    #[error("unsupported NSCN version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported NSCN flags {0:#06x}")]
    UnsupportedFlags(u16),
    #[error("invalid NSCN framing: {0}")]
    Format(&'static str),
    #[error("NSCN entity {entity} has invalid parent index {parent}")]
    InvalidParent { entity: EntityUuid, parent: i32 },
    #[error("NSCN hierarchy cycles through entity {0}")]
    HierarchyCycle(EntityUuid),
    #[error("NSCN schema-set digest does not match its columns")]
    SchemaDigestMismatch,
    #[error("no native NSCN schema is registered for component hash {type_hash:016x}")]
    MissingSchema { type_hash: u64 },
    #[error("NSCN schema hash differs from the native schema for '{component}'")]
    SchemaMismatch { component: &'static str },
    #[error(
        "NSCN type hash {type_hash:016x} is already registered for '{registered}', cannot register '{candidate}'"
    )]
    DuplicateType {
        type_hash: u64,
        registered: &'static str,
        candidate: &'static str,
    },
    #[error("native Schema declares '{actual}', expected '{expected}'")]
    SchemaIdMismatch {
        expected: &'static str,
        actual: ComponentTypeId,
    },
    #[error("could not build native NSCN schema for '{component}': {source}")]
    Schema {
        component: &'static str,
        #[source]
        source: ComponentCodecError,
    },
    #[error(transparent)]
    InvalidSchema(#[from] ComponentRegistryError),
    #[error("could not decode NSCN component '{component}' on {entity}: {source}")]
    Component {
        component: &'static str,
        entity: EntityUuid,
        #[source]
        source: ComponentCodecError,
    },
    #[error("NSCN component '{component}' on {entity} refers to unlisted asset {asset}")]
    MissingAssetReference {
        component: &'static str,
        entity: EntityUuid,
        asset: AssetId,
    },
    #[error("NSCN component '{component}' on {entity} refers to missing entity {target}")]
    MissingEntityReference {
        component: &'static str,
        entity: EntityUuid,
        target: EntityUuid,
    },
}
