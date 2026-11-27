use nara_core::{AssetId, EntityUuid};
use uuid::Uuid;

use super::CookedSceneError;

pub(crate) const MAGIC: &[u8; 4] = b"NSCN";
pub(crate) const VERSION: u16 = 1;
pub(crate) const HEADER_LEN: usize = 32;
const UUID_LEN: usize = 16;
const ENTITY_ROW_LEN: usize = UUID_LEN + 4;
const COLUMN_HEADER_LEN: usize = 8 + 32 + 4 + 4;
const COLUMN_ROW_HEADER_LEN: usize = 4 + 4;
pub(crate) const ROOT_PARENT: i32 = -1;

#[derive(Debug)]
pub(crate) struct EntityRow {
    pub(crate) id: EntityUuid,
    pub(crate) parent: Option<usize>,
}

#[derive(Debug)]
pub(crate) struct Column<'a> {
    pub(crate) type_hash: u64,
    pub(crate) schema_hash: [u8; 32],
    pub(crate) row_count: usize,
    data: &'a [u8],
}

impl Column<'_> {
    pub(crate) fn read_rows(
        &self,
        entity_count: usize,
        mut read: impl FnMut(usize, &[u8]) -> Result<(), CookedSceneError>,
    ) -> Result<(), CookedSceneError> {
        let mut data = Reader::new(self.data);
        let mut previous = None;
        for _ in 0..self.row_count {
            let entity = data.read_u32()? as usize;
            let size = data.read_u32()? as usize;
            if entity >= entity_count || previous.is_some_and(|previous| previous >= entity) {
                return Err(CookedSceneError::Format(
                    "invalid column entity order or index",
                ));
            }
            read(entity, data.take(size)?)?;
            previous = Some(entity);
        }
        data.finish()
    }
}

pub(crate) fn type_hash(component_id: &str) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"nara.nscn.component-id.v1\0");
    hasher.update(component_id.as_bytes());
    let hash = hasher.finalize();
    u64::from_le_bytes(*hash.as_bytes().first_chunk().expect("hash width"))
}

pub(crate) fn schema_digest<'a>(
    columns: impl IntoIterator<Item = (u64, &'a [u8; 32])>,
) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"nara.nscn.schema-set.v1\0");
    for (type_hash, schema_hash) in columns {
        hasher.update(&type_hash.to_le_bytes());
        hasher.update(schema_hash);
    }
    let hash = hasher.finalize();
    *hash.as_bytes().first_chunk().expect("hash width")
}

pub(crate) struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub(crate) fn read_array<const WIDTH: usize>(
        &mut self,
    ) -> Result<[u8; WIDTH], CookedSceneError> {
        let (head, tail) = self
            .0
            .split_first_chunk::<WIDTH>()
            .ok_or(CookedSceneError::Truncated)?;
        self.0 = tail;
        Ok(*head)
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], CookedSceneError> {
        let (head, tail) = self
            .0
            .split_at_checked(len)
            .ok_or(CookedSceneError::Truncated)?;
        self.0 = tail;
        Ok(head)
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, CookedSceneError> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, CookedSceneError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    fn expect_minimum(&self, count: usize, item_len: usize) -> Result<(), CookedSceneError> {
        if count > self.0.len() / item_len {
            return Err(CookedSceneError::Truncated);
        }
        Ok(())
    }

    pub(crate) fn assets(&mut self) -> Result<Vec<AssetId>, CookedSceneError> {
        let count = self.read_u32()? as usize;
        self.expect_minimum(count, UUID_LEN)?;
        let mut assets = Vec::with_capacity(count);
        for _ in 0..count {
            let id = AssetId::try_from_uuid(Uuid::from_bytes(self.read_array()?))
                .map_err(|_| CookedSceneError::Format("nil asset UUID"))?;
            if assets.last().is_some_and(|previous| *previous >= id) {
                return Err(CookedSceneError::Format(
                    "asset IDs must be strictly ascending",
                ));
            }
            assets.push(id);
        }
        Ok(assets)
    }

    pub(crate) fn entities(&mut self, count: usize) -> Result<Vec<EntityRow>, CookedSceneError> {
        self.expect_minimum(count, ENTITY_ROW_LEN)?;
        let mut entities = Vec::<EntityRow>::with_capacity(count);
        for _ in 0..count {
            let id = EntityUuid::try_from_uuid(Uuid::from_bytes(self.read_array()?))
                .map_err(|_| CookedSceneError::Format("nil entity UUID"))?;
            if entities.last().is_some_and(|previous| previous.id >= id) {
                return Err(CookedSceneError::Format(
                    "entity IDs must be strictly ascending",
                ));
            }
            let parent = i32::from_le_bytes(self.read_array()?);
            let parent = match parent {
                ROOT_PARENT => None,
                parent if parent >= 0 && (parent as usize) < count => Some(parent as usize),
                _ => return Err(CookedSceneError::InvalidParent { entity: id, parent }),
            };
            entities.push(EntityRow { id, parent });
        }
        validate_hierarchy(&entities)?;
        Ok(entities)
    }

    pub(crate) fn columns(
        &mut self,
        count: usize,
        entity_count: usize,
    ) -> Result<Vec<Column<'a>>, CookedSceneError> {
        self.expect_minimum(count, COLUMN_HEADER_LEN)?;
        let mut columns = Vec::<Column<'a>>::with_capacity(count);
        for _ in 0..count {
            let type_hash = u64::from_le_bytes(self.read_array()?);
            if columns
                .last()
                .is_some_and(|previous| previous.type_hash >= type_hash)
            {
                return Err(CookedSceneError::Format(
                    "component columns must be strictly ascending",
                ));
            }
            let schema_hash = self.read_array()?;
            let row_count = self.read_u32()? as usize;
            let byte_len = self.read_u32()? as usize;
            if row_count == 0
                || row_count > entity_count
                || row_count > byte_len / COLUMN_ROW_HEADER_LEN
            {
                return Err(CookedSceneError::Format(
                    "invalid component column row count",
                ));
            }
            let column = Column {
                type_hash,
                schema_hash,
                row_count,
                data: self.take(byte_len)?,
            };
            column.read_rows(entity_count, |_, _| Ok(()))?;
            columns.push(column);
        }
        Ok(columns)
    }

    pub(crate) fn finish(self) -> Result<(), CookedSceneError> {
        if !self.0.is_empty() {
            return Err(CookedSceneError::Format("unexpected trailing bytes"));
        }
        Ok(())
    }
}

fn validate_hierarchy(entities: &[EntityRow]) -> Result<(), CookedSceneError> {
    let mut visited_by = vec![usize::MAX; entities.len()];
    for origin in 0..entities.len() {
        let mut next = Some(origin);
        while let Some(index) = next {
            let visitor = &mut visited_by[index];
            if *visitor == origin {
                return Err(CookedSceneError::HierarchyCycle(entities[index].id));
            }
            if *visitor != usize::MAX {
                break;
            }
            *visitor = origin;
            next = entities[index].parent;
        }
    }
    Ok(())
}
