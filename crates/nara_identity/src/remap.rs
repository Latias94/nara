use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{
    EntityReference, RuntimeEntityReference, SceneEntityId, SceneIdentitySnapshot,
    WorldEntityLocator, WorldIdentityDomainId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityReferenceRemap {
    entries: BTreeMap<RuntimeEntityReference, RuntimeEntityReference>,
    entity_references: BTreeMap<EntityReference, EntityReference>,
}

impl EntityReferenceRemap {
    fn between_scene_snapshots(
        source: &SceneIdentitySnapshot,
        target: &SceneIdentitySnapshot,
    ) -> Result<Self, IdentityRemapError> {
        if !source.entity_ids().eq(target.entity_ids()) {
            return Err(IdentityRemapError::IncompleteSceneGroup);
        }

        let mut entries = Vec::with_capacity(source.len().saturating_mul(2));
        for entity_id in source.entity_ids() {
            let source_references = source
                .references(entity_id)
                .expect("source snapshot owns every listed entity id");
            let target_references = target
                .references(entity_id)
                .expect("equal snapshot groups own the same entity ids");
            entries.push((
                source_references.scene.clone(),
                target_references.scene.clone(),
            ));
            match (&source_references.persistent, &target_references.persistent) {
                (Some(source), Some(target)) => entries.push((source.clone(), target.clone())),
                (None, None) => {}
                _ => {
                    return Err(IdentityRemapError::IncompleteIdentityAxes {
                        entity: entity_id.clone(),
                    });
                }
            }
        }
        let expected_sources = entries
            .iter()
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        Self::complete(expected_sources, entries)
    }

    pub(crate) fn complete(
        expected_sources: impl IntoIterator<Item = RuntimeEntityReference>,
        entries: impl IntoIterator<Item = (RuntimeEntityReference, RuntimeEntityReference)>,
    ) -> Result<Self, IdentityRemapError> {
        let mut expected = BTreeSet::new();
        for source in expected_sources {
            if !expected.insert(source.clone()) {
                return Err(IdentityRemapError::DuplicateExpectedSource { reference: source });
            }
        }

        let mut remap = BTreeMap::new();
        let mut target_claims = BTreeMap::<RuntimeEntityReference, RuntimeEntityReference>::new();
        for (source, target) in entries {
            if remap.insert(source.clone(), target.clone()).is_some() {
                return Err(IdentityRemapError::DuplicateSource { reference: source });
            }
            if let Some(existing_source) = target_claims.insert(target.clone(), source.clone()) {
                return Err(IdentityRemapError::DuplicateTarget {
                    target: Box::new(target),
                    first_source: Box::new(existing_source),
                    second_source: Box::new(source),
                });
            }
        }
        if remap.keys().ne(expected.iter()) {
            return Err(IdentityRemapError::IncompleteSourceSet);
        }

        let mut entity_references = BTreeMap::new();
        let mut target_claims = BTreeMap::<EntityReference, EntityReference>::new();
        for (source, target) in &remap {
            if !same_reference_axis(source, target) {
                return Err(IdentityRemapError::IncompatibleReferenceAxes {
                    from: Box::new(source.clone()),
                    to: Box::new(target.clone()),
                });
            }
            let source = durable_reference(source);
            let target = durable_reference(target);
            if entity_references
                .insert(source.clone(), target.clone())
                .is_some()
            {
                return Err(IdentityRemapError::DuplicateEntityReferenceSource {
                    reference: source,
                });
            }
            if let Some(existing_source) = target_claims.insert(target.clone(), source.clone()) {
                return Err(IdentityRemapError::DuplicateEntityReferenceTarget {
                    target: Box::new(target),
                    first_source: Box::new(existing_source),
                    second_source: Box::new(source),
                });
            }
        }
        Ok(Self {
            entries: remap,
            entity_references,
        })
    }

    #[must_use]
    pub fn get(&self, source: &RuntimeEntityReference) -> Option<&RuntimeEntityReference> {
        self.entries.get(source)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn rewrite(
        &self,
        source: &RuntimeEntityReference,
    ) -> Result<RuntimeEntityReference, IdentityRemapError> {
        self.get(source)
            .cloned()
            .ok_or_else(|| IdentityRemapError::MissingSource {
                reference: source.clone(),
            })
    }

    pub fn rewrite_entity_reference(
        &self,
        source: &EntityReference,
    ) -> Result<EntityReference, IdentityRemapError> {
        self.entity_references.get(source).cloned().ok_or_else(|| {
            IdentityRemapError::MissingEntityReferenceSource {
                reference: source.clone(),
            }
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&RuntimeEntityReference, &RuntimeEntityReference)> {
        self.entries.iter()
    }
}

const fn same_reference_axis(
    source: &RuntimeEntityReference,
    target: &RuntimeEntityReference,
) -> bool {
    matches!(
        (source, target),
        (
            RuntimeEntityReference::Scene { .. },
            RuntimeEntityReference::Scene { .. }
        ) | (
            RuntimeEntityReference::Persistent { .. },
            RuntimeEntityReference::Persistent { .. }
        )
    )
}

fn durable_reference(reference: &RuntimeEntityReference) -> EntityReference {
    match reference {
        RuntimeEntityReference::Scene { entity, .. } => EntityReference::SceneLocal {
            entity: entity.clone(),
        },
        RuntimeEntityReference::Persistent { entity } => EntityReference::Persistent {
            entity: entity.clone(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntityLocatorRemap {
    source_domain: WorldIdentityDomainId,
    target_domain: WorldIdentityDomainId,
    references: EntityReferenceRemap,
}

impl WorldEntityLocatorRemap {
    const fn new(
        source_domain: WorldIdentityDomainId,
        target_domain: WorldIdentityDomainId,
        references: EntityReferenceRemap,
    ) -> Self {
        Self {
            source_domain,
            target_domain,
            references,
        }
    }

    pub fn between_scene_snapshots(
        source: &SceneIdentitySnapshot,
        target: &SceneIdentitySnapshot,
    ) -> Result<Self, IdentityRemapError> {
        let references = EntityReferenceRemap::between_scene_snapshots(source, target)?;
        Ok(Self::new(
            source.domain_id(),
            target.domain_id(),
            references,
        ))
    }

    pub fn rewrite(
        &self,
        source: &WorldEntityLocator,
    ) -> Result<WorldEntityLocator, IdentityRemapError> {
        if source.domain_id() != self.source_domain {
            return Err(IdentityRemapError::WrongSourceDomain {
                expected: self.source_domain,
                actual: source.domain_id(),
            });
        }
        Ok(WorldEntityLocator::new(
            self.target_domain,
            self.references.rewrite(source.entity())?,
        ))
    }

    #[must_use]
    pub const fn source_domain(&self) -> WorldIdentityDomainId {
        self.source_domain
    }

    #[must_use]
    pub const fn target_domain(&self) -> WorldIdentityDomainId {
        self.target_domain
    }

    #[must_use]
    pub const fn references(&self) -> &EntityReferenceRemap {
        &self.references
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityRemapError {
    #[error("entity reference remap expected source set contains a duplicate")]
    DuplicateExpectedSource { reference: RuntimeEntityReference },
    #[error("entity reference remap has a duplicate source")]
    DuplicateSource { reference: RuntimeEntityReference },
    #[error("entity reference remap has a duplicate target")]
    DuplicateTarget {
        target: Box<RuntimeEntityReference>,
        first_source: Box<RuntimeEntityReference>,
        second_source: Box<RuntimeEntityReference>,
    },
    #[error("entity reference remap changes the reference axis")]
    IncompatibleReferenceAxes {
        from: Box<RuntimeEntityReference>,
        to: Box<RuntimeEntityReference>,
    },
    #[error("entity reference remap has a duplicate durable source")]
    DuplicateEntityReferenceSource { reference: EntityReference },
    #[error("entity reference remap has a duplicate durable target")]
    DuplicateEntityReferenceTarget {
        target: Box<EntityReference>,
        first_source: Box<EntityReference>,
        second_source: Box<EntityReference>,
    },
    #[error("entity reference remap is missing a source")]
    MissingSource { reference: RuntimeEntityReference },
    #[error("entity reference remap is missing a durable source")]
    MissingEntityReferenceSource { reference: EntityReference },
    #[error("entity reference remap does not cover its complete expected source set")]
    IncompleteSourceSet,
    #[error("scene instance remap does not cover the same entity group")]
    IncompleteSceneGroup,
    #[error("scene instance remap does not cover the same identity axes")]
    IncompleteIdentityAxes { entity: SceneEntityId },
    #[error("world locator remap received the wrong source domain")]
    WrongSourceDomain {
        expected: WorldIdentityDomainId,
        actual: WorldIdentityDomainId,
    },
}
