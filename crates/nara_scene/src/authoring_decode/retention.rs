//! Logical ownership of immutable classification, excluding the shared Schema/Adapter catalog.
//!
//! These facts are additional retained copies, not views into the raw source. In particular,
//! default-only asset projections and complete preservation declarations need their own charge.
//! All walks borrow already admitted state; values/declarations reuse bounded raw cost walkers.

use std::mem::size_of;

use super::*;
use crate::{
    PrefabInstanceHop, PrefabReferenceBinding, ScenePlacement, ScenePlacementAnchor,
    ScenePlacementOwner,
    prefab::{PrefabOverlayCostLimits, prefab_instance_cost},
    provenance::{provenance_asset_ref_bytes, provenance_string_bytes},
};
use nara_reflect::{ComponentPropertyKey, ComponentPropertyStep};

const TREE_ENTRY_BYTES: usize = 6 * size_of::<usize>();

impl AuthoringDocumentState {
    pub(crate) fn checked_retained_bytes(&self) -> Option<usize> {
        let mut bytes = size_of::<Self>()
            .checked_add(provenance_asset_ref_bytes(self.source.as_asset_ref())?)?
            .checked_add(self.diagnostics.retained_bytes())?;
        for (anchor, references) in &self.overlay_asset_references {
            bytes = bytes
                .checked_add(TREE_ENTRY_BYTES)?
                .checked_add(size_of::<SceneEntityId>())?
                .checked_add(size_of::<Vec<DeclaredAssetReference>>())?
                .checked_add(provenance_string_bytes(anchor.as_str())?)?
                .checked_add(asset_references_bytes(references)?)?;
        }
        for fact in &self.record_facts {
            bytes = bytes
                .checked_add(size_of::<AuthoringRecordFact>())?
                .checked_add(location_dynamic_bytes(&fact.location)?)?
                .checked_add(owner_bytes(&fact.owner)?)?
                .checked_add(
                    fact.resolved_owner
                        .as_ref()
                        .map_or(Some(0), |owner| provenance_string_bytes(owner.as_str()))?,
                )?
                .checked_add(provenance_string_bytes(fact.component.as_str())?)?
                .checked_add(asset_references_bytes(
                    &fact.schema_declared_asset_references,
                )?)?;
            if let Some(AuthoringAdapterEvidence {
                catalog: AuthoringAdapterCatalog::Data { owner, .. },
                ..
            }) = &fact.adapter
            {
                bytes = bytes.checked_add(provenance_string_bytes(owner.as_str())?)?;
            }
        }
        for (entity, components) in &self.direct_record_facts {
            bytes = bytes
                .checked_add(TREE_ENTRY_BYTES)?
                .checked_add(size_of::<SceneEntityId>())?
                .checked_add(size_of::<BTreeMap<ComponentTypeId, usize>>())?
                .checked_add(provenance_string_bytes(entity.as_str())?)?;
            for component in components.keys() {
                bytes = bytes
                    .checked_add(TREE_ENTRY_BYTES)?
                    .checked_add(size_of::<ComponentTypeId>())?
                    .checked_add(size_of::<usize>())?
                    .checked_add(provenance_string_bytes(component.as_str())?)?;
            }
        }
        for entry in &self.unavailable_manifest.entries {
            bytes = bytes.checked_add(size_of::<UnavailableRecordManifestEntry>())?;
            bytes = bytes.checked_add(match entry {
                UnavailableRecordManifestEntry::Record(record) => {
                    unavailable_record_dynamic_bytes(record, self.limits)?
                }
                UnavailableRecordManifestEntry::OverlayTarget(target) => {
                    let mut retained = provenance_asset_ref_bytes(target.source.as_asset_ref())?
                        .checked_add(location_dynamic_bytes(&target.location)?)?
                        .checked_add(placement_dynamic_bytes(target.anchor_placement.as_ref())?)?
                        .checked_add(provenance_string_bytes(target.component.as_str())?)?;
                    if let Some(property) = &target.property {
                        retained =
                            retained.checked_add(property_dynamic_bytes(property, self.limits)?)?;
                    }
                    retained
                }
            })?;
        }
        for (anchor, instance) in &self.unavailable_manifest.overlays {
            let cost = prefab_instance_cost(instance, overlay_limits(self.limits)).ok()?;
            bytes = bytes
                .checked_add(TREE_ENTRY_BYTES)?
                .checked_add(size_of::<SceneEntityId>())?
                .checked_add(size_of::<PrefabInstance>())?
                .checked_add(provenance_string_bytes(anchor.as_str())?)?
                .checked_add(cost.nodes.checked_mul(512)?)?
                .checked_add(cost.value_cost.nodes().checked_mul(96)?)?
                .checked_add(cost.logical_bytes())?;
        }
        Some(bytes)
    }
}

pub(crate) fn asset_references_bytes(references: &[DeclaredAssetReference]) -> Option<usize> {
    references.iter().try_fold(0usize, |bytes, reference| {
        bytes
            .checked_add(size_of::<DeclaredAssetReference>())?
            .checked_add(provenance_string_bytes(reference.field_id().as_str())?)?
            .checked_add(field_path_dynamic_bytes(reference.path())?)?
            .checked_add(provenance_asset_ref_bytes(reference.asset_ref())?)
    })
}

fn field_path_dynamic_bytes(path: &nara_reflect::ComponentFieldPath) -> Option<usize> {
    path.segments().iter().try_fold(
        path.segments()
            .len()
            .checked_mul(size_of::<ComponentFieldPathSegment>())?,
        |bytes, segment| match segment {
            ComponentFieldPathSegment::Field(name) => {
                bytes.checked_add(provenance_string_bytes(name)?)
            }
            ComponentFieldPathSegment::Index(_) => Some(bytes),
        },
    )
}

fn owner_bytes(owner: &AuthoringRecordOwnerClaim) -> Option<usize> {
    match owner {
        AuthoringRecordOwnerClaim::Declared(owner) => provenance_string_bytes(owner.as_str()),
        AuthoringRecordOwnerClaim::Unclaimed => Some(0),
    }
}

fn location_dynamic_bytes(location: &AuthoringRecordLocation) -> Option<usize> {
    match location {
        AuthoringRecordLocation::EntityComponent { entity } => {
            provenance_string_bytes(entity.as_str())
        }
        AuthoringRecordLocation::OverlayComponent {
            anchor,
            overlays,
            target,
            ..
        } => {
            let mut bytes = provenance_string_bytes(anchor.as_str())?
                .checked_add(target_dynamic_bytes(target)?)?
                .checked_add(
                    overlays
                        .len()
                        .checked_mul(size_of::<AuthoringOverlayPathSegment>())?,
                )?;
            for segment in overlays {
                let (source, base, target_bytes) = match segment {
                    AuthoringOverlayPathSegment::Root { source, base } => (source, base, 0),
                    AuthoringOverlayPathSegment::Addition {
                        entity,
                        source,
                        base,
                        ..
                    } => (source, base, provenance_string_bytes(entity.as_str())?),
                    AuthoringOverlayPathSegment::InstanceClaim {
                        target,
                        source,
                        base,
                        ..
                    } => (source, base, target_dynamic_bytes(target)?),
                };
                bytes = bytes
                    .checked_add(target_bytes)?
                    .checked_add(provenance_asset_ref_bytes(source)?)?
                    .checked_add(base.checked_retained_bytes()?)?;
            }
            Some(bytes)
        }
    }
}

fn target_dynamic_bytes(target: &PrefabEntityTarget) -> Option<usize> {
    let bytes = provenance_string_bytes(target.entity.entity().as_str())?.checked_add(
        target
            .through
            .len()
            .checked_mul(size_of::<PrefabInstanceHop>())?,
    )?;
    target.through.iter().try_fold(bytes, |bytes, hop| {
        bytes
            .checked_add(provenance_string_bytes(hop.anchor.entity().as_str())?)?
            .checked_add(hop.expected_base.checked_retained_bytes()?)
    })
}

pub(super) fn placement_dynamic_bytes(placement: Option<&ScenePlacement>) -> Option<usize> {
    let Some(placement) = placement else {
        return Some(0);
    };
    let bytes = match &placement.owner {
        ScenePlacementOwner::Root => 0,
        ScenePlacementOwner::Parent { parent } => provenance_string_bytes(parent.as_str())?,
    };
    match &placement.anchor {
        ScenePlacementAnchor::Before { sibling } | ScenePlacementAnchor::After { sibling } => {
            bytes.checked_add(provenance_string_bytes(sibling.as_str())?)
        }
        ScenePlacementAnchor::First | ScenePlacementAnchor::Last => Some(bytes),
    }
}

fn unavailable_record_dynamic_bytes(
    record: &UnavailableRecord,
    limits: SceneFileLimits,
) -> Option<usize> {
    let mut bytes = provenance_asset_ref_bytes(record.source.as_asset_ref())?
        .checked_add(location_dynamic_bytes(&record.location)?)?
        .checked_add(placement_dynamic_bytes(record.placement.as_ref())?)?
        .checked_add(placement_dynamic_bytes(record.anchor_placement.as_ref())?)?
        .checked_add(owner_bytes(&record.owner)?)?
        .checked_add(provenance_string_bytes(record.component.as_str())?)?
        .checked_add(value_bytes(&record.value, limits)?)?
        .checked_add(asset_references_bytes(
            &record.schema_declared_asset_references,
        )?)?;
    for reference in &record.entity_references {
        bytes = bytes
            .checked_add(size_of::<UnavailableEntityReference>())?
            .checked_add(
                reference
                    .path
                    .len()
                    .checked_mul(size_of::<UnavailableValuePathSegment>())?,
            )?;
        for segment in &reference.path {
            if let UnavailableValuePathSegment::Field(name) = segment {
                bytes = bytes.checked_add(provenance_string_bytes(name)?)?;
            }
        }
        bytes = bytes.checked_add(match &reference.reference {
            EntityReference::SceneLocal { entity } => provenance_string_bytes(entity.as_str())?,
            EntityReference::Persistent { entity } => {
                provenance_string_bytes(entity.namespace.as_str())?
            }
        })?;
    }
    Some(bytes)
}

fn property_dynamic_bytes(
    property: &PrefabPropertyClaim,
    limits: SceneFileLimits,
) -> Option<usize> {
    let mut bytes = provenance_string_bytes(property.path.root.as_str())?
        .checked_add(
            property
                .path
                .steps
                .len()
                .checked_mul(size_of::<ComponentPropertyStep>())?,
        )?
        .checked_add(
            property
                .bindings
                .len()
                .checked_mul(size_of::<PrefabReferenceBinding>())?,
        )?;
    for step in &property.path.steps {
        bytes = bytes.checked_add(match step {
            ComponentPropertyStep::Member(id) => provenance_string_bytes(id.as_str())?,
            ComponentPropertyStep::MapEntry(key) => provenance_string_bytes(key)?,
            ComponentPropertyStep::ListItem { by, key } => provenance_string_bytes(by.as_str())?
                .checked_add(match key {
                    ComponentPropertyKey::String(key) => provenance_string_bytes(key)?,
                    ComponentPropertyKey::I64(_) | ComponentPropertyKey::U64(_) => 0,
                })?,
        })?;
    }
    for binding in &property.bindings {
        bytes = bytes
            .checked_add(field_path_dynamic_bytes(&binding.slot)?)?
            .checked_add(provenance_string_bytes(binding.expected.as_str())?)?
            .checked_add(target_dynamic_bytes(&binding.target)?)?;
    }
    if let PrefabPropertyIntent::Set(value) = &property.intent {
        bytes = bytes.checked_add(value_bytes(value, limits)?)?;
    }
    Some(bytes)
}

fn value_bytes(value: &ComponentValue, limits: SceneFileLimits) -> Option<usize> {
    let cost = value.cost_up_to(
        limits.component_value_nodes().get(),
        limits.component_value_bytes().get(),
    )?;
    cost.nodes()
        .checked_mul(96)?
        .checked_add(cost.logical_bytes())
}

fn overlay_limits(limits: SceneFileLimits) -> PrefabOverlayCostLimits {
    PrefabOverlayCostLimits {
        nodes: limits.shape().nodes().get(),
        depth: limits.shape().depth().get(),
        declarations: limits.overlay_declarations().get(),
        entities: limits.entities().get(),
        placements: limits.placements().get(),
        components: limits.components().get(),
        prefab_instances: limits.prefab_instances().get(),
        value_nodes: limits.component_value_nodes().get(),
        value_bytes: limits.component_value_bytes().get(),
        logical_bytes: limits.component_value_bytes().get(),
    }
}
