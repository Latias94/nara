use std::collections::BTreeSet;

use crate::{
    SceneEntityId, SceneEntityProvenanceMap, SourceDocumentKey,
    draft::{ExpandedSceneEntity, SceneDraft},
};

/// One immutable authoring observation and its complete document-relative provenance.
///
/// Authoring compilation creates this value while resolving its actual selected sources.
/// Callers may inspect it, but cannot substitute a document or a provenance map independently.
/// It grants no Native activation authority. The original compilation report alone can restrict
/// its complete projection and seal compiled data under the original admission allowance.
/// The value is move-only so a retained-source borrow cannot extend its allocations independently
/// of the owning retention lease.
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneDocument;
/// fn replace_document(expanded: &mut ExpandedSceneDocument) {
///     expanded.document = Default::default();
/// }
/// ```
///
/// Borrowed expanded documents do not allow their allocations to escape:
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneDocument;
/// fn escape(expanded: &ExpandedSceneDocument) -> ExpandedSceneDocument {
///     expanded.clone()
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct ExpandedSceneDocument {
    document: SceneDraft,
    provenance: SceneEntityProvenanceMap,
}

impl ExpandedSceneDocument {
    /// Transfers an exclusive compiler result without constructing another source copy.
    pub(crate) fn into_compiler_parts(self) -> (SceneDraft, SceneEntityProvenanceMap) {
        (self.document, self.provenance)
    }

    /// Filters only component presentation in an exclusively owned compiler result.
    /// Entity identity, hierarchy and provenance remain paired with the original expansion.
    #[cfg(feature = "serde")]
    pub(crate) fn remove_compiler_components<'a>(
        mut self,
        removals: impl IntoIterator<
            Item = (&'a crate::SceneEntityId, &'a nara_reflect::ComponentTypeId),
        >,
    ) -> Self {
        let document = &mut self.document;
        for (entity, component) in removals {
            if let Ok(index) = document
                .entities
                .binary_search_by(|record| record.id.cmp(entity))
            {
                document.entities[index].components.remove(component);
            }
        }
        self
    }

    pub(crate) fn from_compilation(
        document: SceneDraft,
        provenance: SceneEntityProvenanceMap,
    ) -> Result<Self, InvalidSceneProvenance> {
        validate_scene_provenance(&document, &provenance)?;
        Ok(Self {
            document,
            provenance,
        })
    }

    /// Borrows the ordered roots of this expanded observation.
    #[must_use]
    pub fn roots(&self) -> &[SceneEntityId] {
        &self.document.roots
    }

    /// Allocation capacity retained by this observation, for its Host's storage allowance.
    #[must_use]
    pub fn roots_capacity(&self) -> usize {
        self.document.roots.capacity()
    }

    /// Borrows expanded records in entity-ID order.
    #[must_use]
    pub fn entities(&self) -> &[ExpandedSceneEntity] {
        &self.document.entities
    }

    /// Allocation capacity retained by the entity container, independent of its live length.
    #[must_use]
    pub fn entities_capacity(&self) -> usize {
        self.document.entities.capacity()
    }

    /// Returns the source ownership associated with every materialized record.
    #[must_use]
    pub fn provenance(&self) -> &SceneEntityProvenanceMap {
        &self.provenance
    }
}

/// Closed borrowed inputs to the existing Scene materialization owner.
/// Runtime candidates retain their exact admitted definitions and processed-value contract.
#[derive(Clone, Copy)]
pub(crate) enum SceneMaterializationInput<'a> {
    Compiled(&'a crate::CompiledSceneCandidate),
    #[cfg(feature = "serde")]
    Runtime(&'a crate::runtime_scene::RuntimeSceneCandidate),
}

impl<'a> SceneMaterializationInput<'a> {
    pub(crate) fn source_key(self) -> &'a SourceDocumentKey {
        match self {
            Self::Compiled(candidate) => candidate.source_key(),
            #[cfg(feature = "serde")]
            Self::Runtime(candidate) => candidate.source_key(),
        }
    }

    pub(crate) fn document(self) -> &'a crate::CompiledSceneDocument {
        match self {
            Self::Compiled(candidate) => candidate.document(),
            #[cfg(feature = "serde")]
            Self::Runtime(candidate) => candidate.document(),
        }
    }

    pub(crate) fn provenance(self) -> &'a SceneEntityProvenanceMap {
        match self {
            Self::Compiled(candidate) => candidate.canonical_provenance(),
            #[cfg(feature = "serde")]
            Self::Runtime(candidate) => candidate.canonical_provenance(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidSceneProvenance;

pub(crate) fn validate_scene_provenance(
    document: &SceneDraft,
    provenance: &SceneEntityProvenanceMap,
) -> Result<(), InvalidSceneProvenance> {
    validate_provenance_entries(
        document.entities.iter().map(|entity| &entity.id),
        provenance,
    )
}

pub(crate) fn validate_flat_scene_provenance(
    claims: &crate::compiled::claims::FlatSceneClaims,
) -> Result<(), InvalidSceneProvenance> {
    validate_provenance_entries(
        claims.entities.iter().map(|entity| &entity.id),
        &claims.canonical_provenance,
    )
}

fn validate_provenance_entries<'a>(
    entities: impl ExactSizeIterator<Item = &'a crate::SceneEntityId>,
    provenance: &SceneEntityProvenanceMap,
) -> Result<(), InvalidSceneProvenance> {
    let count = entities.len();
    let entities = entities.collect::<BTreeSet<_>>();
    if entities.len() != count || entities.len() != provenance.len() {
        return Err(InvalidSceneProvenance);
    }
    for entity in entities {
        let source = provenance.get(entity).ok_or(InvalidSceneProvenance)?;
        match source.projection_path() {
            None if source.local_entity_id() == entity => {}
            Some(path)
                if path
                    .steps()
                    .last()
                    .is_some_and(|step| step.entity_id() == source.local_entity_id())
                    && provenance.get(path.root_anchor()).is_some_and(|anchor| {
                        anchor.projection_path().is_none()
                            && anchor.local_entity_id() == path.root_anchor()
                    }) => {}
            _ => return Err(InvalidSceneProvenance),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SceneEntityProvenance;
    use crate::{PrefabProjectionPath, SceneDocument, SceneEntityRecord, SourceDocumentKind};
    use nara_asset::AssetRef;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn draft(ids: &[&str]) -> SceneDraft {
        SceneDraft::new(
            ids.iter().map(|value| id(value)),
            ids.iter().map(|value| ExpandedSceneEntity::new(id(value))),
        )
    }

    fn scene_source_key(name: &str) -> SourceDocumentKey {
        SourceDocumentKey::named(
            "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
            name,
            SourceDocumentKind::Scene,
        )
        .unwrap()
    }

    #[test]
    fn exclusive_authoring_observation_moves_its_original_storage_back_to_the_compiler() {
        let document = draft(&["local"]);
        let origin = SceneEntityProvenance::local(id("local"));
        let expanded = ExpandedSceneDocument::from_compilation(
            document,
            [(id("local"), origin.clone())].into_iter().collect(),
        )
        .unwrap();
        let roots = expanded.roots().as_ptr();
        let entities = expanded.entities().as_ptr();
        let (document, provenance) = expanded.into_compiler_parts();
        assert_eq!(document.roots.as_ptr(), roots);
        assert_eq!(document.entities.as_ptr(), entities);
        assert_eq!(provenance[&id("local")], origin);
    }

    #[test]
    fn distinct_containing_sources_keep_the_same_document_local_identity_separate() {
        use nara_reflect::ComponentRegistryBuilder;
        let document =
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("shared-local-id"))]);
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let resolver = crate::InMemoryPrefabSourceResolver::new();
        let first_key = scene_source_key("scenes.first");
        let second_key = scene_source_key("scenes.second");
        let first = document
            .compile(&registry, &resolver)
            .candidate
            .unwrap()
            .bind_source(first_key.clone());
        let second = document
            .compile(&registry, &resolver)
            .candidate
            .unwrap()
            .bind_source(second_key.clone());

        assert_eq!(first.source_key(), &first_key);
        assert_eq!(second.source_key(), &second_key);
        assert_ne!(first, second);
        assert_eq!(first.canonical_provenance(), second.canonical_provenance());
    }

    #[test]
    fn binding_a_source_moves_the_original_compiled_storage_and_canonical_provenance() {
        use crate::{InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance};
        use nara_reflect::ComponentRegistryBuilder;

        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let request = AssetRef::path("prefabs/source.ron").unwrap();
        let prefab_key = crate::tests::prefab_source_key(&request);
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            prefab_key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("source"))]),
        );
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(PrefabInstance::forest(request));
        let report = SceneDocument::from_ordered_roots([anchor]).compile(&registry, &resolver);
        assert!(!report.diagnostics.has_errors());
        let expanded = report.candidate.unwrap();
        let document_pointer = expanded.entities().as_ptr();
        let provenance_pointers = expanded
            .canonical_provenance()
            .iter()
            .map(|(entity, source)| (entity.clone(), std::ptr::from_ref(source)))
            .collect::<Vec<_>>();
        let expected_provenance = expanded.canonical_provenance().clone();
        let source_key = scene_source_key("scenes.containing");

        let candidate = expanded.bind_source(source_key.clone());

        assert_eq!(candidate.source_key(), &source_key);
        assert_eq!(candidate.document().entities().as_ptr(), document_pointer);
        assert_eq!(candidate.canonical_provenance(), &expected_provenance);
        for (entity, pointer) in provenance_pointers {
            assert_eq!(
                std::ptr::from_ref(&candidate.canonical_provenance()[&entity]),
                pointer
            );
        }
        let projected = candidate
            .canonical_provenance()
            .values()
            .find_map(SceneEntityProvenance::projection_path)
            .unwrap();
        assert!(
            matches!(&projected.steps()[0], crate::PrefabProjectionStep::SourceEntity { canonical_source, .. } if canonical_source == &prefab_key)
        );
        assert_ne!(candidate.source_key(), &prefab_key);
    }

    #[test]
    fn expansion_document_rejects_incomplete_rekeyed_and_swapped_sources() {
        let document = draft(&["left", "right"]);
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
        let original = ExpandedSceneDocument::from_compilation(document, provenance).unwrap();
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
                ExpandedSceneDocument::from_compilation(draft(&["left", "right"]), invalid),
                Err(InvalidSceneProvenance)
            );
        }
    }

    #[test]
    fn expansion_document_admits_only_consistent_projected_source_paths() {
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
            ExpandedSceneDocument::from_compilation(
                draft(&["anchor", "prefab_0"]),
                provenance.clone(),
            )
            .is_ok()
        );
        let mut stale = provenance.clone();
        stale.insert(
            id("prefab_0"),
            SceneEntityProvenance::projected(id("stale"), path),
        );
        assert_eq!(
            ExpandedSceneDocument::from_compilation(draft(&["anchor", "prefab_0"]), stale),
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
            ExpandedSceneDocument::from_compilation(draft(&["anchor", "prefab_0"]), missing_anchor),
            Err(InvalidSceneProvenance)
        );
    }

    #[test]
    fn independent_expansions_keep_same_generated_keys_bound_to_their_own_sources() {
        use crate::{InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance};
        use nara_reflect::ComponentRegistryBuilder;
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let expand = |path: &str, source_id: &str| {
            let source = AssetRef::path(path).unwrap();
            let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
                source.clone(),
                crate::tests::prefab_source_key(&source),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id(source_id))]),
            );
            let mut anchor = SceneEntityRecord::new(id("anchor"));
            anchor.prefab = Some(PrefabInstance::forest(source));
            SceneDocument::from_ordered_roots([anchor])
                .compile(&registry, &resolver)
                .candidate
                .unwrap()
        };
        let first = expand("prefabs/first.ron", "first");
        let second = expand("prefabs/second.ron", "second");
        let (first_key, first_source) = first
            .canonical_provenance()
            .iter()
            .find(|(_, source)| source.projection_path().is_some())
            .unwrap();
        let (second_key, second_source) = second
            .canonical_provenance()
            .iter()
            .find(|(_, source)| source.projection_path().is_some())
            .unwrap();
        assert_eq!(first_key, second_key);
        assert_eq!(first_source.local_entity_id(), &id("first"));
        assert_eq!(second_source.local_entity_id(), &id("second"));
        assert_ne!(
            first_source.projection_path(),
            second_source.projection_path()
        );
    }
}
