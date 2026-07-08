use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::ComponentRegistry;

use crate::{SceneDocument, SceneEntityRecord, ScenePatchDocument};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrefabDocument {
    pub format_version: u32,
    pub entities: Vec<SceneEntityRecord>,
}

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
        let mut document = SceneDocument {
            format_version: self.format_version,
            entities: self.entities.clone(),
        };
        document.canonicalize();
        document
    }

    #[must_use]
    pub fn instantiate_with_patch(
        &self,
        registry: &ComponentRegistry,
        patch: &ScenePatchDocument,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report = patch.apply_to_scene(&mut document, registry);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
    }

    #[must_use]
    pub fn instantiate_with_patch_and_asset_database(
        &self,
        registry: &ComponentRegistry,
        patch: &ScenePatchDocument,
        database: &ProjectAssetDatabase,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report =
            patch.apply_to_scene_with_asset_database(&mut document, registry, database);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PrefabInstantiationReport {
    pub document: Option<SceneDocument>,
    pub inverse: Option<ScenePatchDocument>,
    pub diagnostics: DiagnosticReport,
}

impl PrefabInstantiationReport {
    fn from_patch_report(
        document: SceneDocument,
        patch_report: crate::ScenePatchReport,
    ) -> PrefabInstantiationReport {
        if patch_report.applied {
            return PrefabInstantiationReport {
                document: Some(document),
                inverse: patch_report.inverse,
                diagnostics: patch_report.diagnostics,
            };
        }

        PrefabInstantiationReport {
            document: None,
            inverse: None,
            diagnostics: patch_report.diagnostics,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrefabInstance {
    pub source: AssetRef,
    #[cfg_attr(feature = "serde", serde(default))]
    pub overrides: ScenePatchDocument,
}
