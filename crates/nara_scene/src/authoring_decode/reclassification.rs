//! Source classification and history preparation share pre-allocation admission.
//!
//! Charges are deltas in the existing Scene logical ownership model, not RSS estimates.
//! The caller owns cumulative work, incumbent/candidate retention and scratch lifetime. No
//! refund is issued here: even discarded scratch has performed work and occupied memory.

use std::mem::size_of;

use super::*;
use crate::authoring::prefab_conversion::retention as source_retention;
use crate::provenance::{provenance_asset_ref_bytes, provenance_string_bytes};

pub(super) type AdmissionCallback<'a> =
    dyn FnMut(&'static str, Option<usize>) -> Result<(), ScenePatchReport> + 'a;
pub(super) type ClassificationCallback<'a> =
    dyn FnMut(&'static str, Option<usize>) -> Result<(), AuthoringDocumentPublicationError> + 'a;

const TREE_ENTRY_BYTES: usize = 6 * size_of::<usize>();

impl AuthoringSchemaBasis {
    /// Source owners prove current declarations only. The Host proves complete owner lineage.
    pub(crate) fn require_declaration_extension_of(
        &self,
        previous: &Self,
    ) -> Result<(), ScenePatchReport> {
        for (component, previous) in &previous.0.schemas {
            if !self.0.schemas.get(component).is_some_and(|next| {
                next.owner == previous.owner
                    && next.schema == previous.schema
                    && next.adapter == previous.adapter
            }) {
                return Err(structure::authoring_structure_rejection(
                    "Schema reclassification requires every previous owner, declaration and Adapter unchanged",
                ));
            }
        }
        Ok(())
    }
}

impl AuthoringDocumentState {
    pub(crate) fn reclassify(
        &self,
        document: &SceneDocument,
        successor: &AuthoringSchemaBasis,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        successor.require_declaration_extension_of(&self.schema_basis())?;
        classify_with_admission(document, self, successor, admit)
    }

    pub(crate) fn reclassify_prefab(
        &self,
        document: &PrefabDocument,
        successor: &AuthoringSchemaBasis,
        admit: &mut AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        successor.require_declaration_extension_of(&self.schema_basis())?;
        // The incumbent structural validator accepts SceneDocument. Admit this nominal
        // projection explicitly; it coexists with the borrowed Prefab and successor facts.
        admit_document(&document.roots, &document.entities, self.limits, admit)?;
        classify_with_admission(&document.instantiate(), self, successor, admit)
    }
}

pub(super) fn classify_with_admission(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    successor: &AuthoringSchemaBasis,
    admit: &mut AdmissionCallback<'_>,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
    admit(
        "classification-source",
        size_of::<AuthoringDocumentState>()
            .checked_add(2 * size_of::<usize>())
            .and_then(|bytes| {
                bytes.checked_add(provenance_asset_ref_bytes(state.source.as_asset_ref())?)
            }),
    )?;
    let mut refusal = None;
    let result = {
        let mut callback = |stage, bytes| {
            admit(stage, bytes).map_err(|report| {
                refusal = Some(report);
                // Keep the exact owner refusal outside the classifier's publication error.
                // In particular, refusal must not allocate diagnostic paths on its way out.
                AuthoringDocumentPublicationError::new(DiagnosticReport::default())
            })
        };
        classify_scene_document_impl(
            document,
            state.source.clone(),
            state.kind,
            Arc::clone(&successor.0),
            state.limits,
            Some(&mut callback),
        )
    };
    match refusal {
        Some(report) => Err(report),
        None => result.map_err(publication_report),
    }
}

pub(super) fn publication_report(error: AuthoringDocumentPublicationError) -> ScenePatchReport {
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    }
}

pub(super) fn charge(
    admission: &mut Option<&mut ClassificationCallback<'_>>,
    stage: &'static str,
    bytes: impl FnOnce() -> Option<usize>,
) -> Result<(), AuthoringDocumentPublicationError> {
    if let Some(admit) = admission.as_deref_mut() {
        admit(stage, bytes())?;
    }
    Ok(())
}

/// Cost measurements can need traversal scratch themselves. Admit that scratch while the
/// existing walker runs, then admit the measured copy before its owning caller allocates it.
/// The ordinary classifier skips both the measurement and all budget-only scanning.
pub(super) fn charge_measured(
    admission: &mut Option<&mut ClassificationCallback<'_>>,
    stage: &'static str,
    measure: impl FnOnce(&mut Option<&mut crate::provenance::CostScratchAdmission<'_>>) -> Option<usize>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let Some(admit) = admission.as_deref_mut() else {
        return Ok(());
    };
    let mut refusal = None;
    let bytes = {
        let mut scratch = |bytes| match admit("classification-cost-walk", bytes) {
            Ok(()) => true,
            Err(error) => {
                refusal = Some(error);
                false
            }
        };
        measure(&mut Some(&mut scratch))
    };
    if let Some(error) = refusal {
        return Err(error);
    }
    admit(stage, bytes)
}

pub(super) fn reserve_vec<T>(
    values: &mut Vec<T>,
    additional: usize,
    admission: &mut Option<&mut ClassificationCallback<'_>>,
    stage: &'static str,
) -> Result<(), AuthoringDocumentPublicationError> {
    let Some(admit) = admission.as_deref_mut() else {
        return Ok(());
    };
    if values.capacity().saturating_sub(values.len()) >= additional {
        return Ok(());
    }
    let capacity = values
        .len()
        .checked_add(additional)
        .map(|needed| needed.max(values.capacity().saturating_mul(2)).max(4));
    admit(
        stage,
        capacity.and_then(|capacity| capacity.checked_mul(size_of::<T>())),
    )?;
    let capacity = capacity.ok_or_else(asset_reference_budget_rejection)?;
    values.reserve_exact(capacity - values.len());
    Ok(())
}

pub(super) fn record_identity_bytes(
    basis: &AuthoringClassificationBasis,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
) -> Option<usize> {
    let mut bytes = record
        .owner()
        .map_or(Some(0), |owner| provenance_string_bytes(owner.as_str()))?;
    if let Some(entry) = basis
        .schemas
        .get(component)
        .filter(|entry| record.owner().is_none_or(|owner| owner == &entry.owner))
    {
        bytes = bytes.checked_add(provenance_string_bytes(entry.owner.as_str())?)?;
        if let Some(AuthoringAdapterEvidence {
            catalog: AuthoringAdapterCatalog::Data { owner, .. },
            ..
        }) = &entry.adapter
        {
            bytes = bytes.checked_add(provenance_string_bytes(owner.as_str())?)?;
        }
    }
    Some(bytes)
}

pub(super) fn fact_copy_bytes(
    location: &AuthoringRecordLocation,
    component: &ComponentTypeId,
    classified: &ClassifiedRecord<'_>,
    references: &[DeclaredAssetReference],
) -> Option<usize> {
    retention::location_dynamic_bytes(location)?
        .checked_add(retention::owner_bytes(&classified.owner)?)?
        .checked_add(provenance_string_bytes(component.as_str())?)?
        .checked_add(retention::asset_references_bytes(references)?)
}

pub(super) fn unavailable_copy_bytes(
    source: &AuthoringDocumentSource,
    location: &AuthoringRecordLocation,
    placement: Option<&crate::ScenePlacement>,
    placements: &BTreeMap<SceneEntityId, crate::ScenePlacement>,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    limits: SceneFileLimits,
    scratch: &mut Option<&mut crate::provenance::CostScratchAdmission<'_>>,
) -> Option<usize> {
    let anchor_placement = match location {
        AuthoringRecordLocation::EntityComponent { .. } => None,
        AuthoringRecordLocation::OverlayComponent { anchor, .. } => placements.get(anchor),
    };
    provenance_asset_ref_bytes(source.as_asset_ref())?
        .checked_add(retention::placement_dynamic_bytes(placement)?)?
        .checked_add(retention::placement_dynamic_bytes(anchor_placement)?)?
        .checked_add(provenance_string_bytes(component.as_str())?)?
        .checked_add(retention::value_bytes_impl(&record.value, limits, scratch)?)
}

pub(super) fn asset_references(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    limits: nara_reflect::EntityReferenceTraversalLimits,
    admission: &mut Option<&mut ClassificationCallback<'_>>,
) -> Result<Vec<DeclaredAssetReference>, AuthoringDocumentPublicationError> {
    nara_reflect::__private::collect_declared_effective_asset_references_with_admission(
        schema,
        version,
        value,
        limits,
        |nodes, bytes, allocated| {
            reference_charge(
                admission,
                "classification-asset-references",
                nodes,
                bytes,
                allocated,
            )
        },
    )
    .map_err(|error| match error {
        nara_reflect::__private::AssetReferenceCollectionError::Admission(report) => report,
        nara_reflect::__private::AssetReferenceCollectionError::Reference(_) => {
            rejection(with_public_locator(
                diagnostic_error(
                    "scene.authoring-declared-asset-reference-invalid",
                    "Persistent authoring record has an invalid schema-declared asset reference",
                ),
                "component-id",
                schema.id().as_str(),
            ))
        }
    })
}

pub(super) fn effective_value<'value>(
    schema: &ComponentSchema,
    value: &'value ComponentValue,
    limits: SceneFileLimits,
    admission: &mut Option<&mut ClassificationCallback<'_>>,
) -> Result<std::borrow::Cow<'value, ComponentValue>, AuthoringDocumentPublicationError> {
    nara_reflect::__private::effective_component_value_with_admission(
        schema,
        value,
        limits.component_materialization_limits(),
        |nodes, bytes, allocated| {
            reference_charge(
                admission,
                "classification-defaults",
                nodes,
                bytes,
                allocated,
            )
        },
    )
    .map_err(|error| match error {
        nara_reflect::__private::EffectiveValueAdmissionError::Admission(report) => report,
        nara_reflect::__private::EffectiveValueAdmissionError::Value(error) => {
            rejection(with_codec_error(
                with_public_locator(
                    diagnostic_error(
                        "scene.authoring-component-value-invalid",
                        "Persistent authoring record exceeds its effective-value policy",
                    ),
                    "component-id",
                    schema.id().as_str(),
                ),
                &error,
                Some(schema),
            ))
        }
    })
}

pub(super) fn complete_value<'value>(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    value: &'value ComponentValue,
    limits: SceneFileLimits,
    admission: &mut Option<&mut ClassificationCallback<'_>>,
) -> Result<std::borrow::Cow<'value, ComponentValue>, AuthoringDocumentPublicationError> {
    let effective = effective_value(schema, value, limits, admission)?;
    nara_reflect::__private::validate_component_value_with_admission(
        schema,
        &effective,
        limits.component_materialization_limits(),
        |nodes, bytes, allocated| {
            reference_charge(
                admission,
                "classification-value-validation",
                nodes,
                bytes,
                allocated,
            )
        },
    )
    .map_err(|error| match error {
        nara_reflect::__private::ComponentValueAdmissionError::Admission(report) => report,
        nara_reflect::__private::ComponentValueAdmissionError::Value(error) => {
            rejection(with_codec_error(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.authoring-component-value-invalid",
                            "Persistent authoring record does not satisfy its declared schema",
                        ),
                        "schema-owner",
                        owner.as_str(),
                    ),
                    "component-id",
                    component.as_str(),
                ),
                &error,
                Some(schema),
            ))
        }
    })?;
    Ok(effective)
}

pub(super) fn reference_charge(
    admission: &mut Option<&mut ClassificationCallback<'_>>,
    stage: &'static str,
    nodes: usize,
    bytes: usize,
    allocated: usize,
) -> Result<(), AuthoringDocumentPublicationError> {
    // Reference visitors report work and allocation separately; the source allowance is a
    // byte-denominated work model and conservatively keeps all admitted scratch until return.
    charge(admission, stage, || {
        nodes.checked_add(bytes)?.checked_add(allocated)
    })
}

pub(super) fn validate_local_references(
    schema: &ComponentSchema,
    effective: &ComponentValue,
    namespace: &BTreeSet<&SceneEntityId>,
    limits: nara_reflect::EntityReferenceTraversalLimits,
    admission: &mut Option<&mut ClassificationCallback<'_>>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let invalid = || {
        rejection(with_public_locator(
            diagnostic_error(
                "scene.authoring-local-reference-invalid",
                "Persistent authoring references do not close over their source namespace",
            ),
            "component-id",
            schema.id().as_str(),
        ))
    };
    let plan = nara_reflect::__private::plan_declared_entity_references_with_admission::<(), _>(
        schema,
        effective,
        limits,
        |nodes, bytes, allocated| {
            reference_charge(
                admission,
                "classification-local-references",
                nodes,
                bytes,
                allocated,
            )
        },
    )
    .map_err(|error| match error {
        nara_reflect::__private::EntityReferencePlanError::Admission(report) => report,
        nara_reflect::__private::EntityReferencePlanError::Reference(_) => invalid(),
    })?;
    plan.visit(effective, |_, reference| {
        if matches!(reference, EntityReference::SceneLocal { entity } if !namespace.contains(entity)) {
            Err(())
        } else { Ok(()) }
    }).map_err(|_| invalid())
}

pub(super) fn validate_prefab_shape(
    document: &PrefabDocument,
    limits: SceneFileLimits,
    admit: Option<&mut AdmissionCallback<'_>>,
) -> Result<(), ScenePatchReport> {
    match admit {
        Some(admit) => crate::format::validate_prefab_publication_shape_with_admission(
            document,
            limits,
            |bytes| admit("publication-buffer", Some(bytes)),
        )
        .map_err(|error| match error {
            crate::format::PublicationShapeAdmissionError::Admission(report) => report,
            crate::format::PublicationShapeAdmissionError::Format(_) => {
                structure::authoring_structure_rejection(
                    "Authoring structure exceeds the persistent Prefab format boundary",
                )
            }
        }),
        None => crate::format::validate_prefab_publication_shape(document, limits).map_err(|_| {
            structure::authoring_structure_rejection(
                "Authoring structure exceeds the persistent Prefab format boundary",
            )
        }),
    }
}

pub(super) fn admit_document(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
    admit: &mut AdmissionCallback<'_>,
) -> Result<(), ScenePatchReport> {
    let bytes = source_retention::document_dynamic_bytes_with_admission(
        roots,
        entities,
        limits,
        |bytes| admit("source-cost-walk", bytes),
    )?;
    admit("complete-document", bytes)?;
    admit(
        "candidate-order",
        entities.len().checked_mul(size_of::<SceneEntityRecord>()),
    )
}

pub(super) fn admit_patch_copy(
    patch: &ScenePatchDocument,
    stage: &'static str,
    admit: &mut AdmissionCallback<'_>,
) -> Result<(), ScenePatchReport> {
    let bytes = source_retention::patch_dynamic_bytes_with_admission(patch, |bytes| {
        admit("patch-cost-walk", bytes)
    })?;
    admit(stage, bytes)
}

pub(super) fn placement_index_bytes(document: &SceneDocument) -> Option<usize> {
    let mut bytes = size_of::<BTreeMap<SceneEntityId, crate::ScenePlacement>>();
    for (parent, siblings) in std::iter::once((None, document.roots.as_slice())).chain(
        document
            .entities
            .iter()
            .map(|entity| (Some(&entity.id), entity.children.as_slice())),
    ) {
        if let Some(parent) = parent {
            bytes = bytes.checked_add(provenance_string_bytes(parent.as_str())?)?;
        }
        for (index, entity) in siblings.iter().enumerate() {
            bytes = bytes
                .checked_add(
                    TREE_ENTRY_BYTES
                        + size_of::<SceneEntityId>()
                        + size_of::<crate::ScenePlacement>(),
                )?
                .checked_add(provenance_string_bytes(entity.as_str())?)?;
            if let Some(parent) = parent {
                bytes = bytes.checked_add(provenance_string_bytes(parent.as_str())?)?;
            }
            if index > 0 {
                bytes =
                    bytes.checked_add(provenance_string_bytes(siblings[index - 1].as_str())?)?;
            }
        }
    }
    Some(bytes)
}

pub(super) fn structure_bytes(document: &SceneDocument) -> Option<usize> {
    // Match validate_scene_placement's actual collections: owned IDs, owner and parent maps,
    // completed IDs, per-chain positions/path/current, and each parent's local-child set.
    // Charge cumulative chain storage (each valid source ID enters one unfinished chain).
    let mut bytes = placement_index_bytes(document)?;
    for entity in &document.entities {
        let id = provenance_string_bytes(entity.id.as_str())?;
        bytes = bytes
            // Classification's borrowed namespace.
            .checked_add(TREE_ENTRY_BYTES + size_of::<&SceneEntityId>())?
            // Validation: ids, owners, parents, completed, positions, path, current.
            .checked_add(TREE_ENTRY_BYTES + size_of::<SceneEntityId>())?
            .checked_add(id)?
            .checked_add(
                TREE_ENTRY_BYTES + size_of::<SceneEntityId>() + size_of::<Option<SceneEntityId>>(),
            )?
            .checked_add(id)?
            .checked_add(
                TREE_ENTRY_BYTES + size_of::<SceneEntityId>() + size_of::<Option<SceneEntityId>>(),
            )?
            .checked_add(id)?
            .checked_add(TREE_ENTRY_BYTES + size_of::<SceneEntityId>())?
            .checked_add(id)?
            .checked_add(TREE_ENTRY_BYTES + size_of::<SceneEntityId>() + size_of::<usize>())?
            .checked_add(id)?
            .checked_add(size_of::<SceneEntityId>())?
            .checked_add(id)?
            .checked_add(size_of::<SceneEntityId>())?
            .checked_add(id)?;
        for child in &entity.children {
            bytes = bytes
                // Parent values in owners and parents, and traversal's next current ID.
                .checked_add(id)?
                .checked_add(id)?
                .checked_add(id)?
                .checked_add(TREE_ENTRY_BYTES + size_of::<SceneEntityId>())?
                .checked_add(provenance_string_bytes(child.as_str())?)?;
        }
    }
    Some(bytes)
}

pub(super) fn overlay_copies_bytes(
    document: &SceneDocument,
    limits: SceneFileLimits,
    scratch: &mut Option<&mut crate::provenance::CostScratchAdmission<'_>>,
) -> Option<usize> {
    let mut bytes = 0usize;
    for entity in &document.entities {
        if let Some(instance) = &entity.prefab {
            let cost = match scratch.as_deref_mut() {
                Some(admit) => crate::prefab::prefab_instance_cost_with_admission(
                    instance,
                    retention::overlay_limits(limits),
                    admit,
                )
                .ok()??,
                None => {
                    crate::prefab::prefab_instance_cost(instance, retention::overlay_limits(limits))
                        .ok()?
                }
            };
            bytes = bytes
                .checked_add(
                    TREE_ENTRY_BYTES + size_of::<SceneEntityId>() + size_of::<PrefabInstance>(),
                )?
                .checked_add(provenance_string_bytes(entity.id.as_str())?)?
                .checked_add(cost.nodes.checked_mul(512)?)?
                .checked_add(cost.value_cost.nodes().checked_mul(96)?)?
                .checked_add(cost.logical_bytes())?;
        }
    }
    Some(bytes)
}

pub(super) fn direct_index_bytes(facts: &[AuthoringRecordFact]) -> Option<usize> {
    let mut bytes = 0usize;
    for fact in facts {
        if let AuthoringRecordLocation::EntityComponent { entity } = &fact.location {
            // One outer row per fact is a conservative bound on the distinct entity rows.
            bytes = bytes
                .checked_add(
                    TREE_ENTRY_BYTES
                        + size_of::<SceneEntityId>()
                        + size_of::<BTreeMap<ComponentTypeId, usize>>(),
                )?
                .checked_add(provenance_string_bytes(entity.as_str())?)?
                .checked_add(TREE_ENTRY_BYTES + size_of::<ComponentTypeId>() + size_of::<usize>())?
                .checked_add(provenance_string_bytes(fact.component.as_str())?)?;
        }
    }
    Some(bytes)
}

pub(super) fn diagnostics_bytes(entries: &[UnavailableRecordManifestEntry]) -> Option<usize> {
    entries.iter().try_fold(0usize, |bytes, entry| {
        let (source, component, readiness, location) = match entry {
            UnavailableRecordManifestEntry::Record(record) => (
                &record.source,
                &record.component,
                record.readiness,
                &record.location,
            ),
            UnavailableRecordManifestEntry::OverlayTarget(target) => (
                &target.source,
                &target.component,
                target.readiness,
                &target.location,
            ),
        };
        // This builds only static identity/summary handles and an empty Vec, with no allocation.
        let diagnostic = unavailable_readiness_diagnostic(readiness);
        let mut dynamic = diagnostic
            .code()
            .as_str()
            .len()
            .checked_add(diagnostic.summary().as_str().len())?
            .checked_add(provenance_string_bytes(component.as_str())?)?
            .checked_add("component-id".len() + "source".len())?
            .checked_add(match source.as_asset_ref() {
                AssetRef::Path(path) => provenance_string_bytes(path.as_str())?,
                // UUID formatting has one temporary string and the copied public identifier.
                AssetRef::StableId(_) => {
                    36 + provenance_string_bytes("00000000-0000-0000-0000-000000000000")?
                }
            })?;
        let fields = match location {
            AuthoringRecordLocation::EntityComponent { entity } => {
                dynamic = dynamic
                    .checked_add("entity-id".len())?
                    .checked_add(provenance_string_bytes(entity.as_str())?)?;
                3usize
            }
            AuthoringRecordLocation::OverlayComponent {
                anchor,
                target,
                slot,
                ..
            } => {
                let kind = match slot {
                    AuthoringOverlayRecordSlot::Addition { .. } => "addition",
                    AuthoringOverlayRecordSlot::Component { .. } => "component",
                    AuthoringOverlayRecordSlot::Property { .. } => "property",
                };
                dynamic = dynamic
                    .checked_add(
                        "anchor-id".len()
                            + "entity-id".len()
                            + "overlay-depth".len()
                            + "overlay-declaration".len()
                            + "declaration-index".len(),
                    )?
                    .checked_add(provenance_string_bytes(anchor.as_str())?)?
                    .checked_add(provenance_string_bytes(target.entity.entity().as_str())?)?
                    .checked_add(provenance_string_bytes(kind)?)?
                    .checked_add(2 * size_of::<u64>())?;
                7usize
            }
        };
        // Draft Vec growth (4 then 8), apply_limits' replacement Vec, report slots. Include
        // logical field separators retained by DiagnosticReport in addition to owned text.
        let draft_slots = if fields <= 4 { 4 } else { 4 + 8 };
        bytes
            .checked_add(dynamic)?
            .checked_add(fields)?
            .checked_add(
                (draft_slots + fields).checked_mul(size_of::<nara_diagnostic::DiagnosticField>())?,
            )?
            .checked_add(size_of::<nara_diagnostic::Diagnostic>())
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod measurement_tests {
    use super::*;

    #[test]
    fn raw_measurements_admit_scratch_before_the_copy_charge() {
        let entity = SceneEntityId::new("anchor").unwrap();
        let component = ComponentTypeId::new("test.Measurement");
        let record = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::List(vec![ComponentValue::Null]),
        );
        let location = AuthoringRecordLocation::EntityComponent {
            entity: entity.clone(),
        };
        let source =
            AuthoringDocumentSource::new(AssetRef::path("scenes/measurement.scene.json").unwrap());
        let placements = BTreeMap::new();
        let mut anchor = SceneEntityRecord::new(entity);
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("prefabs/measurement.prefab.json").unwrap(),
            overrides: None,
        });
        let document = SceneDocument::from_ordered_roots([anchor]);
        let property = PrefabPropertyClaim {
            path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new("value")),
            intent: PrefabPropertyIntent::Set(ComponentValue::List(vec![ComponentValue::Null])),
            bindings: Vec::new(),
        };
        for measurement in 0..3 {
            let mut stages = Vec::new();
            let mut callback = |stage, bytes: Option<usize>| {
                stages.push(stage);
                assert!(bytes.is_some_and(|bytes| bytes > 0));
                Err(AuthoringDocumentPublicationError::new(
                    DiagnosticReport::default(),
                ))
            };
            let result =
                charge_measured(
                    &mut Some(&mut callback),
                    "copy",
                    |scratch| match measurement {
                        0 => unavailable_copy_bytes(
                            &source,
                            &location,
                            None,
                            &placements,
                            &component,
                            &record,
                            SceneFileLimits::default(),
                            scratch,
                        ),
                        1 => overlay_copies_bytes(&document, SceneFileLimits::default(), scratch),
                        _ => retention::property_dynamic_bytes_impl(
                            &property,
                            SceneFileLimits::default(),
                            scratch,
                        ),
                    },
                );
            assert!(result.is_err());
            assert_eq!(stages, ["classification-cost-walk"]);
        }
    }

    #[test]
    fn ordinary_classification_skips_cost_measurement() {
        charge_measured(&mut None, "copy", |_| panic!("budget-only measurement ran")).unwrap();
    }
}
