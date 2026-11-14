//! Candidate-local material validation shared by source and processed Scene preparation.

use nara_material::{Material2dAsset, Material2dRecord};
use nara_reflect::{ComponentCodecError, ComponentTypeId, ComponentValue};
use nara_scene::CompiledSceneDocument;

use super::{ProjectAssetCollection, ProjectContentError, ProjectContentErrorKind};

struct SourceMaterialRetention {
    _lease: super::ProjectContentLease,
    _generation: super::ProjectContentGenerationLease,
}

pub(super) fn reserve_source_material_views(
    context: &mut super::LoadContext<'_>,
    collections: &[Box<dyn super::ErasedCollection>],
) -> Result<(), ProjectContentError> {
    let mut bytes = 0usize;
    let mut any = false;
    for collection in collections {
        let Some(collection) = collection
            .as_any()
            .downcast_ref::<ProjectAssetCollection<Material2dAsset>>()
        else {
            continue;
        };
        for value in collection.iter() {
            any = true;
            bytes = bytes
                .checked_add(
                    value
                        .generation()
                        .lifetime_owner_retained_bytes()
                        .ok_or_else(|| {
                            super::overflow_budget(super::ProjectContentBudgetKind::RetainedBytes)
                        })?,
                )
                .ok_or_else(|| {
                    super::overflow_budget(super::ProjectContentBudgetKind::RetainedBytes)
                })?;
        }
    }
    if any {
        bytes = bytes
            .checked_add(
                std::mem::size_of::<SourceMaterialRetention>()
                    + crate::scene_retention::ARC_CONTROL_BLOCK_BYTES,
            )
            .ok_or_else(|| {
                super::overflow_budget(super::ProjectContentBudgetKind::RetainedBytes)
            })?;
        context.add_budget(super::ProjectContentBudgetKind::RetainedBytes, bytes)?;
    }
    Ok(())
}

pub(super) fn attach_source_material_views(
    collections: &mut [Box<dyn super::ErasedCollection>],
    lease: &super::ProjectContentLease,
    generation: &super::ProjectContentGenerationLease,
) {
    let mut retention = None;
    for collection in collections {
        let Some(collection) = collection
            .as_any_mut()
            .downcast_mut::<ProjectAssetCollection<Material2dAsset>>()
        else {
            continue;
        };
        let super::domains::CollectionStorage::Source { products, .. } = &mut collection.storage
        else {
            unreachable!("source content retains source collections");
        };
        for value in products {
            let owner = retention.get_or_insert_with(|| {
                std::sync::Arc::new(SourceMaterialRetention {
                    _lease: lease.clone(),
                    _generation: generation.clone(),
                })
            });
            *value = value.with_lifetime_owner(std::sync::Arc::clone(owner));
        }
    }
}

pub(crate) fn material_record_work_bytes(value: &ComponentValue) -> Option<usize> {
    let cost = value.cost();
    cost.nodes()
        .checked_mul(
            std::mem::size_of::<ComponentValue>()
                .checked_add(crate::scene_retention::BTREE_ENTRY_OVERHEAD)?,
        )?
        .checked_add(cost.logical_bytes())?
        .checked_add(std::mem::size_of::<Material2dRecord>())
}

pub(crate) fn validate_material_record(
    value: &ComponentValue,
    materials: Option<&ProjectAssetCollection<Material2dAsset>>,
) -> Result<(), ComponentCodecError> {
    let record = Material2dRecord::from_component_value(value)?;
    let asset = materials
        .and_then(|materials| materials.get(record.material()))
        .ok_or_else(|| {
            ComponentCodecError::invalid_field("material", "an admitted material product")
        })?;
    record.validate_asset(asset)
}

pub(super) fn validate_source_materials(
    context: &mut super::LoadContext<'_>,
    document: &CompiledSceneDocument,
    collections: &[Box<dyn super::ErasedCollection>],
) -> Result<(), ProjectContentError> {
    let component = ComponentTypeId::new("nara.material2d");
    let materials = collections
        .iter()
        .filter(|collection| {
            collection.domain_type() == std::any::TypeId::of::<nara_material::Material2dDomain>()
        })
        .find_map(|collection| {
            collection
                .as_any()
                .downcast_ref::<ProjectAssetCollection<Material2dAsset>>()
        });
    for entity in document.entities() {
        let Some(record) = entity.components().get(&component) else {
            continue;
        };
        let work =
            context.reserve_work(material_record_work_bytes(record.value()).ok_or_else(
                || super::overflow_budget(super::ProjectContentBudgetKind::WorkBytes),
            )?)?;
        let result = validate_material_record(record.value(), materials).map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetDomain,
                "project.content.material-binding-mismatch",
                "Scene material values do not match the admitted material product",
                "entity",
                entity.id().as_str(),
            )
        });
        context.subtract_budget(super::ProjectContentBudgetKind::WorkBytes, work.reserved);
        result?;
    }
    Ok(())
}
