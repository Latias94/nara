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

use bevy_ecs::{
    bundle::{Bundle, NoBundleEffect},
    component::Component,
    entity::Entity,
    hierarchy::ChildOf,
    world::World,
};
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
        Ok(self.prepare(registry)?.spawn(world, || ()))
    }

    /// Decodes native columns and validates stored references for inspection before insertion.
    pub fn prepare(
        &self,
        registry: &CookedRegistry,
    ) -> Result<PreparedScene<'_>, CookedSceneError> {
        Ok(PreparedScene {
            columns: registry.prepare(self)?,
            entities: &self.entities,
        })
    }

    fn contains_entity(&self, id: EntityUuid) -> bool {
        self.entities
            .binary_search_by_key(&id, |row| row.id)
            .is_ok()
    }
}

/// The native component values and hierarchy decoded from a cooked scene.
pub struct PreparedScene<'a> {
    columns: Vec<Box<dyn registry::ReadyColumn>>,
    entities: &'a [EntityRow],
}

impl PreparedScene<'_> {
    /// Visits scene rows in UUID order, matching the order of the instantiated entity map.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = PreparedSceneRow<'_>> {
        (0..self.entities.len()).map(move |index| PreparedSceneRow { scene: self, index })
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn sprite_sources(
        &self,
    ) -> impl Iterator<Item = (EntityUuid, &crate::components::SpriteSource)> {
        self.columns
            .iter()
            .filter_map(|column| column.typed_rows::<crate::components::SpriteSource>())
            .flatten()
            .map(|(index, sprite)| (self.entities[*index].id, sprite))
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn tilemap_sources(
        &self,
    ) -> impl Iterator<Item = (EntityUuid, &crate::components::TilemapSource)> {
        self.columns
            .iter()
            .filter_map(|column| column.typed_rows::<crate::components::TilemapSource>())
            .flatten()
            .map(|(index, map)| (self.entities[*index].id, map))
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn audio_clips(
        &self,
    ) -> impl Iterator<Item = Result<(EntityUuid, AssetId), CookedSceneError>> {
        self.columns
            .iter()
            .filter_map(|column| column.audio_clips())
            .flatten()
            .map(|(index, clip)| {
                let entity = self.entities[index].id;
                clip.map(|id| (entity, id))
                    .map_err(|source| CookedSceneError::Component {
                        component: registry::AUDIO_CLIP_SOURCE_ID,
                        entity,
                        source,
                    })
            })
    }

    pub(crate) fn spawn<B: Bundle<Effect: NoBundleEffect>>(
        self,
        world: &mut World,
        bundle: impl FnMut() -> B,
    ) -> BTreeMap<EntityUuid, Entity> {
        let spawned: Vec<_> = world
            .spawn_batch(std::iter::repeat_with(bundle).take(self.entities.len()))
            .collect();
        world.insert_batch(self.entities.iter().enumerate().filter_map(|(index, row)| {
            row.parent
                .map(|parent| (spawned[index], ChildOf(spawned[parent])))
        }));
        for column in self.columns {
            column.insert(world, &spawned);
        }
        self.entities
            .iter()
            .map(|row| row.id)
            .zip(spawned)
            .collect()
    }
}

/// A read-only view of one decoded entity before it is inserted into a world.
pub struct PreparedSceneRow<'a> {
    scene: &'a PreparedScene<'a>,
    index: usize,
}

impl<'a> PreparedSceneRow<'a> {
    #[must_use]
    pub fn id(&self) -> EntityUuid {
        self.scene.entities[self.index].id
    }

    #[must_use]
    pub fn parent(&self) -> Option<EntityUuid> {
        self.scene.entities[self.index]
            .parent
            .map(|index| self.scene.entities[index].id)
    }

    /// Returns a stored component value. Bevy inserts required components during spawning.
    #[must_use]
    pub fn get<T: Component>(&self) -> Option<&'a T> {
        let rows = self
            .scene
            .columns
            .iter()
            .find_map(|column| column.typed_rows::<T>())?;
        let index = rows
            .binary_search_by_key(&self.index, |(index, _)| *index)
            .ok()?;
        Some(&rows[index].1)
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
