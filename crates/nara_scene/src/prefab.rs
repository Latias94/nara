use std::collections::BTreeMap;

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentRegistry, ComponentTypeId};

use crate::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord};

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

    #[must_use]
    pub fn validate_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        self.instantiate()
            .validate_with_asset_database(registry, database)
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
