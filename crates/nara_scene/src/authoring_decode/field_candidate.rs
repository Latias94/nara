//! Classification reuse for a candidate constructed exclusively by field-value mutation.

use super::*;

pub(super) struct PreparedFieldCandidate<'a> {
    state: &'a AuthoringDocumentState,
    document: SceneDocument,
    inverse: ScenePatchDocument,
    touched: BTreeSet<(SceneEntityId, ComponentTypeId)>,
}

pub(super) fn prepare<'a>(
    document: SceneDocument,
    state: &'a AuthoringDocumentState,
    edits: &[AuthoringFieldChange],
) -> Result<PreparedFieldCandidate<'a>, ScenePatchReport> {
    // The mutation owner changes only record.value, never source identity, placement, Prefab
    // overrides, component identity, owner or version. Do not expose an arbitrary-document
    // constructor: these unchanged facts are the proof for reusing the previous classification.
    let (mut document, inverse) = prepare_field_edits(document, state, edits)?;
    field_edits_from_patch(&inverse)?;
    document.canonicalize();
    Ok(PreparedFieldCandidate {
        state,
        document,
        inverse,
        touched: edits
            .iter()
            .map(|edit| (edit.entity.clone(), edit.component.clone()))
            .collect(),
    })
}

impl PreparedFieldCandidate<'_> {
    pub(super) fn document(&self) -> &SceneDocument {
        &self.document
    }

    pub(super) fn validate_prefab_shape(mut self) -> Result<Self, crate::SceneFormatError> {
        // Move the nominal projection through its own format boundary; no second value tree.
        let prefab = PrefabDocument {
            roots: self.document.roots,
            entities: self.document.entities,
        };
        crate::format::validate_prefab_publication_shape(&prefab, self.state.limits)?;
        self.document = SceneDocument {
            roots: prefab.roots,
            entities: prefab.entities,
        };
        Ok(self)
    }

    pub(super) fn into_parts(self) -> (SceneDocument, ScenePatchDocument) {
        (self.document, self.inverse)
    }

    pub(super) fn classify(
        &self,
    ) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
        let state = self.state;
        match self.try_classify()? {
            Some(state) => Ok(state),
            None => classify_scene_document(
                &self.document,
                state.source.clone(),
                state.kind,
                Arc::clone(&state.basis),
                state.limits,
            ),
        }
    }

    fn try_classify(
        &self,
    ) -> Result<Option<AuthoringDocumentState>, AuthoringDocumentPublicationError> {
        let state = self.state;
        // Reference projections have a document-wide allowance and full-classifier order.
        // Reuse the scalar fast path only when neither retained nor touched data can add facts;
        // do not admit an edited reference subtree against a fresh, independent allowance.
        if state.schema_declared_asset_references().next().is_some() {
            return Ok(None);
        }
        let mut targets = Vec::with_capacity(self.touched.len());
        // Retain full-classifier order, including the first error in a multi-record edit.
        for (index, fact) in state.record_facts.iter().enumerate() {
            let AuthoringRecordLocation::EntityComponent { entity } = &fact.location else {
                continue;
            };
            if !self
                .touched
                .iter()
                .any(|(target, component)| target == entity && component == &fact.component)
            {
                continue;
            }
            let Some(record) = self
                .document
                .entities
                .iter()
                .find(|record| &record.id == entity)
                .and_then(|record| record.components.get(&fact.component))
            else {
                return Ok(None);
            };
            let owner = record
                .owner()
                .map_or(AuthoringRecordOwnerClaim::Unclaimed, |owner| {
                    AuthoringRecordOwnerClaim::Declared(owner.clone())
                });
            let Some(schema) = state.available_component_schema(entity, &fact.component) else {
                return Ok(None);
            };
            if !fact.readiness.is_available()
                || fact.owner != owner
                || fact.version != record.version
            {
                return Ok(None);
            }
            if schema.fields().iter().any(|field| {
                field
                    .shape()
                    .contains_kind(nara_reflect::ComponentValueKind::AssetRef)
            }) {
                return Ok(None);
            }
            targets.push((index, fact, record));
        }
        if targets.len() != self.touched.len() {
            return Ok(None);
        }
        let mut context = ClassificationContext {
            source: &state.source,
            kind: state.kind,
            basis: &state.basis,
            record_facts: Vec::with_capacity(targets.len()),
            overlay_asset_references: Vec::new(),
            manifest: Vec::new(),
            // Available direct records produce no preservation placement or overlay facts.
            placements: BTreeMap::new(),
            namespace: self
                .document
                .entities
                .iter()
                .map(|entity| &entity.id)
                .collect(),
            limits: state.limits,
            property_read_budget: nara_reflect::ComponentPropertyReadBudget::new(
                state.limits.component_value_nodes(),
                state.limits.component_value_bytes(),
            ),
            asset_reference_nodes: 0,
            asset_reference_bytes: 0,
        };
        for (_, fact, record) in &targets {
            classify_complete_record(
                fact.location.clone(),
                None,
                &fact.component,
                record,
                &mut context,
            )?;
        }
        if !context.manifest.is_empty()
            || context
                .record_facts
                .iter()
                .any(|fact| !fact.readiness.is_available())
        {
            return Ok(None);
        }
        let mut next = state.clone();
        for ((index, _, _), fact) in targets.into_iter().zip(context.record_facts) {
            next.record_facts[index] = fact;
        }
        // Only Available values changed. The exact unavailable manifest, diagnostics, readiness,
        // source/basis and direct schema index are consequently the same admitted facts.
        Ok(Some(next))
    }
}

#[cfg(test)]
mod tests;
