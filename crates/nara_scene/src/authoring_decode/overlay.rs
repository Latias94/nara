//! Inventory of durable declarations. Effective target, argument and version activation belongs
//! to the Prefab compiler, not source admission or Save.

use super::*;
use crate::{PrefabComponentIntent, PrefabOverlay};
#[cfg(test)]
use nara_reflect::ComponentValueKind;
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPathError, ComponentPropertyError,
    DeclaredAssetReferenceError,
};

pub(super) fn classify_instance(
    instance: &PrefabInstance,
    anchor: &SceneEntityId,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let Some(overlay) = &instance.overrides else {
        return Ok(());
    };
    reclassification::charge(&mut context.admission, "overlay-root-path", || {
        crate::provenance::provenance_asset_ref_bytes(&instance.source)?
            .checked_add(overlay.base.checked_retained_bytes()?)
    })?;
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
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    for (index, addition) in overlay.additions.iter().enumerate() {
        for (component, record) in &addition.components {
            reclassification::charge(&mut context.admission, "overlay-addition-target", || {
                crate::provenance::provenance_string_bytes(addition.id.as_str())
            })?;
            classify_record(
                location(
                    anchor,
                    path,
                    AuthoringOverlayRecordSlot::Addition { index },
                    &PrefabEntityTarget::addition(addition.id.clone()),
                    context,
                )?,
                component,
                record,
                context,
            )?;
        }
        if let Some(instance) = &addition.prefab
            && let Some(nested) = &instance.overrides
        {
            reclassification::charge(&mut context.admission, "overlay-nested-path", || {
                retention::overlay_path_dynamic_bytes(path)?
                    .checked_add(std::mem::size_of::<AuthoringOverlayPathSegment>())?
                    .checked_add(crate::provenance::provenance_string_bytes(
                        addition.id.as_str(),
                    )?)?
                    .checked_add(crate::provenance::provenance_asset_ref_bytes(
                        &instance.source,
                    )?)?
                    .checked_add(nested.base.checked_retained_bytes()?)
            })?;
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
            reclassification::charge(&mut context.admission, "overlay-nested-path", || {
                retention::overlay_path_dynamic_bytes(path)?
                    .checked_add(std::mem::size_of::<AuthoringOverlayPathSegment>())?
                    .checked_add(retention::target_dynamic_bytes(&claim.target)?)?
                    .checked_add(crate::provenance::provenance_asset_ref_bytes(
                        &instance.source,
                    )?)?
                    .checked_add(nested.base.checked_retained_bytes()?)
            })?;
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
                        context,
                    )?,
                    component,
                    record,
                    context,
                )?;
            }
            PrefabComponentIntent::Suppress { target, component } => {
                admit_target_identity(component, context)?;
                let classified = context.basis.classify_overlay_target(component, None)?;
                retain_target(
                    location(
                        anchor,
                        path,
                        AuthoringOverlayRecordSlot::Component { index },
                        target,
                        context,
                    )?,
                    AuthoringOverlayTargetKind::SuppressComponent,
                    component,
                    None,
                    None,
                    classified.readiness,
                    context,
                )?;
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
                            context,
                        )?,
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
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<AuthoringRecordLocation, AuthoringDocumentPublicationError> {
    reclassification::charge(&mut context.admission, "overlay-record-location", || {
        crate::provenance::provenance_string_bytes(anchor.as_str())?
            .checked_add(retention::overlay_path_dynamic_bytes(path)?)?
            .checked_add(retention::target_dynamic_bytes(target)?)
    })?;
    Ok(AuthoringRecordLocation::OverlayComponent {
        anchor: anchor.clone(),
        overlays: path.to_vec(),
        slot,
        target: target.clone(),
    })
}

fn classify_record(
    location: AuthoringRecordLocation,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    reclassification::charge(
        &mut context.admission,
        "overlay-record-classification",
        || reclassification::record_identity_bytes(context.basis, component, record),
    )?;
    let mut classified = context.basis.classify_overlay_record(component, record)?;
    let mut references = Vec::new();
    if let Some(schema) = classified
        .schema
        .as_ref()
        .filter(|schema| record.version == schema.version())
    {
        reclassification::charge_measured(
            &mut context.admission,
            "overlay-contextual-defaults",
            |scratch| {
                retention::contextual_defaults_bytes(
                    &record.contextual_defaults,
                    context.limits,
                    scratch,
                )
            },
        )?;
        if let Err(error) = complete_value::check_contextual_record(
            schema,
            record,
            context.limits,
            &mut context.admission,
        ) {
            preserve_contextual_error(schema, error)?;
            classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
            classified.adapter = None;
        }
        // Reference fields omitted from a complete record can select canonical or contextual
        // defaults. Classify that actual value; a raw required-path walk would mark legal
        // omission as unavailable and lose conversion/preview coverage for the record.
        let effective = match reclassification::selected_effective_value(
            schema,
            record,
            context.limits,
            &mut context.admission,
        ) {
            Ok(value) => Some(value),
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(report)) => {
                return Err(report);
            }
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Value(error)) => {
                preserve_contextual_error(
                    schema,
                    complete_value::ContextualRecordError::Value(error),
                )?;
                classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                classified.adapter = None;
                None
            }
        };
        if let Some(effective) = effective {
            match reclassification::check_reference_grammar(
                schema,
                &effective,
                reference_traversal_limits(context.limits.component_materialization_limits()),
                &mut context.admission,
            ) {
                Ok(()) => {}
                Err(nara_reflect::__private::EntityReferencePlanError::Admission(report)) => {
                    return Err(report);
                }
                Err(nara_reflect::__private::EntityReferencePlanError::Reference(error)) => {
                    preserve_traversal_error(&error)?;
                    classified.readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                    classified.adapter = None;
                }
            }
        }
        // Field selection/defaults are already owned by Reflect. Inspect only each declared
        // asset-bearing root; an obsolete sibling cannot erase other understood inventory.
        for field in schema
            .fields()
            .iter()
            .filter(|field| field.has_capability(ComponentCapability::AssetRef))
        {
            let segments = field.path().segments();
            reclassification::reference_charge(
                &mut context.admission,
                "overlay-asset-field-path",
                segments.len(),
                0,
                0,
            )?;
            // get_path owns prefix paths even on success. The admitted Schema already proves
            // reference capabilities; only this actual lookup's temporary paths need storage.
            reclassification::charge(&mut context.admission, "overlay-asset-field-path", || {
                let bytes = segments.iter().try_fold(0usize, |bytes, segment| {
                    bytes
                        .checked_add(std::mem::size_of::<ComponentFieldPathSegment>())?
                        .checked_add(match segment {
                            ComponentFieldPathSegment::Field(name) => name.len(),
                            ComponentFieldPathSegment::Index(_) => 0,
                        })
                })?;
                bytes.checked_mul(segments.len().checked_add(3)?)
            })?;
            let payload = match record.value.get_path(field.path()) {
                Ok(payload) => Some(payload),
                Err(
                    ComponentFieldPathError::MissingField { .. }
                    | ComponentFieldPathError::IndexOutOfBounds { .. },
                ) => record
                    .contextual_defaults
                    .get(field.id())
                    .or(field.default_value()),
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
            let found =
                nara_reflect::__private::collect_declared_field_asset_references_with_admission(
                    component,
                    field,
                    payload,
                    nara_reflect::EntityReferenceTraversalLimits::new(
                        limits.nodes(),
                        limits.bytes(),
                        limits.depth(),
                    ),
                    |nodes, bytes, allocated| {
                        reclassification::reference_charge(
                            &mut context.admission,
                            "overlay-field-assets",
                            nodes,
                            bytes,
                            allocated,
                        )
                    },
                );
            let found = match found {
                Ok(found) => Ok(found),
                Err(nara_reflect::__private::AssetReferenceCollectionError::Admission(report)) => {
                    return Err(report);
                }
                Err(nara_reflect::__private::AssetReferenceCollectionError::Reference(error)) => {
                    Err(error)
                }
            };
            match found {
                Ok(found) => {
                    context.reserve_asset_references(&found)?;
                    reclassification::reserve_vec(
                        &mut references,
                        found.len(),
                        &mut context.admission,
                        "overlay-asset-merge",
                    )?;
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
    )?;
    Ok(())
}

fn classify_property(
    location: AuthoringRecordLocation,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    claim: &PrefabPropertyClaim,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    admit_target_identity(component, context)?;
    let classified = context
        .basis
        .classify_overlay_target(component, Some(version))?;
    let mut readiness = classified.readiness;
    if let Some(schema) = classified
        .schema
        .as_ref()
        .filter(|schema| version == schema.version())
    {
        reclassification::charge_measured(
            &mut context.admission,
            "overlay-property-resolution",
            |scratch| retention::property_dynamic_bytes_impl(claim, context.limits, scratch),
        )?;
        let selected =
            nara_reflect::__private::resolve_component_property_schema_claim_with_admission(
                schema,
                version,
                &claim.path,
                &mut context.property_read_budget,
                |nodes, bytes, allocated| {
                    reclassification::reference_charge(
                        &mut context.admission,
                        "overlay-property-selector",
                        nodes,
                        bytes,
                        allocated,
                    )
                },
            );
        let selected = match selected {
            Ok(selected) => Ok(selected),
            Err(nara_reflect::__private::ComponentPropertyAdmissionError::Admission(report)) => {
                return Err(report);
            }
            Err(nara_reflect::__private::ComponentPropertyAdmissionError::Property(error)) => {
                Err(error)
            }
        };
        match selected {
            Ok(selected) => {
                let payload = match &claim.intent {
                    PrefabPropertyIntent::Set(value) => Some(value),
                    // Absence exposes the target record's existing contextual default in
                    // its declaration occurrence. This claim does not own that environment.
                    PrefabPropertyIntent::Absent if claim.path.steps.is_empty() => {
                        if selected
                            .field
                            .has_capability(ComponentCapability::EntityRef)
                        {
                            None
                        } else {
                            selected.field.default_value()
                        }
                    }
                    PrefabPropertyIntent::Absent => {
                        readiness = AuthoringRecordReadiness::UnresolvedOverlay;
                        None
                    }
                };
                if let Some(payload) = payload {
                    // The shared reference walker also validates scalar shapes and declared
                    // EntityRef leaves. Keep its typed syntax and admission errors together.
                    let limits = context.asset_reference_limits();
                    let traversal_limits = nara_reflect::EntityReferenceTraversalLimits::new(
                        limits.nodes(),
                        limits.bytes(),
                        limits.depth(),
                    );
                    let references = {
                        let callback = |nodes, bytes, allocated| {
                            reclassification::reference_charge(
                                &mut context.admission,
                                "overlay-property-assets",
                                nodes,
                                bytes,
                                allocated,
                            )
                        };
                        let found = match &claim.intent {
                            PrefabPropertyIntent::Set(_) => nara_reflect::__private::collect_declared_value_asset_references_with_admission(
                                component, selected.field, selected.shape, payload, traversal_limits, callback,
                            ),
                            PrefabPropertyIntent::Absent => nara_reflect::__private::collect_declared_field_asset_references_with_admission(
                                component, selected.field, payload, traversal_limits, callback,
                            ),
                        };
                        match found {
                            Ok(found) => Ok(found),
                            Err(
                                nara_reflect::__private::AssetReferenceCollectionError::Admission(
                                    report,
                                ),
                            ) => return Err(report),
                            Err(
                                nara_reflect::__private::AssetReferenceCollectionError::Reference(
                                    error,
                                ),
                            ) => Err(error),
                        }
                    };
                    match references {
                        Ok(references) if references.is_empty() => {}
                        Ok(references) => {
                            context.reserve_asset_references(&references)?;
                            let AuthoringRecordLocation::OverlayComponent { anchor, .. } =
                                &location
                            else {
                                unreachable!("property inventory belongs to its declaring anchor");
                            };
                            reclassification::charge(
                                &mut context.admission,
                                "overlay-asset-anchor",
                                || {
                                    crate::provenance::provenance_string_bytes(anchor.as_str())?
                                        .checked_add(
                                            std::mem::size_of::<SceneEntityId>()
                                                + std::mem::size_of::<Vec<DeclaredAssetReference>>(
                                                )
                                                + 6 * std::mem::size_of::<usize>(),
                                        )
                                },
                            )?;
                            let output = context
                                .overlay_asset_references
                                .entry(anchor.clone())
                                .or_default();
                            reclassification::reserve_vec(
                                output,
                                references.len(),
                                &mut context.admission,
                                "overlay-asset-vector",
                            )?;
                            output.extend(references);
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
    reclassification::charge_measured(
        &mut context.admission,
        "overlay-property-copy",
        |scratch| retention::property_dynamic_bytes_impl(claim, context.limits, scratch),
    )?;
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
    )?;
    Ok(())
}

fn retain_target(
    location: AuthoringRecordLocation,
    target_kind: AuthoringOverlayTargetKind,
    component: &ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    property: Option<PrefabPropertyClaim>,
    readiness: AuthoringRecordReadiness,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    if readiness.is_available() {
        return Ok(());
    }
    let AuthoringRecordLocation::OverlayComponent { anchor, .. } = &location else {
        unreachable!("declarative targets always belong to an overlay");
    };
    reclassification::charge(&mut context.admission, "overlay-target-copy", || {
        crate::provenance::provenance_asset_ref_bytes(context.source.as_asset_ref())?
            .checked_add(retention::placement_dynamic_bytes(
                context.placements.get(anchor),
            )?)?
            .checked_add(crate::provenance::provenance_string_bytes(
                component.as_str(),
            )?)
    })?;
    reclassification::reserve_vec(
        &mut context.manifest,
        1,
        &mut context.admission,
        "overlay-target-vector",
    )?;
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
    Ok(())
}

fn admit_target_identity(
    component: &ComponentTypeId,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    reclassification::charge(
        &mut context.admission,
        "overlay-target-classification",
        || {
            let Some(entry) = context.basis.schemas.get(component) else {
                return Some(0);
            };
            let bytes = crate::provenance::provenance_string_bytes(entry.owner.as_str())?;
            match &entry.adapter {
                Some(AuthoringAdapterEvidence {
                    catalog: AuthoringAdapterCatalog::Data { owner, .. },
                    ..
                }) => {
                    bytes.checked_add(crate::provenance::provenance_string_bytes(owner.as_str())?)
                }
                _ => Some(bytes),
            }
        },
    )
}

fn preserve_reference_error(
    error: DeclaredAssetReferenceError,
) -> Result<(), AuthoringDocumentPublicationError> {
    // A traversal budget is an admission limit, not dormant intent. Invalid/unavailable meaning
    // remains in the exact source manifest and is diagnosed without preventing repair or Save.
    if let DeclaredAssetReferenceError::Traversal { source, .. } = &error {
        preserve_traversal_error(source)?;
    }
    if matches!(
        error,
        DeclaredAssetReferenceError::Materialization {
            source: nara_reflect::ComponentCodecError::MaterializationNodeLimit { .. }
                | nara_reflect::ComponentCodecError::MaterializationByteLimit { .. }
                | nara_reflect::ComponentCodecError::MaterializationDepthLimit { .. },
            ..
        }
    ) {
        return Err(asset_reference_budget_rejection());
    }
    Ok(())
}

fn preserve_traversal_error<E>(
    error: &ComponentEntityReferenceRewriteError<E>,
) -> Result<(), AuthoringDocumentPublicationError> {
    if matches!(
        error,
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
            | ComponentEntityReferenceRewriteError::ByteLimit { .. }
            | ComponentEntityReferenceRewriteError::DepthLimit { .. }
    ) {
        return Err(asset_reference_budget_rejection());
    }
    Ok(())
}

fn preserve_contextual_error(
    schema: &ComponentSchema,
    error: complete_value::ContextualRecordError,
) -> Result<(), AuthoringDocumentPublicationError> {
    use complete_value::ContextualRecordError;
    use nara_reflect::ComponentCodecError;

    match error {
        ContextualRecordError::Admission(error) => Err(error),
        ContextualRecordError::Reference(error) => preserve_traversal_error(&error),
        ContextualRecordError::Value(
            ComponentCodecError::MaterializationNodeLimit { .. }
            | ComponentCodecError::MaterializationByteLimit { .. }
            | ComponentCodecError::MaterializationDepthLimit { .. },
        ) => Err(asset_reference_budget_rejection()),
        ContextualRecordError::Value(
            ComponentCodecError::InvalidField { .. } | ComponentCodecError::MissingField { .. },
        ) => Ok(()),
        error => Err(complete_value::contextual_record_rejection(schema, error)),
    }
}

#[cfg(test)]
mod tests;
