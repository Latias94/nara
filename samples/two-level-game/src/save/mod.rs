//! Explicit single-slot checkpoint data, separate from authoring Scene/Prefab documents.
//!
//! Restore reconstructs transient gameplay at an authored checkpoint. These values never contain
//! World entities, engine Scene instances, task state, or native handles. Pure decoding validates
//! the format; the game must also validate referenced objects against prepared content before
//! committing a restore. File publication has its own retained cleanup owner.

mod dto;
pub mod io;

pub use dto::{
    CURRENT_FORMAT_VERSION, Checkpoint, CheckpointError, CheckpointLocation, CollectedPickup,
    DecodedCheckpoint, MAX_CHECKPOINT_BYTES, MAX_COLLECTED_PICKUPS, MAX_MARKER_COORDINATE,
    MAX_MARKERS, MAX_OBJECT_ID_BYTES, MAX_SCORE, MarkerId, MarkerRecord, decode_checkpoint,
    encode_checkpoint,
};
