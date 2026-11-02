use serde::{Deserialize, Deserializer};
use serde::{Serialize, Serializer, ser::SerializeStruct};

use crate::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, PersistentRuntimeReference,
    PrefabEntityOrigin, SceneEntityId,
};

impl Serialize for PrefabEntityOrigin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct("PrefabEntityOrigin", 2)?;
        wire.serialize_field(
            "kind",
            match self {
                Self::SourceEntity(_) => "source_entity",
                Self::InstanceAddition(_) => "instance_addition",
            },
        )?;
        wire.serialize_field("entity", self.entity())?;
        wire.end()
    }
}

impl<'de> Deserialize<'de> for PrefabEntityOrigin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct OriginWire {
            kind: String,
            entity: SceneEntityId,
        }
        let wire = OriginWire::deserialize(deserializer)?;
        match wire.kind.as_str() {
            "source_entity" => Ok(Self::SourceEntity(wire.entity)),
            "instance_addition" => Ok(Self::InstanceAddition(wire.entity)),
            _ => Err(serde::de::Error::custom("unknown entity origin kind")),
        }
    }
}

impl Serialize for AuthoredReferenceTarget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct("AuthoredReferenceTarget", 2)?;
        match self {
            Self::Entity(entity) => {
                wire.serialize_field("kind", "entity")?;
                wire.serialize_field("value", entity)?;
            }
            Self::Input(input) => {
                wire.serialize_field("kind", "input")?;
                wire.serialize_field("value", input)?;
            }
        }
        wire.end()
    }
}

impl<'de> Deserialize<'de> for AuthoredReferenceTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Endpoint {
            Entity(PrefabEntityOrigin),
            Input(crate::ReferenceInputId),
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TargetWire {
            kind: String,
            value: Endpoint,
        }
        let wire = TargetWire::deserialize(deserializer)?;
        match (wire.kind.as_str(), wire.value) {
            ("entity", Endpoint::Entity(entity)) => Ok(Self::Entity(entity)),
            ("input", Endpoint::Input(input)) => Ok(Self::Input(input)),
            _ => Err(serde::de::Error::custom(
                "reference target fields do not match its kind",
            )),
        }
    }
}

// Decode the authored field directly. An internally tagged enum's content buffer loses RON's
// external enum names in source-key origins. This wire keeps the existing native field shapes
// and permits any field order without buffering the authored reference tree.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceWire {
    kind: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    entity: Option<NativeEntityWire>,
    #[serde(default, deserialize_with = "deserialize_present")]
    reference: Option<AuthoredEntityReference>,
}

// Native payloads contain only validated string/UUID values and no externally tagged enums.
#[derive(Deserialize)]
#[serde(untagged)]
enum NativeEntityWire {
    SceneLocal(SceneEntityId),
    Persistent(PersistentRuntimeReference),
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for EntityReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReferenceWire::deserialize(deserializer)?;
        match (wire.kind.as_str(), wire.entity, wire.reference) {
            ("scene_local", Some(NativeEntityWire::SceneLocal(entity)), None) => {
                Ok(Self::SceneLocal { entity })
            }
            ("persistent", Some(NativeEntityWire::Persistent(entity)), None) => {
                Ok(Self::Persistent { entity })
            }
            ("authored", None, Some(reference)) => Ok(Self::Authored { reference }),
            _ => Err(serde::de::Error::custom(
                "entity reference fields do not match its kind",
            )),
        }
    }
}
