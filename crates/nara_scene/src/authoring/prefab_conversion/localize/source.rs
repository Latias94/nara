//! Exact source declarations rewritten with the selected compiler's normalized target table.

use std::borrow::Cow;

use nara_identity::AuthoredReferenceTarget;
use nara_reflect::{ComponentTypeId, EntityReferenceTraversalLimits};

use super::*;
use crate::prefab::SelectedInstanceFragment;

mod instances;
mod queries;
pub(super) use instances::localize_instance;
pub(super) use queries::collect_source_reference_queries;

#[derive(Clone, Copy)]
pub(super) struct SourceTargets<'a> {
    pub(super) entities: &'a BTreeMap<PrefabEntityTarget, SceneEntityId>,
    pub(super) references: &'a BTreeMap<nara_identity::AuthoredEntityReference, EntityReference>,
    pub(super) identifiers: &'a BTreeMap<SceneEntityId, SceneEntityId>,
    pub(super) anchor: &'a SceneEntityId,
    pub(super) source: &'a [SceneEntityRecord],
}

pub(super) fn localize_placements(
    original: &[SceneEntityPlacement],
    fragment: &SelectedInstanceFragment,
    identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
    anchor: &SceneEntityId,
    source: &[SceneEntityRecord],
    work: &mut LocalizationWork<'_>,
) -> Result<Vec<SceneEntityPlacement>, ScenePatchReport> {
    let cost = crate::prefab::scene_placements_cost(
        original,
        PrefabOverlayCostLimits {
            nodes: SceneFileLimits::default().shape().nodes().get(),
            depth: SceneFileLimits::default().shape().depth().get(),
            placements: SceneFileLimits::default().placements().get(),
            logical_bytes: work.maximum,
            ..PrefabOverlayCostLimits::UNBOUNDED
        },
    )
    .map_err(|_| copy_limit())?;
    work.reserve(
        cost.nodes
            .checked_mul(512)
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or_else(copy_limit)?,
    )?;
    let mut declarations = original.to_vec();
    for declaration in &mut declarations {
        localize_target(&mut declaration.target, &fragment.targets, identifiers);
        if let ScenePlacementParent::Entity(parent) = &mut declaration.placement.parent {
            localize_target(parent, &fragment.targets, identifiers);
        }
        if let crate::ScenePlacementSlot::Before(sibling)
        | crate::ScenePlacementSlot::After(sibling) = &mut declaration.placement.slot
        {
            localize_target(sibling, &fragment.targets, identifiers);
        }
    }
    let mut assigned = declarations
        .iter()
        .map(|entry| entry.target.clone())
        .collect::<BTreeSet<_>>();
    if assigned.len() != declarations.len() {
        return Err(refused(
            "Conversion cannot merge duplicate source placement declarations",
        ));
    }
    let mut append = |entity: &SceneEntityId, placement: ScenePlacement| {
        let target = PrefabEntityTarget::source(local_id(entity, identifiers, source)?);
        if assigned.insert(target.clone()) {
            let entry = SceneEntityPlacement::new(target, placement);
            let cost = crate::prefab::scene_placements_cost(
                std::slice::from_ref(&entry),
                PrefabOverlayCostLimits::UNBOUNDED,
            )
            .map_err(|_| copy_limit())?;
            work.reserve(
                cost.nodes
                    .checked_mul(512)
                    .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
                    .ok_or_else(copy_limit)?,
            )?;
            declarations.push(entry);
        }
        Ok::<_, ScenePatchReport>(())
    };
    if !identifiers
        .get(anchor)
        .is_some_and(|entity| entity == anchor)
    {
        // Forest roots begin before local children. Existing containing-source pins keep
        // their precise mixed order; only newly local roots need this initial relation.
        for root in &fragment.roots {
            let parent = ScenePlacementParent::Entity(PrefabEntityTarget::source(anchor.clone()));
            append(root, ScenePlacement::first(parent))?;
        }
    }
    for record in &fragment.entities {
        let parent = ScenePlacementParent::Entity(PrefabEntityTarget::source(local_id(
            &record.id,
            identifiers,
            source,
        )?));
        for child in &record.children {
            // A root-bound anchor's complete child order can include containing-source
            // locals. Their declarations remain owned by the containing source.
            if !identifiers.contains_key(child) {
                continue;
            }
            // Previously unclaimed inherited children follow explicit First cohorts and
            // precede explicit Last cohorts. Append a First cohort in their actual order;
            // retained relative pins keep mixed local ordering without inverse edges.
            append(child, ScenePlacement::first(parent.clone()))?;
        }
    }
    for entity in identifiers.values() {
        if entity != anchor && !assigned.contains(&PrefabEntityTarget::source(entity.clone())) {
            return Err(refused(
                "Every localized source entity requires one explicit placement",
            ));
        }
    }
    Ok(declarations)
}

fn local_id(
    entity: &SceneEntityId,
    identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
    source: &[SceneEntityRecord],
) -> Result<SceneEntityId, ScenePatchReport> {
    if let Some(local) = identifiers.get(entity) {
        return Ok(local.clone());
    }
    if source
        .binary_search_by(|record| record.id.cmp(entity))
        .is_ok()
    {
        return Ok(entity.clone());
    }
    Err(refused(
        "Conversion cannot capture another occurrence's projected placement",
    ))
}

fn localize_target(
    target: &mut PrefabEntityTarget,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
) {
    if let Some(local) = targets
        .get(target)
        .and_then(|entity| identifiers.get(entity))
    {
        *target = PrefabEntityTarget::source(local.clone());
    }
}

/// Preserve raw omissions and hidden defaults. Only declared entity-reference leaves change;
/// target resolution belongs exclusively to the compiler that supplied `targets`.
pub(super) fn localize_record<'a>(
    original: &'a crate::SceneComponentRecord,
    component: &ComponentTypeId,
    basis: &AuthoringSchemaBasis,
    targets: SourceTargets<'_>,
    limits: EntityReferenceTraversalLimits,
    work: &mut LocalizationWork<'_>,
) -> Result<Cow<'a, crate::SceneComponentRecord>, ScenePatchReport> {
    let Some(schema) = understood_schema(component, original, basis) else {
        return Ok(Cow::Borrowed(original));
    };
    let plan = plan_declared_entity_references_with_admission::<ScenePatchReport, _>(
        schema,
        &original.value,
        limits,
        |_, _, bytes| work.reserve(bytes),
    )
    .map_err(plan_error)?;
    let mut changed = false;
    plan.visit(&original.value, |_, reference| {
        changed |= replacement(reference, targets, work)?.is_some();
        Ok::<_, ScenePatchReport>(())
    })
    .map_err(reference_error)?;
    let mut defaults = Vec::new();
    for (id, value) in &original.contextual_defaults {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == id)
            .ok_or_else(|| refused("Conversion requires the declared contextual default field"))?;
        let plan = plan_declared_value_entity_references_with_admission::<ScenePatchReport, _>(
            field,
            field.shape(),
            value,
            limits,
            |_, _, bytes| work.reserve(bytes),
        )
        .map_err(plan_error)?;
        plan.visit(value, |_, reference| {
            changed |= replacement(reference, targets, work)?.is_some();
            Ok::<_, ScenePatchReport>(())
        })
        .map_err(reference_error)?;
        work.reserve(
            std::mem::size_of::<(nara_reflect::ComponentFieldId, DeclaredEntityReferencePlan)>()
                .checked_add(id.as_str().len())
                .ok_or_else(copy_limit)?,
        )?;
        defaults.push((id.clone(), plan));
    }
    if !changed {
        return Ok(Cow::Borrowed(original));
    }
    let cost = bounded_component_records_source_cost(
        std::iter::once(original),
        ScenePatchApplyLimits::default(),
    )
    .ok_or_else(copy_limit)?;
    work.reserve(
        cost.operation_count()
            .checked_mul(512)
            .and_then(|bytes| bytes.checked_add(cost.value_cost().nodes().checked_mul(96)?))
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
            .ok_or_else(copy_limit)?,
    )?;
    let mut record = original.clone();
    let mut rewrite = |_: &nara_reflect::ComponentFieldPath, reference: &EntityReference| {
        Ok::<_, ScenePatchReport>(
            replacement(reference, targets, work)?.unwrap_or_else(|| reference.clone()),
        )
    };
    plan.rewrite_in_place(&mut record.value, &mut rewrite)
        .map_err(reference_error)?;
    for (id, plan) in defaults {
        let value = record
            .contextual_defaults
            .get_mut(&id)
            .ok_or_else(copy_limit)?;
        plan.rewrite_in_place(value, &mut rewrite)
            .map_err(reference_error)?;
    }
    Ok(Cow::Owned(record))
}

fn replacement(
    reference: &EntityReference,
    targets: SourceTargets<'_>,
    work: &mut LocalizationWork<'_>,
) -> Result<Option<EntityReference>, ScenePatchReport> {
    let EntityReference::Authored { reference } = reference else {
        return Ok(None);
    };
    let enters_selected = reference.through.first().is_some_and(|hop| {
        matches!(&hop.anchor, nara_identity::PrefabEntityOrigin::SourceEntity(entity) if entity == targets.anchor)
    });
    if !enters_selected {
        return Ok(None);
    }
    let cost = reference.cost();
    work.reserve(
        cost.nodes()
            .checked_mul(96)
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
            .ok_or_else(copy_limit)?,
    )?;
    let AuthoredReferenceTarget::Entity(entity) = &reference.target else {
        let resolved = targets.references.get(reference).ok_or_else(|| {
            refused("Conversion requires the selected compiler's resolved input endpoint")
        })?;
        let cost = resolved.cost();
        work.reserve(
            cost.nodes()
                .checked_mul(96)
                .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
                .ok_or_else(copy_limit)?,
        )?;
        return Ok(Some(match resolved {
            EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                entity: local_id(entity, targets.identifiers, targets.source)?,
            },
            EntityReference::Persistent { .. } => resolved.clone(),
            EntityReference::Authored { reference } if matches!(reference.target, AuthoredReferenceTarget::Entity(_))
                && !reference.through.first().is_some_and(|hop| {
                    matches!(&hop.anchor, nara_identity::PrefabEntityOrigin::SourceEntity(entity) if entity == targets.anchor)
                }) => resolved.clone(),
            _ => return Err(refused("Conversion requires a surviving canonical source reference endpoint")),
        }));
    };
    let target = PrefabEntityTarget {
        through: reference.through.clone(),
        entity: entity.clone(),
    };
    let local = targets
        .entities
        .get(&target)
        .and_then(|entity| targets.identifiers.get(entity))
        .ok_or_else(|| {
            refused("Conversion requires the selected compiler's exact entity target")
        })?;
    Ok(Some(EntityReference::SceneLocal {
        entity: local.clone(),
    }))
}

fn understood_schema<'a>(
    component: &ComponentTypeId,
    record: &crate::SceneComponentRecord,
    basis: &'a AuthoringSchemaBasis,
) -> Option<&'a nara_reflect::ComponentSchema> {
    basis.schema(component).filter(|schema| {
        schema.version() == record.version
            && basis.has_authoring_adapter(component)
            && schema.has_capability(nara_reflect::ComponentCapability::Scene)
            && record
                .owner()
                .is_none_or(|owner| basis.component_schema_owner(component) == Some(owner))
    })
}

fn plan_error(
    error: EntityReferencePlanError<ScenePatchReport, ScenePatchReport>,
) -> ScenePatchReport {
    match error {
        EntityReferencePlanError::Reference(error) => reference_error(error),
        EntityReferencePlanError::Admission(report) => report,
    }
}
