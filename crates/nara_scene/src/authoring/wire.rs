use std::{collections::BTreeMap, fmt};

use nara_reflect::ComponentTypeId;
use serde::{
    Deserializer, Serialize, Serializer,
    de::{Error, MapAccess, Visitor},
};

use super::{SceneComponent, SceneEntity};

pub(super) fn serialize_entities<S: Serializer>(
    entities: &[SceneEntity],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut ordered = entities.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|entity| entity.id);
    ordered.serialize(serializer)
}

pub(super) fn deserialize_components<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<ComponentTypeId, SceneComponent>, D::Error> {
    struct ComponentsVisitor;

    impl<'de> Visitor<'de> for ComponentsVisitor {
        type Value = BTreeMap<ComponentTypeId, SceneComponent>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map of unique component IDs to versioned values")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut components = BTreeMap::new();
            while let Some(id) = map.next_key::<ComponentTypeId>()? {
                match components.entry(id) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(map.next_value()?);
                    }
                    std::collections::btree_map::Entry::Occupied(entry) => {
                        return Err(A::Error::custom(format_args!(
                            "duplicate component ID '{}'",
                            entry.key()
                        )));
                    }
                }
            }
            Ok(components)
        }
    }

    deserializer.deserialize_map(ComponentsVisitor)
}

pub(super) mod uuid_text {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(
        value: &impl Display,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: FromStr>(
        deserializer: D,
    ) -> Result<T, D::Error>
    where
        T::Err: Display,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

pub(super) mod optional_uuid_text {
    use nara_core::EntityUuid;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub fn serialize<S: Serializer>(
        value: &Option<EntityUuid>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(|id| id.to_string()).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<EntityUuid>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|id| id.parse().map_err(D::Error::custom))
            .transpose()
    }
}
