//! Complete fragment storage comparison after both raw source trees have been admitted.

use std::io::{self, Write};

use super::super::{ExtractionError, ExtractionWork};
use crate::SceneEntityRecord;

pub(in crate::prefab::extraction) fn entity_storage_matches(
    expected: &SceneEntityRecord,
    stored: &SceneEntityRecord,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    if expected != stored {
        return Ok(false);
    }
    // ComponentFloat equality equates signed zero. Complete storage must preserve its bits,
    // including in opaque values and hidden defaults. Use the standard serializer on one
    // already admitted fragment; no overlay/value interpreter or full candidate is created.
    let expected = encode_entity(expected, work)?;
    let stored = encode_entity(stored, work)?;
    work.visit(
        1,
        expected
            .len()
            .checked_add(stored.len())
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    Ok(expected == stored)
}

fn encode_entity(
    entity: &SceneEntityRecord,
    work: &mut ExtractionWork,
) -> Result<Vec<u8>, ExtractionError> {
    let mut writer = EntityEncodingBuffer {
        bytes: Vec::new(),
        work,
        refusal: None,
    };
    let result = serde_json::to_writer(&mut writer, entity);
    if let Some(error) = writer.refusal {
        return Err(error);
    }
    result.map_err(|_| ExtractionError::InvalidSource)?;
    Ok(writer.bytes)
}

struct EntityEncodingBuffer<'a> {
    bytes: Vec<u8>,
    work: &'a mut ExtractionWork,
    refusal: Option<ExtractionError>,
}

impl Write for EntityEncodingBuffer<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.work.visit(1, buffer.len(), 0)?;
            let length = self
                .bytes
                .len()
                .checked_add(buffer.len())
                .ok_or(ExtractionError::Capacity)?;
            if length > self.bytes.capacity() {
                let capacity = length.max(self.bytes.capacity().saturating_mul(2));
                self.work.admit(Some(capacity))?;
                self.bytes
                    .try_reserve_exact(capacity - self.bytes.len())
                    .map_err(|_| ExtractionError::Capacity)?;
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        })();
        result.map_err(|error| {
            self.refusal = Some(error);
            io::ErrorKind::OutOfMemory.into()
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
