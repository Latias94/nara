use nara_core::SourceUniverseId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    AssetSourcePath, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin, StableAssetId,
};

// Text is borrowed for serialization and owned for deserialization. The same private grammar
// serves both directions without cloning names or paths during serialization.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyWire<Text> {
    kind: KindWire,
    origin: OriginWire<Text>,
}

enum KindWire {
    Scene,
    Prefab,
}

// Literal string tags compose through serde value buffers, including RON content-first fields.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum OriginWire<Text> {
    Named {
        universe: SourceUniverseId,
        name: Text,
    },
    ProjectLocation {
        universe: SourceUniverseId,
        path: Text,
    },
    StableAsset {
        stable_id: StableAssetId,
    },
}

impl Serialize for KindWire {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Scene => "scene",
            Self::Prefab => "prefab",
        })
    }
}

impl<'de> Deserialize<'de> for KindWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let kind = String::deserialize(deserializer)?;
        match kind.as_str() {
            "scene" => Ok(Self::Scene),
            "prefab" => Ok(Self::Prefab),
            _ => Err(serde::de::Error::custom("unknown source document kind")),
        }
    }
}

impl Serialize for SourceDocumentKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let kind = match self.kind() {
            SourceDocumentKind::Scene => KindWire::Scene,
            SourceDocumentKind::Prefab => KindWire::Prefab,
        };
        let origin = match self.origin() {
            SourceDocumentOrigin::Named { universe, name } => OriginWire::Named {
                universe: *universe,
                name: name.as_ref(),
            },
            SourceDocumentOrigin::ProjectLocation { universe, path } => {
                OriginWire::ProjectLocation {
                    universe: *universe,
                    path: path.as_str(),
                }
            }
            SourceDocumentOrigin::StableAsset { stable_id } => OriginWire::StableAsset {
                stable_id: *stable_id,
            },
        };
        KeyWire { kind, origin }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SourceDocumentKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = KeyWire::<String>::deserialize(deserializer)?;
        let kind = match wire.kind {
            KindWire::Scene => SourceDocumentKind::Scene,
            KindWire::Prefab => SourceDocumentKind::Prefab,
        };
        match wire.origin {
            OriginWire::Named { universe, name } => {
                Self::named(universe, name, kind).map_err(serde::de::Error::custom)
            }
            OriginWire::ProjectLocation { universe, path } => {
                let path = AssetSourcePath::new(path).map_err(serde::de::Error::custom)?;
                Ok(Self::project_location(universe, path, kind))
            }
            OriginWire::StableAsset { stable_id } => Ok(Self::stable_asset(stable_id, kind)),
        }
    }
}
