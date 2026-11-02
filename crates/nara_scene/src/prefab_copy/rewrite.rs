//! Schema interpretation and explicit source-fact substitution for complete Prefab documents.

use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, ComponentPropertyAdmissionError,
        ComponentValueAdmissionError, EntityReferencePlanError,
        collect_declared_effective_asset_references_with_admission,
        collect_declared_value_asset_references_with_admission,
        effective_component_value_with_default_overrides_with_budget,
        plan_declared_entity_references_with_admission, plan_declared_value_entity_references,
        resolve_component_property_schema_claim_with_admission,
        validate_component_value_with_admission,
    },
    AssetReferenceRewriteError, ComponentEntityReferenceRewriteError, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    DeclaredAssetReference, EntityReference, rewrite_declared_asset_references_with_admission,
};

use super::{CopyBudget, CopyMapping, PrefabCopyError, PrefabCopySchemaError};
use crate::{
    AuthoringSchemaBasis, PrefabComponentIntent, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceBinding, PrefabPropertyIntent, SceneComponentRecord,
    SceneEntityPlacement, ScenePlacementParent, ScenePlacementSlot,
};

pub(super) fn copy_document(
    document: &mut PrefabDocument,
    source: usize,
    basis: &AuthoringSchemaBasis,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    for placement in &mut document.placements {
        copy_placement(placement, source, mapping, budget)?;
    }
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
    // Validation uses the actual contextual declarations even when an explicit value hides
    // them. The effective tree is temporary: copying never turns an omission into a value pin.
    let limits = budget.materialization()?;
    budget.effective_record(schema, record)?;
    crate::document::validate_contextual_defaults(schema, record, limits)
        .map_err(|error| value_error(source, error))?;
    let (effective, _) = effective_component_value_with_default_overrides_with_budget(
        schema,
        &record.value,
        &record.contextual_defaults,
        limits.nodes().get(),
        limits.bytes().get(),
        limits.depth(),
    )
    .map_err(|error| value_error(source, error))?;
    validate_component_value_with_admission(
        schema,
        effective.as_ref(),
        limits,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        ComponentValueAdmissionError::Admission(error) => error,
        ComponentValueAdmissionError::Value(error) => value_error(source, error),
    })?;
    let traversal = budget.traversal()?;
    let effective_assets = collect_declared_effective_asset_references_with_admission(
        schema,
        record.version,
        effective.as_ref(),
        traversal,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| collection_error(source, error))?;
    for reference in &effective_assets {
        if record.value.get_path(reference.path()).is_err()
            && !record
                .contextual_defaults
                .contains_key(reference.field_id())
        {
            // A canonical asset default has no source-owned replacement declaration. Refuse
            // rather than silently changing omission into an explicit copied asset pin.
            mapping.asset(reference.asset_ref(), source, budget)?;
            return Err(PrefabCopyError::ReferenceDefaultAbsent { source });
        }
    }
    drop(effective);
    let traversal = budget.traversal()?;
    let plan = plan_declared_entity_references_with_admission::<PrefabCopyError, _>(
        schema,
        &record.value,
        traversal,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        EntityReferencePlanError::Admission(error) => error,
        EntityReferencePlanError::Reference(error) => reference_error(source, error),
    })?;
    plan.rewrite_in_place(&mut record.value, |_, reference| {
        copy_reference(reference, source, mapping, budget)
    })
    .map_err(|error| reference_error(source, error))?;
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
    budget.value(&record.value)?;
    for (id, value) in &mut record.contextual_defaults {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == id)
            .ok_or_else(|| PrefabCopyError::Value {
                source,
                error: nara_reflect::ComponentCodecError::invalid_field(
                    "contextual defaults",
                    "declared reference default",
                ),
            })?;
        copy_payload(
            component,
            field,
            field.shape(),
            value,
            source,
            mapping,
            budget,
        )?;
    }
    Ok(())
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
    if let PrefabInstanceBinding::Root { base, .. } = &mut instance.binding {
        mapping.key(base, source, budget)?;
    }
    if let Some(arguments) = &mut instance.arguments {
        mapping.key(&mut arguments.base, source, budget)?;
        for reference in arguments.values.values_mut() {
            *reference = copy_reference(reference, source, mapping, budget)?;
        }
    }
    let Some(overlay) = &mut instance.overrides else {
        return Ok(());
    };
    mapping.key(&mut overlay.base, source, budget)?;
    for addition in &mut overlay.additions {
        for (component, record) in &mut addition.components {
            copy_record(component, record, source, basis, mapping, budget)?;
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
                ..
            } => {
                copy_target(target, source, mapping, budget)?;
                copy_record(component, record, source, basis, mapping, budget)?;
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
                        copy_payload(
                            component,
                            selected.field,
                            selected.shape,
                            value,
                            source,
                            mapping,
                            budget,
                        )?;
                    } else if let Some(default) = selected.field.default_value() {
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
                        if !references.is_empty() {
                            // Absent resumes the Schema default. Retaining it would reintroduce
                            // original asset identities; pinning needs an explicit later contract.
                            return Err(PrefabCopyError::ReferenceDefaultAbsent { source });
                        }
                    }
                }
            }
        }
    }
    for placement in &mut overlay.placements {
        copy_placement(placement, source, mapping, budget)?;
    }
    for target in &mut overlay.suppressions {
        copy_target(target, source, mapping, budget)?;
    }
    Ok(())
}

fn copy_placement(
    declaration: &mut SceneEntityPlacement,
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    copy_target(&mut declaration.target, source, mapping, budget)?;
    if let ScenePlacementParent::Entity(target) = &mut declaration.placement.parent {
        copy_target(target, source, mapping, budget)?;
    }
    if let ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) =
        &mut declaration.placement.slot
    {
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

fn copy_reference(
    reference: &EntityReference,
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<EntityReference, PrefabCopyError> {
    if matches!(reference, EntityReference::Persistent { .. }) {
        return Err(PrefabCopyError::PersistentReference { source });
    }
    budget.reference(reference)?;
    let mut reference = reference.clone();
    if let EntityReference::Authored { reference } = &mut reference {
        // Whole-closure copying retains every lexical declaration and entity/input ID.
        // Only the exact canonical predicates change; origin namespaces never change.
        for hop in &mut reference.through {
            mapping.key(&mut hop.expected_base, source, budget)?;
        }
    }
    Ok(reference)
}

fn copy_payload(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &mut ComponentValue,
    source: usize,
    mapping: &CopyMapping<'_>,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    budget.selected_plan(field, value)?;
    let plan = plan_declared_value_entity_references::<PrefabCopyError>(
        field,
        shape,
        value,
        budget.traversal()?,
    )
    .map_err(|error| reference_error(source, error))?;
    plan.rewrite_in_place(value, |_, reference| {
        copy_reference(reference, source, mapping, budget)
    })
    .map_err(|error| reference_error(source, error))?;
    let traversal = budget.traversal()?;
    let references = collect_declared_value_asset_references_with_admission(
        component,
        field,
        shape,
        value,
        traversal,
        |nodes, bytes, allocated| budget.charge(nodes, bytes, allocated),
    )
    .map_err(|error| collection_error(source, error))?;
    rewrite_assets(value, &references, source, mapping, budget)?;
    budget.value(value)
}

fn value_error(source: usize, error: nara_reflect::ComponentCodecError) -> PrefabCopyError {
    match error {
        nara_reflect::ComponentCodecError::MaterializationNodeLimit { .. }
        | nara_reflect::ComponentCodecError::MaterializationByteLimit { .. }
        | nara_reflect::ComponentCodecError::MaterializationDepthLimit { .. } => {
            PrefabCopyError::BudgetExceeded
        }
        error => PrefabCopyError::Value { source, error },
    }
}

fn reference_error(
    source: usize,
    error: ComponentEntityReferenceRewriteError<PrefabCopyError>,
) -> PrefabCopyError {
    match error {
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            PrefabCopyError::BudgetExceeded
        }
        error => PrefabCopyError::EntityReference {
            source,
            error: Box::new(error),
        },
    }
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
