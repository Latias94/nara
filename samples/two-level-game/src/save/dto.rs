use std::{collections::BTreeSet, num::NonZeroU64};

use nara::{
    core::{
        ByteLimit, DepthLimit, ItemLimit, SerdeShapeError, SerdeShapeLimits,
        SerdeShapePreflightError, preflight_serde_shape,
    },
    identity::SceneEntityId,
};
use serde::{Deserialize, Serialize};

use crate::levels::{LevelId, LevelKey};

const KIND: &str = "two_level_checkpoint";
const CAMPAIGN_REVISION: u32 = 1;
pub const CURRENT_FORMAT_VERSION: u32 = 2;
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024;
pub const MAX_COLLECTED_PICKUPS: usize = 128;
pub const MAX_MARKERS: usize = 16;
pub const MAX_OBJECT_ID_BYTES: usize = 128;
pub const MAX_SCORE: u32 = 1_000_000;
pub const MAX_MARKER_COORDINATE: f32 = 4096.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointLocation {
    pub level: LevelKey,
    pub anchor: SceneEntityId,
}

/// An authored object removed by gameplay, scoped to its persistent level instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectedPickup {
    pub level: LevelKey,
    pub entity: SceneEntityId,
}

/// A game-owned serial number, independent of runtime entity allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarkerId(NonZeroU64);

impl MarkerId {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The one supported persistent dynamic object: a player-placed marker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerRecord {
    pub id: MarkerId,
    pub level: LevelKey,
    pub position: [f32; 2],
}

/// Explicit checkpoint state; live position, velocity, input, and simulation tick are reconstructed.
///
/// Use [`decode_checkpoint`] to admit external bytes. Serde support alone does not apply the byte,
/// shape, collection, or cross-field checks of this format owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub location: CheckpointLocation,
    pub score: u32,
    pub collected: Vec<CollectedPickup>,
    pub markers: Vec<MarkerRecord>,
    pub next_marker_id: MarkerId,
}

impl Checkpoint {
    #[must_use]
    pub fn new(location: CheckpointLocation) -> Self {
        Self {
            location,
            score: 0,
            collected: Vec::new(),
            markers: Vec::new(),
            next_marker_id: MarkerId(NonZeroU64::MIN),
        }
    }

    /// Checks bounded game data without reading or activating any Scene.
    pub fn validate(&self) -> Result<(), CheckpointError> {
        if self.score > MAX_SCORE {
            return Err(CheckpointError::ScoreOutOfRange);
        }
        validate_object_id(&self.location.anchor)?;
        if self.collected.len() > MAX_COLLECTED_PICKUPS {
            return Err(CheckpointError::TooManyCollectedPickups);
        }
        let mut collected = BTreeSet::new();
        for pickup in &self.collected {
            validate_object_id(&pickup.entity)?;
            if !collected.insert(pickup) {
                return Err(CheckpointError::DuplicateCollectedPickup);
            }
        }
        if self.markers.len() > MAX_MARKERS {
            return Err(CheckpointError::TooManyMarkers);
        }
        let mut markers = BTreeSet::new();
        for marker in &self.markers {
            if !markers.insert(marker.id) {
                return Err(CheckpointError::DuplicateMarkerId);
            }
            for coordinate in marker.position {
                if !coordinate.is_finite() {
                    return Err(CheckpointError::NonFiniteMarkerPosition);
                }
                if coordinate.abs() > MAX_MARKER_COORDINATE {
                    return Err(CheckpointError::MarkerPositionOutOfRange);
                }
            }
        }
        if self.next_marker_id.get() == u64::MAX
            || self
                .markers
                .iter()
                .any(|marker| marker.id >= self.next_marker_id)
        {
            return Err(CheckpointError::InvalidNextMarkerId);
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        self.collected.sort();
        self.markers.sort_by_key(|marker| marker.id);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedCheckpoint {
    pub checkpoint: Checkpoint,
    pub migrated_from: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint exceeds its encoded-byte limit")]
    ByteLimitExceeded,
    #[error("checkpoint JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("checkpoint exceeds its structural limits: {0}")]
    Shape(#[source] SerdeShapeError),
    #[error("checkpoint kind does not belong to this game")]
    WrongKind,
    #[error("checkpoint format version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("checkpoint campaign revision {0} is unsupported")]
    UnsupportedCampaign(u32),
    #[error("checkpoint object identifier is too long")]
    ObjectIdTooLong,
    #[error("checkpoint score is outside the supported range")]
    ScoreOutOfRange,
    #[error("checkpoint contains too many collected pickups")]
    TooManyCollectedPickups,
    #[error("checkpoint contains a duplicate collected pickup")]
    DuplicateCollectedPickup,
    #[error("checkpoint contains too many markers")]
    TooManyMarkers,
    #[error("checkpoint contains a duplicate marker ID")]
    DuplicateMarkerId,
    #[error("checkpoint marker position is not finite")]
    NonFiniteMarkerPosition,
    #[error("checkpoint marker position is outside the supported range")]
    MarkerPositionOutOfRange,
    #[error("checkpoint next marker ID is exhausted or would reuse an existing ID")]
    InvalidNextMarkerId,
}

/// Produces bounded, deterministically ordered current-version bytes. It performs no file I/O.
pub fn encode_checkpoint(checkpoint: &Checkpoint) -> Result<Vec<u8>, CheckpointError> {
    checkpoint.validate()?;
    let mut checkpoint = checkpoint.clone();
    checkpoint.canonicalize();
    // All variable data is bounded by validate before serialization allocates the output.
    let bytes = serde_json::to_vec(&Envelope {
        kind: KIND,
        format_version: CURRENT_FORMAT_VERSION,
        campaign_revision: CAMPAIGN_REVISION,
        payload: &checkpoint,
    })
    .map_err(CheckpointError::Json)?;
    preflight(&bytes)?;
    Ok(bytes)
}

/// Admits checkpoint bytes, migrating the known version-one representation into current game data.
///
/// Successful decoding is not proof that referenced anchors or objects exist in prepared content.
pub fn decode_checkpoint(bytes: &[u8]) -> Result<DecodedCheckpoint, CheckpointError> {
    preflight(bytes)?;
    let header: Header = serde_json::from_slice(bytes).map_err(CheckpointError::Json)?;
    if header.kind != KIND {
        return Err(CheckpointError::WrongKind);
    }
    if header.campaign_revision != CAMPAIGN_REVISION {
        return Err(CheckpointError::UnsupportedCampaign(
            header.campaign_revision,
        ));
    }
    let (mut checkpoint, migrated_from) = match header.format_version {
        CURRENT_FORMAT_VERSION => {
            let envelope: Wire<Checkpoint> =
                serde_json::from_slice(bytes).map_err(CheckpointError::Json)?;
            (envelope.payload, None)
        }
        1 => {
            let envelope: Wire<LegacyCheckpoint> =
                serde_json::from_slice(bytes).map_err(CheckpointError::Json)?;
            (envelope.payload.migrate(), Some(1))
        }
        other => return Err(CheckpointError::UnsupportedVersion(other)),
    };
    checkpoint.validate()?;
    checkpoint.canonicalize();
    Ok(DecodedCheckpoint {
        checkpoint,
        migrated_from,
    })
}

fn validate_object_id(id: &SceneEntityId) -> Result<(), CheckpointError> {
    if id.as_str().len() > MAX_OBJECT_ID_BYTES {
        return Err(CheckpointError::ObjectIdTooLong);
    }
    Ok(())
}

fn preflight(bytes: &[u8]) -> Result<(), CheckpointError> {
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::ByteLimitExceeded);
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("the checkpoint depth limit is non-zero"),
        ItemLimit::new(4096).expect("the checkpoint node limit is non-zero"),
        ItemLimit::new(256).expect("the checkpoint container limit is non-zero"),
        ByteLimit::new(MAX_OBJECT_ID_BYTES).expect("the checkpoint string limit is non-zero"),
        ByteLimit::new(32 * 1024).expect("the checkpoint total string limit is non-zero"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => CheckpointError::Shape(error),
        SerdeShapePreflightError::Parse(error) => CheckpointError::Json(error),
    })?;
    deserializer.end().map_err(CheckpointError::Json)
}

#[derive(Serialize)]
struct Envelope<'a> {
    kind: &'static str,
    format_version: u32,
    campaign_revision: u32,
    payload: &'a Checkpoint,
}

#[derive(Deserialize)]
struct Header {
    kind: String,
    format_version: u32,
    campaign_revision: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire<T> {
    #[serde(rename = "kind")]
    _kind: String,
    #[serde(rename = "format_version")]
    _format_version: u32,
    #[serde(rename = "campaign_revision")]
    _campaign_revision: u32,
    payload: T,
}

// Version one addressed a level source directly. Its only game instance was the main instance.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpoint {
    location: LegacyLocation,
    score: u32,
    collected: Vec<LegacyPickup>,
    markers: Vec<LegacyMarker>,
    next_marker_id: MarkerId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLocation {
    level: LevelId,
    anchor: SceneEntityId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPickup {
    level: LevelId,
    entity: SceneEntityId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMarker {
    id: MarkerId,
    level: LevelId,
    position: [f32; 2],
}

impl LegacyCheckpoint {
    fn migrate(self) -> Checkpoint {
        Checkpoint {
            location: CheckpointLocation {
                level: LevelKey::main(self.location.level),
                anchor: self.location.anchor,
            },
            score: self.score,
            collected: self
                .collected
                .into_iter()
                .map(|pickup| CollectedPickup {
                    level: LevelKey::main(pickup.level),
                    entity: pickup.entity,
                })
                .collect(),
            markers: self
                .markers
                .into_iter()
                .map(|marker| MarkerRecord {
                    id: marker.id,
                    level: LevelKey::main(marker.level),
                    position: marker.position,
                })
                .collect(),
            next_marker_id: self.next_marker_id,
        }
    }
}
