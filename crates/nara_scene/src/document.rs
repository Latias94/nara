use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Component;
use nara_reflect::{
    ComponentDecodeContext, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};

use crate::{
    PrefabInstance,
    validation::{
        preflight_authoring_scene, preflight_authoring_scene_with_context, preflight_scene,
        preflight_scene_with_context,
    },
};

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

    #[must_use]
    pub fn validate_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_project_asset_database(database);
        preflight_scene_with_context(self, registry, &mut context).diagnostics
    }

    pub(crate) fn validate_authoring(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        preflight_authoring_scene(self, registry).diagnostics
    }

    pub(crate) fn validate_authoring_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_project_asset_database(database);
        preflight_authoring_scene_with_context(self, registry, &mut context).diagnostics
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

pub(crate) fn validate_scene_entity_id(id: &str) -> Result<(), SceneEntityIdError> {
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
