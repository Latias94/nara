//! Inventory of durable declarations. Effective target, binding and version activation belongs
//! to the Prefab compiler, not source admission or Save.

use super::*;
use crate::{PrefabComponentIntent, PrefabOverlay};
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPathError, ComponentPropertyError,
    ComponentValueKind, DeclaredAssetReferenceError,
    collect_declared_field_asset_references_with_limits,
    collect_declared_value_asset_references_with_limits, resolve_component_property_schema_claim,
};

pub(super) fn classify_instance(
    instance: &PrefabInstance,
    anchor: &SceneEntityId,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let Some(overlay) = &instance.overrides else {
        return Ok(());
    };
    let path = [AuthoringOverlayPathSegment::Root {
        source: instance.source.clone(),
        base: overlay.base.clone(),
    }];
    classify_overlay(overlay, anchor, &path, context)
}

fn classify_overlay(
    overlay: &PrefabOverlay,
    anchor: &SceneEntityId,
    path: &[AuthoringOverlayPathSegment],
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    for (index, addition) in overlay.additions.iter().enumerate() {
        for (component, record) in &addition.components {
            classify_record(
                location(
                    anchor,
                    path,
                    AuthoringOverlayRecordSlot::Addition { index },
                    &PrefabEntityTarget::addition(addition.id.clone()),
                ),
                component,
                record,
                context,
            )?;
        }
        if let Some(instance) = &addition.prefab
            && let Some(nested) = &instance.overrides
        {
            let mut nested_path = path.to_vec();
            nested_path.push(AuthoringOverlayPathSegment::Addition {
                index,
                entity: addition.id.clone(),
                source: instance.source.clone(),
                base: nested.base.clone(),
            });
            classify_overlay(nested, anchor, &nested_path, context)?;
        }
    }
    for (index, claim) in overlay.instances.iter().enumerate() {
        if let Some(instance) = &claim.value
            && let Some(nested) = &instance.overrides
        {
            let mut nested_path = path.to_vec();
            nested_path.push(AuthoringOverlayPathSegment::InstanceClaim {
                index,
                target: claim.target.clone(),
                source: instance.source.clone(),
                base: nested.base.clone(),
            });
            classify_overlay(nested, anchor, &nested_path, context)?;
        }
    }
    for (index, intent) in overlay.components.iter().enumerate() {
        match intent {
            PrefabComponentIntent::Set {
                target,
                component,
                record,
                ..
            } => {
                classify_record(
                    location(
                        anchor,
                        path,
                        AuthoringOverlayRecordSlot::Component { index },
                        target,
                    ),
                    component,
                    record,
                    context,
                )?;
            }
            PrefabComponentIntent::Suppress { target, component } => {
                let classified = context.basis.classify_overlay_target(component, None)?;
                retain_target(
                    location(
                        anchor,
                        path,
                        AuthoringOverlayRecordSlot::Component { index },
                        target,
                    ),
                    AuthoringOverlayTargetKind::SuppressComponent,
                    component,
                    None,
                    None,
                    classified.readiness,
                    context,
                );
            }
            PrefabComponentIntent::Properties {
                target,
                component,
                version,
                claims,
            } => {
                for (claim_index, claim) in claims.iter().enumerate() {
                    classify_property(
                        location(
                            anchor,
                            path,
                            AuthoringOverlayRecordSlot::Property {
                                component_index: index,
                                claim_index,
                            },
                            target,
                        ),
                        component,
                        *version,
                        claim,
                        context,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn location(
    anchor: &SceneEntityId,
    path: &[AuthoringOverlayPathSegment],
    slot: AuthoringOverlayRecordSlot,
    target: &PrefabEntityTarget,
) -> AuthoringRecordLocation {
    AuthoringRecordLocation::OverlayComponent {
        anchor: anchor.clone(),
        overlays: path.to_vec(),
        slot,
        target: target.clone(),
    }
}

fn classify_record(
    location: AuthoringRecordLocation,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let mut classified = context.basis.classify_overlay_record(component, record)?;
    let mut references = Vec::new();
    if let Some(schema) = classified
        .schema
        .as_ref()
        .filter(|schema| record.version == schema.version())
    {
        // Field selection/defaults are already owned by Reflect. Inspect only each declared
        // asset-bearing root; an obsolete sibling cannot erase other understood inventory.
        for field in schema
            .fields()
            .iter()
            .filter(|field| field.shape().contains_kind(ComponentValueKind::AssetRef))
        {
            let payload = match record.value.get_path(field.path()) {
                Ok(payload) => Some(payload),
                Err(
                    ComponentFieldPathError::MissingField { .. }
                    | ComponentFieldPathError::IndexOutOfBounds { .. },
                ) => field.default_value(),
                Err(_) => {
                    classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                    classified.adapter = None;
                    continue;
                }
            };
            let Some(payload) = payload else {
                if field.is_required() {
                    classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                    classified.adapter = None;
                }
                continue;
            };
            let limits = context.asset_reference_limits();
            match collect_declared_field_asset_references_with_limits(
                component,
                field,
                payload,
                nara_reflect::EntityReferenceTraversalLimits::new(
                    limits.nodes(),
                    limits.bytes(),
                    limits.depth(),
                ),
            ) {
                Ok(found) => {
                    context.reserve_asset_references(&found)?;
                    references.extend(found);
                }
                Err(error) => {
                    preserve_reference_error(error)?;
                    classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                    classified.adapter = None;
                }
            }
        }
    }
    retain_complete_record(
        location, None, component, record, classified, references, context,
    );
    Ok(())
}

fn classify_property(
    location: AuthoringRecordLocation,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    claim: &PrefabPropertyClaim,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let classified = context
        .basis
        .classify_overlay_target(component, Some(version))?;
    let mut readiness = classified.readiness;
    if let Some(schema) = classified
        .schema
        .as_ref()
        .filter(|schema| version == schema.version())
    {
        match resolve_component_property_schema_claim(
            schema,
            version,
            &claim.path,
            &mut context.property_read_budget,
        ) {
            Ok(selected) => {
                let payload = match &claim.intent {
                    PrefabPropertyIntent::Set(value) => Some(value),
                    PrefabPropertyIntent::Absent if claim.path.steps.is_empty() => {
                        selected.field.default_value()
                    }
                    PrefabPropertyIntent::Absent => {
                        readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                        None
                    }
                };
                if let Some(payload) = payload
                    && selected.shape.contains_kind(ComponentValueKind::AssetRef)
                {
                    let limits = context.asset_reference_limits();
                    let traversal_limits = nara_reflect::EntityReferenceTraversalLimits::new(
                        limits.nodes(),
                        limits.bytes(),
                        limits.depth(),
                    );
                    let references = match &claim.intent {
                        PrefabPropertyIntent::Set(_) => {
                            collect_declared_value_asset_references_with_limits(
                                component,
                                selected.field,
                                selected.shape,
                                payload,
                                traversal_limits,
                            )
                        }
                        PrefabPropertyIntent::Absent => {
                            collect_declared_field_asset_references_with_limits(
                                component,
                                selected.field,
                                payload,
                                traversal_limits,
                            )
                        }
                    };
                    match references {
                        Ok(references) => {
                            context.reserve_asset_references(&references)?;
                            let AuthoringRecordLocation::OverlayComponent { anchor, .. } =
                                &location
                            else {
                                unreachable!("property inventory belongs to its declaring anchor");
                            };
                            context
                                .overlay_asset_references
                                .entry(anchor.clone())
                                .or_default()
                                .extend(references);
                        }
                        Err(error) => {
                            preserve_reference_error(error)?;
                            readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                        }
                    }
                }
            }
            Err(
                ComponentPropertyError::ReadBudgetExceeded
                | ComponentPropertyError::PathDepthLimit
                | ComponentPropertyError::PathByteLimit,
            ) => {
                return Err(asset_reference_budget_rejection());
            }
            Err(_) => readiness = AuthoringRecordReadiness::UnresolvedOverlay,
        }
    }
    retain_target(
        location,
        match &claim.intent {
            PrefabPropertyIntent::Set(_) => AuthoringOverlayTargetKind::SetProperty,
            PrefabPropertyIntent::Absent => AuthoringOverlayTargetKind::AbsentProperty,
        },
        component,
        Some(version),
        Some(claim.clone()),
        readiness,
        context,
    );
    Ok(())
}

fn retain_target(
    location: AuthoringRecordLocation,
    target_kind: AuthoringOverlayTargetKind,
    component: &ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    property: Option<PrefabPropertyClaim>,
    readiness: AuthoringRecordReadiness,
    context: &mut ClassificationContext<'_>,
) {
    if readiness.is_available() {
        return;
    }
    let AuthoringRecordLocation::OverlayComponent { anchor, .. } = &location else {
        unreachable!("declarative targets always belong to an overlay");
    };
    context
        .manifest
        .push(UnavailableRecordManifestEntry::OverlayTarget(
            UnavailableOverlayTarget {
                source: context.source.clone(),
                document_kind: context.kind,
                anchor_placement: context.placements.get(anchor).cloned(),
                location,
                target_kind,
                component: component.clone(),
                version,
                property,
                readiness,
            },
        ));
}

fn preserve_reference_error(
    error: DeclaredAssetReferenceError,
) -> Result<(), AuthoringDocumentPublicationError> {
    // A traversal budget is an admission limit, not dormant intent. Invalid/unavailable meaning
    // remains in the exact source manifest and is diagnosed without preventing repair or Save.
    if matches!(
        error,
        DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::NodeLimit { .. }
                | ComponentEntityReferenceRewriteError::ByteLimit { .. }
                | ComponentEntityReferenceRewriteError::DepthLimit { .. },
            ..
        }
    ) {
        return Err(asset_reference_budget_rejection());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
