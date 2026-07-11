use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{
    RuntimeEntityReference, SceneEntityId, SceneIdentitySnapshot, WorldEntityLocator,
    WorldIdentityDomainId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityReferenceRemap {
    entries: BTreeMap<RuntimeEntityReference, RuntimeEntityReference>,
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
                    target,
                    first_source: existing_source,
                    second_source: source,
                });
            }
        }
        if remap.keys().ne(expected.iter()) {
            return Err(IdentityRemapError::IncompleteSourceSet);
        }
        Ok(Self { entries: remap })
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

    pub fn iter(&self) -> impl Iterator<Item = (&RuntimeEntityReference, &RuntimeEntityReference)> {
        self.entries.iter()
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
        target: RuntimeEntityReference,
        first_source: RuntimeEntityReference,
        second_source: RuntimeEntityReference,
    },
    #[error("entity reference remap is missing a source")]
    MissingSource { reference: RuntimeEntityReference },
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
