use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::ComponentRegistry;

use crate::{
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref, with_public_locator,
        with_public_u64,
    },
};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
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

    #[must_use]
    pub fn expand_prefabs<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> PrefabExpansionReport {
        self.instantiate().expand_prefabs(registry, resolver)
    }

    #[must_use]
    pub fn expand_prefabs_with_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        self.instantiate()
            .expand_prefabs_with_asset_database(registry, resolver, database)
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
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabInstance {
    pub source: AssetRef,
    #[cfg_attr(feature = "serde", serde(default))]
    pub overrides: ScenePatchDocument,
}

pub trait PrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument>;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryPrefabSourceResolver {
    sources: Vec<(AssetRef, PrefabDocument)>,
}

impl InMemoryPrefabSourceResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_prefab(mut self, source: AssetRef, prefab: PrefabDocument) -> Self {
        self.insert(source, prefab);
        self
    }

    pub fn insert(&mut self, source: AssetRef, prefab: PrefabDocument) -> Option<PrefabDocument> {
        if let Some((_, existing)) = self
            .sources
            .iter_mut()
            .find(|(candidate, _)| candidate == &source)
        {
            return Some(std::mem::replace(existing, prefab));
        }

        self.sources.push((source, prefab));
        None
    }
}

impl PrefabSourceResolver for InMemoryPrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        self.sources
            .iter()
            .find(|(candidate, _)| candidate == source)
            .map(|(_, prefab)| prefab)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabExpansionOptions {
    pub max_depth: usize,
}

impl Default for PrefabExpansionOptions {
    fn default() -> Self {
        Self { max_depth: 32 }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PrefabExpansionReport {
    pub document: Option<SceneDocument>,
    pub diagnostics: DiagnosticReport,
}

impl SceneDocument {
    #[must_use]
    pub fn expand_prefabs<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> PrefabExpansionReport {
        self.expand_prefabs_with_options(registry, resolver, PrefabExpansionOptions::default())
    }

    #[must_use]
    pub fn expand_prefabs_with_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        self.expand_prefabs_with_options_and_asset_database(
            registry,
            resolver,
            PrefabExpansionOptions::default(),
            database,
        )
    }

    #[must_use]
    pub fn expand_prefabs_with_options<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        options: PrefabExpansionOptions,
    ) -> PrefabExpansionReport {
        let mut context = PrefabExpansionContext::new(registry, resolver, options, None);
        context.expand_scene_document(self)
    }

    #[must_use]
    pub fn expand_prefabs_with_options_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        options: PrefabExpansionOptions,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        let mut context = PrefabExpansionContext::new(registry, resolver, options, Some(database));
        context.expand_scene_document(self)
    }
}

struct PrefabExpansionContext<'a, R: PrefabSourceResolver + ?Sized> {
    registry: &'a ComponentRegistry,
    resolver: &'a R,
    options: PrefabExpansionOptions,
    database: Option<&'a ProjectAssetDatabase>,
    stack: Vec<AssetRef>,
    diagnostics: DiagnosticReport,
}

impl<'a, R: PrefabSourceResolver + ?Sized> PrefabExpansionContext<'a, R> {
    fn new(
        registry: &'a ComponentRegistry,
        resolver: &'a R,
        options: PrefabExpansionOptions,
        database: Option<&'a ProjectAssetDatabase>,
    ) -> Self {
        Self {
            registry,
            resolver,
            options,
            database,
            stack: Vec::new(),
            diagnostics: DiagnosticReport::default(),
        }
    }

    fn expand_scene_document(&mut self, document: &SceneDocument) -> PrefabExpansionReport {
        let mut expanded = SceneDocument {
            format_version: document.format_version,
            entities: Vec::new(),
        };
        self.expand_entities(&document.entities, &mut expanded.entities);
        expanded.canonicalize();

        if !self.diagnostics.has_errors() {
            let validation = self.validate_document(&expanded);
            self.diagnostics.extend(validation);
        }

        if self.diagnostics.has_errors() {
            return PrefabExpansionReport {
                document: None,
                diagnostics: std::mem::take(&mut self.diagnostics),
            };
        }

        PrefabExpansionReport {
            document: Some(expanded),
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    fn expand_entities(
        &mut self,
        entities: &[SceneEntityRecord],
        output: &mut Vec<SceneEntityRecord>,
    ) {
        for entity in entities {
            self.expand_entity(entity, output);
        }
    }

    fn expand_entity(&mut self, entity: &SceneEntityRecord, output: &mut Vec<SceneEntityRecord>) {
        let mut anchor = entity.clone();
        let prefab_instance = anchor.prefab.take();
        output.push(anchor);

        if let Some(instance) = prefab_instance {
            self.expand_prefab_instance(&entity.id, &instance, output);
        }
    }

    fn expand_prefab_instance(
        &mut self,
        anchor_id: &SceneEntityId,
        instance: &PrefabInstance,
        output: &mut Vec<SceneEntityRecord>,
    ) {
        if self.stack.len() >= self.options.max_depth {
            self.diagnostics.push(with_asset_ref(
                with_public_u64(
                    with_public_u64(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-depth-exceeded",
                                    "Prefab expansion depth limit was exceeded",
                                ),
                                "entity-id",
                                anchor_id.as_str(),
                            ),
                            "field-path",
                            "prefab.source",
                        ),
                        "current-depth",
                        usize_to_u64(self.stack.len()),
                    ),
                    "maximum-depth",
                    usize_to_u64(self.options.max_depth),
                ),
                "asset-ref",
                &instance.source,
            ));
            return;
        }

        if let Some((cycle_start_index, cycle_from)) = self
            .stack
            .iter()
            .position(|source| source == &instance.source)
            .zip(self.stack.last())
        {
            let diagnostic = with_public_u64(
                with_public_u64(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.prefab-cycle",
                                "Prefab source cycle was detected",
                            ),
                            "entity-id",
                            anchor_id.as_str(),
                        ),
                        "field-path",
                        "prefab.source",
                    ),
                    "cycle-start-index",
                    usize_to_u64(cycle_start_index),
                ),
                "cycle-depth",
                usize_to_u64(
                    self.stack
                        .len()
                        .saturating_sub(cycle_start_index)
                        .saturating_add(1),
                ),
            );
            let diagnostic = with_asset_ref(diagnostic, "cycle-from", cycle_from);
            self.diagnostics
                .push(with_asset_ref(diagnostic, "cycle-to", &instance.source));
            return;
        }

        let Some(prefab) = self.resolver.resolve_prefab(&instance.source) else {
            self.diagnostics.push(with_asset_ref(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.prefab-source-missing",
                            "Prefab source could not be resolved",
                        ),
                        "entity-id",
                        anchor_id.as_str(),
                    ),
                    "field-path",
                    "prefab.source",
                ),
                "asset-ref",
                &instance.source,
            ));
            return;
        };

        self.stack.push(instance.source.clone());

        let mut source_document = prefab.instantiate();
        let patch_report = match self.database {
            Some(database) => instance.overrides.apply_to_scene_with_asset_database(
                &mut source_document,
                self.registry,
                database,
            ),
            None => instance
                .overrides
                .apply_to_scene(&mut source_document, self.registry),
        };

        if !patch_report.applied {
            self.diagnostics.extend(patch_report.diagnostics);
            self.stack.pop();
            return;
        }

        let diagnostics_before = self.diagnostics.stats().published_entries();
        let mut expanded_source = SceneDocument {
            format_version: source_document.format_version,
            entities: Vec::new(),
        };
        self.expand_entities(&source_document.entities, &mut expanded_source.entities);
        expanded_source.canonicalize();

        if self.diagnostics.stats().published_entries() == diagnostics_before {
            let validation = self.validate_document(&expanded_source);
            self.diagnostics.extend(validation);
        }

        if self.diagnostics.stats().published_entries() == diagnostics_before {
            for entity in namespace_prefab_entities(anchor_id, expanded_source.entities) {
                output.push(entity);
            }
        }

        self.stack.pop();
    }

    fn validate_document(&self, document: &SceneDocument) -> DiagnosticReport {
        match self.database {
            Some(database) => document.validate_with_asset_database(self.registry, database),
            None => document.validate(self.registry),
        }
    }
}

fn namespace_prefab_entities(
    anchor_id: &SceneEntityId,
    entities: Vec<SceneEntityRecord>,
) -> Vec<SceneEntityRecord> {
    entities
        .into_iter()
        .map(|mut entity| {
            let local_id = entity.id.clone();
            entity.id = namespace_scene_entity_id(anchor_id, &local_id);
            entity.parent = Some(match entity.parent {
                Some(parent) => namespace_scene_entity_id(anchor_id, &parent),
                None => anchor_id.clone(),
            });
            entity.prefab = None;
            entity
        })
        .collect()
}

fn namespace_scene_entity_id(anchor_id: &SceneEntityId, local_id: &SceneEntityId) -> SceneEntityId {
    SceneEntityId::new(format!("{}/{}", anchor_id.as_str(), local_id.as_str()))
        .expect("namespacing two valid scene entity ids should produce a valid scene entity id")
}
