//! Disposable source/occurrence owner experiment, not an engine API or compatibility adapter.

use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

use nara_asset::{
    AssetMetaIdentityCandidate, AssetPath, AssetRecord, AssetRef, AssetSourceKind,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::ItemLimit;
use nara_ecs::{World, bevy_ecs::world::WorldId};
use nara_identity::SpawnedSceneInstance;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    ExpandedSceneCandidate, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
    advanced::{SceneProductTransactionLimits, replace_expanded_scene_with_product},
    retire_and_despawn_scene_instance, spawn_scene,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug)]
pub enum Error {
    Context,
    Origin,
    PolicyConflict,
    Metadata,
    File,
    Occupied,
    Stale,
    WrongWorld,
    Capacity,
    Scene(Box<nara_engine::diagnostic::DiagnosticReport>),
    KeyLimit { actual: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Universe(Uuid);

impl Universe {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let id = Uuid::parse_str(value).map_err(|_| Error::Context)?;
        if id.is_nil() {
            return Err(Error::Context);
        }
        Ok(Self(id))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Product {
    Primary,
    Named(Atom),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Atom(String);

impl Atom {
    pub fn new(value: &str) -> Result<Self, Error> {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(Error::Origin);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Named(Universe, Atom),
    Asset(StableAssetId),
    Location(Universe, AssetPath),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceKey {
    pub origin: Origin,
    pub product: Product,
}

impl SourceKey {
    fn document_key(&self) -> Result<SourceDocumentKey, Error> {
        if self.product != Product::Primary {
            return Err(Error::Context);
        }
        match &self.origin {
            Origin::Named(universe, name) => SourceDocumentKey::named(
                universe.0.to_string().parse().map_err(|_| Error::Context)?,
                name.0.clone(),
                SourceDocumentKind::Scene,
            )
            .map_err(|_| Error::Origin),
            Origin::Asset(stable_id) => Ok(SourceDocumentKey::stable_asset(
                *stable_id,
                SourceDocumentKind::Scene,
            )),
            Origin::Location(universe, path) => Ok(SourceDocumentKey::project_location(
                universe.0.to_string().parse().map_err(|_| Error::Context)?,
                path.clone(),
                SourceDocumentKind::Scene,
            )),
        }
    }

    pub fn encode_exact(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.encode_into(&mut bytes);
        bytes
    }

    fn encode_into(&self, bytes: &mut Vec<u8>) {
        match &self.origin {
            Origin::Named(universe, name) => {
                bytes.push(0);
                bytes.extend_from_slice(universe.0.as_bytes());
                encode_text(bytes, &name.0);
            }
            Origin::Asset(id) => {
                bytes.push(1);
                bytes.extend_from_slice(id.as_uuid().as_bytes());
            }
            Origin::Location(universe, path) => {
                bytes.push(2);
                bytes.extend_from_slice(universe.0.as_bytes());
                encode_text(bytes, path.as_str());
            }
        }
        match &self.product {
            Product::Primary => bytes.push(0),
            Product::Named(name) => {
                bytes.push(1);
                encode_text(bytes, &name.0);
            }
        }
    }
}

fn encode_text(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
}

#[derive(Debug)]
pub struct BoundScene {
    key: SourceKey,
    candidate: ExpandedSceneCandidate,
}

impl BoundScene {
    pub fn named(universe: Universe, name: &str, document: SceneDocument) -> Result<Self, Error> {
        let key = SourceKey {
            origin: Origin::Named(universe, Atom::new(name)?),
            product: Product::Primary,
        };
        Ok(Self {
            candidate: ExpandedSceneCandidate::plain(key.document_key()?, Arc::new(document)),
            key,
        })
    }

    pub fn key(&self) -> &SourceKey {
        &self.key
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePolicy {
    ProjectLocation,
    StableMetadata,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrialManifest {
    project: TrialProject,
    content: TrialContent,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrialProject {
    source_universe: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrialContent {
    native_document_identity: NativePolicy,
}

pub struct FileSources<'root> {
    root: &'root Path,
    universe: Universe,
    policy: NativePolicy,
    database: ProjectAssetDatabase,
}

impl<'root> FileSources<'root> {
    pub fn open(root: &'root Path) -> Result<Self, Error> {
        let manifest = fs::read_to_string(root.join("nara.toml")).map_err(|_| Error::Context)?;
        let config: TrialManifest = toml::from_str(&manifest).map_err(|_| Error::Context)?;
        let mut result = Self {
            root,
            universe: Universe::parse(&config.project.source_universe)?,
            policy: config.content.native_document_identity,
            database: ProjectAssetDatabase::default(),
        };
        if matches!(result.policy, NativePolicy::StableMetadata) {
            for entry in fs::read_dir(root.join("scenes")).map_err(|_| Error::File)? {
                let entry = entry.map_err(|_| Error::File)?;
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "meta")
                {
                    let bytes = fs::read(&path).map_err(|_| Error::Metadata)?;
                    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&bytes)
                        .map_err(|_| Error::Metadata)?;
                    let expected = result.root.join(metadata.identity().path().meta_path());
                    if expected != path {
                        return Err(Error::Metadata);
                    }
                    result
                        .database
                        .insert(metadata.identity().clone().into())
                        .map_err(|_| Error::Metadata)?;
                }
            }
        }
        Ok(result)
    }

    pub fn load(
        &mut self,
        request: &AssetRef,
        registry: &ComponentRegistry,
    ) -> Result<BoundScene, Error> {
        let mut admission = None;
        let (path, origin) = match self.policy {
            NativePolicy::ProjectLocation => {
                let AssetRef::Path(path) = request else {
                    return Err(Error::PolicyConflict);
                };
                match fs::read(self.root.join(path.meta_path())) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    _ => return Err(Error::PolicyConflict),
                }
                (path.clone(), Origin::Location(self.universe, path.clone()))
            }
            NativePolicy::StableMetadata => {
                let path = match request {
                    AssetRef::Path(path) => path.clone(),
                    AssetRef::StableId(_) => self
                        .database
                        .resolve_ref(request)
                        .map_err(|_| Error::Metadata)?
                        .path()
                        .clone(),
                };
                let bytes =
                    fs::read(self.root.join(path.meta_path())).map_err(|_| Error::Metadata)?;
                let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&bytes)
                    .map_err(|_| Error::Metadata)?;
                if metadata.identity().path() != &path
                    || metadata.identity().source_kind() != &AssetSourceKind::Scene
                {
                    return Err(Error::Metadata);
                }
                let record: AssetRecord = metadata.identity().clone().into();
                if self
                    .database
                    .record_for_path(&path)
                    .is_some_and(|old| old != &record)
                    || self
                        .database
                        .record_for_stable_id(record.stable_id())
                        .is_some_and(|old| old != &record)
                {
                    return Err(Error::Metadata);
                }
                let origin = Origin::Asset(record.stable_id());
                admission = Some(record);
                (path, origin)
            }
        };
        let bytes = fs::read(self.root.join(path.as_str())).map_err(|_| Error::File)?;
        let document = SceneDocumentCandidate::decode_json_bytes(&bytes)
            .map_err(|_| Error::File)?
            .publish(registry)
            .map_err(|_| Error::File)?
            .into_document();
        if let Some(record) = admission
            && self.database.record_for_path(record.path()).is_none()
        {
            self.database.insert(record).map_err(|_| Error::Metadata)?;
        }
        let key = SourceKey {
            origin,
            product: Product::Primary,
        };
        Ok(BoundScene {
            candidate: ExpandedSceneCandidate::plain(key.document_key()?, Arc::new(document)),
            key,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Occurrence {
    namespace: Universe,
    slot: Atom,
}

impl Occurrence {
    pub fn new(namespace: Universe, slot: &str) -> Result<Self, Error> {
        Ok(Self {
            namespace,
            slot: Atom::new(slot)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    occurrence: Occurrence,
    source: SourceKey,
    pub receipt: SpawnedSceneInstance,
}

impl Activation {
    pub fn key(&self, local: SceneEntityId) -> Result<ProjectedKey, Error> {
        let key = ProjectedKey {
            source: self.source.clone(),
            occurrence: self.occurrence.clone(),
            local,
        };
        let actual = key.encode_exact().len();
        if nara_ui_kernel::UiLimits::default()
            .with_max_key_bytes(actual)
            .is_err()
        {
            return Err(Error::KeyLimit { actual });
        }
        Ok(key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedKey {
    source: SourceKey,
    occurrence: Occurrence,
    local: SceneEntityId,
}

impl ProjectedKey {
    pub fn encode_exact(&self) -> Vec<u8> {
        let mut bytes = vec![1];
        self.source.encode_into(&mut bytes);
        bytes.extend_from_slice(self.occurrence.namespace.0.as_bytes());
        encode_text(&mut bytes, &self.occurrence.slot.0);
        encode_text(&mut bytes, self.local.as_str());
        bytes
    }
}

pub struct PlacementIntent<'source> {
    world: WorldId,
    source: &'source BoundScene,
    occurrence: Occurrence,
    predecessor: Option<Activation>,
}

pub struct TrialHost {
    world: World,
    registry: ComponentRegistry,
    active: BTreeMap<Occurrence, Activation>,
}

impl TrialHost {
    pub fn new() -> Self {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        Self {
            world: World::new(),
            registry,
            active: BTreeMap::new(),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// Captures a current-slot intent, not decoded Scene preparation or historical vacancy.
    pub fn intent<'source>(
        &self,
        source: &'source BoundScene,
        occurrence: &Occurrence,
        predecessor: Option<&Activation>,
    ) -> Result<PlacementIntent<'source>, Error> {
        self.check(occurrence, predecessor)?;
        Ok(PlacementIntent {
            world: self.world.id(),
            source,
            occurrence: occurrence.clone(),
            predecessor: predecessor.cloned(),
        })
    }

    fn check(&self, slot: &Occurrence, predecessor: Option<&Activation>) -> Result<(), Error> {
        match (self.active.get(slot), predecessor) {
            (Some(_), None) => Err(Error::Occupied),
            (Some(active), Some(expected)) if active == expected => Ok(()),
            (None, None) if self.active.len() < 8 => Ok(()),
            (None, None) => Err(Error::Capacity),
            _ => Err(Error::Stale),
        }
    }

    pub fn commit(&mut self, intent: PlacementIntent<'_>) -> Result<Activation, Error> {
        if intent.world != self.world.id() {
            return Err(Error::WrongWorld);
        }
        self.check(&intent.occurrence, intent.predecessor.as_ref())?;
        let report = match &intent.predecessor {
            None => spawn_scene(
                &mut self.world,
                &self.registry,
                &intent.source.candidate,
                &SceneOccurrenceKey::new(
                    intent.occurrence.namespace.0.to_string(),
                    intent.occurrence.slot.0.clone(),
                )
                .map_err(|_| Error::Origin)?,
                None,
            ),
            Some(current) => replace_expanded_scene_with_product(
                &mut self.world,
                &self.registry,
                &intent.source.candidate,
                None,
                &current.receipt,
                SceneProductTransactionLimits::new(
                    ItemLimit::new(1).unwrap(),
                    ItemLimit::new(1).unwrap(),
                ),
                &[],
                |_| {},
            ),
        };
        let receipt = report
            .instance
            .ok_or_else(|| Error::Scene(Box::new(report.diagnostics)))?;
        let activation = Activation {
            occurrence: intent.occurrence,
            source: intent.source.key.clone(),
            receipt,
        };
        self.active
            .insert(activation.occurrence.clone(), activation.clone());
        Ok(activation)
    }

    pub fn end(&mut self, current: &Activation) -> Result<usize, Error> {
        self.check(&current.occurrence, Some(current))?;
        let count = retire_and_despawn_scene_instance(&mut self.world, &current.receipt)
            .map_err(|diagnostics| Error::Scene(Box::new(diagnostics)))?;
        self.active.remove(&current.occurrence);
        Ok(count)
    }
}
