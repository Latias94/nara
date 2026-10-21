use nara::project::ProjectPath;
use serde::{Deserialize, Deserializer, Serialize};

/// The game's finite content catalog, independent of a running Scene instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelId {
    Meadow,
    Cavern,
}

impl LevelId {
    pub const ALL: [Self; 2] = [Self::Meadow, Self::Cavern];
    pub const STARTUP: Self = Self::Meadow;

    /// Scene path relative to the project's effective `paths.scenes` directory.
    #[must_use]
    pub fn scene_path(self) -> ProjectPath {
        ProjectPath::new(match self {
            Self::Meadow => "meadow.scene.json",
            Self::Cavern => "cavern.scene.json",
        })
        .expect("the game's static Scene paths are valid")
    }
}

/// Number of independently persistent game instances supported per level source.
pub const MAX_LEVEL_INSTANCES: u16 = 16;

/// Game-owned persistent identity; it is not an engine `SceneInstanceId` or World reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct LevelKey {
    level: LevelId,
    instance: u16,
}

impl LevelKey {
    pub fn new(level: LevelId, instance: u16) -> Result<Self, LevelKeyError> {
        if instance >= MAX_LEVEL_INSTANCES {
            return Err(LevelKeyError::InstanceOutOfRange);
        }
        Ok(Self { level, instance })
    }

    #[must_use]
    pub const fn main(level: LevelId) -> Self {
        Self { level, instance: 0 }
    }

    #[must_use]
    pub const fn level(self) -> LevelId {
        self.level
    }

    #[must_use]
    pub const fn instance(self) -> u16 {
        self.instance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LevelKeyError {
    #[error("level instance is outside the game's supported range")]
    InstanceOutOfRange,
}

impl<'de> Deserialize<'de> for LevelKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            level: LevelId,
            instance: u16,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.level, wire.instance).map_err(serde::de::Error::custom)
    }
}
