//! Whole-open-set Schema preparation preserves each document's source and history owner.

use std::{mem::size_of, sync::Arc};

use nara_scene::{
    __private::{
        PrefabSchemaReclassificationCommit, PreparedPrefabSchemaSource, PreparedSceneSchemaSource,
        RetainedSchemaReclassificationSource, SceneSchemaReclassificationCommit,
        borrow_prepared_prefab_schema_source, borrow_prepared_scene_schema_source,
        require_schema_reclassification_successor,
    },
    AuthoringSchemaBasis, PreparedPrefabSchemaReclassification,
    PreparedSceneSchemaReclassification, SchemaReclassificationWork,
};

use super::{
    EditorDocumentId, EditorDocumentSlot, EditorDocumentSourceBinding, EditorWorkspace,
    EditorWorkspaceCommandReport, EditorWorkspacePersistenceIdentity, WorkspaceDocumentContext,
    workspace_error_report,
};

/// One move-only successor for the exact complete open set of its original workspace.
///
/// This proves source classification and exact source-history replay. The project Host must
/// separately prepare actual current dependency composition and file publication.
#[derive(Debug)]
pub struct PreparedEditorWorkspaceSchemaReclassification {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
    sources: Vec<PreparedSource>,
}

#[derive(Debug)]
struct PreparedSource {
    document: EditorDocumentId,
    binding: Option<EditorDocumentSourceBinding>,
    classification: SourceClassification,
}

#[derive(Debug)]
enum SourceClassification {
    Scene(PreparedSceneSchemaReclassification),
    Prefab(PreparedPrefabSchemaReclassification),
}

/// Exclusive source guards, all successfully bound before the first classification changes.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorWorkspaceSchemaReclassificationCommit<'a> {
    sources: Vec<SourceCommit<'a>>,
}

#[derive(Debug)]
enum SourceCommit<'a> {
    Scene(SceneSchemaReclassificationCommit<'a>),
    Prefab(PrefabSchemaReclassificationCommit<'a>),
}

impl EditorWorkspaceSchemaReclassificationCommit<'_> {
    /// Performs only infallible classification swaps. No source or history is replaced.
    pub fn commit(self) {
        for source in self.sources {
            match source {
                SourceCommit::Scene(source) => source.commit(),
                SourceCommit::Prefab(source) => source.commit(),
            }
        }
    }
}

impl PreparedEditorWorkspaceSchemaReclassification {
    #[must_use]
    pub const fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.successor
    }

    pub fn documents(&self) -> impl ExactSizeIterator<Item = EditorDocumentId> + '_ {
        self.sources.iter().map(|source| source.document)
    }

    /// Borrows one exact original Scene under its prepared successor for Host capture.
    #[doc(hidden)]
    pub fn scene_source<'a>(
        &'a self,
        workspace: &'a EditorWorkspace,
        document: EditorDocumentId,
    ) -> Result<PreparedSceneSchemaSource<'a>, EditorWorkspaceCommandReport> {
        self.validate_owner(workspace)?;
        let source = self.find_source(workspace, document)?;
        let Some(EditorDocumentSlot::Scene(slot)) = workspace.documents.get(&document) else {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source kind changed",
            ));
        };
        if source.binding.as_ref() != slot.source_binding.as_ref() {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source binding changed",
            ));
        }
        let SourceClassification::Scene(prepared) = &source.classification else {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source is not a Scene",
            ));
        };
        borrow_prepared_scene_schema_source(&slot.session, prepared)
            .map_err(|report| source_report(workspace.active_document, document, report))
    }

    /// Borrows one exact original Prefab, including inactive dirty working content.
    #[doc(hidden)]
    pub fn prefab_source<'a>(
        &'a self,
        workspace: &'a EditorWorkspace,
        document: EditorDocumentId,
    ) -> Result<PreparedPrefabSchemaSource<'a>, EditorWorkspaceCommandReport> {
        self.validate_owner(workspace)?;
        let source = self.find_source(workspace, document)?;
        let Some(EditorDocumentSlot::Prefab(slot)) = workspace.documents.get(&document) else {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source kind changed",
            ));
        };
        if source.binding.as_ref() != Some(&slot.source_binding) {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source binding changed",
            ));
        }
        let SourceClassification::Prefab(prepared) = &source.classification else {
            return Err(refused(
                workspace.active_document,
                Some(document),
                "Prepared source is not a Prefab",
            ));
        };
        borrow_prepared_prefab_schema_source(&slot.session, prepared)
            .map_err(|report| source_report(workspace.active_document, document, report))
    }

    /// Revalidates the complete set without publishing, for the Host's pre-write fence.
    #[doc(hidden)]
    pub fn validate(
        &self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.validate_owner(workspace)?;
        if self.sources.len() != workspace.documents.len() {
            return Err(refused(
                workspace.active_document,
                None,
                "The complete open document set changed",
            ));
        }
        for (source, (document, slot)) in self.sources.iter().zip(&workspace.documents) {
            if source.document != *document
                || source.binding.as_ref() != slot.source_binding()
                || slot.authoring_schema_basis().as_ref() != Some(&self.previous)
            {
                return Err(refused(
                    workspace.active_document,
                    Some(*document),
                    "The original open document set or basis changed",
                ));
            }
            match &source.classification {
                SourceClassification::Scene(_) => {
                    self.scene_source(workspace, *document)?;
                }
                SourceClassification::Prefab(_) => {
                    self.prefab_source(workspace, *document)?;
                }
            }
        }
        Ok(())
    }

    fn validate_owner(
        &self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        if Arc::ptr_eq(&self.owner, &workspace.persistence_identity) {
            Ok(())
        } else {
            Err(refused(
                workspace.active_document,
                None,
                "Prepared Schema set belongs to another workspace",
            ))
        }
    }

    fn find_source(
        &self,
        workspace: &EditorWorkspace,
        document: EditorDocumentId,
    ) -> Result<&PreparedSource, EditorWorkspaceCommandReport> {
        self.sources
            .binary_search_by_key(&document, |source| source.document)
            .map(|index| &self.sources[index])
            .map_err(|_| {
                refused(
                    workspace.active_document,
                    Some(document),
                    "Document is outside the prepared Schema set",
                )
            })
    }
}

impl EditorWorkspace {
    /// Prepares the complete set against one nominal successor without changing any live source.
    ///
    /// All incumbent sources/classifications/history are charged before the first successor.
    /// Native-only sessions and extension owners reject this operation explicitly; their
    /// independent authorities cannot silently remain under a contradictory project basis.
    pub fn prepare_schema_reclassification(
        &self,
        previous: &AuthoringSchemaBasis,
        successor: &AuthoringSchemaBasis,
        work: &mut SchemaReclassificationWork,
    ) -> Result<PreparedEditorWorkspaceSchemaReclassification, EditorWorkspaceCommandReport> {
        // Check unsupported owners and exact prior admissions before any preparation allocation.
        for (document, slot) in &self.documents {
            if matches!(slot, EditorDocumentSlot::Extension(_))
                || slot.authoring_schema_basis().as_ref() != Some(previous)
            {
                return Err(refused(
                    self.active_document,
                    Some(*document),
                    "Every open source must share the original authoring Schema basis",
                ));
            }
        }
        require_schema_reclassification_successor(previous, successor)
            .map_err(|report| source_report_for_set(self.active_document, report))?;
        let storage = size_of::<PreparedSource>()
            .checked_add(size_of::<(
                EditorDocumentId,
                RetainedSchemaReclassificationSource,
            )>())
            .and_then(|bytes| bytes.checked_add(size_of::<SourceCommit<'_>>()))
            .and_then(|bytes| bytes.checked_mul(self.documents.len()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<PreparedEditorWorkspaceSchemaReclassification>())
            });
        work.retain_preparation_storage(storage.unwrap_or(usize::MAX))
            .map_err(|report| source_report_for_set(self.active_document, report))?;
        let mut incumbents = Vec::with_capacity(self.documents.len());
        for (document, slot) in &self.documents {
            let incumbent = match slot {
                EditorDocumentSlot::Scene(slot) => {
                    slot.session.retain_schema_reclassification_source(work)
                }
                EditorDocumentSlot::Prefab(slot) => {
                    slot.session.retain_schema_reclassification_source(work)
                }
                EditorDocumentSlot::Extension(_) => {
                    unreachable!("unsupported owners were checked before allocation")
                }
            }
            .map_err(|report| source_report(self.active_document, *document, report))?;
            incumbents.push((*document, incumbent));
        }
        let mut sources = Vec::with_capacity(self.documents.len());
        for ((document, slot), (retained_document, incumbent)) in
            self.documents.iter().zip(incumbents)
        {
            debug_assert_eq!(*document, retained_document);
            let classification = match slot {
                EditorDocumentSlot::Scene(slot) => slot
                    .session
                    .prepare_schema_reclassification_with_retained_source(
                        successor, work, incumbent,
                    )
                    .map(SourceClassification::Scene),
                EditorDocumentSlot::Prefab(slot) => slot
                    .session
                    .prepare_schema_reclassification_with_retained_source(
                        successor, work, incumbent,
                    )
                    .map(SourceClassification::Prefab),
                EditorDocumentSlot::Extension(_) => {
                    unreachable!("unsupported owners were checked before allocation")
                }
            }
            .map_err(|report| source_report(self.active_document, *document, report))?;
            sources.push(PreparedSource {
                document: *document,
                binding: slot.source_binding().cloned(),
                classification,
            });
        }
        Ok(PreparedEditorWorkspaceSchemaReclassification {
            owner: Arc::clone(&self.persistence_identity),
            previous: previous.clone(),
            successor: successor.clone(),
            sources,
        })
    }

    /// Binds every original source exclusively before returning an infallible whole-set commit.
    /// A refusal drops earlier guards without publishing their classifications.
    #[doc(hidden)]
    pub fn bind_schema_reclassification(
        &mut self,
        prepared: PreparedEditorWorkspaceSchemaReclassification,
    ) -> Result<EditorWorkspaceSchemaReclassificationCommit<'_>, EditorWorkspaceCommandReport> {
        prepared.validate(self)?;
        let active = self.active_document;
        let mut sources = Vec::with_capacity(prepared.sources.len());
        for ((document, slot), prepared) in self.documents.iter_mut().zip(prepared.sources) {
            let source = match (slot, prepared.classification) {
                (EditorDocumentSlot::Scene(slot), SourceClassification::Scene(prepared)) => slot
                    .session
                    .bind_schema_reclassification(prepared)
                    .map(SourceCommit::Scene),
                (EditorDocumentSlot::Prefab(slot), SourceClassification::Prefab(prepared)) => slot
                    .session
                    .bind_schema_reclassification(prepared)
                    .map(SourceCommit::Prefab),
                _ => {
                    return Err(refused(
                        active,
                        Some(*document),
                        "Prepared source kind changed",
                    ));
                }
            }
            .map_err(|report| source_report(active, *document, report))?;
            sources.push(source);
        }
        Ok(EditorWorkspaceSchemaReclassificationCommit { sources })
    }
}

fn refused(
    active: Option<EditorDocumentId>,
    document: Option<EditorDocumentId>,
    message: &'static str,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active, document),
        "tooling.workspace-schema-reclassification-rejected",
        message,
    )
}

fn source_report(
    active: Option<EditorDocumentId>,
    document: EditorDocumentId,
    report: nara_scene::ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    let mut result = source_report_for_set(active, report);
    result.document = Some(document);
    result
}

fn source_report_for_set(
    active: Option<EditorDocumentId>,
    report: nara_scene::ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        applied: false,
        active_document: active,
        diagnostics: report.diagnostics.clone(),
        patch_report: Some(report),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
