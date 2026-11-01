//! Explicit project Rust semantics for the EnemyStats authored-data proposal.
//!
//! Inputs are already bounded Scene/Prefab wire documents. Planning borrows the input and
//! converts only a private copy; neither a registry nor ordinary Open executes this code. The
//! actual Host/Scene owner must independently admit the catalogs, basis, complete source set,
//! proposal footprint, storage and publication. These plans grant none of that authority.

use std::{collections::BTreeMap, error::Error, fmt};

use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentPropertyStep, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};
use nara_scene::{
    PrefabComponentIntent, PrefabDocument, PrefabInstance, PrefabPropertyClaim,
    PrefabPropertyIntent, PrefabReferenceBinding, SceneComponentRecord, SceneDocument,
    SceneEntityRecord,
};

pub const OWNER: &str = "trial.enemy-stats";
pub const COMPONENT: &str = "trial.enemy.EnemyStats";
pub const V2: ComponentSchemaVersion = ComponentSchemaVersion(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    Version(ComponentSchemaVersion),
    RecordShape,
    HealthShape,
    ArmorShape,
    HealthOverflow(i64),
    Selector(ComponentPropertyPath),
    RootAbsent(ComponentFieldId),
    DuplicateClaim(ComponentFieldId),
    Bindings,
    CatalogDefinition,
    CatalogGeneration,
}

impl fmt::Display for UpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "EnemyStats v1 -> v2 proposal refused: {self:?}")
    }
}

impl Error for UpgradeError {}

/// The old scalar's durable root identity is retained by the new selected member.
pub fn health_property_v2() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
    ])
}

/// The existing project's compile-time catalog, without managed-tool execution or file I/O.
pub fn v1_catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog::from_json_str(include_str!("../../tools/enemy-stats.catalog.json"))
        .expect("the project's embedded EnemyStats v1 catalog is valid")
}

/// Produces data for the immediate successor of the caller's actual owner catalog.
///
/// The stored EnemyStats definition must be the project's known v1 contract. A prior
/// creation-only change to its initial value is supported; unrelated components are copied.
/// Admission of this successor with that exact predecessor belongs to the real Schema owner.
pub fn successor_catalog(
    previous: &ComponentSchemaCatalog,
) -> Result<ComponentSchemaCatalog, UpgradeError> {
    let expected = v1_catalog();
    let mut next = ComponentSchemaCatalog::successor_of(previous)
        .map_err(|_| UpgradeError::CatalogGeneration)?;
    next.components = previous.components.clone();
    let mut selected = next
        .components
        .iter_mut()
        .filter(|schema| schema.id.as_str() == COMPONENT);
    let schema = selected.next().ok_or(UpgradeError::CatalogDefinition)?;
    if selected.next().is_some()
        || !schema.same_definition_except_initial_value(&expected.components[0])
    {
        return Err(UpgradeError::CatalogDefinition);
    }
    if let Some(value) = &mut schema.authoring_initial_value {
        convert_value(value)?;
    }
    schema.version = V2;
    let health = schema
        .fields
        .iter_mut()
        .find(|field| field.id.as_str() == "health")
        .ok_or(UpgradeError::CatalogDefinition)?;
    health.shape = ComponentValueShape::record_members([ComponentShapeMember::new(
        "milli",
        ComponentValueKind::I64,
    )
    .with_id(ComponentFieldId::new("milli"))]);
    Ok(next)
}

/// Read-only review of the exact borrowed Scene and its privately converted proposal.
#[derive(Debug)]
pub struct SceneUpgradePlan<'a> {
    original: &'a SceneDocument,
    converted: SceneDocument,
}

impl<'a> SceneUpgradePlan<'a> {
    pub fn original(&self) -> &'a SceneDocument {
        self.original
    }

    pub fn converted(&self) -> &SceneDocument {
        &self.converted
    }

    pub fn into_document(self) -> SceneDocument {
        self.converted
    }
}

/// Read-only review of the exact borrowed Prefab and its privately converted proposal.
#[derive(Debug)]
pub struct PrefabUpgradePlan<'a> {
    original: &'a PrefabDocument,
    converted: PrefabDocument,
}

impl<'a> PrefabUpgradePlan<'a> {
    pub fn original(&self) -> &'a PrefabDocument {
        self.original
    }

    pub fn converted(&self) -> &PrefabDocument {
        &self.converted
    }

    pub fn into_document(self) -> PrefabDocument {
        self.converted
    }
}

pub fn plan_scene(original: &SceneDocument) -> Result<SceneUpgradePlan<'_>, UpgradeError> {
    let mut converted = original.clone();
    convert_entities(&mut converted.entities)?;
    Ok(SceneUpgradePlan {
        original,
        converted,
    })
}

pub fn plan_prefab(original: &PrefabDocument) -> Result<PrefabUpgradePlan<'_>, UpgradeError> {
    let mut converted = original.clone();
    convert_entities(&mut converted.entities)?;
    Ok(PrefabUpgradePlan {
        original,
        converted,
    })
}

fn require_v1(version: ComponentSchemaVersion) -> Result<(), UpgradeError> {
    // This is an exact v1 proposal, not a recovery reader. A v2 input must be matched against
    // the owner's saved deterministic output, never passed through multiplication again.
    if version == ComponentSchemaVersion::ONE {
        Ok(())
    } else {
        Err(UpgradeError::Version(version))
    }
}

fn milli(health: i64) -> Result<i64, UpgradeError> {
    health
        .checked_mul(1000)
        .ok_or(UpgradeError::HealthOverflow(health))
}

fn convert_value(value: &mut ComponentValue) -> Result<(), UpgradeError> {
    let ComponentValue::Map(fields) = value else {
        return Err(UpgradeError::RecordShape);
    };
    if !matches!(fields.get("wire_armor"), Some(ComponentValue::I64(_))) {
        return Err(UpgradeError::ArmorShape);
    }
    let Some(ComponentValue::I64(health)) = fields.get("wire_health") else {
        return Err(UpgradeError::HealthShape);
    };
    let health = milli(*health)?;
    fields.insert(
        "wire_health".into(),
        ComponentValue::map([("milli", ComponentValue::I64(health))]),
    );
    Ok(())
}

fn affected_record(component: &ComponentTypeId, record: &SceneComponentRecord) -> bool {
    // An omitted owner is resolved by the caller's admitted project basis. An explicitly
    // different owner is an unaffected unavailable record, even for the same component ID.
    component.as_str() == COMPONENT
        && record
            .owner
            .as_ref()
            .is_none_or(|owner| owner.as_str() == OWNER)
}

fn no_bindings(bindings: &[PrefabReferenceBinding]) -> Result<(), UpgradeError> {
    // Both declared fields are I64 and contain no local reference slots. Even a binding
    // named wire_health is invalid here; unknown components' bindings are never inspected.
    if bindings.is_empty() {
        Ok(())
    } else {
        Err(UpgradeError::Bindings)
    }
}

fn convert_record(record: &mut SceneComponentRecord) -> Result<(), UpgradeError> {
    require_v1(record.version)?;
    convert_value(&mut record.value)?;
    record.version = V2;
    Ok(())
}

fn convert_components(
    components: &mut BTreeMap<ComponentTypeId, SceneComponentRecord>,
) -> Result<(), UpgradeError> {
    for (component, record) in components {
        if affected_record(component, record) {
            convert_record(record)?;
        }
    }
    Ok(())
}

fn convert_entities(entities: &mut [SceneEntityRecord]) -> Result<(), UpgradeError> {
    for entity in entities {
        convert_components(&mut entity.components)?;
        if let Some(instance) = &mut entity.prefab {
            convert_instance(instance)?;
        }
    }
    Ok(())
}

fn convert_instance(instance: &mut PrefabInstance) -> Result<(), UpgradeError> {
    let Some(overlay) = &mut instance.overrides else {
        return Ok(());
    };
    // Visit the stored tree, without resolving targets, suppressions or surviving values.
    // Dormant and repeated declarations each keep their own pin and receive one conversion.
    for addition in &mut overlay.additions {
        for (component, bindings) in &addition.bindings {
            if component.as_str() == COMPONENT
                && addition
                    .components
                    .get(component)
                    .is_none_or(|record| affected_record(component, record))
            {
                no_bindings(bindings)?;
            }
        }
        convert_components(&mut addition.components)?;
        if let Some(nested) = &mut addition.prefab {
            convert_instance(nested)?;
        }
    }
    for claim in &mut overlay.instances {
        if let Some(nested) = &mut claim.value {
            convert_instance(nested)?;
        }
    }
    for intent in &mut overlay.components {
        match intent {
            PrefabComponentIntent::Set {
                component,
                record,
                bindings,
                ..
            } if affected_record(component, record) => {
                no_bindings(bindings)?;
                convert_record(record)?;
            }
            PrefabComponentIntent::Properties {
                component,
                version,
                claims,
                ..
            } if component.as_str() == COMPONENT => {
                require_v1(*version)?;
                convert_claims(claims)?;
                // Empty and armor-only batches still assert the affected component version.
                *version = V2;
            }
            _ => {}
        }
    }
    Ok(())
}

fn convert_claims(claims: &mut [PrefabPropertyClaim]) -> Result<(), UpgradeError> {
    let mut health_seen = false;
    let mut armor_seen = false;
    for claim in claims {
        let seen = match (claim.path.root.as_str(), claim.path.steps.is_empty()) {
            ("health", true) => &mut health_seen,
            ("armor", true) => &mut armor_seen,
            _ => return Err(UpgradeError::Selector(claim.path.clone())),
        };
        // For these two scalar roots, overlap is exactly a repeated root. There is no
        // project-local implementation of the engine's general selector grammar here.
        if *seen {
            return Err(UpgradeError::DuplicateClaim(claim.path.root.clone()));
        }
        *seen = true;
        no_bindings(&claim.bindings)?;
        let PrefabPropertyIntent::Set(value) = &mut claim.intent else {
            // Both fields are required without defaults. Mapping health's root absence to
            // a nested milli absence has no supported equivalent declaration.
            return Err(UpgradeError::RootAbsent(claim.path.root.clone()));
        };
        if claim.path.root.as_str() == "health" {
            let ComponentValue::I64(health) = value else {
                return Err(UpgradeError::HealthShape);
            };
            *health = milli(*health)?;
            claim.path = health_property_v2();
        } else if !matches!(value, ComponentValue::I64(_)) {
            return Err(UpgradeError::ArmorShape);
        }
    }
    Ok(())
}
