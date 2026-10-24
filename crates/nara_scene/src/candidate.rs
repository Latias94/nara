use std::{collections::BTreeSet, sync::Arc};

use crate::{SceneDocument, SceneEntityProvenance, SceneEntityProvenanceMap};

/// One immutable materialization input and its exact source ownership.
///
/// Prefab expansion creates this value with the provenance established while resolving sources.
/// Callers may inspect it, but cannot substitute a document or a provenance map independently.
/// [`Self::plain`] explicitly treats every record as local to the supplied scene.
/// The value is move-only so a retained-source borrow cannot extend its allocations independently
/// of the owning retention lease.
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneCandidate;
/// fn replace_document(candidate: &mut ExpandedSceneCandidate) {
///     candidate.document = Default::default();
/// }
/// ```
///
/// Retained-source views lend the candidate without allowing its allocations to escape:
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneCandidate;
/// fn escape(candidate: &ExpandedSceneCandidate) -> ExpandedSceneCandidate {
///     candidate.clone()
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct ExpandedSceneCandidate {
    document: Arc<SceneDocument>,
    provenance: SceneEntityProvenanceMap,
}

impl ExpandedSceneCandidate {
    /// Retains a plain scene without copying its document allocation.
    ///
    /// Component, hierarchy and reference admission still occur when materializing the candidate.
    /// Prefab-bearing documents must first use the ordinary expansion APIs.
    #[must_use]
    pub fn plain(document: Arc<SceneDocument>) -> Self {
        let provenance = document
            .entities
            .iter()
            .map(|entity| {
                (
                    entity.id.clone(),
                    SceneEntityProvenance::local(entity.id.clone()),
                )
            })
            .collect();
        Self {
            document,
            provenance,
        }
    }

    pub(crate) fn from_expansion(
        document: SceneDocument,
        provenance: SceneEntityProvenanceMap,
    ) -> Result<Self, InvalidSceneProvenance> {
        validate_scene_provenance(&document, &provenance)?;
        Ok(Self {
            document: Arc::new(document),
            provenance,
        })
    }

    /// Returns the exact immutable expanded document.
    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    /// Returns the source ownership associated with every materialized record.
    #[must_use]
    pub fn provenance(&self) -> &SceneEntityProvenanceMap {
        &self.provenance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidSceneProvenance;

pub(crate) fn validate_scene_provenance(
    document: &SceneDocument,
    provenance: &SceneEntityProvenanceMap,
) -> Result<(), InvalidSceneProvenance> {
    let entities = document
        .entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    if entities.len() != document.entities.len() || entities.len() != provenance.len() {
        return Err(InvalidSceneProvenance);
    }
    for entity in &document.entities {
        let source = provenance.get(&entity.id).ok_or(InvalidSceneProvenance)?;
        match source.projection_path() {
            None if source.source_entity_id() == &entity.id => {}
            Some(path)
                if path
                    .source_chain()
                    .last()
                    .is_some_and(|step| step.source_entity() == source.source_entity_id())
                    && provenance.get(path.root_anchor()).is_some_and(|anchor| {
                        anchor.projection_path().is_none()
                            && anchor.source_entity_id() == path.root_anchor()
                    }) => {}
            _ => return Err(InvalidSceneProvenance),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PrefabProjectionPath, SceneEntityId, SceneEntityRecord};
    use nara_asset::AssetRef;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    #[test]
    fn plain_candidate_shares_the_exact_document_and_supplies_local_sources() {
        let document = Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]));
        let candidate = ExpandedSceneCandidate::plain(Arc::clone(&document));
        assert!(std::ptr::eq(candidate.document(), document.as_ref()));
        assert_eq!(candidate.provenance().len(), 2);
        for record in &document.entities {
            assert_eq!(
                candidate.provenance()[&record.id].source_entity_id(),
                &record.id
            );
            assert!(
                candidate.provenance()[&record.id]
                    .projection_path()
                    .is_none()
            );
        }
    }

    #[test]
    fn expansion_candidate_rejects_incomplete_rekeyed_and_swapped_sources() {
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]);
        let original = ExpandedSceneCandidate::plain(Arc::new(document.clone()));
        let provenance = original.provenance();
        let mut missing = provenance.clone();
        missing.remove(&id("left"));
        let mut extra = provenance.clone();
        extra.insert(id("extra"), SceneEntityProvenance::local(id("extra")));
        let mut rekeyed = provenance.clone();
        let source = rekeyed.remove(&id("left")).unwrap();
        rekeyed.insert(id("stale"), source);
        let swapped = [
            (id("left"), provenance[&id("right")].clone()),
            (id("right"), provenance[&id("left")].clone()),
        ]
        .into_iter()
        .collect();
        for invalid in [missing, extra, rekeyed, swapped] {
            assert_eq!(
                ExpandedSceneCandidate::from_expansion(document.clone(), invalid),
                Err(InvalidSceneProvenance)
            );
        }
    }

    #[test]
    fn expansion_candidate_admits_only_consistent_projected_source_paths() {
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("anchor")),
            SceneEntityRecord::new(id("prefab_0")),
        ]);
        let path = PrefabProjectionPath::new(id("anchor")).with_step(
            AssetRef::path("prefabs/source.ron").unwrap(),
            crate::tests::prefab_source_key(&AssetRef::path("prefabs/source.ron").unwrap()),
            id("source"),
        );
        let provenance = [
            (id("anchor"), SceneEntityProvenance::local(id("anchor"))),
            (
                id("prefab_0"),
                SceneEntityProvenance::projected(id("source"), path.clone()),
            ),
        ]
        .into_iter()
        .collect::<SceneEntityProvenanceMap>();
        assert!(
            ExpandedSceneCandidate::from_expansion(document.clone(), provenance.clone()).is_ok()
        );
        let mut stale = provenance.clone();
        stale.insert(
            id("prefab_0"),
            SceneEntityProvenance::projected(id("stale"), path),
        );
        assert_eq!(
            ExpandedSceneCandidate::from_expansion(document.clone(), stale),
            Err(InvalidSceneProvenance)
        );
        let mut missing_anchor = provenance;
        missing_anchor.insert(
            id("prefab_0"),
            SceneEntityProvenance::projected(
                id("source"),
                PrefabProjectionPath::new(id("absent")).with_step(
                    AssetRef::path("prefabs/source.ron").unwrap(),
                    crate::tests::prefab_source_key(&AssetRef::path("prefabs/source.ron").unwrap()),
                    id("source"),
                ),
            ),
        );
        assert_eq!(
            ExpandedSceneCandidate::from_expansion(document, missing_anchor),
            Err(InvalidSceneProvenance)
        );
    }

    #[test]
    fn independent_expansions_keep_same_generated_keys_bound_to_their_own_sources() {
        use crate::{
            InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, ScenePatchDocument,
        };
        use nara_reflect::ComponentRegistry;
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let expand = |path: &str, source_id: &str| {
            let source = AssetRef::path(path).unwrap();
            let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
                source.clone(),
                crate::tests::prefab_source_key(&source),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id(source_id))]),
            );
            let mut anchor = SceneEntityRecord::new(id("anchor"));
            anchor.prefab = Some(PrefabInstance {
                source,
                overrides: ScenePatchDocument::default(),
            });
            SceneDocument::from_ordered_roots([anchor])
                .expand_prefabs(&registry, &resolver)
                .candidate
                .unwrap()
        };
        let first = expand("prefabs/first.ron", "first");
        let second = expand("prefabs/second.ron", "second");
        let (first_key, first_source) = first
            .provenance()
            .iter()
            .find(|(_, source)| source.projection_path().is_some())
            .unwrap();
        let (second_key, second_source) = second
            .provenance()
            .iter()
            .find(|(_, source)| source.projection_path().is_some())
            .unwrap();
        assert_eq!(first_key, second_key);
        assert_eq!(first_source.source_entity_id(), &id("first"));
        assert_eq!(second_source.source_entity_id(), &id("second"));
        assert_ne!(
            first_source.projection_path(),
            second_source.projection_path()
        );
    }
}
