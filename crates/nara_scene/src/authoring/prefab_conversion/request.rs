//! Owned preparation and dependency-fenced publication through the source history.

use std::{error::Error, fmt, sync::Arc};

use nara_asset::{AssetRef, ProjectAssetDatabase};

use super::localize::{self, CapturedPrefabSources};
use crate::{
    PrefabAuthoringSession, PrefabDocument, PrefabExpansionOptions, PrefabExpansionWorkingSet,
    PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit, ResolvedPrefabSource,
    SceneAuthoringHistoryLimits, SceneAuthoringRevision, SceneAuthoringSession, SceneDocument,
    SceneEntityId, SceneEntityProvenance, ScenePatchReport,
    authoring::{AuthoringHistoryScope, prepared::stale_basis},
    authoring_decode::AuthoringDocumentState,
    prefab::SelectedInstanceSource,
};

/// Atomic source refusal or the dependency provider's original acquisition error.
#[derive(Debug)]
pub enum PrefabConversionError<E> {
    Source(ScenePatchReport),
    Loader(E),
}

impl<E: fmt::Display> fmt::Display for PrefabConversionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(_) => {
                formatter.write_str("Prefab conversion was refused by the source owner")
            }
            Self::Loader(error) => write!(
                formatter,
                "Prefab conversion source acquisition failed: {error}"
            ),
        }
    }
}

impl<E: Error + 'static> Error for PrefabConversionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(_) => None,
            Self::Loader(error) => Some(error),
        }
    }
}

/// One complete projected occurrence and its newly allocated persistent source ID.
#[derive(Debug)]
pub struct LocalizedPrefabEntity {
    pub(super) provenance: SceneEntityProvenance,
    pub(super) entity: SceneEntityId,
}

impl LocalizedPrefabEntity {
    #[must_use]
    pub fn provenance(&self) -> &SceneEntityProvenance {
        &self.provenance
    }

    #[must_use]
    pub fn entity(&self) -> &SceneEntityId {
        &self.entity
    }
}

/// The exact accepted source transaction and the reviewed identity mapping.
#[derive(Debug)]
pub struct PrefabConversionReceipt {
    report: ScenePatchReport,
    entities: Vec<LocalizedPrefabEntity>,
}

impl PrefabConversionReceipt {
    #[must_use]
    pub fn report(&self) -> &ScenePatchReport {
        &self.report
    }

    #[must_use]
    pub fn entities(&self) -> &[LocalizedPrefabEntity] {
        &self.entities
    }
}

macro_rules! conversion_owner {
    ($session:ty, $document:ty, $request:ident, $prepared:ident, $edit:ty, $kind:ident) => {
        /// An immutable source-owner snapshot that can be moved to a background task.
        ///
        /// Dropping the request cancels without changing the source or reserving IDs.
        /// Preparation preserves all selected nested values and removes every selected
        /// nested Prefab link. Ordinary source children remain outside that projection.
        #[derive(Debug)]
        pub struct $request {
            revision: SceneAuthoringRevision,
            document: Arc<$document>,
            state: Arc<AuthoringDocumentState>,
            history: SceneAuthoringHistoryLimits,
            anchor: SceneEntityId,
            options: PrefabExpansionOptions,
        }

        impl $request {
            #[must_use]
            pub const fn base_revision(&self) -> SceneAuthoringRevision {
                self.revision
            }

            #[must_use]
            pub fn anchor(&self) -> &SceneEntityId {
                &self.anchor
            }

            /// Prepares one complete candidate without publishing source or history state.
            ///
            /// The loader returns immutable, canonically bound authoring sources. It owns
            /// decoding/cache admission and must honor the suspended compiler working set.
            /// This view includes the conversion's already retained dependency snapshots.
            /// Only actually demanded sources are requested and retained.
            pub fn prepare_with_loader<F, E>(
                self,
                database: Option<&ProjectAssetDatabase>,
                loader: F,
            ) -> Result<$prepared, PrefabConversionError<E>>
            where
                F: FnMut(
                    &AssetRef,
                    &mut PrefabExpansionWorkingSet,
                ) -> Result<Option<ResolvedPrefabSource>, E>,
            {
                let basis = self.state.schema_basis();
                let localized = localize::prepare(
                    SelectedInstanceSource::$kind(&self.document),
                    &self.anchor,
                    &basis,
                    self.history,
                    self.options,
                    database,
                    loader,
                )?;
                let edit = <$edit>::prepare_from_source(
                    self.revision,
                    &self.document,
                    &self.state,
                    self.history,
                    &localized.patch,
                    AuthoringHistoryScope::PrefabConversion(localized.footprint),
                )
                .map_err(PrefabConversionError::Source)?;
                Ok($prepared {
                    edit,
                    anchor: self.anchor,
                    entities: localized.entities,
                    dependencies: localized.dependencies,
                    asset_references: localized.asset_references,
                })
            }
        }

        /// A move-only reviewed candidate with its actual dependency snapshots.
        ///
        /// No ordinary prepared edit can be extracted from this token. Commit consumes
        /// the original candidate after rechecking the root and all captured requests.
        /// Hosts must additionally retain and revalidate physical file/lookup receipts
        /// and the current choice of file versus working copy.
        #[derive(Debug)]
        pub struct $prepared {
            edit: $edit,
            anchor: SceneEntityId,
            entities: Vec<LocalizedPrefabEntity>,
            dependencies: CapturedPrefabSources,
            asset_references: Vec<nara_reflect::DeclaredAssetReference>,
        }

        impl $prepared {
            #[must_use]
            pub const fn base_revision(&self) -> SceneAuthoringRevision {
                self.edit.base_revision()
            }

            #[must_use]
            pub fn anchor(&self) -> &SceneEntityId {
                &self.anchor
            }

            #[must_use]
            pub fn document(&self) -> &$document {
                self.edit.document()
            }

            #[must_use]
            pub fn entities(&self) -> &[LocalizedPrefabEntity] {
                &self.entities
            }

            /// Typed references in selected effective values, including materialized defaults.
            ///
            /// These observations do not grant native resource or file authority.
            #[must_use]
            pub fn effective_asset_references(&self) -> &[nara_reflect::DeclaredAssetReference] {
                &self.asset_references
            }
        }

        impl $session {
            /// Captures this exact admitted source revision for complete instance conversion.
            ///
            /// The anchor is a directly owned record with an instance declaration. The
            /// request shares immutable source/classification state; no session is reopened.
            pub fn request_complete_prefab_conversion(
                &self,
                revision: SceneAuthoringRevision,
                anchor: &SceneEntityId,
                options: PrefabExpansionOptions,
            ) -> Result<$request, ScenePatchReport> {
                if revision != self.revision() {
                    return Err(stale_basis());
                }
                let state = self.authority.published_shared().ok_or_else(|| {
                    localize::refused("Conversion requires a current source authoring admission")
                })?;
                let record = self
                    .document
                    .entities
                    .binary_search_by(|record| record.id.cmp(anchor))
                    .ok()
                    .map(|index| &self.document.entities[index])
                    .filter(|record| record.prefab.is_some())
                    .ok_or_else(|| {
                        localize::refused(
                            "Conversion requires an actual direct source instance anchor",
                        )
                    })?;
                if record.id.as_str().len() > self.history_limits().max_entry_logical_bytes() {
                    return Err(localize::copy_limit());
                }
                Ok($request {
                    revision,
                    document: Arc::clone(&self.document),
                    state,
                    history: self.history_limits(),
                    anchor: record.id.clone(),
                    options,
                })
            }

            /// Revalidates every actual semantic dependency and publishes the reviewed candidate.
            ///
            /// Missing, rebound or changed sources reject atomically. The provider's
            /// concrete error is preserved. Undo/Redo later uses the same source history
            /// and never reloads dependencies or allocates new IDs.
            pub fn commit_complete_prefab_conversion_with_loader<F, E>(
                &mut self,
                prepared: $prepared,
                loader: F,
            ) -> Result<PrefabConversionReceipt, PrefabConversionError<E>>
            where
                F: FnMut(&AssetRef) -> Result<Option<ResolvedPrefabSource>, E>,
            {
                let basis = self
                    .authoring_schema_basis()
                    .filter(|_| self.revision() == prepared.base_revision())
                    .ok_or_else(|| PrefabConversionError::Source(stale_basis()))?;
                prepared.dependencies.validate(&basis, loader)?;
                let report = self.commit_authoring_edit(prepared.edit);
                if !report.applied {
                    return Err(PrefabConversionError::Source(report));
                }
                Ok(PrefabConversionReceipt {
                    report,
                    entities: prepared.entities,
                })
            }
        }
    };
}

conversion_owner!(
    SceneAuthoringSession,
    SceneDocument,
    ScenePrefabConversionRequest,
    PreparedScenePrefabConversion,
    PreparedSceneAuthoringEdit,
    Scene
);
conversion_owner!(
    PrefabAuthoringSession,
    PrefabDocument,
    PrefabConversionRequest,
    PreparedPrefabConversion,
    PreparedPrefabAuthoringEdit,
    Prefab
);
