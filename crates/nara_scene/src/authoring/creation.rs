use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use nara_asset::AssetReferenceResolver;
use nara_reflect::{ComponentRegistry, ComponentTypeId};

use super::{
    PrefabAuthoringSession, SceneAuthoringRevision, SceneAuthoringSession, history_error_report,
};
use crate::{
    SceneComponentRecord, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePatchReport, ScenePlacement,
};

const MAX_ENTITY_ID_ATTEMPTS: usize = 8;

/// Exact identity and history result of one successfully published source creation.
///
/// This receipt does not prove the entity remains present after later edits or Undo.
#[derive(Debug)]
pub struct CreatedSourceEntity {
    entity: SceneEntityId,
    report: ScenePatchReport,
}

impl CreatedSourceEntity {
    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn report(&self) -> &ScenePatchReport {
        &self.report
    }

    #[must_use]
    pub fn into_parts(self) -> (SceneEntityId, ScenePatchReport) {
        (self.entity, self.report)
    }
}

macro_rules! source_creation {
    ($session:ty) => {
        impl $session {
            /// Creates an entity through this native source owner's existing validation/history.
            ///
            /// Component records retain native registry admission; this is not a raw-record
            /// escape from Published authoring. No ID reservation is exposed to the caller.
            pub fn create_entity_with_registry(
                &mut self,
                base_revision: SceneAuthoringRevision,
                registry: &ComponentRegistry,
                placement: ScenePlacement,
                components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
            ) -> Result<CreatedSourceEntity, ScenePatchReport> {
                self.create_native_entity(base_revision, registry, None, placement, components)
            }

            /// Creates through native codecs with this project's exact asset resolver.
            ///
            /// The resolver validates both retained source records and the new component records,
            /// just as it does for ordinary native patches and history replay.
            pub fn create_entity_with_asset_resolution(
                &mut self,
                base_revision: SceneAuthoringRevision,
                registry: &ComponentRegistry,
                asset_resolution: AssetReferenceResolver<'_>,
                placement: ScenePlacement,
                components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
            ) -> Result<CreatedSourceEntity, ScenePatchReport> {
                self.create_native_entity(
                    base_revision,
                    registry,
                    Some(asset_resolution),
                    placement,
                    components,
                )
            }

            fn create_native_entity(
                &mut self,
                base_revision: SceneAuthoringRevision,
                registry: &ComponentRegistry,
                asset_resolution: Option<AssetReferenceResolver<'_>>,
                placement: ScenePlacement,
                components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
            ) -> Result<CreatedSourceEntity, ScenePatchReport> {
                if base_revision != self.revision() {
                    return Err(stale_creation());
                }
                if self.authority.require_native().is_err() {
                    return Err(contract_mismatch());
                }
                let entity = allocate_entity_id(|id| {
                    self.document()
                        .entities
                        .iter()
                        .any(|record| &record.id == id)
                })?;
                let mut record = SceneEntityRecord::new(entity.clone());
                record.components = components;
                let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: record,
                    placement,
                }]);
                let report = match asset_resolution {
                    Some(asset_resolution) => {
                        self.apply_patch_with_asset_resolution(&patch, registry, asset_resolution)
                    }
                    None => self.apply_patch(&patch, registry),
                };
                finish_creation(entity, report)
            }

            /// Publishes an empty entity through the admitted source transaction.
            ///
            /// Identity allocation and publication stay in this owner. Templates and transaction-
            /// local multi-entity construction have their separate, stricter authoring contracts.
            #[cfg(feature = "serde")]
            pub fn create_authoring_entity(
                &mut self,
                base_revision: SceneAuthoringRevision,
                placement: ScenePlacement,
            ) -> Result<CreatedSourceEntity, ScenePatchReport> {
                if base_revision != self.revision() {
                    return Err(stale_creation());
                }
                if self.authority.published().is_none() {
                    return Err(contract_mismatch());
                }
                let entity = allocate_entity_id(|id| {
                    self.document()
                        .entities
                        .iter()
                        .any(|record| &record.id == id)
                })?;
                let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(entity.clone()),
                    placement,
                }]);
                let report = self.apply_authoring_patch(&patch);
                finish_creation(entity, report)
            }
        }
    };
}

source_creation!(SceneAuthoringSession);
source_creation!(PrefabAuthoringSession);

fn finish_creation(
    entity: SceneEntityId,
    report: ScenePatchReport,
) -> Result<CreatedSourceEntity, ScenePatchReport> {
    if report.applied {
        Ok(CreatedSourceEntity { entity, report })
    } else {
        Err(report)
    }
}

pub(crate) fn allocate_entity_id(
    occupied: impl FnMut(&SceneEntityId) -> bool,
) -> Result<SceneEntityId, ScenePatchReport> {
    allocate_entity_id_with(occupied, getrandom::fill)
}

fn allocate_entity_id_with(
    mut occupied: impl FnMut(&SceneEntityId) -> bool,
    mut fill: impl FnMut(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<SceneEntityId, ScenePatchReport> {
    for _ in 0..MAX_ENTITY_ID_ATTEMPTS {
        let mut bytes = [0_u8; 16];
        fill(&mut bytes).map_err(|_| {
            history_error_report(
                "scene.authoring-entity-id-entropy-unavailable",
                "Source entity creation could not obtain fresh identity entropy",
            )
        })?;
        let entity = SceneEntityId::new(URL_SAFE_NO_PAD.encode(bytes)).map_err(|_| {
            history_error_report(
                "scene.authoring-entity-id-invalid",
                "Generated source entity identity could not be represented",
            )
        })?;
        if !occupied(&entity) {
            return Ok(entity);
        }
    }
    Err(history_error_report(
        "scene.authoring-entity-id-collision-limit",
        "Source entity creation exhausted its bounded fresh-identity attempts",
    ))
}

fn stale_creation() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-creation-stale-basis",
        "Entity creation requires its exact source incarnation and revision",
    )
}

fn contract_mismatch() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-creation-contract-mismatch",
        "Entity creation requires the source owner's matching admission contract",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_source_identity_keeps_all_entropy_in_path_safe_bounded_text() {
        for byte in [0, 1, 0xfb, 0xff] {
            let entity = allocate_entity_id_with(
                |_| false,
                |bytes| {
                    bytes.fill(byte);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(entity.as_str().len(), 22);
            assert_eq!(URL_SAFE_NO_PAD.decode(entity.as_str()).unwrap(), [byte; 16]);
            assert!(
                entity
                    .as_str()
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            );
        }
    }

    #[test]
    fn source_identity_collision_and_entropy_failures_are_bounded() {
        let mut fills = 0;
        assert!(
            allocate_entity_id_with(
                |_| true,
                |bytes| {
                    fills += 1;
                    bytes.fill(0);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(fills, 8);
        let mut lookups = 0;
        assert!(
            allocate_entity_id_with(
                |_| {
                    lookups += 1;
                    false
                },
                |_| { Err(getrandom::Error::UNSUPPORTED) }
            )
            .is_err()
        );
        assert_eq!(lookups, 0);
    }

    #[test]
    fn source_identity_retries_conflicts_without_rewriting_the_generated_value() {
        let occupied = SceneEntityId::new(URL_SAFE_NO_PAD.encode([0; 16])).unwrap();
        let mut fills = 0_u8;
        let generated = allocate_entity_id_with(
            |id| id == &occupied,
            |bytes| {
                bytes.fill(fills);
                fills += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(fills, 2);
        assert_ne!(generated, occupied);
        assert_eq!(URL_SAFE_NO_PAD.decode(generated.as_str()).unwrap(), [1; 16]);
    }
}
