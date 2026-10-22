use std::collections::{BTreeSet, HashMap, HashSet};

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_identity::WorldEntityLocator;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    effective_component_value,
};
use nara_scene::{
    AuthoringRecordFact, AuthoringRecordLocation, AuthoringRecordReadiness, PrefabAuthoringSession,
    SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePatchReport, ScenePlacement,
};

use crate::{diagnostic, snapshot::WorldIdentitySnapshot};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SceneInspectorState;

impl SceneInspectorState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn model(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneInspectorModel {
        self.model_with_selection(session, registry, None, world_snapshot)
    }

    #[must_use]
    pub fn model_with_selection(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        selected_entity: Option<&SceneEntityId>,
        world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneInspectorModel {
        build_inspector_model(
            InspectorSession::Scene(session),
            Some(registry),
            selected_entity,
            world_snapshot,
        )
    }

    /// Projects one generically published authoring Scene without consulting a Runtime registry.
    #[must_use]
    pub fn authoring_model_with_selection(
        &self,
        session: &SceneAuthoringSession,
        selected_entity: Option<&SceneEntityId>,
    ) -> SceneInspectorModel {
        build_inspector_model(
            InspectorSession::Scene(session),
            None,
            selected_entity,
            None,
        )
    }

    /// Uses the same persistent tree and scalar Inspector for an independent Prefab source.
    #[must_use]
    pub fn prefab_model_with_selection(
        &self,
        session: &PrefabAuthoringSession,
        registry: Option<&ComponentRegistry>,
        selected_entity: Option<&SceneEntityId>,
    ) -> SceneInspectorModel {
        build_inspector_model(
            InspectorSession::Prefab(session),
            registry,
            selected_entity,
            None,
        )
    }

    pub fn apply_command(
        &self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.apply_command_with_patch_apply(session, registry, command, |session, patch| {
            session.apply_patch(patch, registry)
        })
    }

    pub fn apply_command_with_asset_database(
        &self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.apply_command_with_patch_apply(session, registry, command, |session, patch| {
            session.apply_patch_with_asset_database(patch, registry, database)
        })
    }

    /// Applies a field mutation admitted for a generically published authoring
    /// document. The session reclassifies the candidate and proves preservation before commit.
    pub fn apply_authoring_command(
        &self,
        session: &mut SceneAuthoringSession,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        if !matches!(
            &command,
            SceneInspectorCommand::SetField { .. }
                | SceneInspectorCommand::RemoveField { .. }
                | SceneInspectorCommand::SetAssetRefField { .. }
        ) {
            return inspector_authoring_command_error_report();
        }
        let entity = command.target_entity().clone();
        if !document_has_entity(session.document(), &entity) {
            return inspector_entity_error_report(
                "tooling.inspector-missing-entity",
                "inspector command targets an entity that is not in the authoring document",
                &entity,
            );
        }
        let patch = match command.to_patch(session.document()) {
            Ok(patch) => patch,
            Err(diagnostic) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(diagnostic);
                return SceneInspectorCommandReport {
                    applied: false,
                    patch: None,
                    patch_report: None,
                    diagnostics,
                };
            }
        };
        let (entity, component, component_version) = match &command {
            SceneInspectorCommand::SetField {
                entity,
                component,
                component_version,
                ..
            }
            | SceneInspectorCommand::SetAssetRefField {
                entity,
                component,
                component_version,
                ..
            }
            | SceneInspectorCommand::RemoveField {
                entity,
                component,
                component_version,
                ..
            } => (entity, component, component_version),
            _ => unreachable!("the authoring command kind was checked before conversion"),
        };
        let Some(record) = session
            .document()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .and_then(|record| record.components.get(component))
        else {
            return inspector_authoring_component_error_report(
                "tooling.inspector-authoring-component-missing",
                "inspector field command targets a component that is not in the authoring document",
                entity,
                component,
            );
        };
        if record.version != *component_version {
            return inspector_authoring_component_error_report(
                "tooling.inspector-authoring-version-mismatch",
                "inspector field command does not match the persistent component version",
                entity,
                component,
            );
        }
        let patch_report = session.apply_authoring_patch(&patch);
        SceneInspectorCommandReport {
            applied: patch_report.applied,
            patch: Some(patch),
            patch_report: Some(patch_report.clone()),
            diagnostics: patch_report.diagnostics,
        }
    }

    fn apply_command_with_patch_apply(
        &self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
        mut apply_patch: impl FnMut(&mut SceneAuthoringSession, &ScenePatchDocument) -> ScenePatchReport,
    ) -> SceneInspectorCommandReport {
        let entity = command.target_entity().clone();
        if !document_has_entity(session.document(), &entity) {
            return inspector_entity_error_report(
                "tooling.inspector-missing-entity",
                "inspector command targets an entity that is not in the authoring document",
                &entity,
            );
        }

        let patch = match command.to_patch_for_document(session.document(), registry) {
            Ok(patch) => patch,
            Err(diagnostic) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(diagnostic);
                return SceneInspectorCommandReport {
                    applied: false,
                    patch: None,
                    patch_report: None,
                    diagnostics,
                };
            }
        };
        let patch_report = apply_patch(session, &patch);
        SceneInspectorCommandReport {
            applied: patch_report.applied,
            patch: Some(patch),
            patch_report: Some(patch_report.clone()),
            diagnostics: patch_report.diagnostics,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorModel {
    pub selected_entity: Option<SceneEntityId>,
    pub entities: Vec<SceneInspectorEntityRow>,
    pub selected_entity_view: Option<SceneInspectorEntityView>,
    pub world_snapshot: Option<WorldIdentitySnapshot>,
    pub history: SceneAuthoringHistoryStatus,
    pub live_dirty: bool,
    pub diagnostics: DiagnosticReport,
    pub composition: SceneCompositionCapabilities,
}

/// Catalog-derived composition choices, including explicit unavailable reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneCompositionCapabilities {
    pub generic_authoring: bool,
    pub components: Vec<SceneComponentCompositionCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneComponentCompositionCapability {
    pub component: ComponentTypeId,
    pub add_unavailable_reason: Option<&'static str>,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneInspectorEntityRow {
    pub id: SceneEntityId,
    pub name: Option<String>,
    pub parent: Option<SceneEntityId>,
    pub depth: usize,
    pub inspectable_component_count: usize,
    pub has_prefab: bool,
    pub selected: bool,
    pub live_locator: Option<WorldEntityLocator>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorEntityView {
    pub id: SceneEntityId,
    pub parent: Option<SceneEntityId>,
    pub has_prefab: bool,
    pub components: Vec<SceneInspectorComponentView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorComponentView {
    pub component: ComponentTypeId,
    pub document_version: ComponentSchemaVersion,
    pub schema_version: ComponentSchemaVersion,
    pub access: SceneInspectorComponentAccess,
    pub capabilities: BTreeSet<ComponentCapability>,
    pub fields: Vec<SceneInspectorFieldView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneInspectorComponentAccess {
    read_only_reason: Option<SceneInspectorReadOnlyReason>,
}

impl SceneInspectorComponentAccess {
    #[must_use]
    pub const fn editable() -> Self {
        Self {
            read_only_reason: None,
        }
    }

    #[must_use]
    pub const fn read_only(reason: SceneInspectorReadOnlyReason) -> Self {
        Self {
            read_only_reason: Some(reason),
        }
    }

    #[must_use]
    pub const fn is_editable(self) -> bool {
        self.read_only_reason.is_none()
    }

    #[must_use]
    pub const fn read_only_reason(self) -> Option<SceneInspectorReadOnlyReason> {
        self.read_only_reason
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneInspectorReadOnlyReason {
    KnownUnbound,
    UnknownSchema,
    Unmigratable,
    FutureSchema,
}

impl SceneInspectorReadOnlyReason {
    fn from_record_readiness(readiness: AuthoringRecordReadiness) -> Option<Self> {
        match readiness {
            AuthoringRecordReadiness::Available => None,
            AuthoringRecordReadiness::KnownUnbound => Some(Self::KnownUnbound),
            AuthoringRecordReadiness::UnknownSchema => Some(Self::UnknownSchema),
            AuthoringRecordReadiness::Unmigratable => Some(Self::Unmigratable),
            AuthoringRecordReadiness::FutureSchema => Some(Self::FutureSchema),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorFieldView {
    pub id: ComponentFieldId,
    pub aliases: Vec<String>,
    pub path: ComponentFieldPath,
    pub value_kind: ComponentValueKind,
    pub shape: ComponentValueShape,
    pub required: bool,
    pub capabilities: BTreeSet<ComponentCapability>,
    pub default_value: Option<ComponentValue>,
    pub value: Option<ComponentValue>,
    pub state: SceneInspectorFieldState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneInspectorFieldState {
    Present,
    Defaulted,
    Missing,
    InvalidPath(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneInspectorCommand {
    SetField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        value: ComponentValue,
    },
    RemoveField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        asset_ref: AssetRef,
    },
    MoveEntity {
        entity: SceneEntityId,
        to: ScenePlacement,
    },
}

impl SceneInspectorCommand {
    #[must_use]
    pub fn target_entity(&self) -> &SceneEntityId {
        match self {
            Self::SetField { entity, .. }
            | Self::RemoveField { entity, .. }
            | Self::SetAssetRefField { entity, .. }
            | Self::MoveEntity { entity, .. } => entity,
        }
    }

    pub fn to_patch(&self, document: &SceneDocument) -> Result<ScenePatchDocument, Diagnostic> {
        let operation = match self {
            Self::SetField {
                entity,
                component,
                component_version,
                field,
                value,
            } => ScenePatchOperation::SetField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
                value: value.clone(),
            },
            Self::RemoveField {
                entity,
                component,
                component_version,
                field,
            } => ScenePatchOperation::RemoveField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
            },
            Self::SetAssetRefField {
                entity,
                component,
                component_version,
                field,
                asset_ref,
            } => ScenePatchOperation::SetAssetRefField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
                asset_ref: asset_ref.clone(),
            },
            Self::MoveEntity { entity, to } => {
                let Some(from) = ScenePlacement::current(document, entity) else {
                    return Err(diagnostic::with_entity(
                        diagnostic::error(
                            "tooling.inspector-placement-missing",
                            "inspector move target has no validated document placement",
                        ),
                        entity,
                    ));
                };
                ScenePatchOperation::MoveEntity {
                    entity: entity.clone(),
                    from,
                    to: to.clone(),
                }
            }
        };
        Ok(ScenePatchDocument::new([operation]))
    }

    pub(crate) fn to_patch_for_document(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
    ) -> Result<ScenePatchDocument, Diagnostic> {
        let patch = self.to_patch(document)?;
        let Some((entity_id, component_id)) = self.component_target() else {
            return Ok(patch);
        };
        let Some(component) = document
            .entities
            .iter()
            .find(|entity| &entity.id == entity_id)
            .and_then(|entity| entity.components.get(component_id))
        else {
            return Ok(patch);
        };
        let Some(schema) = registry.schema(component_id) else {
            return Ok(patch);
        };
        if component.version == schema.version {
            return Ok(patch);
        }

        let migrated = registry
            .migrate_component_value(component_id, component.version, &component.value)
            .map_err(|_| {
                component_migration_diagnostic(
                    entity_id,
                    component_id,
                    component.version,
                    schema.version,
                )
            })?;
        let mut operations = Vec::with_capacity(patch.operations.len() + 1);
        operations.push(ScenePatchOperation::ReplaceComponent {
            entity: entity_id.clone(),
            component: component_id.clone(),
            value: SceneComponentRecord::new(migrated.version, migrated.value),
        });
        operations.extend(patch.operations);
        Ok(ScenePatchDocument::new(operations))
    }

    fn component_target(&self) -> Option<(&SceneEntityId, &ComponentTypeId)> {
        match self {
            Self::SetField {
                entity, component, ..
            }
            | Self::RemoveField {
                entity, component, ..
            }
            | Self::SetAssetRefField {
                entity, component, ..
            } => Some((entity, component)),
            Self::MoveEntity { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorCommandReport {
    pub applied: bool,
    pub patch: Option<ScenePatchDocument>,
    pub patch_report: Option<ScenePatchReport>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Clone, Copy)]
enum InspectorSession<'a> {
    Scene(&'a SceneAuthoringSession),
    Prefab(&'a PrefabAuthoringSession),
}

#[derive(Clone, Copy)]
struct InspectorDocument<'a> {
    roots: &'a [SceneEntityId],
    entities: &'a [nara_scene::SceneEntityRecord],
}

impl<'a> InspectorSession<'a> {
    fn document(self) -> InspectorDocument<'a> {
        match self {
            Self::Scene(session) => InspectorDocument {
                roots: &session.document().roots,
                entities: &session.document().entities,
            },
            Self::Prefab(session) => InspectorDocument {
                roots: &session.document().roots,
                entities: &session.document().entities,
            },
        }
    }

    fn authoring_record_facts(self) -> Option<&'a [AuthoringRecordFact]> {
        match self {
            Self::Scene(session) => session.authoring_record_facts(),
            Self::Prefab(session) => session.authoring_record_facts(),
        }
    }

    fn authoring_readiness(self) -> Option<nara_scene::AuthoringDocumentReadiness> {
        match self {
            Self::Scene(session) => session.authoring_readiness(),
            Self::Prefab(session) => session.authoring_readiness(),
        }
    }

    fn authoring_component_schema(
        self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Option<&'a ComponentSchema> {
        match self {
            Self::Scene(session) => session.authoring_component_schema(entity, component),
            Self::Prefab(session) => session.authoring_component_schema(entity, component),
        }
    }

    fn catalog_components(self) -> Vec<(&'a ComponentSchema, bool)> {
        match self {
            Self::Scene(session) => session.authoring_catalog_components().collect(),
            Self::Prefab(session) => session.authoring_catalog_components().collect(),
        }
    }

    fn history_status(self) -> SceneAuthoringHistoryStatus {
        match self {
            Self::Scene(session) => session.history_status(),
            Self::Prefab(session) => session.history_status(),
        }
    }

    fn is_live_dirty(self) -> bool {
        match self {
            Self::Scene(session) => session.is_live_dirty(),
            Self::Prefab(_) => false,
        }
    }

    fn live_instance(self) -> Option<&'a nara_identity::SpawnedSceneInstance> {
        match self {
            Self::Scene(session) => session.live_instance(),
            Self::Prefab(_) => None,
        }
    }
}

fn build_inspector_model(
    session: InspectorSession<'_>,
    registry: Option<&ComponentRegistry>,
    selected_entity: Option<&SceneEntityId>,
    world_snapshot: Option<&WorldIdentitySnapshot>,
) -> SceneInspectorModel {
    let mut diagnostics = DiagnosticReport::default();
    if registry.is_some_and(|registry| !registry.is_frozen()) {
        diagnostics.push(diagnostic::error(
            "tooling.inspector-registry-not-frozen",
            "inspector requires a frozen component registry",
        ));
    }
    let document = session.document();
    let authoring_record_facts =
        DirectAuthoringRecordFactIndex::new(session.authoring_record_facts().unwrap_or_default());
    if registry.is_none() && session.authoring_readiness().is_none() {
        diagnostics.push(diagnostic::error(
            "tooling.inspector-authoring-publication-required",
            "registry-free inspector projection requires a published authoring document",
        ));
    }
    let entity_by_id = document
        .entities
        .iter()
        .map(|entity| (entity.id.as_str(), entity))
        .collect::<HashMap<_, _>>();
    let mut selected_record = None;
    let entities = ordered_scene_entities(document, &entity_by_id)
        .into_iter()
        .map(|(entity, parent, depth)| {
            let selected = selected_entity == Some(&entity.id);
            if selected {
                selected_record = Some((entity, parent));
            }
            SceneInspectorEntityRow {
                id: entity.id.clone(),
                name: entity
                    .components
                    .get(&ComponentTypeId::new("nara.scene.Name"))
                    .filter(|record| record.version == ComponentSchemaVersion::ONE)
                    .and_then(|record| record.value.as_str())
                    .map(str::to_owned),
                parent: parent.cloned(),
                depth,
                inspectable_component_count: inspectable_component_count(
                    session,
                    entity,
                    registry,
                    &authoring_record_facts,
                ),
                has_prefab: entity.prefab.is_some(),
                selected,
                live_locator: session
                    .live_instance()
                    .and_then(|instance| instance.locator(&entity.id)),
            }
        })
        .collect::<Vec<_>>();

    let selected_entity_view = match (selected_entity, selected_record) {
        (Some(_), Some((entity, parent))) => Some(build_entity_view(
            session,
            entity,
            parent,
            registry,
            &authoring_record_facts,
            &mut diagnostics,
        )),
        (Some(id), None) => {
            diagnostics.push(diagnostic::with_entity(
                diagnostic::warning(
                    "tooling.inspector-selected-entity-missing",
                    "selected entity is no longer present in the authoring document",
                ),
                id,
            ));
            None
        }
        (None, _) => None,
    };

    SceneInspectorModel {
        selected_entity: selected_entity.cloned(),
        entities,
        selected_entity_view,
        world_snapshot: world_snapshot.cloned(),
        history: session.history_status(),
        live_dirty: session.is_live_dirty(),
        diagnostics,
        composition: build_composition_capabilities(session, registry),
    }
}

fn build_composition_capabilities(
    session: InspectorSession<'_>,
    registry: Option<&ComponentRegistry>,
) -> SceneCompositionCapabilities {
    let generic_authoring = session.authoring_readiness().is_some();
    let schemas = if generic_authoring {
        session.catalog_components()
    } else {
        registry
            .and_then(|registry| registry.schemas().ok())
            .into_iter()
            .flatten()
            .map(|schema| (schema, true))
            .collect()
    };
    let components = schemas
        .into_iter()
        .map(|(schema, available)| {
            let removable = available
                && schema.has_capability(ComponentCapability::Scene)
                && schema.has_capability(ComponentCapability::Edit)
                && schema
                    .fields()
                    .iter()
                    .all(|field| field.has_capability(ComponentCapability::Edit));
            let add_unavailable_reason = if !available {
                Some("No authoring adapter for this exact catalog")
            } else if !removable {
                Some("Component does not admit whole-record data editing")
            } else if schema.authoring_initial_value().is_none() {
                Some("Component has no complete creation template")
            } else {
                None
            };
            SceneComponentCompositionCapability {
                component: schema.id().clone(),
                add_unavailable_reason,
                removable,
            }
        })
        .collect();
    SceneCompositionCapabilities {
        generic_authoring,
        components,
    }
}

fn inspectable_component_count(
    session: InspectorSession<'_>,
    entity: &nara_scene::SceneEntityRecord,
    registry: Option<&ComponentRegistry>,
    authoring_record_facts: &DirectAuthoringRecordFactIndex<'_>,
) -> usize {
    entity
        .components
        .keys()
        .filter(|component_id| {
            if let Some(fact) = authoring_record_facts.get(&entity.id, component_id) {
                if !fact.readiness().is_available() {
                    return true;
                }
                if registry.is_none() {
                    return session
                        .authoring_component_schema(&entity.id, component_id)
                        .is_some_and(|schema| schema.has_capability(ComponentCapability::Inspect));
                }
            }
            registry
                .filter(|registry| registry.is_frozen())
                .and_then(|registry| registry.schema(component_id))
                .is_some_and(|schema| schema.has_capability(ComponentCapability::Inspect))
        })
        .count()
}

fn build_entity_view(
    session: InspectorSession<'_>,
    entity: &nara_scene::SceneEntityRecord,
    parent: Option<&SceneEntityId>,
    registry: Option<&ComponentRegistry>,
    authoring_record_facts: &DirectAuthoringRecordFactIndex<'_>,
    diagnostics: &mut DiagnosticReport,
) -> SceneInspectorEntityView {
    let components = entity
        .components
        .iter()
        .filter_map(|(component_id, component)| {
            if let Some(fact) = authoring_record_facts.get(&entity.id, component_id) {
                if !fact.readiness().is_available() {
                    if fact.readiness() == AuthoringRecordReadiness::KnownUnbound {
                        if let Some(schema) =
                            session.authoring_component_schema(&entity.id, component_id)
                        {
                            if component.version != schema.version()
                                || !schema.has_capability(ComponentCapability::Inspect)
                            {
                                return Some(build_unavailable_component_view(
                                    component_id,
                                    component,
                                    fact,
                                ));
                            }
                            let effective = match effective_component_value(
                                schema,
                                &component.value,
                                ComponentValueMaterializationLimits::default(),
                            ) {
                                Ok(effective) => effective,
                                Err(error) => {
                                    diagnostics.push(inspector_effective_value_diagnostic(
                                        &entity.id,
                                        component_id,
                                        &error,
                                    ));
                                    return None;
                                }
                            };
                            return Some(build_component_view(
                                component_id,
                                component,
                                schema,
                                &component.value,
                                effective.as_ref(),
                                SceneInspectorComponentAccess::read_only(
                                    SceneInspectorReadOnlyReason::KnownUnbound,
                                ),
                            ));
                        }
                    }
                    return Some(build_unavailable_component_view(
                        component_id,
                        component,
                        fact,
                    ));
                }
                if registry.is_none() {
                    let schema = session.authoring_component_schema(&entity.id, component_id)?;
                    if !schema.has_capability(ComponentCapability::Inspect) {
                        return None;
                    }
                    let effective = match effective_component_value(
                        schema,
                        &component.value,
                        ComponentValueMaterializationLimits::default(),
                    ) {
                        Ok(effective) => effective,
                        Err(error) => {
                            diagnostics.push(inspector_effective_value_diagnostic(
                                &entity.id,
                                component_id,
                                &error,
                            ));
                            return None;
                        }
                    };
                    return Some(build_component_view(
                        component_id,
                        component,
                        schema,
                        &component.value,
                        effective.as_ref(),
                        SceneInspectorComponentAccess::editable(),
                    ));
                }
            }
            let registry = registry.filter(|registry| registry.is_frozen())?;
            let Some(schema) = registry.schema(component_id) else {
                diagnostics.push(diagnostic::with_component(
                    diagnostic::with_entity(
                        diagnostic::warning(
                            "tooling.inspector-unknown-component",
                            "inspector cannot find schema for a component on the selected entity",
                        ),
                        &entity.id,
                    ),
                    component_id,
                ));
                return None;
            };
            if !schema.has_capability(ComponentCapability::Inspect) {
                return None;
            }
            match registry.migrate_component_value(
                component_id,
                component.version,
                &component.value,
            ) {
                Ok(migrated) => {
                    let effective = match effective_component_value(
                        schema,
                        &migrated.value,
                        ComponentValueMaterializationLimits::default(),
                    ) {
                        Ok(effective) => effective,
                        Err(error) => {
                            diagnostics.push(inspector_effective_value_diagnostic(
                                &entity.id,
                                component_id,
                                &error,
                            ));
                            return None;
                        }
                    };
                    Some(build_component_view(
                        component_id,
                        component,
                        schema,
                        &migrated.value,
                        effective.as_ref(),
                        SceneInspectorComponentAccess::editable(),
                    ))
                }
                Err(_) => {
                    diagnostics.push(component_migration_diagnostic(
                        &entity.id,
                        component_id,
                        component.version,
                        schema.version,
                    ));
                    None
                }
            }
        })
        .collect();

    SceneInspectorEntityView {
        id: entity.id.clone(),
        parent: parent.cloned(),
        has_prefab: entity.prefab.is_some(),
        components,
    }
}

fn ordered_scene_entities<'document>(
    document: InspectorDocument<'document>,
    entity_by_id: &HashMap<&'document str, &'document nara_scene::SceneEntityRecord>,
) -> Vec<(
    &'document nara_scene::SceneEntityRecord,
    Option<&'document SceneEntityId>,
    usize,
)> {
    let mut pending = document
        .roots
        .iter()
        .rev()
        .map(|root| (root, None, 0usize))
        .collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(document.entities.len());
    let mut visited = HashSet::with_capacity(document.entities.len());
    while let Some((id, parent, depth)) = pending.pop() {
        if !visited.insert(id.as_str()) {
            continue;
        }
        let Some(entity) = entity_by_id.get(id.as_str()).copied() else {
            continue;
        };
        ordered.push((entity, parent, depth));
        pending.extend(
            entity
                .children
                .iter()
                .rev()
                .map(|child| (child, Some(&entity.id), depth.saturating_add(1))),
        );
    }
    ordered
}

fn build_component_view(
    component_id: &ComponentTypeId,
    component: &SceneComponentRecord,
    schema: &ComponentSchema,
    raw_value: &ComponentValue,
    effective_value: &ComponentValue,
    access: SceneInspectorComponentAccess,
) -> SceneInspectorComponentView {
    SceneInspectorComponentView {
        component: component_id.clone(),
        document_version: component.version,
        schema_version: schema.version,
        access,
        capabilities: schema.capabilities.clone(),
        fields: build_field_views(schema, raw_value, effective_value),
    }
}

fn build_unavailable_component_view(
    component_id: &ComponentTypeId,
    component: &SceneComponentRecord,
    fact: &AuthoringRecordFact,
) -> SceneInspectorComponentView {
    SceneInspectorComponentView {
        component: component_id.clone(),
        document_version: component.version,
        schema_version: component.version,
        access: SceneInspectorComponentAccess::read_only(
            SceneInspectorReadOnlyReason::from_record_readiness(fact.readiness())
                .expect("unavailable authoring facts cannot be Available"),
        ),
        capabilities: BTreeSet::new(),
        fields: Vec::new(),
    }
}

struct DirectAuthoringRecordFactIndex<'facts> {
    facts: HashMap<(&'facts str, &'facts str), &'facts AuthoringRecordFact>,
}

impl<'facts> DirectAuthoringRecordFactIndex<'facts> {
    fn new(facts: &'facts [AuthoringRecordFact]) -> Self {
        let facts = facts
            .iter()
            .filter_map(|fact| {
                let AuthoringRecordLocation::EntityComponent { entity } = fact.location() else {
                    return None;
                };
                Some(((entity.as_str(), fact.component().as_str()), fact))
            })
            .collect();
        Self { facts }
    }

    fn get(
        &self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Option<&'facts AuthoringRecordFact> {
        self.facts
            .get(&(entity.as_str(), component.as_str()))
            .copied()
    }
}

fn component_migration_diagnostic(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    source_version: ComponentSchemaVersion,
    target_version: ComponentSchemaVersion,
) -> Diagnostic {
    diagnostic::with_public_u64(
        diagnostic::with_public_u64(
            diagnostic::with_component(
                diagnostic::with_entity(
                    diagnostic::error(
                        "tooling.inspector-component-migration-failed",
                        "inspector could not migrate a component to the current schema",
                    ),
                    entity,
                ),
                component,
            ),
            "source-version",
            u64::from(source_version.get()),
        ),
        "target-version",
        u64::from(target_version.get()),
    )
}

fn build_field_views(
    schema: &ComponentSchema,
    raw_value: &ComponentValue,
    effective_value: &ComponentValue,
) -> Vec<SceneInspectorFieldView> {
    schema
        .fields
        .iter()
        .filter(|field| field.has_capability(ComponentCapability::Inspect))
        .map(|field| build_field_view(field, raw_value, effective_value))
        .collect()
}

fn build_field_view(
    field: &ComponentFieldSchema,
    raw_value: &ComponentValue,
    effective_value: &ComponentValue,
) -> SceneInspectorFieldView {
    match raw_value.get_path(&field.path) {
        Ok(_) => SceneInspectorFieldView {
            id: field.id.clone(),
            aliases: field.aliases.clone(),
            path: field.path.clone(),
            value_kind: field.value_kind(),
            shape: field.shape().clone(),
            required: field.required,
            capabilities: field.capabilities.clone(),
            default_value: field.default_value.clone(),
            value: effective_value.get_path(&field.path).ok().cloned(),
            state: SceneInspectorFieldState::Present,
        },
        Err(raw_error) => {
            let effective = effective_value.get_path(&field.path);
            let state = if effective.is_ok() {
                SceneInspectorFieldState::Defaulted
            } else if field.required {
                SceneInspectorFieldState::InvalidPath(raw_error.to_string())
            } else {
                SceneInspectorFieldState::Missing
            };
            SceneInspectorFieldView {
                id: field.id.clone(),
                aliases: field.aliases.clone(),
                path: field.path.clone(),
                value_kind: field.value_kind(),
                shape: field.shape().clone(),
                required: field.required,
                capabilities: field.capabilities.clone(),
                default_value: field.default_value.clone(),
                value: effective.ok().cloned(),
                state,
            }
        }
    }
}

fn inspector_effective_value_diagnostic(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    _error: &nara_reflect::ComponentCodecError,
) -> Diagnostic {
    diagnostic::with_component(
        diagnostic::with_entity(
            diagnostic::error(
                "tooling.inspector-effective-value-failed",
                "inspector could not materialize a component's effective defaults",
            ),
            entity,
        ),
        component,
    )
}

fn document_has_entity(document: &SceneDocument, entity: &SceneEntityId) -> bool {
    document.entities.iter().any(|record| record.id == *entity)
}

fn inspector_entity_error_report(
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_entity(
        diagnostic::error(code, summary),
        entity,
    ));
    SceneInspectorCommandReport {
        applied: false,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

fn inspector_authoring_command_error_report() -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::error(
        "tooling.inspector-authoring-command-unsupported",
        "generically published authoring documents only accept preservation-safe field edits",
    ));
    SceneInspectorCommandReport {
        applied: false,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

fn inspector_authoring_component_error_report(
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_component(
        diagnostic::with_entity(diagnostic::error(code, summary), entity),
        component,
    ));
    SceneInspectorCommandReport {
        applied: false,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_identity::EntityReference;
    use nara_scene::SceneEntityRecord;

    fn scene_id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).expect("test scene entity id must be valid")
    }

    #[test]
    fn generic_asset_ref_inspector_edit_preserves_unavailable_records_and_history() {
        use nara_reflect::{
            AuthoringSchemaBundle, AuthoringSchemaBundleOwner, PersistentComponentSchemaOwnerId,
        };
        use nara_scene::{AuthoringDocumentSource, SceneDocumentCandidate};

        let resolved =
            nara_reflect::__private::resolve_schema_provider(nara_sprite::SPRITE_SCHEMA_PROVIDER)
                .unwrap();
        let owner =
            PersistentComponentSchemaOwnerId::try_new(nara_sprite::SPRITE_SCHEMA_OWNER_ID.as_str())
                .unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            resolved.current().catalog().clone(),
            resolved
                .predecessor()
                .map(|record| record.catalog().clone()),
        )])
        .unwrap();
        let sprite = ComponentTypeId::new("nara.sprite.Sprite");
        let sprite_schema = resolved
            .current()
            .catalog()
            .components()
            .iter()
            .find(|schema| schema.id() == &sprite)
            .unwrap();
        let sprite_version = sprite_schema.version();
        let sprite_value = sprite_schema.authoring_initial_value().unwrap().clone();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
            resolved.contribution_receipt(),
            "nara.sprite.first-party-authoring-adapter",
            1,
            1,
            [sprite.clone()],
        )
        .unwrap();
        let entity = scene_id("player");
        let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
            .with_component(
                sprite.clone(),
                SceneComponentRecord::new(sprite_version, sprite_value).with_owner(owner),
            )
            .with_component(
                ComponentTypeId::new("project.Unavailable"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7)),
            )]);
        let published = SceneDocumentCandidate::decode_json_bytes(
            original.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/main.scene.json").unwrap()),
            &bundle,
            &[adapter],
        )
        .unwrap()
        .publish();
        let manifest = published.unavailable_manifest().clone();
        let mut session = SceneAuthoringSession::from_published_authoring_document(published);
        let inspector = SceneInspectorState::new();
        let model = inspector.authoring_model_with_selection(&session, Some(&entity));
        let sprite_view = model
            .selected_entity_view
            .unwrap()
            .components
            .into_iter()
            .find(|component| component.component == sprite)
            .unwrap();
        let image = sprite_view
            .fields
            .iter()
            .find(|field| field.id == ComponentFieldId::new("material.image"))
            .unwrap();
        assert!(sprite_view.access.is_editable());
        assert_eq!(image.value_kind, ComponentValueKind::AssetRef);
        assert_eq!(image.value, Some(ComponentValue::Null));

        let reset = SceneInspectorCommand::RemoveField {
            entity: entity.clone(),
            component: sprite.clone(),
            component_version: sprite_version,
            field: ComponentFieldId::new("material.image"),
        };
        let removed = inspector.apply_authoring_command(&mut session, reset.clone());
        assert!(removed.applied, "{:#?}", removed.diagnostics);
        assert!(
            session.document().entities[0].components[&sprite]
                .value
                .get("material")
                .unwrap()
                .get("image")
                .is_none()
        );
        let model = inspector.authoring_model_with_selection(&session, Some(&entity));
        let reset_image = model
            .selected_entity_view
            .unwrap()
            .components
            .into_iter()
            .find(|component| component.component == sprite)
            .unwrap()
            .fields
            .into_iter()
            .find(|field| field.id == ComponentFieldId::new("material.image"))
            .unwrap();
        assert_eq!(reset_image.state, SceneInspectorFieldState::Defaulted);
        assert_eq!(reset_image.value, Some(ComponentValue::Null));
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        let accepted = session.document().clone();
        let revision = session.revision();
        let history = session.history_status();
        let repeated = inspector.apply_authoring_command(&mut session, reset);
        assert!(!repeated.applied);
        assert_eq!(session.document(), &accepted);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &accepted);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);

        let image_ref = AssetRef::path("textures/player.png").unwrap();
        let command = SceneInspectorCommand::SetAssetRefField {
            entity: entity.clone(),
            component: sprite.clone(),
            component_version: sprite_version,
            field: ComponentFieldId::new("material.image"),
            asset_ref: image_ref.clone(),
        };
        let applied = inspector.apply_authoring_command(&mut session, command);
        assert!(applied.applied, "{:#?}", applied.diagnostics);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        assert_eq!(
            session.authoring_record_facts().unwrap()[0].schema_declared_asset_references()[0]
                .asset_ref(),
            &image_ref
        );
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);
        assert!(session.redo_authoring().applied);

        let accepted = session.document().clone();
        let revision = session.revision();
        let history = session.history_status();
        let rejected = inspector.apply_authoring_command(
            &mut session,
            SceneInspectorCommand::SetAssetRefField {
                entity,
                component: sprite,
                component_version: sprite_version,
                field: ComponentFieldId::new("layer"),
                asset_ref: image_ref,
            },
        );
        assert!(!rejected.applied);
        assert_eq!(session.document(), &accepted);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    }

    #[test]
    fn source_local_entity_reference_inspector_edit_exposes_shape_and_history() {
        use nara_reflect::{
            AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldPath,
            ComponentFieldSchema, ComponentSchemaCatalog, PersistentComponentSchemaOwnerId,
        };
        use nara_scene::{AuthoringDocumentSource, SceneDocumentCandidate};

        let owner = PersistentComponentSchemaOwnerId::try_new("studio.links").unwrap();
        let component = ComponentTypeId::new("studio.Link");
        let schema = ComponentSchema::new(component.clone(), "Link", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueShape::EntityRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
            &bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        let source = scene_id("source");
        let target = scene_id("target");
        let other = scene_id("other");
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(source.clone()).with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone(),
                        }),
                    )]),
                )
                .with_owner(owner),
            ),
            SceneEntityRecord::new(target.clone()),
            SceneEntityRecord::new(other.clone()),
        ]);
        let published = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/links.scene.json").unwrap()),
            &bundle,
            &[adapter],
        )
        .unwrap()
        .publish();
        let mut session = SceneAuthoringSession::from_published_authoring_document(published);
        let inspector = SceneInspectorState::new();
        let model = inspector.authoring_model_with_selection(&session, Some(&source));
        let field = &model
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components
            .iter()
            .find(|candidate| candidate.component == component)
            .unwrap()
            .fields[0];
        assert_eq!(field.shape, ComponentValueShape::EntityRef);
        assert!(field.capabilities.contains(&ComponentCapability::Edit));

        let command = SceneInspectorCommand::SetField {
            entity: source.clone(),
            component: component.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("target"),
            value: ComponentValue::EntityReference(EntityReference::SceneLocal { entity: other }),
        };
        let original = session.document().clone();
        let revision = session.revision();
        let applied = inspector.apply_authoring_command(&mut session, command);
        assert!(applied.applied, "{:#?}", applied.diagnostics);
        assert_ne!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_ne!(session.document(), &original);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);
        assert_eq!(session.history_status().redo_depth, 1);
        assert!(session.redo_authoring().applied);

        let before = (
            session.document().clone(),
            session.revision(),
            session.history_status(),
        );
        let rejected = inspector.apply_authoring_command(
            &mut session,
            SceneInspectorCommand::SetField {
                entity: source,
                component,
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("target"),
                value: ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: scene_id("missing"),
                }),
            },
        );
        assert!(!rejected.applied);
        assert!(rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-local-reference-invalid"
        }));
        assert_eq!(
            (
                session.document().clone(),
                session.revision(),
                session.history_status(),
            ),
            before
        );
    }

    #[test]
    fn traversal_follows_ordered_roots_and_children() {
        let root = scene_id("root");
        let child = scene_id("root/child");
        let grandchild = scene_id("root/child/grandchild");
        let sibling_root = scene_id("sibling-root");
        let document = SceneDocument::new(
            [root.clone(), sibling_root.clone()],
            [
                SceneEntityRecord::new(root.clone()).with_children([child.clone()]),
                SceneEntityRecord::new(child.clone()).with_children([grandchild.clone()]),
                SceneEntityRecord::new(grandchild.clone()),
                SceneEntityRecord::new(sibling_root.clone()),
            ],
        );

        let entity_by_id = document
            .entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity))
            .collect::<HashMap<_, _>>();
        let observed = ordered_scene_entities(
            InspectorDocument {
                roots: &document.roots,
                entities: &document.entities,
            },
            &entity_by_id,
        )
        .into_iter()
        .map(|(entity, parent, depth)| (entity.id.clone(), parent.cloned(), depth))
        .collect::<Vec<_>>();

        assert_eq!(
            observed,
            vec![
                (root.clone(), None, 0),
                (child.clone(), Some(root), 1),
                (grandchild, Some(child), 2),
                (sibling_root, None, 0),
            ]
        );
    }
}
