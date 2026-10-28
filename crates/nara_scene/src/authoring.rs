use std::{
    collections::VecDeque,
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::World;
use nara_identity::SpawnedSceneInstance;
use nara_reflect::ComponentRegistry;

use crate::{
    ExpandedSceneCandidate, PrefabDocument, PrefabSourceResolver, SceneDocument,
    SceneOccurrenceKey, ScenePatchDocument, ScenePatchReport, SceneSpawnReport, SourceDocumentKey,
    diagnostics::{error as diagnostic_error, info as diagnostic_info},
    patch::{PreparedScenePatch, scene_patch_cost},
    retire_and_despawn_scene_instance,
    spawn::SceneSpawner,
};

mod creation;
pub use creation::CreatedSourceEntity;
#[cfg(feature = "serde")]
mod construction;
#[cfg(feature = "serde")]
pub(crate) mod prefab_conversion;
#[cfg(feature = "serde")]
pub use construction::{
    AuthoringConstructionDraft, CommittedAuthoringConstruction, ConstructionEntity,
    ConstructionEntityMap, ConstructionPlacement, LocalConstructionEntity,
    PreparedPrefabAuthoringConstruction, PreparedSceneAuthoringConstruction,
};
#[cfg(feature = "serde")]
pub use prefab_conversion::{
    LocalizedPrefabEntity, PrefabConversionError, PrefabConversionReceipt, PrefabConversionRequest,
    PreparedPrefabConversion, PreparedScenePrefabConversion, ScenePrefabConversionRequest,
};

#[cfg(feature = "serde")]
mod prefab_declarations;
#[cfg(feature = "serde")]
pub use prefab_declarations::{
    PrefabDeclarationError, PrefabDeclarationLimits, PrefabInstanceObservation,
};
#[cfg(feature = "serde")]
mod prepared;
#[cfg(feature = "serde")]
pub use prepared::{PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit};
#[cfg(feature = "serde")]
mod schema_reclassification;
#[cfg(feature = "serde")]
pub use schema_reclassification::{
    PrefabSchemaReclassificationCommit, PrefabSchemaReclassificationRequest,
    PreparedPrefabSchemaReclassification, PreparedSceneSchemaReclassification,
    RetainedSchemaReclassificationSource, SceneSchemaReclassificationCommit,
    SceneSchemaReclassificationRequest, SchemaReclassificationKind, SchemaReclassificationLimits,
    SchemaReclassificationUsage, SchemaReclassificationWork,
};
#[cfg(feature = "serde")]
pub use schema_reclassification::{
    PreparedPrefabSchemaSource, PreparedSceneSchemaSource, borrow_prepared_prefab_schema_source,
    borrow_prepared_scene_schema_source, require_schema_reclassification_successor,
};
#[cfg(feature = "serde")]
mod property;
#[cfg(feature = "serde")]
pub use property::AuthoringPropertyEdit;

pub use nara_core::{
    DocumentContentState as SceneAuthoringContentState,
    DocumentHistoryStatus as SceneAuthoringHistoryStatus,
    DocumentRevision as SceneAuthoringRevision, DocumentSourceId as SceneAuthoringSourceId,
};

/// Combined retention limits and per-entry limits for one authoring session.
///
/// Each entry is charged the maximum of its forward and reverse replay costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneAuthoringHistoryLimits {
    max_records: usize,
    max_operations: usize,
    max_value_nodes: usize,
    max_logical_bytes: usize,
    max_entry_operations: usize,
    max_entry_value_nodes: usize,
    max_entry_logical_bytes: usize,
}

impl SceneAuthoringHistoryLimits {
    pub const DEFAULT_MAX_RECORDS: usize = 256;
    pub const DEFAULT_MAX_OPERATIONS: usize = 1_048_576;
    pub const DEFAULT_MAX_VALUE_NODES: usize = 1_048_576;
    pub const DEFAULT_MAX_LOGICAL_BYTES: usize = 64 * 1024 * 1024;
    pub const DEFAULT_MAX_ENTRY_OPERATIONS: usize = 262_144;
    pub const DEFAULT_MAX_ENTRY_VALUE_NODES: usize = 262_144;
    pub const DEFAULT_MAX_ENTRY_LOGICAL_BYTES: usize = 16 * 1024 * 1024;

    #[must_use]
    pub const fn new(
        max_records: usize,
        max_operations: usize,
        max_value_nodes: usize,
        max_logical_bytes: usize,
        max_entry_operations: usize,
        max_entry_value_nodes: usize,
        max_entry_logical_bytes: usize,
    ) -> Option<Self> {
        if max_records == 0
            || max_operations == 0
            || max_value_nodes == 0
            || max_logical_bytes == 0
            || max_entry_operations == 0
            || max_entry_value_nodes == 0
            || max_entry_logical_bytes == 0
            || max_entry_operations > max_operations
            || max_entry_value_nodes > max_value_nodes
            || max_entry_logical_bytes > max_logical_bytes
        {
            return None;
        }
        Some(Self {
            max_records,
            max_operations,
            max_value_nodes,
            max_logical_bytes,
            max_entry_operations,
            max_entry_value_nodes,
            max_entry_logical_bytes,
        })
    }

    #[must_use]
    pub const fn max_records(self) -> usize {
        self.max_records
    }

    #[must_use]
    pub const fn max_operations(self) -> usize {
        self.max_operations
    }

    #[must_use]
    pub const fn max_value_nodes(self) -> usize {
        self.max_value_nodes
    }

    #[must_use]
    pub const fn max_logical_bytes(self) -> usize {
        self.max_logical_bytes
    }

    #[must_use]
    pub const fn max_entry_operations(self) -> usize {
        self.max_entry_operations
    }

    #[must_use]
    pub const fn max_entry_value_nodes(self) -> usize {
        self.max_entry_value_nodes
    }

    #[must_use]
    pub const fn max_entry_logical_bytes(self) -> usize {
        self.max_entry_logical_bytes
    }
}

impl Default for SceneAuthoringHistoryLimits {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_MAX_RECORDS,
            Self::DEFAULT_MAX_OPERATIONS,
            Self::DEFAULT_MAX_VALUE_NODES,
            Self::DEFAULT_MAX_LOGICAL_BYTES,
            Self::DEFAULT_MAX_ENTRY_OPERATIONS,
            Self::DEFAULT_MAX_ENTRY_VALUE_NODES,
            Self::DEFAULT_MAX_ENTRY_LOGICAL_BYTES,
        )
        .expect("default authoring history limits are valid")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneAuthoringHistoryUsage {
    pub records: usize,
    pub operations: usize,
    pub value_nodes: usize,
    pub logical_bytes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneAuthoringHistoryCost {
    operations: usize,
    value_nodes: usize,
    logical_bytes: usize,
}

impl SceneAuthoringHistoryCost {
    fn for_patch(patch: &ScenePatchDocument) -> Self {
        let patch_cost = scene_patch_cost(patch);
        let values = patch_cost.value_cost();
        Self {
            operations: patch_cost.operation_count(),
            value_nodes: values.nodes(),
            logical_bytes: patch_cost.logical_bytes(),
        }
    }

    const fn max(self, other: Self) -> Self {
        Self {
            operations: if self.operations > other.operations {
                self.operations
            } else {
                other.operations
            },
            value_nodes: if self.value_nodes > other.value_nodes {
                self.value_nodes
            } else {
                other.value_nodes
            },
            logical_bytes: if self.logical_bytes > other.logical_bytes {
                self.logical_bytes
            } else {
                other.logical_bytes
            },
        }
    }

    const fn saturating_add(self, other: Self) -> Self {
        Self {
            operations: self.operations.saturating_add(other.operations),
            value_nodes: self.value_nodes.saturating_add(other.value_nodes),
            logical_bytes: self.logical_bytes.saturating_add(other.logical_bytes),
        }
    }

    const fn saturating_sub(self, other: Self) -> Self {
        Self {
            operations: self.operations.saturating_sub(other.operations),
            value_nodes: self.value_nodes.saturating_sub(other.value_nodes),
            logical_bytes: self.logical_bytes.saturating_sub(other.logical_bytes),
        }
    }
}

#[derive(Debug, Clone)]
struct SceneAuthoringHistoryEntry {
    patch: ScenePatchDocument,
    from_state: SceneAuthoringContentState,
    to_state: SceneAuthoringContentState,
    cost: SceneAuthoringHistoryCost,
    scope: AuthoringHistoryScope,
}

#[derive(Debug, Default, Clone)]
enum AuthoringHistoryScope {
    #[default]
    Ordinary,
    #[cfg(feature = "serde")]
    PrefabConversion(prefab_conversion::PrefabConversionFootprint),
}

impl AuthoringHistoryScope {
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Ordinary => 0,
            #[cfg(feature = "serde")]
            Self::PrefabConversion(footprint) => footprint.retained_bytes(),
        }
    }

    fn add_cost(&self, mut cost: SceneAuthoringHistoryCost) -> SceneAuthoringHistoryCost {
        cost.logical_bytes = cost.logical_bytes.saturating_add(self.retained_bytes());
        cost
    }
}

#[derive(Debug, Clone)]
struct AuthoringHistory {
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    limits: SceneAuthoringHistoryLimits,
    cost: SceneAuthoringHistoryCost,
    undo_stack: VecDeque<SceneAuthoringHistoryEntry>,
    redo_stack: VecDeque<SceneAuthoringHistoryEntry>,
}

impl AuthoringHistory {
    fn new(limits: SceneAuthoringHistoryLimits) -> Self {
        let revision = SceneAuthoringRevision::initial();
        Self {
            revision,
            content_state: revision.content_state(),
            limits,
            cost: SceneAuthoringHistoryCost::default(),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    fn replace_source(&mut self) {
        self.revision = SceneAuthoringRevision::initial();
        self.content_state = self.revision.content_state();
        self.clear();
    }

    const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    const fn content_state(&self) -> SceneAuthoringContentState {
        self.content_state
    }

    fn owns_content_state(&self, state: SceneAuthoringContentState) -> bool {
        self.revision.source_id() == state.source_id()
    }

    const fn limits(&self) -> SceneAuthoringHistoryLimits {
        self.limits
    }

    fn usage(&self) -> SceneAuthoringHistoryUsage {
        SceneAuthoringHistoryUsage {
            records: self.undo_stack.len().saturating_add(self.redo_stack.len()),
            operations: self.cost.operations,
            value_nodes: self.cost.value_nodes,
            logical_bytes: self.cost.logical_bytes,
        }
    }

    fn status(&self) -> SceneAuthoringHistoryStatus {
        SceneAuthoringHistoryStatus {
            undo_depth: self.undo_stack.len(),
            redo_depth: self.redo_stack.len(),
        }
    }

    fn apply_forward<D>(
        &mut self,
        document: &mut Arc<D>,
        patch: &ScenePatchDocument,
        prepared: Result<(D, ScenePatchReport), ScenePatchReport>,
    ) -> ScenePatchReport {
        let (candidate, report) = match prepared {
            Ok(prepared) => prepared,
            Err(report) => return report,
        };
        if !report.applied || patch.is_empty() {
            return report;
        }

        let (report, cost) = match self.admit_forward_report(patch, report) {
            Ok(admitted) => admitted,
            Err(report) => return report,
        };
        self.publish_forward(document, candidate, report, cost)
    }

    fn publish_forward<D>(
        &mut self,
        document: &mut Arc<D>,
        candidate: D,
        report: ScenePatchReport,
        cost: SceneAuthoringHistoryCost,
    ) -> ScenePatchReport {
        self.publish_forward_with_scope(
            document,
            candidate,
            report,
            cost,
            AuthoringHistoryScope::Ordinary,
        )
    }

    fn publish_forward_with_scope<D>(
        &mut self,
        document: &mut Arc<D>,
        candidate: D,
        report: ScenePatchReport,
        cost: SceneAuthoringHistoryCost,
        scope: AuthoringHistoryScope,
    ) -> ScenePatchReport {
        if !Self::entry_is_admissible(self.limits, cost) {
            return reject_history_publication(
                report,
                "scene.authoring-history-entry-limit-exceeded",
                "authoring patch exceeds the per-entry history limit",
            );
        }
        let inverse = report
            .inverse
            .clone()
            .expect("admitted history owns its inverse");

        let before_state = self.content_state;
        *document = Arc::new(candidate);
        self.advance_revision();
        let after_state = self.content_state_for_current_revision();
        self.content_state = after_state;
        self.clear_redo();
        self.push_undo(SceneAuthoringHistoryEntry {
            patch: inverse,
            from_state: after_state,
            to_state: before_state,
            cost,
            scope,
        });
        report
    }

    fn admit_forward_report(
        &self,
        patch: &ScenePatchDocument,
        report: ScenePatchReport,
    ) -> Result<(ScenePatchReport, SceneAuthoringHistoryCost), ScenePatchReport> {
        Self::admit_report(self.limits, patch, report, &AuthoringHistoryScope::Ordinary)
    }

    fn admit_report(
        limits: SceneAuthoringHistoryLimits,
        patch: &ScenePatchDocument,
        report: ScenePatchReport,
        scope: &AuthoringHistoryScope,
    ) -> Result<(ScenePatchReport, SceneAuthoringHistoryCost), ScenePatchReport> {
        let Some(inverse) = report.inverse.as_ref() else {
            return Err(reject_history_publication(
                report,
                "scene.authoring-history-inverse-missing",
                "accepted authoring patch did not produce an inverse",
            ));
        };
        let cost = scope.add_cost(
            SceneAuthoringHistoryCost::for_patch(patch)
                .max(SceneAuthoringHistoryCost::for_patch(inverse)),
        );
        if !Self::entry_is_admissible(limits, cost) {
            return Err(reject_history_publication(
                report,
                "scene.authoring-history-entry-limit-exceeded",
                "authoring patch exceeds the per-entry history limit",
            ));
        }
        Ok((report, cost))
    }

    #[cfg(feature = "serde")]
    fn admit_input(
        limits: SceneAuthoringHistoryLimits,
        patch: &ScenePatchDocument,
        scope: &AuthoringHistoryScope,
    ) -> Result<(), ScenePatchReport> {
        // Let the source preparer report cheap envelope/arity errors before value work.
        if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION
            || patch.is_empty()
            || patch.operations.len() > crate::authoring_decode::MAX_AUTHORING_EDIT_OPERATIONS
            || patch
                .operations
                .iter()
                .filter(|operation| {
                    crate::authoring_decode::is_authoring_field_operation(operation)
                })
                .take(crate::authoring_decode::MAX_AUTHORING_FIELD_WRITES + 1)
                .count()
                > crate::authoring_decode::MAX_AUTHORING_FIELD_WRITES
        {
            return Ok(());
        }
        let remaining_bytes = limits
            .max_entry_logical_bytes
            .checked_sub(scope.retained_bytes())
            .and_then(nara_core::ByteLimit::new)
            .ok_or_else(|| {
                history_error_report(
                    "scene.authoring-history-entry-limit-exceeded",
                    "authoring preparation and its scope exceed the per-entry history limit",
                )
            })?;
        let patch_limits = crate::ScenePatchApplyLimits::default()
            .with_validation_work(
                nara_core::ItemLimit::new(limits.max_entry_operations)
                    .expect("nonzero history limit"),
            )
            .with_validation_value_nodes(
                nara_core::ItemLimit::new(limits.max_entry_value_nodes)
                    .expect("nonzero history limit"),
            )
            .with_validation_value_bytes(remaining_bytes);
        let admitted = crate::patch::bounded_scene_patch_cost(patch, patch_limits)
            .is_some_and(|cost| cost.logical_bytes() <= remaining_bytes.get());
        if !admitted {
            return Err(history_error_report(
                "scene.authoring-history-entry-limit-exceeded",
                "authoring patch exceeds the per-entry history limit",
            ));
        }
        Ok(())
    }

    fn apply_history<D>(
        &mut self,
        document: &mut Arc<D>,
        direction: HistoryDirection,
        apply: impl FnOnce(
            &ScenePatchDocument,
            &D,
            &AuthoringHistoryScope,
        ) -> Result<(D, ScenePatchReport), ScenePatchReport>,
    ) -> ScenePatchReport {
        let Some(entry) = self.pop(direction) else {
            return history_miss_report(direction.empty_code(), direction.empty_summary());
        };
        if self.content_state != entry.from_state {
            self.restore(direction, entry);
            return history_error_report(
                "scene.authoring-history-state-mismatch",
                "authoring history does not match the current content state",
            );
        }

        let (candidate, report) = match apply(&entry.patch, document.as_ref(), &entry.scope) {
            Ok(prepared) => prepared,
            Err(report) => {
                self.restore(direction, entry);
                return report;
            }
        };
        let report =
            match admit_history_replay_report(&entry.patch, &entry.scope, entry.cost, report, None)
            {
                Ok(report) => report,
                Err(report) => {
                    self.restore(direction, entry);
                    return report;
                }
            };
        let inverse = report
            .inverse
            .clone()
            .expect("admitted history owns its inverse");

        let reverse = SceneAuthoringHistoryEntry {
            patch: inverse,
            from_state: entry.to_state,
            to_state: entry.from_state,
            cost: entry.cost,
            scope: entry.scope,
        };
        *document = Arc::new(candidate);
        self.advance_revision();
        self.content_state = entry.to_state;
        self.restore(direction.opposite(), reverse);
        report
    }

    fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.cost = SceneAuthoringHistoryCost::default();
    }

    fn entry_is_admissible(
        limits: SceneAuthoringHistoryLimits,
        cost: SceneAuthoringHistoryCost,
    ) -> bool {
        cost.operations <= limits.max_entry_operations
            && cost.value_nodes <= limits.max_entry_value_nodes
            && cost.logical_bytes <= limits.max_entry_logical_bytes
    }

    fn clear_redo(&mut self) {
        for entry in self.redo_stack.drain(..) {
            self.cost = self.cost.saturating_sub(entry.cost);
        }
    }

    fn push_undo(&mut self, entry: SceneAuthoringHistoryEntry) {
        self.cost = self.cost.saturating_add(entry.cost);
        self.undo_stack.push_back(entry);
        while self.usage_exceeds_limits() {
            let evicted = self
                .undo_stack
                .pop_front()
                .expect("an admitted entry fits alone within the total history limits");
            self.cost = self.cost.saturating_sub(evicted.cost);
        }
    }

    fn usage_exceeds_limits(&self) -> bool {
        self.undo_stack.len().saturating_add(self.redo_stack.len()) > self.limits.max_records
            || self.cost.operations > self.limits.max_operations
            || self.cost.value_nodes > self.limits.max_value_nodes
            || self.cost.logical_bytes > self.limits.max_logical_bytes
    }

    fn pop(&mut self, direction: HistoryDirection) -> Option<SceneAuthoringHistoryEntry> {
        match direction {
            HistoryDirection::Undo => self.undo_stack.pop_back(),
            HistoryDirection::Redo => self.redo_stack.pop_back(),
        }
    }

    fn restore(&mut self, direction: HistoryDirection, entry: SceneAuthoringHistoryEntry) {
        match direction {
            HistoryDirection::Undo => self.undo_stack.push_back(entry),
            HistoryDirection::Redo => self.redo_stack.push_back(entry),
        }
    }

    fn content_state_for_current_revision(&self) -> SceneAuthoringContentState {
        self.revision.content_state()
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.next();
    }
}

fn admit_history_replay_report(
    patch: &ScenePatchDocument,
    scope: &AuthoringHistoryScope,
    maximum: SceneAuthoringHistoryCost,
    report: ScenePatchReport,
    mut admission: Option<&mut dyn FnMut(Option<usize>) -> Result<(), ScenePatchReport>>,
) -> Result<ScenePatchReport, ScenePatchReport> {
    let Some(inverse) = report.inverse.as_ref() else {
        return Err(reject_history_publication(
            report,
            "scene.authoring-history-inverse-missing",
            "accepted history patch did not produce an inverse",
        ));
    };
    let mut cost = |patch: &ScenePatchDocument| match admission.as_deref_mut() {
        None => Ok(SceneAuthoringHistoryCost::for_patch(patch)),
        Some(admit) => crate::patch::bounded_scene_patch_cost_with_admission(
            patch,
            crate::ScenePatchApplyLimits::default(),
            admit,
        )?
        .map(|cost| SceneAuthoringHistoryCost {
            operations: cost.operation_count(),
            value_nodes: cost.value_cost().nodes(),
            logical_bytes: cost.logical_bytes(),
        })
        .ok_or_else(|| {
            history_error_report(
                "scene.authoring-history-cost-changed",
                "History replay exceeds its owning cost boundary",
            )
        }),
    };
    let observed = scope.add_cost(cost(patch)?.max(cost(inverse)?));
    if observed.operations > maximum.operations
        || observed.value_nodes > maximum.value_nodes
        || observed.logical_bytes > maximum.logical_bytes
    {
        return Err(reject_history_publication(
            report,
            "scene.authoring-history-cost-changed",
            "history inverse exceeded the cost admitted by the originating edit",
        ));
    }
    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAuthoringSyncReport {
    pub synced: bool,
    pub removed_entities: usize,
    pub live_instance: Option<SpawnedSceneInstance>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAuthoringClearReport {
    pub cleared: bool,
    pub removed_entities: usize,
    pub live_instance: Option<SpawnedSceneInstance>,
    pub diagnostics: DiagnosticReport,
}

/// Rejection raised when a native session operation would discard published authoring authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringSessionAuthorityError {
    /// The session owns a published authoring document and cannot enter a native authority path.
    PublishedAuthoringDocument,
}

impl Display for AuthoringSessionAuthorityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PublishedAuthoringDocument => formatter.write_str(
                "published authoring documents must retain their exact classification authority",
            ),
        }
    }
}

impl Error for AuthoringSessionAuthorityError {}

/// Failure to replace a session source through the native persistent-file publication path.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub enum AuthoringSessionReplacementError {
    /// Replacement would discard the session's published authoring authority.
    Authority(AuthoringSessionAuthorityError),
    /// Native persistent-file publication rejected the replacement candidate.
    Publication(crate::SceneFilePublicationError),
}

#[cfg(feature = "serde")]
impl Display for AuthoringSessionReplacementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(error) => Display::fmt(error, formatter),
            Self::Publication(error) => Display::fmt(error, formatter),
        }
    }
}

#[cfg(feature = "serde")]
impl Error for AuthoringSessionReplacementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Authority(error) => Some(error),
            Self::Publication(error) => Some(error),
        }
    }
}

#[cfg(feature = "serde")]
impl From<AuthoringSessionAuthorityError> for AuthoringSessionReplacementError {
    fn from(error: AuthoringSessionAuthorityError) -> Self {
        Self::Authority(error)
    }
}

#[cfg(feature = "serde")]
impl From<crate::SceneFilePublicationError> for AuthoringSessionReplacementError {
    fn from(error: crate::SceneFilePublicationError) -> Self {
        Self::Publication(error)
    }
}

#[derive(Debug)]
enum AuthoringSessionAuthority {
    Native,
    #[cfg(feature = "serde")]
    Published(Arc<crate::authoring_decode::AuthoringDocumentState>),
}

impl AuthoringSessionAuthority {
    const fn require_native(&self) -> Result<(), AuthoringSessionAuthorityError> {
        match self {
            Self::Native => Ok(()),
            #[cfg(feature = "serde")]
            Self::Published(_) => Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument),
        }
    }

    #[cfg(feature = "serde")]
    fn published(&self) -> Option<&crate::authoring_decode::AuthoringDocumentState> {
        match self {
            Self::Native => None,
            Self::Published(state) => Some(state.as_ref()),
        }
    }

    #[cfg(feature = "serde")]
    fn published_shared(&self) -> Option<Arc<crate::authoring_decode::AuthoringDocumentState>> {
        match self {
            Self::Native => None,
            Self::Published(state) => Some(Arc::clone(state)),
        }
    }

    #[cfg(feature = "serde")]
    const fn is_published(&self) -> bool {
        matches!(self, Self::Published(_))
    }
}

#[derive(Debug)]
pub struct SceneAuthoringSession {
    document: Arc<SceneDocument>,
    history: AuthoringHistory,
    live_instance: Option<SpawnedSceneInstance>,
    live_dirty: bool,
    source_upgrade_required: bool,
    authority: AuthoringSessionAuthority,
}

/// Authoring owner for one canonical Prefab source.
///
/// A Prefab session deliberately keeps the nominal `PrefabDocument` as its source authority.
/// Supported ordered entity and field operations mutate an unpublished Prefab clone directly;
/// component-structure edits remain closed. Scene projection is used only for semantic validation
/// and never becomes authoring or persistence state. This preserves source kind, root order, and
/// the Scene/Prefab workspace boundary.
#[derive(Debug)]
pub struct PrefabAuthoringSession {
    document: Arc<PrefabDocument>,
    history: AuthoringHistory,
    source_upgrade_required: bool,
    authority: AuthoringSessionAuthority,
}

impl PrefabAuthoringSession {
    /// Projects the immutable admission retained by a data-only authoring session.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_schema_basis(&self) -> Option<crate::AuthoringSchemaBasis> {
        self.authority.published().map(|state| state.schema_basis())
    }

    /// Projects the exact retained catalog independently from the records already present.
    #[cfg(feature = "serde")]
    pub fn authoring_catalog_components(
        &self,
    ) -> impl Iterator<Item = (&nara_reflect::ComponentSchema, bool)> {
        self.authority
            .published()
            .into_iter()
            .flat_map(|state| state.catalog_components())
    }

    /// Copies complete creation data without executing a native component provider.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_initial_component(
        &self,
        component: &nara_reflect::ComponentTypeId,
    ) -> Option<crate::SceneComponentRecord> {
        self.authority.published()?.initial_component(component)
    }

    /// Returns the retained schema only for a current Available direct record.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_component_schema(
        &self,
        entity: &crate::SceneEntityId,
        component: &nara_reflect::ComponentTypeId,
    ) -> Option<&nara_reflect::ComponentSchema> {
        self.authority
            .published()?
            .available_component_schema(entity, component)
    }

    /// Borrows actual direct records without changing the source or interpreting unavailable values.
    #[cfg(feature = "serde")]
    pub fn authoring_entity_view(
        &self,
        entity: &crate::SceneEntityId,
    ) -> Result<crate::AuthoringEntityView<'_>, crate::AuthoringReadError> {
        self.authority
            .published()
            .ok_or(crate::AuthoringReadError::NotAdmitted)?
            .entity_view(&self.document.entities, entity)
    }
    #[must_use]
    pub fn new(document: PrefabDocument) -> Self {
        Self::new_with_history_limits(document, SceneAuthoringHistoryLimits::default())
    }

    #[must_use]
    pub fn new_with_history_limits(
        document: PrefabDocument,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        Self::new_with_source_state(document, false, history_limits)
    }

    fn new_with_source_state(
        document: PrefabDocument,
        source_upgrade_required: bool,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        Self {
            document: Arc::new(document),
            history: AuthoringHistory::new(history_limits),
            source_upgrade_required,
            authority: AuthoringSessionAuthority::Native,
        }
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn from_published_authoring_document(
        published: crate::PublishedAuthoringPrefabDocument,
    ) -> Self {
        Self::from_published_authoring_document_with_history_limits(
            published,
            SceneAuthoringHistoryLimits::default(),
        )
    }

    #[cfg(feature = "serde")]
    /// Creates a data-authoring owner with fixed, explicit two-way history limits.
    pub fn from_published_authoring_document_with_history_limits(
        published: crate::PublishedAuthoringPrefabDocument,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        let (document, state) = published.into_parts();
        Self {
            document,
            history: AuthoringHistory::new(history_limits),
            source_upgrade_required: false,
            authority: AuthoringSessionAuthority::Published(state),
        }
    }

    /// Captures owned Prefab re-admission without copying its document, history or live state.
    ///
    /// Native sessions have no authoring classification basis and return `None`. The returned
    /// value can prepare a candidate independently; it grants no Workspace publication authority.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_readmission(&self) -> Option<crate::AuthoringDocumentReadmission> {
        self.authority
            .published()
            .map(|state| state.readmission(self.history.limits()))
    }

    /// Reopens a decoded Prefab with the exact authoring authority owned by this session.
    ///
    /// Sessions created through native publication have no authoring classification basis and
    /// fail closed. A successful reopen starts a new authoring lineage while retaining the
    /// session's history limits.
    #[cfg(feature = "serde")]
    pub fn try_reopen_authoring_candidate(
        &self,
        candidate: crate::PrefabDocumentCandidate,
    ) -> Result<Self, crate::AuthoringSessionReopenError> {
        self.authoring_readmission()
            .ok_or(crate::AuthoringSessionReopenError::ClassificationBasisUnavailable)?
            .reopen_prefab_candidate(candidate)
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate(
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        Ok(Self::new_with_source_state(
            published.into_document(),
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate_with_asset_database(
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let published = candidate
            .canonicalize(registry)?
            .publish_with_asset_database(registry, database)?;
        let source_upgrade_required = published.source_upgrade_required();
        Ok(Self::new_with_source_state(
            published.into_document(),
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        &self.document
    }

    /// Returns the source locator retained by authoring admission.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_source(&self) -> Option<&crate::AuthoringDocumentSource> {
        self.authority.published().map(|state| state.source())
    }

    /// Retains the exact immutable source without copying its document or authoring state.
    #[doc(hidden)]
    #[must_use]
    pub fn shared_document(&self) -> Arc<PrefabDocument> {
        Arc::clone(&self.document)
    }

    #[must_use]
    pub fn revision(&self) -> SceneAuthoringRevision {
        self.history.revision()
    }

    #[must_use]
    pub fn content_state(&self) -> SceneAuthoringContentState {
        self.history.content_state()
    }

    #[must_use]
    pub fn owns_content_state(&self, state: SceneAuthoringContentState) -> bool {
        self.history.owns_content_state(state)
    }

    #[must_use]
    pub const fn history_limits(&self) -> SceneAuthoringHistoryLimits {
        self.history.limits()
    }

    #[must_use]
    pub fn history_status(&self) -> SceneAuthoringHistoryStatus {
        self.history.status()
    }

    #[must_use]
    pub fn history_usage(&self) -> SceneAuthoringHistoryUsage {
        self.history.usage()
    }

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_readiness(&self) -> Option<crate::AuthoringDocumentReadiness> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::readiness)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_record_facts(&self) -> Option<&[crate::AuthoringRecordFact]> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::record_facts)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn unavailable_record_manifest(&self) -> Option<&crate::UnavailableRecordManifest> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::unavailable_manifest)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_diagnostics(&self) -> Option<&nara_diagnostic::DiagnosticReport> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::diagnostics)
    }

    /// Replaces a native source and starts a fresh authoring lineage.
    ///
    /// Published authoring sessions reject this operation without changing document or history.
    pub fn replace_document(
        &mut self,
        document: PrefabDocument,
    ) -> Result<(), AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        self.replace_document_with_source_state(document, false);
        Ok(())
    }

    fn replace_document_with_source_state(
        &mut self,
        document: PrefabDocument,
        source_upgrade_required: bool,
    ) {
        self.document = Arc::new(document);
        self.history.replace_source();
        self.source_upgrade_required = source_upgrade_required;
        self.authority = AuthoringSessionAuthority::Native;
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate(
        &mut self,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<(), AuthoringSessionReplacementError> {
        self.authority.require_native()?;
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        self.replace_document_with_source_state(published.into_document(), source_upgrade_required);
        Ok(())
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate_with_asset_database(
        &mut self,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<(), AuthoringSessionReplacementError> {
        self.authority.require_native()?;
        let published = candidate
            .canonicalize(registry)?
            .publish_with_asset_database(registry, database)?;
        let source_upgrade_required = published.source_upgrade_required();
        self.replace_document_with_source_state(published.into_document(), source_upgrade_required);
        Ok(())
    }

    /// Applies one failure-atomic patch to the Prefab source.
    pub fn apply_patch(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        let prepared = patch
            .prepare_to_prefab(&self.document, registry)
            .map(crate::patch::PreparedPrefabPatch::into_parts);
        self.history
            .apply_forward(&mut self.document, patch, prepared)
    }

    /// Applies one failure-atomic patch with project asset validation.
    pub fn apply_patch_with_asset_database(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        let prepared = patch
            .prepare_to_prefab_with_asset_database(&self.document, registry, database)
            .map(crate::patch::PreparedPrefabPatch::into_parts);
        self.history
            .apply_forward(&mut self.document, patch, prepared)
    }

    #[cfg(feature = "serde")]
    pub fn apply_authoring_field_edit(
        &mut self,
        edit: crate::AuthoringFieldEdit,
    ) -> ScenePatchReport {
        self.apply_authoring_patch(&edit.to_patch())
    }

    /// Commits one admitted data transaction while preserving unavailable source records.
    #[cfg(feature = "serde")]
    pub fn apply_authoring_patch(&mut self, patch: &ScenePatchDocument) -> ScenePatchReport {
        let prepared = match self.prepare_authoring_edit(self.revision(), patch) {
            Ok(prepared) => prepared,
            Err(report) => return report,
        };
        self.commit_authoring_edit(prepared)
    }

    /// Applies one history step while retaining the Prefab source kind.
    pub fn undo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Undo);
        }
        self.history.apply_history(
            &mut self.document,
            HistoryDirection::Undo,
            |patch, document, _scope| {
                patch
                    .prepare_to_prefab(document, registry)
                    .map(crate::patch::PreparedPrefabPatch::into_parts)
            },
        )
    }

    /// Applies one history step through published authoring authority without a Runtime registry.
    #[cfg(feature = "serde")]
    pub fn undo_authoring(&mut self) -> ScenePatchReport {
        if !self.authority.is_published() {
            return authoring_history_session_rejection();
        }
        self.apply_authoring_history(HistoryDirection::Undo)
    }

    /// Applies one history step with project asset validation.
    pub fn undo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Undo);
        }
        self.history.apply_history(
            &mut self.document,
            HistoryDirection::Undo,
            |patch, document, _scope| {
                patch
                    .prepare_to_prefab_with_asset_database(document, registry, database)
                    .map(crate::patch::PreparedPrefabPatch::into_parts)
            },
        )
    }

    /// Reapplies one history step while retaining the Prefab source kind.
    pub fn redo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Redo);
        }
        self.history.apply_history(
            &mut self.document,
            HistoryDirection::Redo,
            |patch, document, _scope| {
                patch
                    .prepare_to_prefab(document, registry)
                    .map(crate::patch::PreparedPrefabPatch::into_parts)
            },
        )
    }

    /// Reapplies one history step through published authoring authority without a Runtime registry.
    #[cfg(feature = "serde")]
    pub fn redo_authoring(&mut self) -> ScenePatchReport {
        if !self.authority.is_published() {
            return authoring_history_session_rejection();
        }
        self.apply_authoring_history(HistoryDirection::Redo)
    }

    /// Reapplies one history step with project asset validation.
    pub fn redo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Redo);
        }
        self.history.apply_history(
            &mut self.document,
            HistoryDirection::Redo,
            |patch, document, _scope| {
                patch
                    .prepare_to_prefab_with_asset_database(document, registry, database)
                    .map(crate::patch::PreparedPrefabPatch::into_parts)
            },
        )
    }

    #[cfg(feature = "serde")]
    fn apply_authoring_history(&mut self, direction: HistoryDirection) -> ScenePatchReport {
        let state = self
            .authority
            .published()
            .expect("an admitted authoring document owns classification state");
        let mut next_state = None;
        let report =
            self.history
                .apply_history(&mut self.document, direction, |patch, document, scope| {
                    let prepared = schema_reclassification::prepare_prefab_step(
                        document, state, patch, scope, None,
                    )?;
                    next_state = Some(prepared.state);
                    Ok((prepared.document, prepared.report))
                });
        if report.applied {
            self.authority = AuthoringSessionAuthority::Published(Arc::new(
                next_state.expect("accepted authoring history publishes classification state"),
            ));
        }
        report
    }

    /// Marks one content state as saved after the Host accepted a Prefab receipt.
    #[doc(hidden)]
    pub fn acknowledge_source_saved(&mut self, state: SceneAuthoringContentState) -> bool {
        if !self.history.owns_content_state(state) {
            return false;
        }
        self.source_upgrade_required = false;
        true
    }
}

impl SceneAuthoringSession {
    /// Projects the immutable admission retained by a data-only authoring session.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_schema_basis(&self) -> Option<crate::AuthoringSchemaBasis> {
        self.authority.published().map(|state| state.schema_basis())
    }

    /// Projects the exact retained catalog independently from the records already present.
    #[cfg(feature = "serde")]
    pub fn authoring_catalog_components(
        &self,
    ) -> impl Iterator<Item = (&nara_reflect::ComponentSchema, bool)> {
        self.authority
            .published()
            .into_iter()
            .flat_map(|state| state.catalog_components())
    }

    /// Copies complete creation data without executing a native component provider.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_initial_component(
        &self,
        component: &nara_reflect::ComponentTypeId,
    ) -> Option<crate::SceneComponentRecord> {
        self.authority.published()?.initial_component(component)
    }
    #[must_use]
    pub fn new(document: SceneDocument) -> Self {
        Self::new_with_history_limits(document, SceneAuthoringHistoryLimits::default())
    }

    #[must_use]
    pub fn new_with_history_limits(
        document: SceneDocument,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        Self::new_with_source_state(document, false, history_limits)
    }

    fn new_with_source_state(
        document: SceneDocument,
        source_upgrade_required: bool,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        Self {
            document: Arc::new(document),
            history: AuthoringHistory::new(history_limits),
            live_instance: None,
            live_dirty: true,
            source_upgrade_required,
            authority: AuthoringSessionAuthority::Native,
        }
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn from_published_authoring_document(
        published: crate::PublishedAuthoringSceneDocument,
    ) -> Self {
        Self::from_published_authoring_document_with_history_limits(
            published,
            SceneAuthoringHistoryLimits::default(),
        )
    }

    #[cfg(feature = "serde")]
    /// Creates a data-authoring owner with fixed, explicit two-way history limits.
    pub fn from_published_authoring_document_with_history_limits(
        published: crate::PublishedAuthoringSceneDocument,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        let (document, state) = published.into_parts();
        Self {
            document,
            history: AuthoringHistory::new(history_limits),
            live_instance: None,
            live_dirty: true,
            source_upgrade_required: false,
            authority: AuthoringSessionAuthority::Published(state),
        }
    }

    /// Captures owned Scene re-admission without copying its document, history or live state.
    ///
    /// Native sessions have no authoring classification basis and return `None`. The returned
    /// value can prepare a candidate independently; it grants no Workspace publication authority.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_readmission(&self) -> Option<crate::AuthoringDocumentReadmission> {
        self.authority
            .published()
            .map(|state| state.readmission(self.history.limits()))
    }

    /// Reopens a decoded Scene with the exact authoring authority owned by this session.
    ///
    /// Sessions created through native publication have no authoring classification basis and
    /// fail closed. A successful reopen starts a new authoring lineage while retaining the
    /// session's history limits; runtime registry publication is not consulted.
    #[cfg(feature = "serde")]
    pub fn try_reopen_authoring_candidate(
        &self,
        candidate: crate::SceneDocumentCandidate,
    ) -> Result<Self, crate::AuthoringSessionReopenError> {
        self.authoring_readmission()
            .ok_or(crate::AuthoringSessionReopenError::ClassificationBasisUnavailable)?
            .reopen_scene_candidate(candidate)
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate(
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        Ok(Self::new_with_source_state(
            published.into_document(),
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate_with_asset_database(
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let published = candidate
            .canonicalize(registry)?
            .publish_with_asset_database(registry, database)?;
        let source_upgrade_required = published.source_upgrade_required();
        Ok(Self::new_with_source_state(
            published.into_document(),
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    /// Returns the source locator retained by authoring admission.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_source(&self) -> Option<&crate::AuthoringDocumentSource> {
        self.authority.published().map(|state| state.source())
    }

    /// Retains the exact immutable source without copying its document or authoring state.
    #[doc(hidden)]
    #[must_use]
    pub fn shared_document(&self) -> Arc<SceneDocument> {
        Arc::clone(&self.document)
    }

    /// Returns the existing authoring basis for an Available, current direct component.
    ///
    /// This read-only projection never resolves a Runtime registry or admits an unavailable
    /// record for editing.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_component_schema(
        &self,
        entity: &crate::SceneEntityId,
        component: &nara_reflect::ComponentTypeId,
    ) -> Option<&nara_reflect::ComponentSchema> {
        self.authority
            .published()?
            .available_component_schema(entity, component)
    }

    /// Borrows actual direct records without constructing an Inspector or Runtime projection.
    #[cfg(feature = "serde")]
    pub fn authoring_entity_view(
        &self,
        entity: &crate::SceneEntityId,
    ) -> Result<crate::AuthoringEntityView<'_>, crate::AuthoringReadError> {
        self.authority
            .published()
            .ok_or(crate::AuthoringReadError::NotAdmitted)?
            .entity_view(&self.document.entities, entity)
    }

    #[must_use]
    pub fn revision(&self) -> SceneAuthoringRevision {
        self.history.revision()
    }

    #[must_use]
    pub fn content_state(&self) -> SceneAuthoringContentState {
        self.history.content_state()
    }

    #[must_use]
    pub fn owns_content_state(&self, state: SceneAuthoringContentState) -> bool {
        self.history.owns_content_state(state)
    }

    #[must_use]
    pub const fn history_limits(&self) -> SceneAuthoringHistoryLimits {
        self.history.limits()
    }

    #[must_use]
    pub fn history_usage(&self) -> SceneAuthoringHistoryUsage {
        self.history.usage()
    }

    /// Replaces a native source and starts a fresh authoring lineage.
    ///
    /// Published authoring sessions reject this operation without changing document or history.
    pub fn replace_document(
        &mut self,
        document: SceneDocument,
    ) -> Result<(), AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        self.replace_document_with_source_state(document, false);
        Ok(())
    }

    fn replace_document_with_source_state(
        &mut self,
        document: SceneDocument,
        source_upgrade_required: bool,
    ) {
        self.document = Arc::new(document);
        self.history.replace_source();
        self.live_dirty = true;
        self.source_upgrade_required = source_upgrade_required;
        self.authority = AuthoringSessionAuthority::Native;
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate(
        &mut self,
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<(), AuthoringSessionReplacementError> {
        self.authority.require_native()?;
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        self.replace_document_with_source_state(published.into_document(), source_upgrade_required);
        Ok(())
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate_with_asset_database(
        &mut self,
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<(), AuthoringSessionReplacementError> {
        self.authority.require_native()?;
        let published = candidate
            .canonicalize(registry)?
            .publish_with_asset_database(registry, database)?;
        let source_upgrade_required = published.source_upgrade_required();
        self.replace_document_with_source_state(published.into_document(), source_upgrade_required);
        Ok(())
    }

    #[must_use]
    pub fn history_status(&self) -> SceneAuthoringHistoryStatus {
        self.history.status()
    }

    #[must_use]
    pub fn live_instance(&self) -> Option<&SpawnedSceneInstance> {
        self.live_instance.as_ref()
    }

    #[must_use]
    pub fn is_live_dirty(&self) -> bool {
        self.live_dirty
    }

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_readiness(&self) -> Option<crate::AuthoringDocumentReadiness> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::readiness)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_record_facts(&self) -> Option<&[crate::AuthoringRecordFact]> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::record_facts)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn unavailable_record_manifest(&self) -> Option<&crate::UnavailableRecordManifest> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::unavailable_manifest)
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn authoring_diagnostics(&self) -> Option<&nara_diagnostic::DiagnosticReport> {
        self.authority
            .published()
            .map(crate::authoring_decode::AuthoringDocumentState::diagnostics)
    }

    #[doc(hidden)]
    pub fn acknowledge_source_saved(&mut self, state: SceneAuthoringContentState) -> bool {
        if !self.owns_content_state(state) {
            return false;
        }
        self.source_upgrade_required = false;
        true
    }

    pub fn apply_patch(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        self.apply_forward_patch_with(patch, patch.prepare_to_scene(&self.document, registry))
    }

    pub fn apply_patch_with_asset_database(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        self.apply_forward_patch_with(
            patch,
            patch.prepare_to_scene_with_asset_database(&self.document, registry, database),
        )
    }

    #[cfg(feature = "serde")]
    pub fn apply_authoring_field_edit(
        &mut self,
        edit: crate::AuthoringFieldEdit,
    ) -> ScenePatchReport {
        self.apply_authoring_patch(&edit.to_patch())
    }

    /// Commits one admitted data transaction while preserving unavailable source records.
    #[cfg(feature = "serde")]
    pub fn apply_authoring_patch(&mut self, patch: &ScenePatchDocument) -> ScenePatchReport {
        let prepared = match self.prepare_authoring_edit(self.revision(), patch) {
            Ok(prepared) => prepared,
            Err(report) => return report,
        };
        self.commit_authoring_edit(prepared)
    }

    #[cfg(feature = "serde")]
    pub fn apply_file_patch_candidate(
        &mut self,
        candidate: crate::ScenePatchDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        match candidate.into_canonical_document(registry) {
            Ok((patch, _)) => self.apply_patch(&patch, registry),
            Err(error) => crate::format::publication_error_patch_report(error),
        }
    }

    #[cfg(feature = "serde")]
    pub fn apply_file_patch_candidate_with_asset_database(
        &mut self,
        candidate: crate::ScenePatchDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        if self.authority.is_published() {
            return admitted_document_patch_rejection();
        }
        match candidate.into_canonical_document(registry) {
            Ok((patch, _)) => self.apply_patch_with_asset_database(&patch, registry, database),
            Err(error) => crate::format::publication_error_patch_report(error),
        }
    }

    pub fn undo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Undo);
        }
        self.apply_history_patch_with(HistoryDirection::Undo, |patch, document| {
            patch.prepare_to_scene(document, registry)
        })
    }

    /// Applies one history step through published authoring authority without a Runtime registry.
    #[cfg(feature = "serde")]
    pub fn undo_authoring(&mut self) -> ScenePatchReport {
        if !self.authority.is_published() {
            return authoring_history_session_rejection();
        }
        self.apply_authoring_history(HistoryDirection::Undo)
    }

    pub fn undo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Undo);
        }
        self.apply_history_patch_with(HistoryDirection::Undo, |patch, document| {
            patch.prepare_to_scene_with_asset_database(document, registry, database)
        })
    }

    pub fn redo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Redo);
        }
        self.apply_history_patch_with(HistoryDirection::Redo, |patch, document| {
            patch.prepare_to_scene(document, registry)
        })
    }

    /// Reapplies one history step through published authoring authority without a Runtime registry.
    #[cfg(feature = "serde")]
    pub fn redo_authoring(&mut self) -> ScenePatchReport {
        if !self.authority.is_published() {
            return authoring_history_session_rejection();
        }
        self.apply_authoring_history(HistoryDirection::Redo)
    }

    pub fn redo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        #[cfg(feature = "serde")]
        if self.authority.is_published() {
            return self.apply_authoring_history(HistoryDirection::Redo);
        }
        self.apply_history_patch_with(HistoryDirection::Redo, |patch, document| {
            patch.prepare_to_scene_with_asset_database(document, registry, database)
        })
    }

    /// Synchronizes a native-authority document with an explicit source and logical placement.
    /// Published authoring sessions reject before inspecting or mutating the World.
    pub fn sync_world(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
    ) -> Result<SceneAuthoringSyncReport, AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        let candidate = ExpandedSceneCandidate::plain(source.clone(), Arc::clone(&self.document));
        Ok(self.sync_bound_candidate(world, registry, &candidate, occurrence, None))
    }

    pub fn sync_world_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        database: &ProjectAssetDatabase,
    ) -> Result<SceneAuthoringSyncReport, AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        let candidate = ExpandedSceneCandidate::plain(source.clone(), Arc::clone(&self.document));
        Ok(self.sync_bound_candidate(world, registry, &candidate, occurrence, Some(database)))
    }

    pub fn sync_world_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        resolver: &R,
    ) -> Result<SceneAuthoringSyncReport, AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        let report = self.document.expand_prefabs(registry, resolver);
        Ok(self.sync_expansion(world, registry, source, occurrence, report, None))
    }

    pub fn sync_world_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> Result<SceneAuthoringSyncReport, AuthoringSessionAuthorityError> {
        self.authority.require_native()?;
        let report = self
            .document
            .expand_prefabs_with_asset_database(registry, resolver, database);
        Ok(self.sync_expansion(world, registry, source, occurrence, report, Some(database)))
    }

    fn sync_expansion(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        expansion: crate::PrefabExpansionReport,
        database: Option<&ProjectAssetDatabase>,
    ) -> SceneAuthoringSyncReport {
        let mut diagnostics = expansion.diagnostics;
        let mut report = match expansion.candidate {
            Some(expanded) => SceneSpawner::new().sync(
                world,
                registry,
                &expanded.bind_source(source.clone()),
                occurrence,
                database,
                self.live_instance.as_ref(),
            ),
            None => SceneSpawnReport::default(),
        };
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        self.finish_world_sync(report)
    }

    fn sync_bound_candidate(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        occurrence: &SceneOccurrenceKey,
        database: Option<&ProjectAssetDatabase>,
    ) -> SceneAuthoringSyncReport {
        let report = SceneSpawner::new().sync(
            world,
            registry,
            candidate,
            occurrence,
            database,
            self.live_instance.as_ref(),
        );
        self.finish_world_sync(report)
    }

    pub fn clear_live_world(&mut self, world: &mut World) -> SceneAuthoringClearReport {
        let Some(current) = self.live_instance.clone() else {
            return SceneAuthoringClearReport {
                cleared: true,
                removed_entities: 0,
                live_instance: None,
                diagnostics: DiagnosticReport::default(),
            };
        };
        let removed_entities = match retire_and_despawn_scene_instance(world, &current) {
            Ok(removed_entities) => removed_entities,
            Err(diagnostics) => {
                return SceneAuthoringClearReport {
                    cleared: false,
                    removed_entities: 0,
                    live_instance: Some(current),
                    diagnostics,
                };
            }
        };

        let diagnostics = DiagnosticReport::default();
        self.live_instance = None;
        self.live_dirty = !self.document.entities.is_empty();
        SceneAuthoringClearReport {
            cleared: true,
            removed_entities,
            live_instance: None,
            diagnostics,
        }
    }

    fn apply_forward_patch_with(
        &mut self,
        patch: &ScenePatchDocument,
        prepared: Result<PreparedScenePatch, ScenePatchReport>,
    ) -> ScenePatchReport {
        let report = self.history.apply_forward(
            &mut self.document,
            patch,
            prepared.map(PreparedScenePatch::into_parts),
        );
        if report.applied && !patch.is_empty() {
            self.live_dirty = true;
        }
        report
    }

    fn apply_history_patch_with(
        &mut self,
        direction: HistoryDirection,
        apply: impl FnOnce(
            &ScenePatchDocument,
            &SceneDocument,
        ) -> Result<PreparedScenePatch, ScenePatchReport>,
    ) -> ScenePatchReport {
        let report =
            self.history
                .apply_history(&mut self.document, direction, |patch, document, _scope| {
                    apply(patch, document).map(PreparedScenePatch::into_parts)
                });
        if report.applied {
            self.live_dirty = true;
        }
        report
    }

    #[cfg(feature = "serde")]
    fn apply_authoring_history(&mut self, direction: HistoryDirection) -> ScenePatchReport {
        let state = self
            .authority
            .published()
            .expect("an admitted authoring document owns classification state");
        let mut next_state = None;
        let report =
            self.history
                .apply_history(&mut self.document, direction, |patch, document, scope| {
                    let prepared = schema_reclassification::prepare_scene_step(
                        document, state, patch, scope, None,
                    )?;
                    next_state = Some(prepared.state);
                    Ok((prepared.document, prepared.report))
                });
        if report.applied {
            self.authority = AuthoringSessionAuthority::Published(Arc::new(
                next_state.expect("accepted authoring history publishes classification state"),
            ));
            self.live_dirty = true;
        }
        report
    }

    fn finish_world_sync(&mut self, report: SceneSpawnReport) -> SceneAuthoringSyncReport {
        let removed_entities = report.retired_entities();
        let SceneSpawnReport {
            instance,
            diagnostics,
            ..
        } = report;

        if diagnostics.has_errors() {
            return SceneAuthoringSyncReport {
                synced: false,
                removed_entities: 0,
                live_instance: self.live_instance.clone(),
                diagnostics,
            };
        }

        let Some(instance) = instance else {
            let mut diagnostics = diagnostics;
            diagnostics.push(diagnostic_error(
                "scene.identity-instance-missing",
                "Successful scene synchronization did not publish an identity instance",
            ));
            return SceneAuthoringSyncReport {
                synced: false,
                removed_entities: 0,
                live_instance: self.live_instance.clone(),
                diagnostics,
            };
        };

        self.live_instance = Some(instance.clone());
        self.live_dirty = false;

        SceneAuthoringSyncReport {
            synced: true,
            removed_entities,
            live_instance: Some(instance),
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    const fn opposite(self) -> Self {
        match self {
            Self::Undo => Self::Redo,
            Self::Redo => Self::Undo,
        }
    }

    const fn empty_code(self) -> &'static str {
        match self {
            Self::Undo => "scene.undo-empty",
            Self::Redo => "scene.redo-empty",
        }
    }

    const fn empty_summary(self) -> &'static str {
        match self {
            Self::Undo => "undo history is empty",
            Self::Redo => "redo history is empty",
        }
    }
}

fn history_miss_report(code: &'static str, summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_info(code, summary));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn history_error_report(code: &'static str, summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(code, summary));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

#[cfg(feature = "serde")]
fn admitted_document_patch_rejection() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-operation-not-admitted",
        "Admitted authoring documents require a preservation-safe authoring command",
    )
}

#[cfg(feature = "serde")]
fn authoring_field_edit_session_rejection() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-field-edit-session-unavailable",
        "Authoring field edits require a document admitted through the authoring boundary",
    )
}

#[cfg(feature = "serde")]
fn authoring_history_session_rejection() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-history-session-unavailable",
        "Authoring history requires a document admitted through the authoring boundary",
    )
}

fn reject_history_publication(
    mut report: ScenePatchReport,
    code: &'static str,
    summary: &'static str,
) -> ScenePatchReport {
    report.applied = false;
    report.inverse = None;
    report.diagnostics.push(diagnostic_error(code, summary));
    report
}
