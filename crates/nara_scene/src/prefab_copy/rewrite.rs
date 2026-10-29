//! Schema interpretation and explicit source-fact substitution for complete Prefab documents.

use std::borrow::Cow;

use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, ComponentPropertyAdmissionError,
        ComponentValueAdmissionError, EffectiveValueAdmissionError,
        collect_declared_effective_asset_references_with_admission,
        collect_declared_value_asset_references_with_admission,
        effective_component_value_with_admission,
        resolve_component_property_schema_claim_with_admission,
        validate_component_value_with_admission,
    },
    AssetReferenceRewriteError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, DeclaredAssetReference, rewrite_declared_asset_references_with_admission,
};

use super::{CopyBudget, CopyMapping, PrefabCopyError, PrefabCopySchemaError};
use crate::{
    AuthoringSchemaBasis, PrefabComponentIntent, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabPlacementParent, PrefabPlacementSlot, PrefabPropertyIntent,
    PrefabReferenceBinding, SceneComponentRecord,
};

pub(super) fn copy_document(
    document: &mut PrefabDocument,
    source: usize,
    basis: &AuthoringSchemaBasis,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    for entity in &mut document.entities {
        for (component, record) in &mut entity.components {
            copy_record(component, record, source, basis, mapping, budget)?;
        }
        if let Some(instance) = &mut entity.prefab {
            copy_instance(instance, source, basis, mapping, budget)?;
        }
    }
    Ok(())
}

fn schema<'a>(
    component: &ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    source: usize,
    basis: &'a AuthoringSchemaBasis,
    budget: &mut CopyBudget,
) -> Result<&'a ComponentSchema, PrefabCopyError> {
    budget.charge(1, component.as_str().len(), 0)?;
    let reject = |reason| PrefabCopyError::Schema { source, reason };
    let schema = basis
        .schema(component)
        .ok_or_else(|| reject(PrefabCopySchemaError::UnknownComponent))?;
    if version.is_some_and(|version| version != schema.version()) {
        return Err(reject(PrefabCopySchemaError::VersionMismatch));
    }
    if !basis.has_authoring_adapter(component) {
        return Err(reject(PrefabCopySchemaError::AdapterUnavailable));
    }
    Ok(schema)
}

fn copy_record(
    component: &ComponentTypeId,
    record: &mut SceneComponentRecord,
    source: usize,
    basis: &AuthoringSchemaBasis,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    let schema = schema(component, Some(record.version), source, basis, budget)?;
    if record
        .owner()
        .is_some_and(|owner| basis.component_schema_owner(component) != Some(owner))
    {
        return Err(PrefabCopyError::Schema {
            source,
            reason: PrefabCopySchemaError::OwnerMismatch,
        });
    }
    // Preserve the current optional owner claim; an explicit claim must match the admitted owner.
    let limits = budget.materialization()?;
    let effective = effective_component_value_with_admission(
        schema,
        &record.value,
        limits,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        EffectiveValueAdmissionError::Admission(error) => error,
        EffectiveValueAdmissionError::Value(error) => PrefabCopyError::Value { source, error },
    })?;
    if let Cow::Owned(value) = effective {
        record.value = value;
    }
    validate_component_value_with_admission(
        schema,
        &record.value,
        limits,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        ComponentValueAdmissionError::Admission(error) => error,
        ComponentValueAdmissionError::Value(error) => PrefabCopyError::Value { source, error },
    })?;
    let traversal = budget.traversal()?;
    let references = collect_declared_effective_asset_references_with_admission(
        schema,
        record.version,
        &record.value,
        traversal,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| collection_error(source, error))?;
    rewrite_assets(&mut record.value, &references, source, mapping, budget)?;
    budget.value(&record.value, source)
}

fn copy_instance(
    instance: &mut PrefabInstance,
    source: usize,
    basis: &AuthoringSchemaBasis,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    if !instance.source.is_primary() {
        return Err(PrefabCopyError::AuxiliaryPrefabSource { source });
    }
    instance.source = mapping.asset(&instance.source, source, budget)?;
    let Some(overlay) = &mut instance.overrides else {
        return Ok(());
    };
    mapping.key(&mut overlay.base, source, budget)?;
    for addition in &mut overlay.additions {
        for (component, record) in &mut addition.components {
            copy_record(component, record, source, basis, mapping, budget)?;
        }
        for bindings in addition.bindings.values_mut() {
            copy_bindings(bindings, source, mapping, budget)?;
        }
        if let Some(instance) = &mut addition.prefab {
            copy_instance(instance, source, basis, mapping, budget)?;
        }
    }
    for claim in &mut overlay.instances {
        copy_target(&mut claim.target, source, mapping, budget)?;
        if let Some(instance) = &mut claim.value {
            copy_instance(instance, source, basis, mapping, budget)?;
        }
    }
    for intent in &mut overlay.components {
        match intent {
            PrefabComponentIntent::Set {
                target,
                component,
                record,
                bindings,
                ..
            } => {
                copy_target(target, source, mapping, budget)?;
                copy_record(component, record, source, basis, mapping, budget)?;
                copy_bindings(bindings, source, mapping, budget)?;
            }
            PrefabComponentIntent::Suppress { target, component } => {
                copy_target(target, source, mapping, budget)?;
                schema(component, None, source, basis, budget)?;
            }
            PrefabComponentIntent::Properties {
                target,
                component,
                version,
                claims,
            } => {
                copy_target(target, source, mapping, budget)?;
                let schema = schema(component, Some(*version), source, basis, budget)?;
                for claim in claims {
                    // Share both selector limits and preparation work across all documents.
                    let mut reads = budget.property_reads;
                    let selected = resolve_component_property_schema_claim_with_admission(
                        schema,
                        *version,
                        &claim.path,
                        &mut reads,
                        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
                    );
                    budget.property_reads = reads;
                    let selected = selected.map_err(|error| match error {
                        ComponentPropertyAdmissionError::Admission(error) => error,
                        ComponentPropertyAdmissionError::Property(error) => {
                            PrefabCopyError::Property { source, error }
                        }
                    })?;
                    let payload = match &mut claim.intent {
                        PrefabPropertyIntent::Set(value) => Some(value),
                        PrefabPropertyIntent::Absent => None,
                    };
                    if let Some(value) = payload {
                        let traversal = budget.traversal()?;
                        // Scalar properties intentionally yield an empty, payload-relative path.
                        let references = collect_declared_value_asset_references_with_admission(
                            component,
                            selected.field,
                            selected.shape,
                            value,
                            traversal,
                            |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
                        )
                        .map_err(|error| collection_error(source, error))?;
                        rewrite_assets(value, &references, source, mapping, budget)?;
                        budget.value(value, source)?;
                    } else if let Some(default) = selected.field.default_value() {
                        budget.value(default, source)?;
                        let traversal = budget.traversal()?;
                        let references = collect_declared_value_asset_references_with_admission(
                            component,
                            selected.field,
                            selected.field.shape(),
                            default,
                            traversal,
                            |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
                        )
                        .map_err(|error| collection_error(source, error))?;
                        if !references.is_empty() || contains_local_reference(default, budget)? {
                            // Absent resumes the Schema default. Retaining it would reintroduce
                            // original asset identities; pinning needs an explicit later contract.
                            return Err(PrefabCopyError::ReferenceDefaultAbsent { source });
                        }
                    }
                    copy_bindings(&mut claim.bindings, source, mapping, budget)?;
                }
            }
        }
    }
    for placement in &mut overlay.placements {
        copy_target(&mut placement.target, source, mapping, budget)?;
        if let PrefabPlacementParent::Entity(target) = &mut placement.parent {
            copy_target(target, source, mapping, budget)?;
        }
        if let PrefabPlacementSlot::Before(target) | PrefabPlacementSlot::After(target) =
            &mut placement.slot
        {
            copy_target(target, source, mapping, budget)?;
        }
    }
    for target in &mut overlay.suppressions {
        copy_target(target, source, mapping, budget)?;
    }
    Ok(())
}

fn copy_target(
    target: &mut PrefabEntityTarget,
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    budget.charge(1, 0, 0)?;
    for hop in &mut target.through {
        mapping.key(&mut hop.expected_base, source, budget)?;
    }
    Ok(())
}

fn copy_bindings(
    bindings: &mut [PrefabReferenceBinding],
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    for binding in bindings {
        copy_target(&mut binding.target, source, mapping, budget)?;
    }
    Ok(())
}

fn rewrite_assets(
    value: &mut ComponentValue,
    references: &[DeclaredAssetReference],
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    budget.allocate(
        references
            .len()
            .checked_mul(std::mem::size_of::<nara_asset::AssetRef>()),
    )?;
    let mut replacements = Vec::with_capacity(references.len());
    for reference in references {
        replacements.push(mapping.asset(reference.asset_ref(), source, budget)?);
    }
    let mut replacements = replacements.into_iter();
    let traversal = budget.traversal()?;
    rewrite_declared_asset_references_with_admission(
        value,
        references,
        traversal,
        |_| replacements.next().ok_or(PrefabCopyError::BudgetExceeded),
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        AssetReferenceRewriteError::Admission(error)
        | AssetReferenceRewriteError::Rewrite { error, .. } => error,
        error => PrefabCopyError::AssetRewrite {
            source,
            error: Box::new(error),
        },
    })
}

fn collection_error(
    source: usize,
    error: AssetReferenceCollectionError<PrefabCopyError>,
) -> PrefabCopyError {
    match error {
        AssetReferenceCollectionError::Admission(error) => error,
        AssetReferenceCollectionError::Reference(error) => {
            PrefabCopyError::AssetReference { source, error }
        }
    }
}

// Typed entity leaves cannot be ordinary strings. This runs only after borrowed depth/cost
// admission; it does not discover asset references or infer Schema semantics.
fn contains_local_reference(
    value: &ComponentValue,
    budget: &mut CopyBudget,
) -> Result<bool, PrefabCopyError> {
    budget.charge(1, 0, 0)?;
    match value {
        ComponentValue::EntityReference(_) => Ok(true),
        ComponentValue::List(values) => {
            for value in values {
                if contains_local_reference(value, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        ComponentValue::Map(values) => {
            for value in values.values() {
                if contains_local_reference(value, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}
