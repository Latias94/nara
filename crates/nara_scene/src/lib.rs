//! Scene runtime hierarchy components and persistent scene document data.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_app::{App, CoreStage, Plugin};
use nara_asset::AssetRef;
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Bundle, Component, Entity, World};
use nara_reflect::{
    ComponentCodecError, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PreparedComponent, Reflect, bevy_reflect,
};
pub use nara_transform::Transform2d;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Component, Reflect)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Name(pub String);

impl Name {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct Parent(pub Entity);

#[derive(Debug, Clone, PartialEq, Eq, Default, Component)]
pub struct Children(pub Vec<Entity>);

impl Children {
    pub fn push(&mut self, child: Entity) {
        self.0.push(child);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Reflect)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
pub struct SceneEntityId(String);

impl SceneEntityId {
    pub fn new(id: impl Into<String>) -> Result<Self, SceneEntityIdError> {
        let id = id.into();
        validate_scene_entity_id(&id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SceneEntityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SceneEntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneDocument {
    pub format_version: u32,
    pub entities: Vec<SceneEntityRecord>,
}

impl SceneDocument {
    pub const CURRENT_FORMAT_VERSION: u32 = 1;

    #[must_use]
    pub fn new(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            entities,
        }
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        preflight_scene(self, registry).diagnostics
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| SceneFormatError::Json(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = serde_json::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Json(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }

    #[cfg(feature = "serde")]
    pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| SceneFormatError::Ron(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_ron_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = ron::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Ron(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            entities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneEntityRecord {
    pub id: SceneEntityId,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub parent: Option<SceneEntityId>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub prefab: Option<PrefabInstance>,
}

impl SceneEntityRecord {
    #[must_use]
    pub fn new(id: SceneEntityId) -> Self {
        Self {
            id,
            parent: None,
            components: BTreeMap::new(),
            prefab: None,
        }
    }

    #[must_use]
    pub fn with_parent(mut self, parent: SceneEntityId) -> Self {
        self.parent = Some(parent);
        self
    }

    #[must_use]
    pub fn with_component(
        mut self,
        component_type: ComponentTypeId,
        component: SceneComponentRecord,
    ) -> Self {
        self.components.insert(component_type, component);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneComponentRecord {
    pub version: ComponentSchemaVersion,
    pub value: ComponentValue,
}

impl SceneComponentRecord {
    #[must_use]
    pub fn new(version: ComponentSchemaVersion, value: ComponentValue) -> Self {
        Self { version, value }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrefabDocument {
    pub format_version: u32,
    pub entities: Vec<SceneEntityRecord>,
}

pub type PrefabComponentOverrides =
    BTreeMap<SceneEntityId, BTreeMap<ComponentTypeId, SceneComponentRecord>>;

impl PrefabDocument {
    pub const CURRENT_FORMAT_VERSION: u32 = 1;

    #[must_use]
    pub fn new(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            entities,
        }
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn instantiate(&self) -> SceneDocument {
        self.instantiate_with_overrides(&PrefabComponentOverrides::new())
    }

    #[must_use]
    pub fn instantiate_with_overrides(
        &self,
        overrides: &PrefabComponentOverrides,
    ) -> SceneDocument {
        let mut entities = self.entities.clone();
        for entity in &mut entities {
            if let Some(component_overrides) = overrides.get(&entity.id) {
                for (component_id, component) in component_overrides {
                    entity
                        .components
                        .insert(component_id.clone(), component.clone());
                }
            }
        }
        let mut document = SceneDocument {
            format_version: self.format_version,
            entities,
        };
        document.canonicalize();
        document
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        self.instantiate().validate(registry)
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| SceneFormatError::Json(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = serde_json::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Json(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }

    #[cfg(feature = "serde")]
    pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| SceneFormatError::Ron(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_ron_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = ron::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Ron(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }
}

impl Default for PrefabDocument {
    fn default() -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            entities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrefabInstance {
    pub source: AssetRef,
    #[cfg_attr(feature = "serde", serde(default))]
    pub overrides: BTreeMap<ComponentTypeId, SceneComponentRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneInstanceId(u64);

impl SceneInstanceId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct SceneEntitySource {
    pub instance_id: SceneInstanceId,
    pub entity_id: SceneEntityId,
}

impl SceneEntitySource {
    #[must_use]
    pub fn export_id(&self) -> SceneEntityId {
        if self.instance_id.raw() == 1 {
            return self.entity_id.clone();
        }

        SceneEntityId::new(format!(
            "instance_{}/{}",
            self.instance_id.raw(),
            self.entity_id.as_str()
        ))
        .expect("scene entity source should produce valid export ids")
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneEntityMap {
    entities: BTreeMap<SceneEntityId, Entity>,
}

impl SceneEntityMap {
    pub fn insert(&mut self, scene_id: SceneEntityId, entity: Entity) -> Option<Entity> {
        self.entities.insert(scene_id, entity)
    }

    #[must_use]
    pub fn get(&self, scene_id: &SceneEntityId) -> Option<Entity> {
        self.entities.get(scene_id).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SceneEntityId, Entity)> + '_ {
        self.entities
            .iter()
            .map(|(scene_id, entity)| (scene_id, *entity))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneSpawnReport {
    pub entity_map: SceneEntityMap,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SceneExportReport {
    pub document: SceneDocument,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default)]
pub struct SceneSpawner {
    next_instance_id: u64,
}

impl SceneSpawner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_instance_id: 1,
        }
    }

    pub fn spawn(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
    ) -> SceneSpawnReport {
        let preflight = preflight_scene(document, registry);
        if preflight.diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics: preflight.diagnostics,
            };
        }

        let instance_id = SceneInstanceId::from_raw(self.next_instance_id);
        self.next_instance_id = self.next_instance_id.saturating_add(1).max(1);

        let mut entity_map = SceneEntityMap::default();
        for entity in &preflight.entities {
            let runtime_entity = world.spawn_empty().id();
            world.entity_mut(runtime_entity).insert(SceneEntitySource {
                instance_id,
                entity_id: entity.id.clone(),
            });
            entity_map.insert(entity.id.clone(), runtime_entity);
        }

        let mut diagnostics = preflight.diagnostics;
        for entity in preflight.entities {
            let Some(runtime_entity) = entity_map.get(&entity.id) else {
                diagnostics.push(
                    Diagnostic::error("scene.internal-missing-entity", "missing spawned entity")
                        .with_entity_id(entity.id.as_str()),
                );
                continue;
            };

            for component in entity.components {
                if let Err(error) = component.apply(world, runtime_entity) {
                    diagnostics.push(
                        Diagnostic::error("scene.component-apply-failed", error.to_string())
                            .with_entity_id(entity.id.as_str()),
                    );
                }
            }

            if let Some(parent_id) = entity.parent {
                if let Some(parent_entity) = entity_map.get(&parent_id) {
                    world
                        .entity_mut(runtime_entity)
                        .insert(Parent(parent_entity));
                }
            }
        }

        sync_children(world);

        SceneSpawnReport {
            entity_map,
            diagnostics,
        }
    }

    pub fn spawn_prefab(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
    ) -> SceneSpawnReport {
        self.spawn_prefab_with_overrides(world, registry, prefab, &PrefabComponentOverrides::new())
    }

    pub fn spawn_prefab_with_overrides(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        overrides: &PrefabComponentOverrides,
    ) -> SceneSpawnReport {
        let mut diagnostics = validate_prefab_overrides(prefab, overrides);
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics,
            };
        }

        let mut report = self.spawn(
            world,
            registry,
            &prefab.instantiate_with_overrides(overrides),
        );
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }
}

#[must_use]
pub fn spawn_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn(world, registry, document)
}

#[must_use]
pub fn spawn_prefab(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab(world, registry, prefab)
}

#[must_use]
pub fn spawn_prefab_with_overrides(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    overrides: &PrefabComponentOverrides,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab_with_overrides(world, registry, prefab, overrides)
}

#[must_use]
pub fn export_scene(world: &World, registry: &ComponentRegistry) -> SceneExportReport {
    let mut diagnostics = DiagnosticReport::default();
    let mut entities = world
        .iter_entities()
        .map(|entity_ref| entity_ref.id())
        .collect::<Vec<_>>();
    entities.sort_by_key(|entity| entity.index());

    let mut id_by_entity = BTreeMap::<Entity, SceneEntityId>::new();
    for (ordinal, entity) in entities.iter().copied().enumerate() {
        let scene_id = world
            .get::<SceneEntitySource>(entity)
            .map(SceneEntitySource::export_id)
            .unwrap_or_else(|| {
                SceneEntityId::new(format!("entity_{}", ordinal + 1))
                    .expect("generated export ids should be valid")
            });
        id_by_entity.insert(entity, scene_id);
    }

    let mut records = Vec::new();
    for entity in entities {
        let id = id_by_entity
            .get(&entity)
            .expect("entity id should be assigned before export")
            .clone();
        let parent = world
            .get::<Parent>(entity)
            .and_then(|parent| id_by_entity.get(&parent.0).cloned());

        if world.get::<Parent>(entity).is_some() && parent.is_none() {
            diagnostics.push(
                Diagnostic::warning(
                    "scene.export-parent-skipped",
                    "parent entity is not exported with this scene",
                )
                .with_entity_id(id.as_str()),
            );
        }

        let mut components = BTreeMap::new();
        for schema in registry.schemas().filter(|schema| schema.serializable) {
            let Some(encoded) = registry.encode_component(&schema.id, world, entity) else {
                continue;
            };
            match encoded {
                Ok(Some(value)) => {
                    components.insert(
                        schema.id.clone(),
                        SceneComponentRecord::new(schema.version, value),
                    );
                }
                Ok(None) => {}
                Err(error) => diagnostics.push(
                    Diagnostic::warning("scene.export-component-failed", error.to_string())
                        .with_entity_id(id.as_str())
                        .with_component_id(schema.id.as_str()),
                ),
            }
        }

        if components.is_empty() && world.get::<SceneEntitySource>(entity).is_none() {
            continue;
        }

        records.push(SceneEntityRecord {
            id,
            parent,
            components,
            prefab: None,
        });
    }

    let exported_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    for record in &mut records {
        if let Some(parent) = &record.parent {
            if !exported_ids.contains(parent) {
                diagnostics.push(
                    Diagnostic::warning(
                        "scene.export-parent-skipped",
                        "parent entity is not exported with this scene",
                    )
                    .with_entity_id(record.id.as_str()),
                );
                record.parent = None;
            }
        }
    }

    SceneExportReport {
        document: SceneDocument::new(records),
        diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneEntityIdError {
    Empty,
    LeadingSlash,
    TrailingSlash,
    EmptySegment,
    CurrentDirectorySegment,
    ParentDirectorySegment,
    InvalidCharacter(char),
}

impl Display for SceneEntityIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("scene entity id is empty"),
            Self::LeadingSlash => formatter.write_str("scene entity id must not start with '/'"),
            Self::TrailingSlash => formatter.write_str("scene entity id must not end with '/'"),
            Self::EmptySegment => formatter.write_str("scene entity id has an empty segment"),
            Self::CurrentDirectorySegment => {
                formatter.write_str("scene entity id must not contain '.' segments")
            }
            Self::ParentDirectorySegment => {
                formatter.write_str("scene entity id must not contain '..' segments")
            }
            Self::InvalidCharacter(character) => {
                write!(
                    formatter,
                    "scene entity id contains invalid character '{character}'"
                )
            }
        }
    }
}

impl Error for SceneEntityIdError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneFormatError {
    Json(String),
    Ron(String),
}

impl Display for SceneFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "JSON scene format error: {error}"),
            Self::Ron(error) => write!(formatter, "RON scene format error: {error}"),
        }
    }
}

impl Error for SceneFormatError {}

struct PreparedScene {
    entities: Vec<PreparedSceneEntity>,
    diagnostics: DiagnosticReport,
}

struct PreparedSceneEntity {
    id: SceneEntityId,
    parent: Option<SceneEntityId>,
    components: Vec<PreparedComponent>,
}

fn preflight_scene(document: &SceneDocument, registry: &ComponentRegistry) -> PreparedScene {
    let mut diagnostics = DiagnosticReport::default();
    let mut seen = BTreeSet::<SceneEntityId>::new();
    let mut ids = BTreeSet::<SceneEntityId>::new();

    if document.format_version != SceneDocument::CURRENT_FORMAT_VERSION {
        diagnostics.push(Diagnostic::error(
            "scene.unsupported-format-version",
            format!(
                "scene format version {} is unsupported; expected {}",
                document.format_version,
                SceneDocument::CURRENT_FORMAT_VERSION
            ),
        ));
    }

    for entity in &document.entities {
        if let Err(error) = validate_scene_entity_id(entity.id.as_str()) {
            diagnostics.push(
                Diagnostic::error("scene.invalid-entity-id", error.to_string())
                    .with_entity_id(entity.id.as_str()),
            );
        }
        if !seen.insert(entity.id.clone()) {
            diagnostics.push(
                Diagnostic::error("scene.duplicate-entity-id", "duplicate scene entity id")
                    .with_entity_id(entity.id.as_str()),
            );
        }
        ids.insert(entity.id.clone());
    }

    for entity in &document.entities {
        if let Some(parent) = &entity.parent {
            if let Err(error) = validate_scene_entity_id(parent.as_str()) {
                diagnostics.push(
                    Diagnostic::error("scene.invalid-parent-id", error.to_string())
                        .with_entity_id(entity.id.as_str())
                        .with_field_path("parent"),
                );
            }
            if !ids.contains(parent) {
                diagnostics.push(
                    Diagnostic::error("scene.missing-parent", "parent entity id does not exist")
                        .with_entity_id(entity.id.as_str()),
                );
            }
        }
        if let Some(prefab) = &entity.prefab {
            diagnostics.push(
                Diagnostic::error(
                    "scene.prefab-instance-unsupported",
                    "external prefab source resolution is not implemented in this slice; instantiate PrefabDocument directly",
                )
                .with_entity_id(entity.id.as_str())
                .with_field_path("prefab.source")
                .with_asset_ref(prefab.source.to_string()),
            );
        }
    }

    detect_parent_cycles(document, &mut diagnostics);

    let mut prepared_entities = Vec::new();
    for entity in sorted_entities(document) {
        let mut prepared_components = Vec::new();
        for (component_id, component) in &entity.components {
            let Some(schema) = registry.schema(component_id) else {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.unknown-component",
                        "component type is not registered",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                );
                continue;
            };
            if !schema.serializable {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.component-not-serializable",
                        "component is registered but not scene-serializable",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                );
                continue;
            }
            if component.version != schema.version {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.unsupported-component-version",
                        "component schema version is unsupported",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                );
                continue;
            }

            match registry.preflight_component(component_id, &component.value) {
                Some(Ok(prepared)) => prepared_components.push(prepared),
                Some(Err(error)) => {
                    let mut diagnostic =
                        Diagnostic::error("scene.invalid-component-payload", error.to_string())
                            .with_entity_id(entity.id.as_str())
                            .with_component_id(component_id.as_str());
                    if let Some(field_path) = codec_error_field_path(&error) {
                        diagnostic = diagnostic.with_field_path(field_path);
                    }
                    if let Some(asset_ref) = codec_error_asset_ref(&error) {
                        diagnostic = diagnostic.with_asset_ref(asset_ref);
                    }
                    diagnostics.push(diagnostic);
                }
                None => diagnostics.push(
                    Diagnostic::error(
                        "scene.missing-component-codec",
                        "component has no scene codec",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                ),
            }
        }

        prepared_entities.push(PreparedSceneEntity {
            id: entity.id.clone(),
            parent: entity.parent.clone(),
            components: prepared_components,
        });
    }

    PreparedScene {
        entities: prepared_entities,
        diagnostics,
    }
}

fn codec_error_field_path(error: &ComponentCodecError) -> Option<&str> {
    match error {
        ComponentCodecError::MissingField { field }
        | ComponentCodecError::InvalidField { field, .. }
        | ComponentCodecError::InvalidAssetRef { field, .. } => Some(field.as_str()),
        ComponentCodecError::EntityMissing | ComponentCodecError::Message(_) => None,
    }
}

fn codec_error_asset_ref(error: &ComponentCodecError) -> Option<&str> {
    match error {
        ComponentCodecError::InvalidAssetRef { asset_ref, .. } => Some(asset_ref.as_str()),
        ComponentCodecError::MissingField { .. }
        | ComponentCodecError::InvalidField { .. }
        | ComponentCodecError::EntityMissing
        | ComponentCodecError::Message(_) => None,
    }
}

fn validate_prefab_overrides(
    prefab: &PrefabDocument,
    overrides: &PrefabComponentOverrides,
) -> DiagnosticReport {
    let ids = prefab
        .entities
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let mut diagnostics = DiagnosticReport::default();
    for entity_id in overrides.keys() {
        if !ids.contains(entity_id) {
            diagnostics.push(
                Diagnostic::error(
                    "scene.unknown-prefab-override-entity",
                    "prefab override targets an entity id that does not exist in the prefab",
                )
                .with_entity_id(entity_id.as_str()),
            );
        }
    }
    diagnostics
}

fn sorted_entities(document: &SceneDocument) -> Vec<&SceneEntityRecord> {
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities
}

fn detect_parent_cycles(document: &SceneDocument, diagnostics: &mut DiagnosticReport) {
    let parents = document
        .entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.parent.clone()))
        .collect::<BTreeMap<_, _>>();

    for entity in &document.entities {
        let mut visiting = BTreeSet::new();
        let mut current = Some(entity.id.clone());
        while let Some(id) = current {
            if !visiting.insert(id.clone()) {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.parent-cycle",
                        "scene hierarchy contains a parent cycle",
                    )
                    .with_entity_id(entity.id.as_str()),
                );
                break;
            }
            current = parents.get(&id).and_then(Clone::clone);
        }
    }
}

fn validate_scene_entity_id(id: &str) -> Result<(), SceneEntityIdError> {
    if id.is_empty() {
        return Err(SceneEntityIdError::Empty);
    }
    if id.starts_with('/') {
        return Err(SceneEntityIdError::LeadingSlash);
    }
    if id.ends_with('/') {
        return Err(SceneEntityIdError::TrailingSlash);
    }

    for segment in id.split('/') {
        match segment {
            "" => return Err(SceneEntityIdError::EmptySegment),
            "." => return Err(SceneEntityIdError::CurrentDirectorySegment),
            ".." => return Err(SceneEntityIdError::ParentDirectorySegment),
            _ => {}
        }
    }

    for character in id.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/') {
            continue;
        }
        return Err(SceneEntityIdError::InvalidCharacter(character));
    }

    Ok(())
}

pub fn spawn_child<B: Bundle>(world: &mut World, parent: Entity, bundle: B) -> Entity {
    let child = world.spawn(bundle).id();
    world.entity_mut(child).insert(Parent(parent));
    child
}

pub fn sync_children(world: &mut World) {
    {
        let mut query = world.query::<&mut Children>();
        for mut children in query.iter_mut(world) {
            children.clear();
        }
    }

    let links = {
        let mut query = world.query::<(Entity, &Parent)>();
        query
            .iter(world)
            .map(|(child, parent)| (child, parent.0))
            .collect::<Vec<_>>()
    };

    for (child, parent) in links {
        if world.get_entity(parent).is_err() {
            continue;
        }

        let mut parent_entity = world.entity_mut(parent);
        if let Some(mut children) = parent_entity.get_mut::<Children>() {
            children.push(child);
        } else {
            parent_entity.insert(Children(vec![child]));
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchyPlugin;

impl Plugin for HierarchyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        register_scene_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
        app.add_systems(CoreStage::PostUpdate, sync_children);
    }
}

pub fn register_scene_components(registry: &mut ComponentRegistry) {
    registry
        .register_serializable_component::<Name, _, _>(
            ComponentTypeId::new("nara.scene.Name"),
            ComponentSchemaVersion(1),
            |value| {
                Ok(Name::new(value.as_str().ok_or_else(|| {
                    ComponentCodecError::invalid_field("Name", "string")
                })?))
            },
            |name| Ok(ComponentValue::String(name.as_str().to_string())),
        )
        .expect("nara.scene.Name component registration should be unique");

    registry
        .register_serializable_component::<Visibility, _, _>(
            ComponentTypeId::new("nara.scene.Visibility"),
            ComponentSchemaVersion(1),
            |value| match value.as_str() {
                Some("visible") => Ok(Visibility::Visible),
                Some("hidden") => Ok(Visibility::Hidden),
                _ => Err(ComponentCodecError::invalid_field(
                    "Visibility",
                    "'visible' or 'hidden'",
                )),
            },
            |visibility| {
                Ok(ComponentValue::String(match visibility {
                    Visibility::Visible => "visible".to_string(),
                    Visibility::Hidden => "hidden".to_string(),
                }))
            },
        )
        .expect("nara.scene.Visibility component registration should be unique");
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_reflect::bevy_reflect;
    use nara_reflect::{
        ComponentCodecError, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, Reflect,
    };

    #[derive(Clone, Debug, PartialEq, Component, Reflect)]
    struct TestPosition {
        x: i32,
    }

    #[test]
    fn syncs_parent_child_links() {
        let mut world = World::new();
        let parent = world.spawn((Name::new("parent"),)).id();
        let child = spawn_child(&mut world, parent, (Name::new("child"),));

        sync_children(&mut world);

        let parent_ref = world.get_entity(parent).unwrap();
        let children = parent_ref.get::<Children>().unwrap();
        assert_eq!(children.as_slice(), &[child]);
    }

    #[test]
    fn validates_scene_entity_id_shape() {
        assert_eq!(SceneEntityId::new(""), Err(SceneEntityIdError::Empty));
        assert_eq!(
            SceneEntityId::new("/player"),
            Err(SceneEntityIdError::LeadingSlash)
        );
        assert_eq!(
            SceneEntityId::new("root//player"),
            Err(SceneEntityIdError::EmptySegment)
        );
        assert_eq!(
            SceneEntityId::new("root/../player"),
            Err(SceneEntityIdError::ParentDirectorySegment)
        );
        assert!(SceneEntityId::new("root/player-1").is_ok());
    }

    #[test]
    fn validation_reports_duplicate_missing_parent_cycle_and_unknown_component() {
        let registry = test_registry();
        let id = scene_id("player");
        let missing_parent = scene_id("missing");
        let cycle_a = scene_id("cycle_a");
        let cycle_b = scene_id("cycle_b");
        let unknown_component = ComponentTypeId::new("nara.test.Unknown");
        let document = SceneDocument {
            format_version: SceneDocument::CURRENT_FORMAT_VERSION,
            entities: vec![
                SceneEntityRecord::new(id.clone()),
                SceneEntityRecord::new(id),
                SceneEntityRecord::new(scene_id("orphan")).with_parent(missing_parent),
                SceneEntityRecord::new(cycle_a.clone()).with_parent(cycle_b.clone()),
                SceneEntityRecord::new(cycle_b).with_parent(cycle_a),
                SceneEntityRecord::new(scene_id("unknown")).with_component(
                    unknown_component,
                    SceneComponentRecord::new(ComponentSchemaVersion(1), ComponentValue::Null),
                ),
            ],
        };

        let report = document.validate(&registry);
        let codes = report
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>();

        assert!(codes.contains(&"scene.duplicate-entity-id"));
        assert!(codes.contains(&"scene.missing-parent"));
        assert!(codes.contains(&"scene.parent-cycle"));
        assert!(codes.contains(&"scene.unknown-component"));
        assert!(report.has_errors());
    }

    #[test]
    fn validates_document_format_version() {
        let registry = test_registry();
        let document = SceneDocument {
            format_version: SceneDocument::CURRENT_FORMAT_VERSION + 1,
            entities: vec![SceneEntityRecord::new(scene_id("player"))],
        };

        let report = document.validate(&registry);

        assert!(report.has_errors());
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code.as_str() == "scene.unsupported-format-version")
        );
    }

    #[test]
    fn prefab_default_uses_current_format_version() {
        assert_eq!(
            PrefabDocument::default().format_version,
            PrefabDocument::CURRENT_FORMAT_VERSION
        );
    }

    #[test]
    fn unsupported_prefab_instance_prevents_world_mutation() {
        let registry = test_registry();
        let document = SceneDocument::new([SceneEntityRecord {
            id: scene_id("enemy"),
            parent: None,
            components: BTreeMap::new(),
            prefab: Some(PrefabInstance {
                source: AssetRef::path("prefabs/enemy.ron").unwrap(),
                overrides: BTreeMap::new(),
            }),
        }]);
        let mut world = World::new();
        let before = world.iter_entities().count();

        let report = spawn_scene(&mut world, &registry, &document);

        assert!(report.diagnostics.has_errors());
        assert_eq!(world.iter_entities().count(), before);
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "scene.prefab-instance-unsupported"
                && diagnostic.context.field_path.as_deref() == Some("prefab.source")
                && diagnostic.context.asset_ref.as_deref() == Some("prefabs/enemy.ron")
        }));
    }

    #[test]
    fn invalid_component_payload_does_not_mutate_world() {
        let registry = test_registry();
        let document = SceneDocument::new([SceneEntityRecord::new(scene_id("bad"))
            .with_component(
                position_type_id(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(1),
                    ComponentValue::map([(
                        "x",
                        ComponentValue::String("not a number".to_string()),
                    )]),
                ),
            )]);
        let mut world = World::new();
        let before = world.iter_entities().count();

        let report = spawn_scene(&mut world, &registry, &document);

        assert!(report.diagnostics.has_errors());
        assert_eq!(world.iter_entities().count(), before);
        assert!(report.entity_map.is_empty());
    }

    #[test]
    fn spawns_hierarchy_records_source_and_exports_stable_document() {
        let registry = test_registry();
        let parent_id = scene_id("parent");
        let child_id = scene_id("parent/child");
        let document = SceneDocument::new([
            SceneEntityRecord::new(parent_id.clone())
                .with_component(position_type_id(), position_record(1)),
            SceneEntityRecord::new(child_id.clone())
                .with_parent(parent_id.clone())
                .with_component(position_type_id(), position_record(2)),
        ]);
        let mut world = World::new();
        let mut spawner = SceneSpawner::new();

        let report = spawner.spawn(&mut world, &registry, &document);

        assert!(!report.diagnostics.has_errors());
        assert_eq!(report.entity_map.len(), 2);
        let parent = report.entity_map.get(&parent_id).unwrap();
        let child = report.entity_map.get(&child_id).unwrap();
        assert_eq!(
            world
                .get::<Children>(parent)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            vec![child]
        );
        assert_eq!(
            world.get::<SceneEntitySource>(child).unwrap().entity_id,
            child_id
        );

        let export = export_scene(&world, &registry);

        assert!(!export.diagnostics.has_errors());
        assert_eq!(export.document.entities.len(), 2);
        assert_eq!(
            export
                .document
                .entities
                .iter()
                .map(|entity| entity.id.as_str())
                .collect::<Vec<_>>(),
            vec!["parent", "parent/child"]
        );
        assert_eq!(
            export.document.entities[1]
                .parent
                .as_ref()
                .unwrap()
                .as_str(),
            "parent"
        );
    }

    #[test]
    fn repeated_prefab_spawns_export_with_instance_namespaces() {
        let registry = test_registry();
        let id = scene_id("enemy");
        let document = SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(7))]);
        let mut world = World::new();
        let mut spawner = SceneSpawner::new();

        assert!(
            !spawner
                .spawn(&mut world, &registry, &document)
                .diagnostics
                .has_errors()
        );
        assert!(
            !spawner
                .spawn(&mut world, &registry, &document)
                .diagnostics
                .has_errors()
        );

        let export = export_scene(&world, &registry);
        let ids = export
            .document
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["enemy", "instance_2/enemy"]);
    }

    #[test]
    fn direct_prefab_spawn_supports_whole_component_overrides() {
        let registry = test_registry();
        let id = scene_id("enemy");
        let prefab = PrefabDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]);
        let mut overrides = PrefabComponentOverrides::new();
        overrides.insert(
            id.clone(),
            BTreeMap::from([(position_type_id(), position_record(9))]),
        );
        let mut world = World::new();
        let mut spawner = SceneSpawner::new();

        let report =
            spawner.spawn_prefab_with_overrides(&mut world, &registry, &prefab, &overrides);

        assert!(!report.diagnostics.has_errors());
        let entity = report.entity_map.get(&id).unwrap();
        assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 9);
    }

    #[test]
    fn unknown_prefab_override_entity_prevents_world_mutation() {
        let registry = test_registry();
        let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("enemy"))
            .with_component(position_type_id(), position_record(1))]);
        let mut overrides = PrefabComponentOverrides::new();
        overrides.insert(scene_id("missing"), BTreeMap::new());
        let mut world = World::new();
        let before = world.iter_entities().count();

        let report = spawn_prefab_with_overrides(&mut world, &registry, &prefab, &overrides);

        assert!(report.diagnostics.has_errors());
        assert_eq!(world.iter_entities().count(), before);
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "scene.unknown-prefab-override-entity"
                && diagnostic.context.entity_id.as_deref() == Some("missing")
        }));
    }

    #[test]
    fn export_drops_parent_that_is_not_in_document() {
        let registry = test_registry();
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let child = world
            .spawn((
                Parent(parent),
                SceneEntitySource {
                    instance_id: SceneInstanceId::from_raw(1),
                    entity_id: scene_id("child"),
                },
                TestPosition { x: 3 },
            ))
            .id();

        let export = export_scene(&world, &registry);

        assert_eq!(export.document.entities.len(), 1);
        assert_eq!(export.document.entities[0].id.as_str(), "child");
        assert_eq!(export.document.entities[0].parent, None);
        assert_eq!(world.get::<Parent>(child).unwrap().0, parent);
        assert!(
            export
                .diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code.as_str() == "scene.export-parent-skipped")
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn scene_entity_id_deserialization_validates_shape() {
        let error = serde_json::from_str::<SceneDocument>(
            r#"{"format_version":1,"entities":[{"id":"root/../player","components":{}}]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains(".."));
    }

    fn test_registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry
            .register_serializable_component::<TestPosition, _, _>(
                position_type_id(),
                ComponentSchemaVersion(1),
                |value| {
                    let x = value
                        .get("x")
                        .and_then(ComponentValue::as_i64)
                        .ok_or_else(|| ComponentCodecError::invalid_field("x", "i64"))?;
                    Ok(TestPosition {
                        x: i32::try_from(x)
                            .map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
                    })
                },
                |position| {
                    Ok(ComponentValue::map([(
                        "x",
                        ComponentValue::I64(i64::from(position.x)),
                    )]))
                },
            )
            .unwrap();
        registry
    }

    fn scene_id(id: &str) -> SceneEntityId {
        SceneEntityId::new(id).unwrap()
    }

    fn position_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.test.Position")
    }

    fn position_record(x: i32) -> SceneComponentRecord {
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::I64(i64::from(x)))]),
        )
    }
}
