use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt::{self, Display},
};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error, MapAccess, Visitor, value::MapAccessDeserializer},
};

use super::{
    PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneDocumentError,
    SceneEntity, UnknownComponent,
    compact::{self, Schemas},
};

const VERSION: u32 = 1;

pub(super) enum DocumentKind {
    Scene,
    #[cfg(any(unix, windows))]
    Prefab,
}

pub(super) fn decode(
    source: &str,
    schemas: &Schemas<'_>,
) -> Result<SceneDocument, SceneDocumentError> {
    let mut deserializer = ron::Deserializer::from_str(source)?;
    if !deserializer.extensions().is_empty() {
        return Err(SceneDocumentError::Extensions);
    }
    let name = if deserializer.remainder().starts_with("Prefab") {
        "Prefab"
    } else {
        "Scene"
    };
    let input = deserializer
        .deserialize_struct(name, &[], DocumentVisitor)
        .map_err(|error| deserializer.span_error(error))?;
    deserializer
        .end()
        .map_err(|error| deserializer.span_error(error))?;
    if input.version != VERSION {
        return Err(SceneDocumentError::UnsupportedVersion(input.version));
    }
    let entities = input
        .entities
        .into_iter()
        .map(|entity| entity.decode(schemas, &input.assets))
        .collect::<Result<_, _>>()?;
    Ok(SceneDocument {
        id: input.id,
        assets: input.assets,
        entities,
    })
}

pub(super) fn encode(
    document: &SceneDocument,
    schemas: &Schemas<'_>,
    kind: DocumentKind,
) -> Result<String, SceneDocumentError> {
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_unstable_by_key(|entity| entity.id);
    let output = DocumentOutput {
        version: VERSION,
        id: document.id,
        assets: &document.assets,
        entities: entities
            .into_iter()
            .map(|entity| EntityOutput::encode(entity, schemas, &document.assets))
            .collect::<Result<_, _>>()?,
    };
    let mut source = match kind {
        DocumentKind::Scene => String::from("Scene"),
        #[cfg(any(unix, windows))]
        DocumentKind::Prefab => String::from("Prefab"),
    };
    let config = ron::ser::PrettyConfig::new().new_line("\n");
    let mut serializer = ron::ser::Serializer::new(&mut source, Some(config))
        .map_err(SceneDocumentError::Serialize)?;
    output
        .serialize(&mut serializer)
        .map_err(SceneDocumentError::Serialize)?;
    source.push('\n');
    Ok(source)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentInput {
    version: u32,
    #[serde(deserialize_with = "uuid_text::deserialize")]
    id: DocumentId,
    #[serde(default, deserialize_with = "unique_map")]
    assets: BTreeMap<String, SceneAsset>,
    #[serde(default)]
    entities: Vec<EntityInput>,
}

struct DocumentVisitor;

impl<'de> Visitor<'de> for DocumentVisitor {
    type Value = DocumentInput;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Scene or Prefab document")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        DocumentInput::deserialize(MapAccessDeserializer::new(map))
    }
}

#[derive(Serialize)]
struct DocumentOutput<'a> {
    version: u32,
    #[serde(serialize_with = "uuid_text::serialize")]
    id: DocumentId,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    assets: &'a BTreeMap<String, SceneAsset>,
    entities: Vec<EntityOutput<'a>>,
}

impl Serialize for SceneAsset {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Asset<'a> {
            #[serde(serialize_with = "uuid_text::serialize")]
            id: AssetId,
            hint: &'a str,
        }
        Asset {
            id: self.id,
            hint: self.hint.as_str(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SceneAsset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Asset {
            #[serde(deserialize_with = "uuid_text::deserialize")]
            id: AssetId,
            hint: String,
        }
        let Asset { id, hint } = Asset::deserialize(deserializer)?;
        let hint = AssetPath::new(hint).map_err(D::Error::custom)?;
        Ok(Self { id, hint })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityInput {
    #[serde(deserialize_with = "uuid_text::deserialize")]
    id: EntityUuid,
    name: String,
    #[serde(default, deserialize_with = "optional_uuid_text::deserialize")]
    parent: Option<EntityUuid>,
    #[serde(default, deserialize_with = "unique_map")]
    components: BTreeMap<ComponentTypeId, RawRon>,
    #[serde(default, deserialize_with = "unique_map")]
    component_versions: BTreeMap<ComponentTypeId, ComponentSchemaVersion>,
    #[serde(default, deserialize_with = "present")]
    prefab: Option<String>,
    #[serde(default, deserialize_with = "unique_map")]
    overrides: BTreeMap<String, RawRon>,
    #[serde(default, deserialize_with = "unique_map")]
    added_components: BTreeMap<ComponentTypeId, RawRon>,
    #[serde(default, deserialize_with = "unique_map")]
    added_component_versions: BTreeMap<ComponentTypeId, ComponentSchemaVersion>,
    #[serde(default)]
    removed_components: Vec<ComponentTypeId>,
}

impl EntityInput {
    fn decode(
        self,
        schemas: &Schemas<'_>,
        assets: &BTreeMap<String, SceneAsset>,
    ) -> Result<SceneEntity, SceneDocumentError> {
        let decode =
            |payloads, versions| decode_components(self.id, payloads, versions, schemas, assets);
        let components = decode(self.components, self.component_versions)?;
        let prefab = match self.prefab {
            Some(reference) => {
                let asset = compact::decode_asset(&reference, assets).map_err(|source| {
                    SceneDocumentError::InvalidPrefabReference {
                        entity: self.id,
                        source,
                    }
                })?;
                let added_components =
                    decode(self.added_components, self.added_component_versions)?;
                let mut removed_components = BTreeSet::new();
                for component in self.removed_components {
                    if removed_components.contains(&component) {
                        return Err(SceneDocumentError::DuplicateRemovedComponent {
                            entity: self.id,
                            component,
                        });
                    }
                    removed_components.insert(component);
                }
                Some(PrefabInstance {
                    asset,
                    overrides: self.overrides,
                    added_components,
                    removed_components,
                })
            }
            None if self.overrides.is_empty()
                && self.added_components.is_empty()
                && self.added_component_versions.is_empty()
                && self.removed_components.is_empty() =>
            {
                None
            }
            None => return Err(SceneDocumentError::OrphanPrefabEdits { entity: self.id }),
        };
        Ok(SceneEntity {
            id: self.id,
            name: self.name,
            parent: self.parent,
            components,
            prefab,
        })
    }
}

#[derive(Serialize)]
struct EntityOutput<'a> {
    #[serde(serialize_with = "uuid_text::serialize")]
    id: EntityUuid,
    name: &'a str,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "optional_uuid_text::serialize"
    )]
    parent: Option<EntityUuid>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    components: BTreeMap<&'a ComponentTypeId, Cow<'a, RawRon>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    component_versions: BTreeMap<&'a ComponentTypeId, ComponentSchemaVersion>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_present"
    )]
    prefab: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_present"
    )]
    overrides: Option<&'a BTreeMap<String, RawRon>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    added_components: BTreeMap<&'a ComponentTypeId, Cow<'a, RawRon>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    added_component_versions: BTreeMap<&'a ComponentTypeId, ComponentSchemaVersion>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_present"
    )]
    removed_components: Option<&'a BTreeSet<ComponentTypeId>>,
}

impl<'a> EntityOutput<'a> {
    fn encode(
        entity: &'a SceneEntity,
        schemas: &Schemas<'_>,
        assets: &BTreeMap<String, SceneAsset>,
    ) -> Result<Self, SceneDocumentError> {
        let components = encode_components(entity.id, &entity.components, schemas, assets)?;
        let added = entity
            .prefab
            .as_ref()
            .map(|prefab| encode_components(entity.id, &prefab.added_components, schemas, assets))
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            id: entity.id,
            name: &entity.name,
            parent: entity.parent,
            components: components.payloads,
            component_versions: components.versions,
            prefab: entity
                .prefab
                .as_ref()
                .map(|prefab| compact::asset_text(prefab.asset, assets)),
            overrides: entity
                .prefab
                .as_ref()
                .map(|prefab| &prefab.overrides)
                .filter(|values| !values.is_empty()),
            added_components: added.payloads,
            added_component_versions: added.versions,
            removed_components: entity
                .prefab
                .as_ref()
                .map(|prefab| &prefab.removed_components)
                .filter(|values| !values.is_empty()),
        })
    }
}

fn decode_components(
    entity: EntityUuid,
    payloads: BTreeMap<ComponentTypeId, RawRon>,
    mut versions: BTreeMap<ComponentTypeId, ComponentSchemaVersion>,
    schemas: &Schemas<'_>,
    assets: &BTreeMap<String, SceneAsset>,
) -> Result<BTreeMap<ComponentTypeId, SceneComponent>, SceneDocumentError> {
    let records = payloads
        .into_iter()
        .map(|(id, raw)| {
            let version = versions.remove(&id).unwrap_or_default();
            let record = match schemas.matching(&id, version) {
                Some(schema) => SceneComponent::known(
                    version,
                    compact::decode_component(&raw, schema, assets).map_err(|source| {
                        SceneDocumentError::InvalidComponentValue {
                            entity,
                            component: id.clone(),
                            source,
                        }
                    })?,
                ),
                None => SceneComponent::Unknown(UnknownComponent {
                    type_id: id.clone(),
                    version,
                    raw,
                }),
            };
            Ok((id, record))
        })
        .collect::<Result<_, SceneDocumentError>>()?;
    if let Some((component, _)) = versions.into_iter().next() {
        return Err(SceneDocumentError::OrphanComponentVersion { entity, component });
    }
    Ok(records)
}

#[derive(Default)]
struct EncodedComponents<'a> {
    payloads: BTreeMap<&'a ComponentTypeId, Cow<'a, RawRon>>,
    versions: BTreeMap<&'a ComponentTypeId, ComponentSchemaVersion>,
}

fn encode_components<'a>(
    entity: EntityUuid,
    records: &'a BTreeMap<ComponentTypeId, SceneComponent>,
    schemas: &Schemas<'_>,
    assets: &BTreeMap<String, SceneAsset>,
) -> Result<EncodedComponents<'a>, SceneDocumentError> {
    let mut encoded = EncodedComponents::default();
    for (id, record) in records {
        let version = record.version();
        if version != ComponentSchemaVersion::ONE {
            encoded.versions.insert(id, version);
        }
        let raw = match record {
            SceneComponent::Unknown(unknown) => Cow::Borrowed(&unknown.raw),
            SceneComponent::Known { value, .. } => {
                let schema = schemas.matching(id, version).ok_or_else(|| {
                    SceneDocumentError::MissingComponentSchema {
                        entity,
                        component: id.clone(),
                        version,
                    }
                })?;
                let raw = compact::encode_component(value, schema, assets).map_err(|source| {
                    SceneDocumentError::InvalidComponentValue {
                        entity,
                        component: id.clone(),
                        source,
                    }
                })?;
                Cow::Owned(raw)
            }
        };
        encoded.payloads.insert(id, raw);
    }
    Ok(encoded)
}

fn unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord + Display,
    V: Deserialize<'de>,
{
    struct UniqueMap<K, V>(std::marker::PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for UniqueMap<K, V>
    where
        K: Deserialize<'de> + Ord + Display,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map with unique keys")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some(key) = map.next_key()? {
                match entries.entry(key) {
                    Entry::Vacant(entry) => {
                        entry.insert(map.next_value()?);
                    }
                    Entry::Occupied(entry) => {
                        return Err(A::Error::custom(format_args!(
                            "duplicate key '{}'",
                            entry.key()
                        )));
                    }
                }
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(UniqueMap(std::marker::PhantomData))
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

fn serialize_present<S: Serializer, T: Serialize>(
    value: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value
        .as_ref()
        .ok_or_else(|| serde::ser::Error::custom("missing present field"))?
        .serialize(serializer)
}

mod uuid_text {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(
        value: &impl Display,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>, T: FromStr>(
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

mod optional_uuid_text {
    use nara_core::EntityUuid;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(
        value: &Option<EntityUuid>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(|id| id.to_string()).serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<EntityUuid>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|id| id.parse().map_err(D::Error::custom))
            .transpose()
    }
}
