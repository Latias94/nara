use nara::asset::{AssetProductRef, ImportedProductId, StableAssetId};
#[cfg(any(feature = "development", feature = "cook"))]
use nara::project::ProjectPath;
use serde::{Deserialize, Deserializer, Serialize};

/// Number of members in each supplied Scene fixture, including the explorer and static walls.
pub const AUTHORED_LEVEL_MEMBERS: usize = 11;

/// Stable authored entity IDs for the four room boundaries and the interior wall.
pub const WALL_ENTITY_IDS: [&str; 5] = [
    "wall-west",
    "wall-east",
    "wall-north",
    "wall-south",
    "wall-inner",
];

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

    #[must_use]
    pub const fn scene_file_name(self) -> &'static str {
        match self {
            Self::Meadow => "meadow.scene.json",
            Self::Cavern => "cavern.scene.json",
        }
    }

    /// Scene path relative to the project's effective `paths.scenes` directory.
    #[cfg(any(feature = "development", feature = "cook"))]
    #[must_use]
    pub fn scene_path(self) -> ProjectPath {
        ProjectPath::new(self.scene_file_name()).expect("the game's static Scene paths are valid")
    }

    /// Complete shipping Scene identity, independent of authoring `ProjectLocation` identities.
    #[must_use]
    pub fn scene_product(self) -> AssetProductRef {
        primary_product(match self {
            Self::Meadow => "624ebd5d-b7b1-4da9-94f8-86d689caeac1",
            Self::Cavern => "0c9fa5da-20ef-4c87-a559-c733bc231350",
        })
    }

    /// The existing Image primary identity declared by this level's source metadata.
    #[must_use]
    pub fn image_product(self) -> AssetProductRef {
        primary_product(match self {
            Self::Meadow => "969e2daa-9f60-463a-97fa-03b3d295ea5f",
            Self::Cavern => "2ba63e2c-41ef-4d3e-918e-82fdf417523d",
        })
    }
}

fn primary_product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).expect("the game's static Product UUIDs are valid"),
        ImportedProductId::PRIMARY,
    )
    .expect("the game's static Product UUIDs are non-nil")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipping_products_are_complete_and_image_identities_match_the_existing_metadata() {
        let metadata = [
            include_str!("../assets/textures/meadow.png.meta"),
            include_str!("../assets/textures/cavern.png.meta"),
        ];
        let mut products = std::collections::BTreeSet::new();
        for (level, metadata) in LevelId::ALL.into_iter().zip(metadata) {
            let metadata: serde_json::Value = serde_json::from_str(metadata).unwrap();
            let image = level.image_product();
            assert_eq!(
                image.source().to_string(),
                metadata["payload"]["asset"]["stable_id"].as_str().unwrap()
            );
            for product in [level.scene_product(), image] {
                assert_eq!(product.product(), ImportedProductId::PRIMARY);
                assert!(!product.source().as_uuid().is_nil());
                assert!(products.insert(product));
            }
        }
    }
}
