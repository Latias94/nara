use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Debug, Formatter},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_ecs::World;
use thiserror::Error;

use crate::{
    App, RuntimeAdmissionError, RuntimeAdmissionReservation, RuntimeAdmissionRetirement,
    RuntimeCandidate, RuntimeCandidateRetirementState, RuntimeCloseEvidence, RuntimeClosePolicy,
    RuntimeObligationLedger, RuntimePreparationRetirement,
};

use super::{
    Plugin, PluginCapability, PluginDeclaration, PluginError, PluginGroupId, PluginId,
    PluginSchemaProviderId, PluginServiceId, PluginSlotId,
    definition::{PluginDefinition, PluginDefinitionKey, PluginPrepareError},
    fingerprint::{
        FingerprintEncoder, PluginPlanFingerprint, encode_definition, encode_edit, fingerprint_plan,
    },
    group::{
        ErasedPluginGroup, PluginGroupItem, PluginInput, PluginInputCollection, PluginSlot,
        PluginSlotPresence, Plugins, ReplayablePlugins, ResolvedEditTarget, ResolvedPluginEdit,
        resolve_edits, sealed,
    },
};

#[derive(Clone)]
enum PluginMaterializer {
    Prefix(PluginDefinitionWitness),
    Direct(Arc<dyn Plugin>),
    Definition(PluginDefinition),
}

#[derive(Clone)]
pub(crate) enum PluginDefinitionWitness {
    OpaqueDirect,
    Repeatable(PluginDefinition),
}

impl PluginDefinitionWitness {
    fn exact_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpaqueDirect, Self::OpaqueDirect) => true,
            (Self::Repeatable(left), Self::Repeatable(right)) => left.exact_eq(right),
            _ => false,
        }
    }

    fn matches_definition(&self, definition: &PluginDefinition) -> bool {
        matches!(self, Self::Repeatable(existing) if existing.exact_eq(definition))
    }

    fn definition_key(&self) -> Option<PluginDefinitionKey> {
        match self {
            Self::OpaqueDirect => None,
            Self::Repeatable(definition) => definition.key,
        }
    }
}

impl PluginMaterializer {
    fn exact_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Prefix(left), Self::Prefix(right)) => left.exact_eq(right),
            (Self::Prefix(left), Self::Definition(right))
            | (Self::Definition(right), Self::Prefix(left)) => left.matches_definition(right),
            (Self::Definition(left), Self::Definition(right)) => left.exact_eq(right),
            _ => false,
        }
    }

    fn definition_key(&self) -> Option<PluginDefinitionKey> {
        match self {
            Self::Definition(definition) => definition.key,
            Self::Prefix(witness) => witness.definition_key(),
            Self::Direct(_) => None,
        }
    }

    fn admitted_witness(&self) -> PluginDefinitionWitness {
        match self {
            Self::Prefix(witness) => witness.clone(),
            Self::Direct(_) => PluginDefinitionWitness::OpaqueDirect,
            Self::Definition(definition) => PluginDefinitionWitness::Repeatable(definition.clone()),
        }
    }
}

#[derive(Clone)]
struct PluginDraft {
    declaration: &'static PluginDeclaration,
    slot: Option<PluginSlot>,
    materializer: PluginMaterializer,
    provenance: BTreeSet<PluginGroupId>,
    order_hint: usize,
    disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPlanEntry {
    pub(super) declaration: &'static PluginDeclaration,
    pub(super) definition_key: Option<PluginDefinitionKey>,
    pub(super) slot: Option<PluginSlot>,
    pub(super) group_provenance: Vec<PluginGroupId>,
}

impl PluginPlanEntry {
    #[must_use]
    pub const fn plugin_id(&self) -> PluginId {
        self.declaration.id
    }

    #[must_use]
    pub const fn declaration(&self) -> &'static PluginDeclaration {
        self.declaration
    }

    #[must_use]
    pub const fn definition_key(&self) -> Option<PluginDefinitionKey> {
        self.definition_key
    }

    #[must_use]
    pub const fn slot(&self) -> Option<PluginSlotId> {
        match self.slot {
            Some(slot) => Some(slot.id),
            None => None,
        }
    }

    #[must_use]
    pub const fn slot_contract(&self) -> Option<PluginSlot> {
        self.slot
    }

    #[must_use]
    pub fn group_provenance(&self) -> &[PluginGroupId] {
        &self.group_provenance
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPluginGroup {
    pub(super) id: PluginGroupId,
    pub(super) definition_fingerprint: PluginPlanFingerprint,
    pub(super) plugins: Vec<PluginId>,
}

impl ResolvedPluginGroup {
    #[must_use]
    pub const fn id(&self) -> PluginGroupId {
        self.id
    }

    #[must_use]
    pub const fn definition_fingerprint(&self) -> PluginPlanFingerprint {
        self.definition_fingerprint
    }

    #[must_use]
    pub fn plugins(&self) -> &[PluginId] {
        &self.plugins
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginPlanError {
    #[error("plugin static declaration panicked")]
    DeclarationPanicked,
    #[error("plugin group {group} expansion panicked")]
    GroupPanicked { group: PluginGroupId },
    #[error("plugin group dependency cycle: {chain:?}")]
    GroupCycle { chain: Vec<PluginGroupId> },
    #[error("plugin group {group} was bound to divergent definitions")]
    DivergentGroup { group: PluginGroupId },
    #[error("plugin slot {slot} expects {expected}, got {actual}")]
    SlotPluginMismatch {
        slot: PluginSlotId,
        expected: PluginId,
        actual: PluginId,
    },
    #[error("plugin edit target is missing")]
    MissingEditTarget,
    #[error("plugin edit target is ambiguous")]
    AmbiguousEditTarget,
    #[error("required plugin slot {slot} cannot be disabled")]
    RequiredSlotDisabled { slot: PluginSlotId },
    #[error("plugin slot {slot} is claimed by both {first} and {duplicate}")]
    DuplicateSlot {
        slot: PluginSlotId,
        first: PluginId,
        duplicate: PluginId,
    },
    #[error("plugin slot {slot} has divergent required/optional contracts")]
    DivergentSlotContract { slot: PluginSlotId },
    #[error("plugin slot {slot} is both active and disabled")]
    ActiveSlotDisabled { slot: PluginSlotId },
    #[error("plugin {plugin} appears in more than one occurrence")]
    DuplicatePlugin { plugin: PluginId },
    #[error("plugin {plugin} has divergent definitions")]
    DivergentDefinition { plugin: PluginId },
    #[error("plugin {plugin} requires missing plugin {required}")]
    MissingPlugin {
        plugin: PluginId,
        required: PluginId,
    },
    #[error("plugin {plugin} requires missing capability {required}")]
    MissingCapability {
        plugin: PluginId,
        required: PluginCapability,
    },
    #[error("plugin {plugin} requires missing service {required}")]
    MissingService {
        plugin: PluginId,
        required: PluginServiceId,
    },
    #[error("plugin {plugin} requires missing schema provider {required}")]
    MissingSchemaProvider {
        plugin: PluginId,
        required: PluginSchemaProviderId,
    },
    #[error("plugin {plugin} conflicts with {conflict}")]
    Conflict {
        plugin: PluginId,
        conflict: PluginId,
    },
    #[error("plugin ordering cycle: {plugins:?}")]
    OrderingCycle { plugins: Vec<PluginId> },
    #[error("a later plugin plan would reorder the committed App prefix")]
    ImmutablePrefix,
}

#[derive(Clone)]
pub(crate) struct CompositionPrefix {
    entries: Vec<PluginPlanEntry>,
    witnesses: Vec<PluginDefinitionWitness>,
    groups: Vec<ResolvedPluginGroup>,
    disabled_slots: BTreeSet<PluginSlotId>,
}

struct ResolvedComposition {
    entries: Vec<PluginPlanEntry>,
    ordered_drafts: Vec<PluginDraft>,
    groups: Vec<ResolvedPluginGroup>,
    disabled_slots: BTreeSet<PluginSlotId>,
    fingerprint: PluginPlanFingerprint,
    prefix_len: usize,
}

#[derive(Clone)]
pub struct PluginPlan {
    entries: Vec<PluginPlanEntry>,
    definitions: Vec<PluginDefinition>,
    groups: Vec<ResolvedPluginGroup>,
    disabled_slots: BTreeSet<PluginSlotId>,
    fingerprint: PluginPlanFingerprint,
}

impl Debug for PluginPlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginPlan")
            .field("entries", &self.entries)
            .field("groups", &self.groups)
            .field("disabled_slots", &self.disabled_slots)
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

impl PluginPlan {
    pub fn resolve<M>(plugins: impl ReplayablePlugins<M>) -> Result<Self, PluginPlanError> {
        let mut collection = PluginInputCollection::default();
        catch_unwind(AssertUnwindSafe(|| {
            sealed::ReplayablePlugins::<M>::collect_replayable(plugins, &mut collection);
        }))
        .map_err(|_| PluginPlanError::DeclarationPanicked)?;
        let resolved = resolve_collection(collection, None)?;
        let definitions = resolved
            .ordered_drafts
            .into_iter()
            .map(|draft| match draft.materializer {
                PluginMaterializer::Definition(definition) => Ok(definition),
                PluginMaterializer::Direct(_) | PluginMaterializer::Prefix(_) => {
                    Err(PluginPlanError::DivergentDefinition {
                        plugin: draft.declaration.id,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            entries: resolved.entries,
            definitions,
            groups: resolved.groups,
            disabled_slots: resolved.disabled_slots,
            fingerprint: resolved.fingerprint,
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[PluginPlanEntry] {
        &self.entries
    }

    #[must_use]
    pub fn groups(&self) -> &[ResolvedPluginGroup] {
        &self.groups
    }

    #[must_use]
    pub const fn disabled_slots(&self) -> &BTreeSet<PluginSlotId> {
        &self.disabled_slots
    }

    #[must_use]
    pub const fn fingerprint(&self) -> PluginPlanFingerprint {
        self.fingerprint
    }

    pub fn instantiate(&self) -> Result<SealedApp, PluginInstantiationError> {
        self.instantiate_retained()
            .map_err(RetainedPluginInstantiationFailure::into_error)
    }

    /// Instantiates this repeatable plan while retaining failed App ownership for bounded cleanup.
    pub fn instantiate_retained(&self) -> Result<SealedApp, RetainedPluginInstantiationFailure> {
        self.instantiate_retained_with_close_policy(RuntimeClosePolicy::default())
    }

    /// Instantiates this repeatable plan with an explicit failed-preparation cleanup policy.
    pub fn instantiate_retained_with_close_policy(
        &self,
        close_policy: RuntimeClosePolicy,
    ) -> Result<SealedApp, RetainedPluginInstantiationFailure> {
        let prepared = match self.prepare_definitions() {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(RetainedPluginInstantiationFailure::new(
                    error.into(),
                    RuntimePreparationRetirement::complete(),
                ));
            }
        };
        self.commit_and_seal(App::new(), prepared, close_policy)
            .map_err(|failure| {
                RetainedPluginInstantiationFailure::new(failure.error.into(), failure.retirement)
            })
    }

    /// Builds one sealed App and transfers the caller's complete ledger directly into an
    /// unpublished runtime candidate.
    ///
    /// The supplied ledger exists before plugin preparation begins. Every failure path retains the
    /// fresh App, caller reservations, and plugin-registered close participants in one retryable
    /// owner.
    pub fn instantiate_runtime_candidate(
        &self,
        reservation: RuntimeAdmissionReservation,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<RuntimeCandidate, RuntimeConstructionFailure> {
        let mut app = App::new_with_runtime_obligations(obligations);
        let prepared = match self.prepare_definitions() {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(RuntimeConstructionFailure::plugin(
                    error.into(),
                    RuntimePreparationRetirement::from_reserved_app(app, reservation, close_policy),
                ));
            }
        };
        if let Err(error) = app.commit_plugin_batch(self.commit_batch(prepared)) {
            return Err(RuntimeConstructionFailure::plugin(
                error.into(),
                RuntimePreparationRetirement::from_reserved_app(app, reservation, close_policy),
            ));
        }
        if let Err(error) = app.seal_internal() {
            return Err(RuntimeConstructionFailure::plugin(
                error.into(),
                RuntimePreparationRetirement::from_reserved_app(app, reservation, close_policy),
            ));
        }
        reservation
            .admit(
                SealedApp { app },
                RuntimeObligationLedger::new(),
                close_policy,
            )
            .map_err(|failure| {
                let error = failure.error();
                RuntimeConstructionFailure::admission(error, failure.begin_retirement())
            })
    }

    fn prepare_definitions(&self) -> Result<Vec<Arc<dyn Plugin>>, PluginPrepareError> {
        self.definitions
            .iter()
            .map(PluginDefinition::prepare)
            .collect()
    }

    fn commit_and_seal(
        &self,
        mut app: App,
        prepared: Vec<Arc<dyn Plugin>>,
        close_policy: RuntimeClosePolicy,
    ) -> Result<SealedApp, PreparedAppFailure> {
        if let Err(error) = app.commit_plugin_batch(self.commit_batch(prepared)) {
            return Err(PreparedAppFailure {
                error,
                retirement: RuntimePreparationRetirement::from_app(app, close_policy),
            });
        }
        if let Err(error) = app.seal_internal() {
            return Err(PreparedAppFailure {
                error,
                retirement: RuntimePreparationRetirement::from_app(app, close_policy),
            });
        }
        Ok(SealedApp { app })
    }

    fn commit_batch(&self, prepared: Vec<Arc<dyn Plugin>>) -> PluginCommitBatch {
        PluginCommitBatch {
            entries: self.entries.clone(),
            witnesses: self
                .definitions
                .iter()
                .cloned()
                .map(PluginDefinitionWitness::Repeatable)
                .collect(),
            groups: self.groups.clone(),
            disabled_slots: self.disabled_slots.clone(),
            fingerprint: self.fingerprint,
            prefix_len: 0,
            prepared,
        }
    }
}

struct PreparedAppFailure {
    error: PluginError,
    retirement: RuntimePreparationRetirement,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginInstantiationError {
    #[error("plugin preparation failed: {0}")]
    Prepare(#[from] PluginPrepareError),
    #[error("plugin lifecycle commit failed: {0}")]
    Plugin(#[from] PluginError),
}

impl PluginInstantiationError {
    #[must_use]
    pub const fn prepare_error(&self) -> Option<&PluginPrepareError> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Plugin(_) => None,
        }
    }
}

/// Plugin-instantiation failure that keeps every acquired runtime owner retryable.
#[must_use = "plugin instantiation failures may retain unfinished cleanup authority"]
pub struct RetainedPluginInstantiationFailure {
    error: PluginInstantiationError,
    retirement: RuntimePreparationRetirement,
}

impl RetainedPluginInstantiationFailure {
    fn new(error: PluginInstantiationError, retirement: RuntimePreparationRetirement) -> Self {
        Self { error, retirement }
    }

    #[must_use]
    pub const fn error(&self) -> &PluginInstantiationError {
        &self.error
    }

    #[must_use]
    pub fn retirement_state(&self) -> crate::RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> Option<&crate::RuntimeCloseEvidence> {
        self.retirement.close_evidence()
    }

    pub fn drive_retirement(&mut self) -> crate::RuntimeCandidateRetirementState {
        self.retirement.drive_retirement()
    }

    #[must_use]
    pub fn into_error(self) -> PluginInstantiationError {
        self.error
    }
}

impl Debug for RetainedPluginInstantiationFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedPluginInstantiationFailure")
            .field("error", &self.error)
            .field("retirement", &self.retirement)
            .finish()
    }
}

impl fmt::Display for RetainedPluginInstantiationFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for RetainedPluginInstantiationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeConstructionError {
    #[error("runtime plugin construction failed: {0}")]
    Plugin(PluginInstantiationError),
    #[error("runtime candidate admission failed: {0}")]
    Admission(RuntimeAdmissionError),
}

enum RuntimeConstructionCleanup {
    Plugin(RuntimePreparationRetirement),
    Admission(RuntimeAdmissionRetirement),
}

/// Construction failure that retains the one pre-publication owner until cleanup completes.
#[must_use = "runtime construction failures may retain unfinished cleanup authority"]
pub struct RuntimeConstructionFailure {
    error: RuntimeConstructionError,
    cleanup: RuntimeConstructionCleanup,
}

impl RuntimeConstructionFailure {
    fn plugin(error: PluginInstantiationError, retirement: RuntimePreparationRetirement) -> Self {
        Self {
            error: RuntimeConstructionError::Plugin(error),
            cleanup: RuntimeConstructionCleanup::Plugin(retirement),
        }
    }

    fn admission(error: RuntimeAdmissionError, retirement: RuntimeAdmissionRetirement) -> Self {
        Self {
            error: RuntimeConstructionError::Admission(error),
            cleanup: RuntimeConstructionCleanup::Admission(retirement),
        }
    }

    #[must_use]
    pub const fn error(&self) -> &RuntimeConstructionError {
        &self.error
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        match &self.cleanup {
            RuntimeConstructionCleanup::Plugin(retirement) => retirement.retirement_state(),
            RuntimeConstructionCleanup::Admission(retirement) => retirement.retirement_state(),
        }
    }

    #[must_use]
    pub fn close_evidence(&self) -> Option<&RuntimeCloseEvidence> {
        match &self.cleanup {
            RuntimeConstructionCleanup::Plugin(retirement) => retirement.close_evidence(),
            RuntimeConstructionCleanup::Admission(retirement) => Some(retirement.close_evidence()),
        }
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        match &mut self.cleanup {
            RuntimeConstructionCleanup::Plugin(retirement) => retirement.drive_retirement(),
            RuntimeConstructionCleanup::Admission(retirement) => retirement.drive_retirement(),
        }
    }
}

impl Debug for RuntimeConstructionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeConstructionFailure")
            .field("error", &self.error)
            .field("retirement_state", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for RuntimeConstructionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for RuntimeConstructionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AddPluginsError {
    #[error("plugin plan resolution failed: {0}")]
    Plan(#[from] PluginPlanError),
    #[error("plugin preparation failed: {0}")]
    Prepare(#[from] PluginPrepareError),
    #[error("plugin lifecycle commit failed: {0}")]
    Plugin(#[from] PluginError),
}

pub struct SealedApp {
    pub(crate) app: App,
}

impl Debug for SealedApp {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedApp")
            .field("plugin_plan_fingerprint", &self.plugin_plan_fingerprint())
            .field("started", &self.app.started())
            .finish_non_exhaustive()
    }
}

impl SealedApp {
    #[must_use]
    pub fn world(&self) -> &World {
        self.app.world()
    }

    #[must_use]
    pub const fn plugin_plan_fingerprint(&self) -> PluginPlanFingerprint {
        self.app.configuration_fingerprint()
    }

    #[must_use]
    pub const fn has_raw_runner(&self) -> bool {
        self.app.has_raw_runner()
    }

    #[must_use]
    pub const fn started(&self) -> bool {
        self.app.started()
    }
}

pub(crate) struct PluginCommitBatch {
    pub entries: Vec<PluginPlanEntry>,
    pub witnesses: Vec<PluginDefinitionWitness>,
    pub groups: Vec<ResolvedPluginGroup>,
    pub disabled_slots: BTreeSet<PluginSlotId>,
    pub fingerprint: PluginPlanFingerprint,
    pub prefix_len: usize,
    pub prepared: Vec<Arc<dyn Plugin>>,
}

pub(crate) fn install_plugins<M>(
    app: &mut App,
    plugins: impl Plugins<M>,
) -> Result<(), AddPluginsError> {
    app.reject_hook_composition_mutation()?;
    let mut collection = PluginInputCollection::default();
    catch_unwind(AssertUnwindSafe(|| {
        sealed::Plugins::<M>::collect(plugins, &mut collection);
    }))
    .map_err(|_| PluginPlanError::DeclarationPanicked)?;
    let prefix = app.composition_prefix();
    let resolved = resolve_collection(collection, Some(prefix))?;
    let prepared = resolved
        .ordered_drafts
        .iter()
        .skip(resolved.prefix_len)
        .map(|draft| match &draft.materializer {
            PluginMaterializer::Direct(plugin) => Ok(Arc::clone(plugin)),
            PluginMaterializer::Definition(definition) => definition.prepare(),
            PluginMaterializer::Prefix(_) => {
                unreachable!("prefix entries precede the new suffix")
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let witnesses = resolved
        .ordered_drafts
        .iter()
        .map(|draft| draft.materializer.admitted_witness())
        .collect();
    app.commit_plugin_batch(PluginCommitBatch {
        entries: resolved.entries,
        witnesses,
        groups: resolved.groups,
        disabled_slots: resolved.disabled_slots,
        fingerprint: resolved.fingerprint,
        prefix_len: resolved.prefix_len,
        prepared,
    })?;
    Ok(())
}

fn resolve_collection(
    collection: PluginInputCollection,
    prefix: Option<CompositionPrefix>,
) -> Result<ResolvedComposition, PluginPlanError> {
    let mut resolver = PluginResolver::new(prefix.as_ref());
    resolver.expand(collection)?;
    resolver.resolve(prefix)
}

struct PluginResolver {
    drafts: Vec<PluginDraft>,
    group_fingerprints: BTreeMap<PluginGroupId, PluginPlanFingerprint>,
    group_stack: Vec<PluginGroupId>,
    pending_edits: Vec<(PluginGroupId, Vec<ResolvedPluginEdit>)>,
    explicit_edges: BTreeSet<(PluginId, PluginId)>,
    disabled_slots: BTreeSet<PluginSlotId>,
    next_order_hint: usize,
}

impl PluginResolver {
    fn new(prefix: Option<&CompositionPrefix>) -> Self {
        let mut drafts = Vec::new();
        let mut next_order_hint = 0;
        if let Some(prefix) = prefix {
            for (entry, witness) in prefix.entries.iter().zip(&prefix.witnesses) {
                drafts.push(PluginDraft {
                    declaration: entry.declaration,
                    slot: entry.slot,
                    materializer: PluginMaterializer::Prefix(witness.clone()),
                    provenance: entry.group_provenance.iter().copied().collect(),
                    order_hint: next_order_hint,
                    disabled: false,
                });
                next_order_hint += 1;
            }
        }
        Self {
            drafts,
            group_fingerprints: prefix
                .map(|prefix| {
                    prefix
                        .groups
                        .iter()
                        .map(|group| (group.id, group.definition_fingerprint))
                        .collect()
                })
                .unwrap_or_default(),
            group_stack: Vec::new(),
            pending_edits: Vec::new(),
            explicit_edges: BTreeSet::new(),
            disabled_slots: prefix
                .map(|prefix| prefix.disabled_slots.clone())
                .unwrap_or_default(),
            next_order_hint,
        }
    }

    fn expand(&mut self, collection: PluginInputCollection) -> Result<(), PluginPlanError> {
        for input in collection.roots {
            match input {
                PluginInput::Direct {
                    declaration,
                    plugin,
                } => self.push_draft(
                    declaration,
                    None,
                    PluginMaterializer::Direct(plugin),
                    BTreeSet::new(),
                )?,
                PluginInput::Definition(definition) => {
                    let definition = definition.resolve_declaration()?;
                    self.push_draft(
                        definition.resolved_declaration(),
                        None,
                        PluginMaterializer::Definition(definition),
                        BTreeSet::new(),
                    )?;
                }
                PluginInput::Group { group, edits } => {
                    let id = group.id();
                    self.expand_group(group, BTreeSet::new())?;
                    self.pending_edits.push((id, resolve_edits(edits)?));
                }
            }
        }
        Ok(())
    }

    fn expand_group(
        &mut self,
        group: Box<dyn ErasedPluginGroup>,
        mut provenance: BTreeSet<PluginGroupId>,
    ) -> Result<PluginPlanFingerprint, PluginPlanError> {
        let id = group.id();
        if let Some(cycle_start) = self
            .group_stack
            .iter()
            .position(|candidate| *candidate == id)
        {
            let mut chain = self.group_stack[cycle_start..].to_vec();
            chain.push(id);
            return Err(PluginPlanError::GroupCycle { chain });
        }
        self.group_stack.push(id);
        provenance.insert(id);
        let builder = catch_unwind(AssertUnwindSafe(|| group.build()))
            .map_err(|_| PluginPlanError::GroupPanicked { group: id })?;
        let mut encoder = FingerprintEncoder::new(b"nara.plugin-group-definition.v2");
        encoder.string(b"group-id", id.as_str());
        encoder.u64(b"item-count", builder.items.len() as u64);
        for item in builder.items {
            match item {
                PluginGroupItem::Entry { slot, definition } => {
                    let definition = definition.resolve_declaration()?;
                    encoder.bytes(b"item-kind", b"entry");
                    encode_definition(&mut encoder, &definition, slot);
                    self.push_draft(
                        definition.resolved_declaration(),
                        slot,
                        PluginMaterializer::Definition(definition),
                        provenance.clone(),
                    )?;
                }
                PluginGroupItem::Group { group, edits } => {
                    let nested_id = group.id();
                    let nested_fingerprint = self.expand_group(group, provenance.clone())?;
                    let edits = resolve_edits(edits)?;
                    encoder.bytes(b"item-kind", b"nested-group");
                    encoder.string(b"nested-group-id", nested_id.as_str());
                    encoder.digest(b"nested-group-fingerprint", &nested_fingerprint.0);
                    encoder.u64(b"nested-edit-count", edits.len() as u64);
                    for edit in &edits {
                        encode_edit(&mut encoder, edit);
                    }
                    self.pending_edits.push((nested_id, edits));
                }
            }
        }
        self.group_stack.pop();
        let fingerprint = encoder.finish();
        if let Some(existing) = self.group_fingerprints.get(&id)
            && *existing != fingerprint
        {
            return Err(PluginPlanError::DivergentGroup { group: id });
        }
        self.group_fingerprints.insert(id, fingerprint);
        Ok(fingerprint)
    }

    fn push_draft(
        &mut self,
        declaration: &'static PluginDeclaration,
        slot: Option<PluginSlot>,
        materializer: PluginMaterializer,
        provenance: BTreeSet<PluginGroupId>,
    ) -> Result<(), PluginPlanError> {
        if let Some(slot) = slot
            && slot.expected_plugin != declaration.id
        {
            return Err(PluginPlanError::SlotPluginMismatch {
                slot: slot.id,
                expected: slot.expected_plugin,
                actual: declaration.id,
            });
        }
        self.drafts.push(PluginDraft {
            declaration,
            slot,
            materializer,
            provenance,
            order_hint: self.next_order_hint,
            disabled: false,
        });
        self.next_order_hint += 1;
        Ok(())
    }

    fn resolve(
        mut self,
        prefix: Option<CompositionPrefix>,
    ) -> Result<ResolvedComposition, PluginPlanError> {
        self.apply_edits()?;
        let prefix_len = prefix.as_ref().map_or(0, |prefix| prefix.entries.len());
        let mut selected = Vec::<PluginDraft>::new();
        let mut by_plugin = BTreeMap::<PluginId, usize>::new();
        let mut by_slot = BTreeMap::<PluginSlotId, PluginId>::new();
        for draft in self.drafts.into_iter().filter(|draft| !draft.disabled) {
            if let Some(slot) = draft.slot {
                if let Some(first) = by_slot.get(&slot.id).copied()
                    && first != draft.declaration.id
                {
                    return Err(PluginPlanError::DuplicateSlot {
                        slot: slot.id,
                        first,
                        duplicate: draft.declaration.id,
                    });
                }
                by_slot.insert(slot.id, draft.declaration.id);
            }
            if let Some(existing_index) = by_plugin.get(&draft.declaration.id).copied() {
                let existing = &mut selected[existing_index];
                if existing.slot != draft.slot {
                    if let (Some(existing_slot), Some(candidate_slot)) = (existing.slot, draft.slot)
                        && existing_slot.id == candidate_slot.id
                    {
                        return Err(PluginPlanError::DivergentSlotContract {
                            slot: existing_slot.id,
                        });
                    }
                    return Err(PluginPlanError::DuplicatePlugin {
                        plugin: draft.declaration.id,
                    });
                }
                if !existing.materializer.exact_eq(&draft.materializer) {
                    return Err(PluginPlanError::DivergentDefinition {
                        plugin: draft.declaration.id,
                    });
                }
                existing.provenance.extend(draft.provenance);
                continue;
            }
            by_plugin.insert(draft.declaration.id, selected.len());
            selected.push(draft);
        }

        for draft in &selected {
            if let Some(slot) = draft.slot
                && self.disabled_slots.contains(&slot.id)
            {
                return Err(PluginPlanError::ActiveSlotDisabled { slot: slot.id });
            }
        }

        validate_closure(&selected, &by_plugin)?;
        let ordered_drafts = stable_order(selected, &self.explicit_edges)?;
        if prefix_len > ordered_drafts.len()
            || ordered_drafts[..prefix_len]
                .iter()
                .any(|draft| !matches!(draft.materializer, PluginMaterializer::Prefix(_)))
        {
            return Err(PluginPlanError::ImmutablePrefix);
        }
        if ordered_drafts[prefix_len..]
            .iter()
            .any(|draft| matches!(draft.materializer, PluginMaterializer::Prefix(_)))
        {
            return Err(PluginPlanError::ImmutablePrefix);
        }

        let entries = ordered_drafts
            .iter()
            .map(|draft| PluginPlanEntry {
                declaration: draft.declaration,
                definition_key: draft.materializer.definition_key(),
                slot: draft.slot,
                group_provenance: draft.provenance.iter().copied().collect(),
            })
            .collect::<Vec<_>>();
        let groups = resolved_groups(
            prefix.as_ref().map(|prefix| prefix.groups.as_slice()),
            &self.group_fingerprints,
            &entries,
        );
        let fingerprint = fingerprint_plan(&entries, &groups, &self.disabled_slots);
        Ok(ResolvedComposition {
            entries,
            ordered_drafts,
            groups,
            disabled_slots: self.disabled_slots,
            fingerprint,
            prefix_len,
        })
    }

    fn apply_edits(&mut self) -> Result<(), PluginPlanError> {
        for (group, edits) in std::mem::take(&mut self.pending_edits) {
            for edit in edits {
                match edit {
                    ResolvedPluginEdit::Disable(target) => {
                        let indices = self.logical_targets(group, target)?;
                        let slot = self.drafts[indices[0]].slot.ok_or(
                            PluginPlanError::RequiredSlotDisabled {
                                slot: PluginSlotId::new("nara.plugin.unslotted"),
                            },
                        )?;
                        if slot.presence == PluginSlotPresence::Required {
                            return Err(PluginPlanError::RequiredSlotDisabled { slot: slot.id });
                        }
                        for index in indices {
                            self.drafts[index].disabled = true;
                        }
                        self.disabled_slots.insert(slot.id);
                    }
                    ResolvedPluginEdit::Configure(target, definition) => {
                        let indices = self.logical_targets(group, target)?;
                        let declaration = definition.resolved_declaration();
                        if self.drafts[indices[0]].declaration.id != declaration.id {
                            return Err(PluginPlanError::SlotPluginMismatch {
                                slot: self.drafts[indices[0]]
                                    .slot
                                    .map(PluginSlot::id)
                                    .unwrap_or(PluginSlotId::new("nara.plugin.unslotted")),
                                expected: self.drafts[indices[0]].declaration.id,
                                actual: declaration.id,
                            });
                        }
                        for index in indices {
                            self.drafts[index].declaration = declaration;
                            self.drafts[index].materializer =
                                PluginMaterializer::Definition(definition.clone());
                        }
                    }
                    ResolvedPluginEdit::InsertAfter(target, definition) => {
                        let indices = self.logical_targets(group, target)?;
                        let anchor = self.drafts[indices[0]].declaration.id;
                        let declaration = definition.resolved_declaration();
                        let inserted = declaration.id;
                        self.push_draft(
                            declaration,
                            None,
                            PluginMaterializer::Definition(definition),
                            BTreeSet::from([group]),
                        )?;
                        self.explicit_edges.insert((anchor, inserted));
                    }
                    ResolvedPluginEdit::InsertBefore(target, definition) => {
                        let indices = self.logical_targets(group, target)?;
                        let anchor = self.drafts[indices[0]].declaration.id;
                        let declaration = definition.resolved_declaration();
                        let inserted = declaration.id;
                        self.push_draft(
                            declaration,
                            None,
                            PluginMaterializer::Definition(definition),
                            BTreeSet::from([group]),
                        )?;
                        self.explicit_edges.insert((inserted, anchor));
                    }
                }
            }
        }
        Ok(())
    }

    fn logical_targets(
        &self,
        group: PluginGroupId,
        target: ResolvedEditTarget,
    ) -> Result<Vec<usize>, PluginPlanError> {
        let matches = self
            .drafts
            .iter()
            .enumerate()
            .filter(|(_, draft)| {
                draft.provenance.contains(&group)
                    && match target {
                        ResolvedEditTarget::Plugin(plugin) => draft.declaration.id == plugin,
                        ResolvedEditTarget::Slot(slot) => {
                            draft.slot.is_some_and(|candidate| candidate.id == slot)
                        }
                    }
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(first) = matches.first().copied() else {
            return Err(PluginPlanError::MissingEditTarget);
        };
        for candidate in matches.iter().copied().skip(1) {
            let first_draft = &self.drafts[first];
            let candidate_draft = &self.drafts[candidate];
            if first_draft.declaration.id != candidate_draft.declaration.id
                || first_draft.slot != candidate_draft.slot
            {
                if let (Some(first_slot), Some(candidate_slot)) =
                    (first_draft.slot, candidate_draft.slot)
                    && first_slot.id == candidate_slot.id
                {
                    return Err(PluginPlanError::DivergentSlotContract {
                        slot: first_slot.id,
                    });
                }
                return Err(PluginPlanError::AmbiguousEditTarget);
            }
        }
        Ok(matches)
    }
}

fn validate_closure(
    drafts: &[PluginDraft],
    selected: &BTreeMap<PluginId, usize>,
) -> Result<(), PluginPlanError> {
    let capabilities = drafts
        .iter()
        .flat_map(|draft| draft.declaration.provides.iter().copied())
        .collect::<BTreeSet<_>>();
    let services = drafts
        .iter()
        .flat_map(|draft| draft.declaration.provides_services.iter().copied())
        .collect::<BTreeSet<_>>();
    let schema = drafts
        .iter()
        .flat_map(|draft| draft.declaration.provides_schema.iter().copied())
        .collect::<BTreeSet<_>>();

    for draft in drafts {
        let declaration = draft.declaration;
        for required in declaration.requires_plugins {
            if !selected.contains_key(required) {
                return Err(PluginPlanError::MissingPlugin {
                    plugin: declaration.id,
                    required: *required,
                });
            }
        }
        for required in declaration.requires_capabilities {
            if !capabilities.contains(required) {
                return Err(PluginPlanError::MissingCapability {
                    plugin: declaration.id,
                    required: *required,
                });
            }
        }
        for required in declaration.requires_services {
            if !services.contains(required) {
                return Err(PluginPlanError::MissingService {
                    plugin: declaration.id,
                    required: *required,
                });
            }
        }
        for required in declaration.requires_schema {
            if !schema.contains(required) {
                return Err(PluginPlanError::MissingSchemaProvider {
                    plugin: declaration.id,
                    required: *required,
                });
            }
        }
        for conflict in declaration.conflicts {
            if selected.contains_key(conflict) {
                return Err(PluginPlanError::Conflict {
                    plugin: declaration.id,
                    conflict: *conflict,
                });
            }
        }
    }
    Ok(())
}

fn stable_order(
    drafts: Vec<PluginDraft>,
    explicit_edges: &BTreeSet<(PluginId, PluginId)>,
) -> Result<Vec<PluginDraft>, PluginPlanError> {
    let mut by_id = drafts
        .into_iter()
        .map(|draft| (draft.declaration.id, draft))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = by_id
        .keys()
        .copied()
        .map(|id| (id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut indegree = by_id
        .keys()
        .copied()
        .map(|id| (id, 0_usize))
        .collect::<BTreeMap<_, _>>();

    let mut edges = explicit_edges.clone();
    for draft in by_id.values() {
        for required in draft.declaration.requires_plugins {
            edges.insert((*required, draft.declaration.id));
        }
    }
    for (before, after) in edges {
        if before == after || !by_id.contains_key(&before) || !by_id.contains_key(&after) {
            continue;
        }
        if outgoing
            .get_mut(&before)
            .expect("known plugin")
            .insert(after)
        {
            *indegree.get_mut(&after).expect("known plugin") += 1;
        }
    }

    let mut ordered = Vec::with_capacity(by_id.len());
    while !by_id.is_empty() {
        let next = by_id
            .iter()
            .filter(|(id, _)| indegree.get(id).copied().unwrap_or_default() == 0)
            .min_by_key(|(id, draft)| (draft.order_hint, **id))
            .map(|(id, _)| *id);
        let Some(next) = next else {
            return Err(PluginPlanError::OrderingCycle {
                plugins: by_id.keys().copied().collect(),
            });
        };
        let draft = by_id.remove(&next).expect("the selected plugin exists");
        for successor in outgoing.remove(&next).unwrap_or_default() {
            *indegree.get_mut(&successor).expect("known successor") -= 1;
        }
        ordered.push(draft);
    }
    Ok(ordered)
}

fn resolved_groups(
    prefix: Option<&[ResolvedPluginGroup]>,
    fingerprints: &BTreeMap<PluginGroupId, PluginPlanFingerprint>,
    entries: &[PluginPlanEntry],
) -> Vec<ResolvedPluginGroup> {
    let mut groups = prefix
        .unwrap_or_default()
        .iter()
        .cloned()
        .map(|group| (group.id, group))
        .collect::<BTreeMap<_, _>>();
    for (id, fingerprint) in fingerprints {
        let plugins = entries
            .iter()
            .filter(|entry| entry.group_provenance.contains(id))
            .map(PluginPlanEntry::plugin_id)
            .collect();
        groups.insert(
            *id,
            ResolvedPluginGroup {
                id: *id,
                definition_fingerprint: *fingerprint,
                plugins,
            },
        );
    }
    groups.into_values().collect()
}

pub(crate) fn prefix_from_parts(
    entries: Vec<PluginPlanEntry>,
    witnesses: Vec<PluginDefinitionWitness>,
    groups: Vec<ResolvedPluginGroup>,
    disabled_slots: BTreeSet<PluginSlotId>,
) -> CompositionPrefix {
    CompositionPrefix {
        entries,
        witnesses,
        groups,
        disabled_slots,
    }
}
