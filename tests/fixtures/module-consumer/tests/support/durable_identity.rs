use std::{collections::BTreeMap, error::Error, fmt, fs, path::PathBuf};

use nara_asset::{
    AssetDatabaseError, AssetMetaIdentityCandidate, AssetPath, AssetRecord, AssetRef,
    ProjectAssetDatabase, StableAssetId,
};
use nara_ecs::Component;
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabDocumentCandidate, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
};

pub const SCENE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
pub const ACTOR_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
pub const SHARED_ID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
pub const SCENE_PATH: &str = "source-a/startup.scene.ron";
pub const ACTOR_PATH: &str = "source-a/actor.prefab.ron";
pub const SHARED_PATH: &str = "source-b/shared.prefab.ron";
pub const DURABLE_LINKS_ID: &str = "consumer.durable.Links";

#[derive(Component, Debug, Clone, PartialEq)]
pub struct DurableLinks(pub ComponentValue);

pub fn durable_links_value(reference: &EntityReference) -> ComponentValue {
    ComponentValue::map([(
        "party",
        ComponentValue::List(vec![
            ComponentValue::EntityReference(reference.clone()),
            ComponentValue::Null,
        ]),
    )])
}

pub fn durable_links_registry(required: bool, reference: &EntityReference) -> ComponentRegistry {
    let shape = ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
        ComponentValueShape::EntityRef,
    )));
    let mut links = if required {
        ComponentFieldSchema::required(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
    } else {
        ComponentFieldSchema::optional(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
    }
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    links.default_value = Some(durable_links_value(reference));
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DurableLinks, _, _>(
            ComponentSchema::new(
                ComponentTypeId::new(DURABLE_LINKS_ID),
                "Durable links",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([links]),
            |value| Ok(DurableLinks(value.field("links")?.clone())),
            |links| Ok(ComponentValue::map([("links", links.0.clone())])),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

pub fn durable_links_document(omitted: bool, reference: &EntityReference) -> SceneDocument {
    let value = if omitted {
        ComponentValue::Map(BTreeMap::new())
    } else {
        ComponentValue::map([("links", durable_links_value(reference))])
    };
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("source").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(DURABLE_LINKS_ID),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )])
}

pub struct ResolvedSource {
    pub database: ProjectAssetDatabase,
    records: Vec<AssetRecord>,
}

pub fn fixture_bytes(path: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/durable-identity");
    fs::read(root.join(path)).expect("the committed durable identity fixture must be readable")
}

pub fn stable_id(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

pub fn resolve_source(paths: &[&str]) -> ResolvedSource {
    let mut database = ProjectAssetDatabase::default();
    let mut records = Vec::new();
    for path in paths {
        assert!(!fixture_bytes(path).is_empty());
        let meta =
            AssetMetaIdentityCandidate::decode_json_bytes(&fixture_bytes(&format!("{path}.meta")))
                .expect("fixture metadata must pass the real bounded decoder")
                .into_identity();
        records.push(AssetRecord::from(meta.clone()));
        database
            .insert_identity(AssetPath::new(*path).unwrap(), meta)
            .expect("each independently resolved source must have unique identities");
    }
    ResolvedSource { database, records }
}

pub fn merge_source(
    published: &mut ProjectAssetDatabase,
    source: &ResolvedSource,
) -> Result<(), AssetDatabaseError> {
    let mut candidate = published.clone();
    for record in &source.records {
        candidate.insert(record.clone())?;
    }
    *published = candidate;
    Ok(())
}

pub fn merged_database() -> ProjectAssetDatabase {
    let first = resolve_source(&[SCENE_PATH, ACTOR_PATH]);
    let second = resolve_source(&[SHARED_PATH]);
    let mut database = first.database;
    merge_source(&mut database, &second).unwrap();
    database
}

pub fn load_prefab(
    database: &ProjectAssetDatabase,
    registry: &ComponentRegistry,
    id: StableAssetId,
) -> PrefabDocument {
    let record = database.resolve_ref(&AssetRef::StableId(id)).unwrap();
    PrefabDocumentCandidate::decode_ron_bytes(&fixture_bytes(record.path().as_str()))
        .unwrap()
        .canonicalize(registry)
        .unwrap()
        .publish_with_asset_database(registry, database)
        .unwrap()
        .into_document()
}

pub fn prefab_resolver(
    database: &ProjectAssetDatabase,
    registry: &ComponentRegistry,
) -> InMemoryPrefabSourceResolver {
    [stable_id(ACTOR_ID), stable_id(SHARED_ID)]
        .into_iter()
        .fold(InMemoryPrefabSourceResolver::new(), |resolver, id| {
            resolver.with_prefab(
                AssetRef::StableId(id),
                SourceDocumentKey::stable_asset(id, SourceDocumentKind::Prefab),
                load_prefab(database, registry, id),
            )
        })
}

#[derive(Debug)]
pub enum ConversionError {
    InvalidDocument(String),
    UnresolvedPrefab {
        entity: SceneEntityId,
        source: AssetRef,
        cause: AssetDatabaseError,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDocument(message) => formatter.write_str(message),
            Self::UnresolvedPrefab {
                entity,
                source,
                cause,
            } => {
                write!(
                    formatter,
                    "entity {entity} has unresolved prefab {source}: {cause}"
                )
            }
        }
    }
}

impl Error for ConversionError {}

/// Converts only this pre-1.0 v1 fixture's typed prefab references, preserving bytes on failure.
#[allow(
    clippy::result_large_err,
    reason = "The public AssetDatabaseError payload is intentionally preserved for typed assertions."
)]
pub fn stabilize_v1_scene_bytes(
    encoded: &mut Vec<u8>,
    registry: &ComponentRegistry,
    database: &ProjectAssetDatabase,
) -> Result<(), ConversionError> {
    let invalid = |error: &dyn Error| ConversionError::InvalidDocument(error.to_string());
    let mut document = SceneDocumentCandidate::decode_ron_bytes(encoded)
        .map_err(|error| invalid(&error))?
        .publish(registry)
        .map_err(|error| invalid(&error))?
        .into_document();
    for entity in &mut document.entities {
        if let Some(prefab) = &mut entity.prefab {
            let record = database.resolve_ref(&prefab.source).map_err(|cause| {
                ConversionError::UnresolvedPrefab {
                    entity: entity.id.clone(),
                    source: prefab.source.clone(),
                    cause,
                }
            })?;
            prefab.source = AssetRef::StableId(record.stable_id());
        }
    }
    let candidate_bytes = document
        .to_ron_string()
        .map_err(|error| invalid(&error))?
        .into_bytes();
    SceneDocumentCandidate::decode_ron_bytes(&candidate_bytes)
        .map_err(|error| invalid(&error))?
        .canonicalize(registry)
        .map_err(|error| invalid(&error))?
        .publish_with_asset_database(registry, database)
        .map_err(|error| invalid(&error))?;
    *encoded = candidate_bytes;
    Ok(())
}

pub fn stable_scene(
    registry: &ComponentRegistry,
    database: &ProjectAssetDatabase,
) -> SceneDocument {
    let mut encoded = fixture_bytes(SCENE_PATH);
    stabilize_v1_scene_bytes(&mut encoded, registry, database).unwrap();
    SceneDocumentCandidate::decode_ron_bytes(&encoded)
        .unwrap()
        .canonicalize(registry)
        .unwrap()
        .publish_with_asset_database(registry, database)
        .unwrap()
        .into_document()
}

pub struct CopiedClosure {
    pub scene: SceneDocument,
    pub actor: PrefabDocument,
    pub database: ProjectAssetDatabase,
    pub stable_ids: BTreeMap<StableAssetId, StableAssetId>,
}

/// Copies the explicit scene/actor closure; shared source documents remain outside this operation.
pub fn copy_as_new(
    scene: &SceneDocument,
    actor: &PrefabDocument,
    database: &ProjectAssetDatabase,
) -> CopiedClosure {
    let stable_ids = [stable_id(SCENE_ID), stable_id(ACTOR_ID)]
        .into_iter()
        .map(|id| (id, StableAssetId::try_random().unwrap()))
        .collect::<BTreeMap<_, _>>();
    let mut copied_database = database.clone();
    for (old, path) in [
        (stable_id(SCENE_ID), "copy/startup.scene.ron"),
        (stable_id(ACTOR_ID), "copy/actor.prefab.ron"),
    ] {
        let record = database.record_for_stable_id(old).unwrap();
        copied_database
            .insert(AssetRecord::new(
                stable_ids[&old],
                AssetPath::new(path).unwrap(),
                record.source_kind().clone(),
            ))
            .unwrap();
    }
    let mut scene = scene.clone();
    let mut actor = actor.clone();
    remap_copied_prefab_references(&mut scene.entities, &stable_ids);
    remap_copied_prefab_references(&mut actor.entities, &stable_ids);
    CopiedClosure {
        scene,
        actor,
        database: copied_database,
        stable_ids,
    }
}

fn remap_copied_prefab_references(
    entities: &mut [SceneEntityRecord],
    stable_ids: &BTreeMap<StableAssetId, StableAssetId>,
) {
    for entity in entities {
        if let Some(prefab) = &mut entity.prefab
            && let AssetRef::StableId(id) = prefab.source
            && let Some(replacement) = stable_ids.get(&id)
        {
            prefab.source = AssetRef::StableId(*replacement);
        }
    }
}
