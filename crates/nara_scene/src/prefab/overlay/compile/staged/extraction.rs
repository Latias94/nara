//! Source-owned cut membership from the original compiler's structural closure.
//!
//! This observation neither interprets component meaning nor proves relocation, source
//! publication or Native admission. Complete physical records remain in the borrowed source.

use super::*;

/// Positions refer only to the exact immutable input used for structural closure.
/// Nested declarations follow their containing record rather than visual parentage.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceCutMembership {
    pub(crate) root: usize,
    pub(crate) entities: Vec<usize>,
    /// Snapshot-local canonical equality, including all surviving declaration facets. These
    /// ordinals have no source/Runtime identity or authority outside this captured structure.
    pub(crate) targets: BTreeMap<PrefabEntityTarget, usize>,
    pub(crate) target_fact_count: usize,
}

pub(in crate::prefab::overlay::compile) fn capture_source_cut<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: SourceTopology<'_>,
    root: &SceneEntityId,
) -> Result<SourceCutMembership, SceneCompilationFailure>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    // Admission precedes even a failed borrowed lookup. A missing root must not scan an
    // arbitrarily large immutable source outside the caller's cumulative traversal allowance.
    context
        .admit_terminal_work(source.entities.len(), 0, Some(0))
        .map_err(|_| SceneCompilationFailureKind::WorkLimit)?;
    let lookup_bytes = source.entities.iter().try_fold(0usize, |bytes, entity| {
        bytes
            .checked_add(entity.id.as_str().len())?
            .checked_add(root.as_str().len())
    });
    context
        .admit_terminal_work(0, lookup_bytes.unwrap_or(usize::MAX), Some(0))
        .map_err(|_| SceneCompilationFailureKind::WorkLimit)?;
    // Reject a missing root before opening any source or cloning its ID.
    let root = source
        .entities
        .iter()
        .position(|entity| &entity.id == root)
        .ok_or(SceneCompilationFailureKind::MissingTarget)?;
    let mut compiler = Compiler::start(context, source, None)?;
    let mut scanned = 0;
    let layout = compiler.close_structure(&mut scanned)?;
    compiler.source_owned_cut(root, &layout)
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn source_owned_cut(
        &mut self,
        root: usize,
        layout: &SceneDraft,
    ) -> Result<SourceCutMembership, SceneCompilationFailure> {
        // Borrowed hierarchy/visited indexes, traversal stack and the direct-record result all
        // share the compiler's original finite allowance before their first allocation.
        self.admit(layout.entities.len().checked_mul(256).and_then(|bytes| {
            bytes.checked_add(self.root.entities.len().checked_mul(size_of::<usize>())?)
        }))?;
        let index = layout
            .entities
            .iter()
            .map(|entity| (&entity.id, entity))
            .collect::<BTreeMap<_, _>>();
        let root_node = *self.scopes[0]
            .nodes
            .source_entity(&self.root.entities[root].id)
            .ok_or(SceneCompilationFailureKind::MissingTarget)?;
        let canonical = self.canonical_node(0, root_node);
        let root_locator = &self.nodes[canonical].locator;
        let mut pending = Vec::with_capacity(layout.entities.len());
        let mut selected = BTreeSet::new();
        pending.push(root_locator);
        while let Some(entity) = pending.pop() {
            if !selected.insert(entity) {
                return Err(SceneCompilationFailureKind::OwnerAdmission.into());
            }
            let record = index
                .get(entity)
                .ok_or(SceneCompilationFailureKind::MissingTarget)?;
            pending.extend(record.children.iter());
        }
        let mut entities = Vec::with_capacity(self.root.entities.len());
        for (position, record) in self.root.entities.iter().enumerate() {
            let node = *self.scopes[0]
                .nodes
                .source_entity(&record.id)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let canonical = self.canonical_node(0, node);
            if selected.contains(&self.nodes[canonical].locator) {
                entities.push(position);
            }
        }
        self.admit(
            self.target_index_bytes(0)
                .and_then(|bytes| bytes.checked_add(self.scopes[0].output.len().checked_mul(256)?)),
        )?;
        let targets = self.target_nodes(0)?;
        let targets = targets
            .into_iter()
            .map(|(target, locator)| {
                let node = self.locators[&locator];
                (target, self.canonical_node(0, node))
            })
            .collect();
        Ok(SourceCutMembership {
            root,
            entities,
            targets,
            target_fact_count: self.nodes.len(),
        })
    }
}
