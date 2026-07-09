use std::collections::{BTreeMap, BTreeSet};

use nara_asset::AssetRefExportPolicy;
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_reflect::{ComponentEncodeContext, ComponentRegistry};

use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource,
    hierarchy::Parent,
};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SceneExportReport {
    pub document: SceneDocument,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SceneExportOptions {
    pub asset_ref_export_policy: AssetRefExportPolicy,
}

#[must_use]
pub fn export_scene(world: &World, registry: &ComponentRegistry) -> SceneExportReport {
    export_scene_with_options(world, registry, SceneExportOptions::default())
}

#[must_use]
pub fn export_scene_with_options(
    world: &World,
    registry: &ComponentRegistry,
    options: SceneExportOptions,
) -> SceneExportReport {
    let mut diagnostics = DiagnosticReport::default();
    let encode_context =
        ComponentEncodeContext::new().with_asset_ref_export_policy(options.asset_ref_export_policy);
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
            let Some(encoded) =
                registry.encode_component_with_context(&schema.id, world, entity, &encode_context)
            else {
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
                    Diagnostic::error("scene.export-component-failed", error.to_string())
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
