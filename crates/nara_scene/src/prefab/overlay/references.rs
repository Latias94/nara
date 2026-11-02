//! Reference intent travels with values; the shared Reflect planner supplies transient slots.

use std::collections::BTreeMap;

use nara_identity::{AuthoredEntityReference, EntityReference};
use nara_reflect::__private::plan_declared_value_entity_references;
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPath, ComponentFieldSchema, ComponentValue,
    ComponentValueMaterializationLimits, ComponentValueShape,
};

use super::SceneCompilationFailureKind;

/// Collects lexical reference intent without looking up a target or an input argument.
/// The compiler admits complete planner/path/target storage before calling this helper.
pub(super) fn plan_property_references(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    payload: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<BTreeMap<ComponentFieldPath, AuthoredEntityReference>, SceneCompilationFailureKind> {
    let plan = plan_declared_value_entity_references::<SceneCompilationFailureKind>(
        field,
        shape,
        payload,
        crate::validation::reference_traversal_limits(limits),
    )
    .map_err(reference_error)?;
    let mut references = BTreeMap::new();
    plan.visit(payload, |path, reference| {
        let reference = match reference {
            EntityReference::SceneLocal { entity } => {
                AuthoredEntityReference::source(entity.clone())
            }
            EntityReference::Authored { reference } => reference.clone(),
            EntityReference::Persistent { .. } => return Ok(()),
        };
        references.insert(path.clone(), reference);
        Ok(())
    })
    .map_err(reference_error)?;
    Ok(references)
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<SceneCompilationFailureKind>,
) -> SceneCompilationFailureKind {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            SceneCompilationFailureKind::WorkLimit
        }
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        _ => SceneCompilationFailureKind::Reference,
    }
}
