use std::{collections::BTreeSet, sync::Arc};

use crate::{SceneDocument, SceneEntityProvenance, SceneEntityProvenanceMap, SourceDocumentKey};

/// One immutable expanded document and its complete document-relative provenance.
///
/// Prefab expansion creates this value with the provenance established while resolving sources.
/// Callers may inspect it, but cannot substitute a document or a provenance map independently.
/// [`Self::plain`] explicitly treats every record as local to the supplied scene.
/// No containing source or runtime occurrence is inferred by document-only expansion. The source
/// owner must consume this value with [`Self::bind_source`] before materialization.
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
    document: Arc<SceneDocument>,
    provenance: SceneEntityProvenanceMap,
}

impl ExpandedSceneDocument {
    /// Filters only component presentation in an exclusively owned compiler result.
    /// Entity identity, hierarchy and provenance remain paired with the original expansion.
    #[cfg(feature = "serde")]
    pub(crate) fn remove_compiler_components<'a>(
        mut self,
        removals: impl IntoIterator<
            Item = (&'a crate::SceneEntityId, &'a nara_reflect::ComponentTypeId),
        >,
    ) -> Self {
        let document = Arc::get_mut(&mut self.document)
            .expect("a move-only compiler result owns its document allocation exclusively");
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

    /// Retains a plain scene without copying its document allocation.
    ///
    /// Component, hierarchy and reference admission still occur when materializing the document.
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

    /// Binds the containing source without copying the document or its provenance.
    ///
    /// The source owner supplies the admitted identity; it is not inferred from document content,
    /// Prefab requests or runtime placement. Binding does not claim a runtime occurrence.
    #[must_use]
    pub fn bind_source(self, source_key: SourceDocumentKey) -> ExpandedSceneCandidate {
        ExpandedSceneCandidate {
            source_key,
            expanded: self,
        }
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

/// One immutable materialization input bound to its required containing source.
///
/// Construct a plain candidate with an explicit source, or consume an [`ExpandedSceneDocument`]
/// with [`ExpandedSceneDocument::bind_source`]. Document and provenance cannot be assembled or
/// replaced independently. The logical occurrence is supplied separately at materialization.
///
/// Retained-source views lend the candidate without allowing its allocations to escape:
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneCandidate;
/// fn escape(candidate: &ExpandedSceneCandidate) -> ExpandedSceneCandidate {
///     candidate.clone()
/// }
/// ```
///
/// The containing source cannot be replaced independently:
///
/// ```compile_fail
/// use nara_scene::{ExpandedSceneCandidate, SourceDocumentKey};
/// fn replace_source(candidate: &mut ExpandedSceneCandidate, source: SourceDocumentKey) {
///     candidate.source_key = source;
/// }
/// ```
///
/// A plain candidate cannot omit its source:
///
/// ```compile_fail
/// use std::sync::Arc;
/// use nara_scene::{ExpandedSceneCandidate, SceneDocument};
/// fn without_source(document: Arc<SceneDocument>) -> ExpandedSceneCandidate {
///     ExpandedSceneCandidate::plain(document)
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct ExpandedSceneCandidate {
    source_key: SourceDocumentKey,
    expanded: ExpandedSceneDocument,
}

impl ExpandedSceneCandidate {
    /// Retains a plain scene with explicit containing source and complete local provenance.
    ///
    /// Component, hierarchy and reference admission still occur when materializing the candidate.
    /// Prefab-bearing documents must first use the ordinary expansion APIs and bind their result.
    #[must_use]
    pub fn plain(source_key: SourceDocumentKey, document: Arc<SceneDocument>) -> Self {
        ExpandedSceneDocument::plain(document).bind_source(source_key)
    }

    /// Returns the containing source explicitly associated by its owner.
    #[must_use]
    pub const fn source_key(&self) -> &SourceDocumentKey {
        &self.source_key
    }

    /// Returns the exact immutable expanded document.
    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        self.expanded.document()
    }

    /// Returns the source ownership associated with every materialized record.
    #[must_use]
    pub fn provenance(&self) -> &SceneEntityProvenanceMap {
        self.expanded.provenance()
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
            None if source.local_entity_id() == &entity.id => {}
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
    use crate::{PrefabProjectionPath, SceneEntityId, SceneEntityRecord, SourceDocumentKind};
    use nara_asset::AssetRef;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
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
    fn plain_expanded_document_shares_the_exact_document_without_binding_a_source() {
        let document = Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("local"),
        )]));
        let expanded = ExpandedSceneDocument::plain(Arc::clone(&document));
        assert!(std::ptr::eq(expanded.document(), document.as_ref()));
        assert_eq!(expanded.provenance().len(), 1);
        assert_eq!(
            expanded.provenance()[&id("local")],
            SceneEntityProvenance::local(id("local"))
        );
        assert_eq!(Arc::strong_count(&document), 2);
    }

    #[test]
    fn plain_candidate_shares_the_exact_document_and_supplies_local_sources() {
        let document = Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]));
        let source_key = scene_source_key("scenes.main");
        let candidate = ExpandedSceneCandidate::plain(source_key.clone(), Arc::clone(&document));
        assert_eq!(candidate.source_key(), &source_key);
        assert!(std::ptr::eq(candidate.document(), document.as_ref()));
        assert_eq!(candidate.provenance().len(), 2);
        for record in &document.entities {
            assert_eq!(
                candidate.provenance()[&record.id].local_entity_id(),
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
    fn distinct_containing_sources_keep_the_same_document_local_identity_separate() {
        let document = Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("shared-local-id"),
        )]));
        let first_key = scene_source_key("scenes.first");
        let second_key = scene_source_key("scenes.second");
        let first = ExpandedSceneCandidate::plain(first_key.clone(), Arc::clone(&document));
        let second = ExpandedSceneCandidate::plain(second_key.clone(), Arc::clone(&document));

        assert_eq!(first.source_key(), &first_key);
        assert_eq!(second.source_key(), &second_key);
        assert_ne!(first, second);
        assert!(std::ptr::eq(first.document(), second.document()));
        assert_eq!(first.provenance(), second.provenance());
        assert_eq!(Arc::strong_count(&document), 3);
    }

    #[test]
    fn binding_a_source_moves_the_exact_expanded_document_and_provenance() {
        use crate::{InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance};
        use nara_reflect::ComponentRegistry;

        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let request = AssetRef::path("prefabs/source.ron").unwrap();
        let prefab_key = crate::tests::prefab_source_key(&request);
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            prefab_key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("source"))]),
        );
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(PrefabInstance {
            source: request,
            overrides: None,
        });
        let report =
            SceneDocument::from_ordered_roots([anchor]).expand_prefabs(&registry, &resolver);
        assert!(!report.diagnostics.has_errors());
        let expanded = report.candidate.unwrap();
        let document_pointer = std::ptr::from_ref(expanded.document());
        let provenance_pointers = expanded
            .provenance()
            .iter()
            .map(|(entity, source)| (entity.clone(), std::ptr::from_ref(source)))
            .collect::<Vec<_>>();
        let expected_provenance = expanded.provenance().clone();
        let source_key = scene_source_key("scenes.containing");

        let candidate = expanded.bind_source(source_key.clone());

        assert_eq!(candidate.source_key(), &source_key);
        assert_eq!(std::ptr::from_ref(candidate.document()), document_pointer);
        assert_eq!(candidate.provenance(), &expected_provenance);
        for (entity, pointer) in provenance_pointers {
            assert_eq!(
                std::ptr::from_ref(&candidate.provenance()[&entity]),
                pointer
            );
        }
        let projected = candidate
            .provenance()
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
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("left")),
            SceneEntityRecord::new(id("right")),
        ]);
        let original = ExpandedSceneDocument::plain(Arc::new(document.clone()));
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
                ExpandedSceneDocument::from_expansion(document.clone(), invalid),
                Err(InvalidSceneProvenance)
            );
        }
    }

    #[test]
    fn expansion_document_admits_only_consistent_projected_source_paths() {
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
            ExpandedSceneDocument::from_expansion(document.clone(), provenance.clone()).is_ok()
        );
        let mut stale = provenance.clone();
        stale.insert(
            id("prefab_0"),
            SceneEntityProvenance::projected(id("stale"), path),
        );
        assert_eq!(
            ExpandedSceneDocument::from_expansion(document.clone(), stale),
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
            ExpandedSceneDocument::from_expansion(document, missing_anchor),
            Err(InvalidSceneProvenance)
        );
    }

    #[test]
    fn independent_expansions_keep_same_generated_keys_bound_to_their_own_sources() {
        use crate::{InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance};
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
                overrides: None,
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
        assert_eq!(first_source.local_entity_id(), &id("first"));
        assert_eq!(second_source.local_entity_id(), &id("second"));
        assert_ne!(
            first_source.projection_path(),
            second_source.projection_path()
        );
    }
}
