//! Finite trusted child proposals retain their project, execution and document authority here.

use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentTypeId, ComponentValue, resolve_component_property,
};
use nara_scene::{
    ConstructionEntity, LocalConstructionEntity, ScenePlacement, ScenePlacementOwner,
};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest, TaskTerminal,
};
use nara_tooling::{
    EditorDocumentQueryBasis, EditorPropertyRead, EditorPropertyReadLimits, EditorPropertyTarget,
    EditorPropertyValue, EditorSourceToolActivationId, EditorSourceToolActive,
    EditorSourceToolAttemptId, EditorSourceToolCommit, EditorSourceToolError,
    EditorSourceToolIntegerProperty, EditorSourceToolIntent, EditorSourceToolRequestId,
    EditorSourceToolReview, EditorSourceToolReviewSummary, EditorSourceToolView,
    PreparedEditorDocumentConstruction,
};
use serde::{Deserialize, Serialize};

use super::{EditorEditPreparationError, EditorProjectSession};

mod process;
#[cfg(test)]
mod tests;

const TOOL_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x736f_7572_6365_746c);
const WIRE_BYTES: usize = 64 * 1024;
const VALUE_NODES: usize = 4096;
const VALUE_BYTES: usize = 32 * 1024;
const MAX_WRITES: usize = 64;
const MAX_FIELDS: usize = 16;
const MAX_JOBS: usize = 2;

/// Explicit trusted Host launch configuration. This is not an executable package or sandbox.
#[derive(Debug, Clone)]
pub struct EditorSourceToolCommand {
    executable: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    timeout: Duration,
}

impl EditorSourceToolCommand {
    pub fn new(
        executable: PathBuf,
        arguments: Vec<OsString>,
        working_directory: PathBuf,
        timeout: Duration,
    ) -> Result<Self, EditorSourceToolError> {
        if !executable.is_absolute()
            || !working_directory.is_absolute()
            || arguments.len() > 16
            || timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || executable
                .as_os_str()
                .len()
                .saturating_add(working_directory.as_os_str().len())
                .saturating_add(arguments.iter().map(|value| value.len()).sum::<usize>())
                > 32 * 1024
        {
            return Err(EditorSourceToolError::Definition);
        }
        Ok(Self {
            executable,
            arguments,
            working_directory,
            timeout,
        })
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
    #[must_use]
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// One selected component contract and finite command, supplied explicitly by a trusted Host.
#[derive(Debug, Clone)]
pub struct EditorSourceToolDefinition {
    command: EditorSourceToolCommand,
    owner: ComponentSchemaOwnerSnapshot,
    component: ComponentTypeId,
}

impl EditorSourceToolDefinition {
    pub fn new(
        command: EditorSourceToolCommand,
        owner: ComponentSchemaOwnerSnapshot,
        component: ComponentTypeId,
    ) -> Result<Self, EditorSourceToolError> {
        let schema = owner
            .schema(&component)
            .ok_or(EditorSourceToolError::Definition)?;
        if schema.fields().len() > MAX_FIELDS || schema.authoring_initial_value().is_none() {
            return Err(EditorSourceToolError::Definition);
        }
        Ok(Self {
            command,
            owner,
            component,
        })
    }
}

impl From<EditorEditPreparationError> for EditorSourceToolError {
    fn from(error: EditorEditPreparationError) -> Self {
        match error {
            EditorEditPreparationError::Closing => Self::Closing,
            EditorEditPreparationError::WorkspaceUnavailable => Self::WorkspaceUnavailable,
            EditorEditPreparationError::QueryBasis(error) => Self::QueryBasis(error),
            EditorEditPreparationError::StructureContractMismatch { document } => {
                Self::StructureContractMismatch { document }
            }
            EditorEditPreparationError::Document(report) => Self::Preparation(report),
        }
    }
}

#[derive(Clone)]
struct Activation {
    active: EditorSourceToolActive,
    definition: EditorSourceToolDefinition,
}

struct Invocation {
    activation: Activation,
    request: EditorSourceToolRequestId,
    basis: EditorDocumentQueryBasis,
    intent: EditorSourceToolIntent,
}

struct ReviewedOperation {
    activation: EditorSourceToolActivationId,
    request: EditorSourceToolRequestId,
    basis: EditorDocumentQueryBasis,
    intent: EditorSourceToolIntent,
    title: Arc<str>,
    created: Option<LocalConstructionEntity>,
    prepared: PreparedEditorDocumentConstruction,
}

type ProcessTask = TaskHandle<Result<Vec<u8>, process::ProcessFailure>>;

struct Probe {
    attempt: EditorSourceToolAttemptId,
    sequence: u64,
    definition: EditorSourceToolDefinition,
    task: ProcessTask,
}

struct PendingInvocation {
    invocation: Invocation,
    sequence: u64,
    task: ProcessTask,
}

#[derive(Default)]
pub(super) struct EditorSourceTools {
    active: Option<Activation>,
    probe: Option<Probe>,
    invocation: Option<PendingInvocation>,
    retiring: Vec<ProcessTask>,
    reviewed: Option<ReviewedOperation>,
    attempt: Option<EditorSourceToolAttemptId>,
    latest: Option<EditorSourceToolRequestId>,
    sequence: u64,
    attempt_error: Option<EditorSourceToolError>,
    request_error: Option<EditorSourceToolError>,
    #[cfg(test)]
    next_gates: process::ProcessTestGates,
}

impl EditorSourceTools {
    fn jobs(&self) -> usize {
        usize::from(self.probe.is_some())
            + usize::from(self.invocation.is_some())
            + self.retiring.len()
    }

    fn next_sequence(&mut self) -> Result<u64, EditorSourceToolError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(EditorSourceToolError::Limit)?;
        Ok(self.sequence)
    }

    fn retire_invocation(&mut self) {
        if let Some(pending) = self.invocation.take() {
            pending.task.cancel();
            self.retiring.push(pending.task);
        }
        self.reviewed = None;
    }

    pub(super) fn cancel(&mut self) {
        self.active = None;
        self.latest = None;
        self.request_error = None;
        self.retire_invocation();
        if let Some(probe) = self.probe.take() {
            self.attempt_error = Some(EditorSourceToolError::Process("cancelled"));
            probe.task.cancel();
            self.retiring.push(probe.task);
        }
    }

    pub(super) fn view(&self) -> EditorSourceToolView {
        let review = self
            .reviewed
            .as_ref()
            .map(|review| EditorSourceToolReviewSummary {
                request: review.request.clone(),
                basis: review.basis.clone(),
                intent: review.intent.clone(),
                title: review.title.clone(),
                operations: review.prepared.patch().operations.len(),
                created_entity: review
                    .created
                    .as_ref()
                    .and_then(|local| review.prepared.entity(local).cloned()),
            });
        EditorSourceToolView {
            active: self
                .active
                .as_ref()
                .map(|activation| activation.active.clone()),
            probing: self.probe.is_some(),
            working: self.invocation.is_some(),
            retiring: !self.retiring.is_empty(),
            attempt: self.attempt.clone(),
            request: self.latest.clone(),
            review,
            attempt_error: self.attempt_error.clone(),
            request_error: self.request_error.clone(),
        }
    }
}

#[derive(Serialize)]
struct WireRequest {
    protocol_version: u32,
    request: u64,
    operation: &'static str,
    name: String,
    fields: BTreeMap<String, ComponentValue>,
    parameters: BTreeMap<String, ComponentValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireReply {
    protocol_version: u32,
    request: u64,
    result: WireResult,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireResult {
    Description {
        catalog: serde_json::Value,
        #[serde(default)]
        integer_property: Option<WireIntegerProperty>,
    },
    Proposal {
        title: String,
        writes: Vec<WireWrite>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireIntegerProperty {
    field: String,
    parameter: String,
    label: String,
    initial: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWrite {
    property: ComponentPropertyPath,
    value: ComponentValue,
}

impl EditorProjectSession {
    #[must_use]
    pub fn source_tool_view(&self) -> EditorSourceToolView {
        self.source_tools.view()
    }

    #[must_use]
    pub fn source_tool_review(&self) -> Option<EditorSourceToolReview<'_>> {
        self.source_tools.reviewed.as_ref().map(|operation| {
            EditorSourceToolReview::new(
                &operation.request,
                &operation.title,
                &operation.prepared,
                operation.created.as_ref(),
            )
        })
    }

    /// Probes a trusted finite child. Only a matching, exited candidate replaces active routing.
    pub fn enable_source_tool(
        &mut self,
        definition: EditorSourceToolDefinition,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolError> {
        self.ensure_source_tool_admission()?;
        self.advance_source_tools();
        if self.source_tools.probe.is_some() || self.source_tools.jobs() >= MAX_JOBS {
            return Err(EditorSourceToolError::Busy);
        }
        let sequence = self.source_tools.next_sequence()?;
        let request = WireRequest {
            protocol_version: 1,
            request: sequence,
            operation: "describe",
            name: String::new(),
            fields: BTreeMap::new(),
            parameters: BTreeMap::new(),
        };
        let task = self.spawn_source_tool(&definition.command, &request)?;
        let attempt = EditorSourceToolAttemptId::issue();
        self.source_tools.attempt = Some(attempt.clone());
        self.source_tools.attempt_error = None;
        self.source_tools.probe = Some(Probe {
            attempt: attempt.clone(),
            sequence,
            definition,
            task,
        });
        Ok(attempt)
    }

    /// Revokes new execution and retained reviews immediately; physical work remains owned.
    pub fn disable_source_tool(
        &mut self,
        activation: &EditorSourceToolActivationId,
    ) -> Result<(), EditorSourceToolError> {
        let active = self
            .source_tools
            .active
            .as_ref()
            .ok_or(EditorSourceToolError::Disabled)?;
        if active.active.id() != activation {
            return Err(EditorSourceToolError::StaleActivation);
        }
        self.source_tools.cancel();
        self.advance_source_tools();
        Ok(())
    }

    /// Cancels only the currently pending probe identified by its own attempt token.
    pub fn cancel_source_tool_attempt(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
    ) -> Result<(), EditorSourceToolError> {
        let current = self
            .source_tools
            .probe
            .as_ref()
            .ok_or(EditorSourceToolError::StaleAttempt)?;
        if &current.attempt != attempt {
            return Err(EditorSourceToolError::StaleAttempt);
        }
        let probe = self
            .source_tools
            .probe
            .take()
            .expect("the current probe was observed");
        self.source_tools.attempt_error = Some(EditorSourceToolError::Process("cancelled"));
        probe.task.cancel();
        self.source_tools.retiring.push(probe.task);
        self.advance_source_tools();
        Ok(())
    }

    /// Captures copied input against the original source observation without publishing an edit.
    pub fn request_source_tool(
        &mut self,
        activation_id: &EditorSourceToolActivationId,
        basis: &EditorDocumentQueryBasis,
        intent: EditorSourceToolIntent,
        parameters: BTreeMap<String, ComponentValue>,
    ) -> Result<EditorSourceToolRequestId, EditorSourceToolError> {
        self.ensure_source_tool_admission()?;
        let current = self
            .source_tools
            .active
            .clone()
            .ok_or(EditorSourceToolError::Disabled)?;
        if current.active.id() != activation_id {
            return Err(EditorSourceToolError::StaleActivation);
        }
        self.advance_source_tools();
        let activation = self
            .source_tools
            .active
            .clone()
            .ok_or(EditorSourceToolError::Disabled)?;
        if activation.active.id() != activation_id {
            return Err(EditorSourceToolError::StaleActivation);
        }
        let request = EditorSourceToolRequestId::issue();
        self.source_tools.latest = Some(request.clone());
        self.source_tools.retire_invocation();
        self.source_tools.request_error = None;
        let result = self.start_source_tool_invocation(
            basis,
            intent,
            parameters,
            activation,
            request.clone(),
        );
        if let Err(error) = &result {
            self.source_tools.request_error = Some(error.clone());
        }
        result.map(|()| request)
    }

    /// Requests the admitted integer facet against the exact source target and basis.
    pub fn request_source_tool_integer_property(
        &mut self,
        activation: &EditorSourceToolActivationId,
        target: EditorPropertyTarget,
        value: i64,
    ) -> Result<EditorSourceToolRequestId, EditorSourceToolError> {
        self.ensure_source_tool_admission()?;
        let active = self
            .source_tools
            .active
            .as_ref()
            .ok_or(EditorSourceToolError::Disabled)?;
        if active.active.id() != activation {
            return Err(EditorSourceToolError::StaleActivation);
        }
        if !active.active.matches_integer_property(&target) {
            return Err(EditorSourceToolError::PropertyFacet);
        }
        self.advance_source_tools();
        let active = self
            .source_tools
            .active
            .as_ref()
            .ok_or(EditorSourceToolError::Disabled)?;
        if active.active.id() != activation {
            return Err(EditorSourceToolError::StaleActivation);
        }
        if !active.active.matches_integer_property(&target) {
            return Err(EditorSourceToolError::PropertyFacet);
        }
        let parameter = active
            .active
            .integer_property()
            .ok_or(EditorSourceToolError::PropertyFacet)?
            .parameter()
            .to_owned();
        self.request_source_tool(
            activation,
            &target.basis,
            EditorSourceToolIntent::Edit {
                entity: target.entity,
            },
            BTreeMap::from([(parameter, ComponentValue::I64(value))]),
        )
    }

    /// Accepts the same reviewed candidate only while its tool, request and source remain current.
    pub fn accept_source_tool_review(
        &mut self,
        request: &EditorSourceToolRequestId,
    ) -> Result<EditorSourceToolCommit, EditorSourceToolError> {
        self.ensure_source_tool_admission()?;
        self.advance_source_tools();
        self.validate_source_tool_request(request)?;
        let reviewed = self
            .source_tools
            .reviewed
            .take()
            .ok_or(EditorSourceToolError::Busy)?;
        self.validate_authoring_query_basis(&reviewed.basis)?;
        let committed = self
            .commit_authoring_construction(reviewed.prepared)
            .map_err(|outcome| EditorSourceToolError::Commit(Box::new(outcome)))?;
        let created_entity = reviewed
            .created
            .as_ref()
            .and_then(|local| committed.entity(local).cloned());
        Ok(EditorSourceToolCommit {
            report: committed.report().clone(),
            created_entity,
        })
    }

    fn ensure_source_tool_admission(&self) -> Result<(), EditorSourceToolError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            Err(EditorSourceToolError::Closing)
        } else {
            Ok(())
        }
    }

    fn validate_source_tool_request(
        &self,
        request: &EditorSourceToolRequestId,
    ) -> Result<(), EditorSourceToolError> {
        let active = self
            .source_tools
            .active
            .as_ref()
            .ok_or(EditorSourceToolError::Disabled)?;
        if self.source_tools.latest.as_ref() != Some(request) {
            return Err(EditorSourceToolError::StaleRequest);
        }
        if self
            .source_tools
            .reviewed
            .as_ref()
            .is_some_and(|review| review.activation != *active.active.id())
        {
            return Err(EditorSourceToolError::StaleActivation);
        }
        Ok(())
    }

    fn start_source_tool_invocation(
        &mut self,
        basis: &EditorDocumentQueryBasis,
        intent: EditorSourceToolIntent,
        parameters: BTreeMap<String, ComponentValue>,
        activation: Activation,
        request: EditorSourceToolRequestId,
    ) -> Result<(), EditorSourceToolError> {
        if self.source_tools.jobs() >= MAX_JOBS {
            return Err(EditorSourceToolError::Busy);
        }
        self.validate_authoring_query_basis(basis)?;
        let definition = &activation.definition;
        let schema = definition
            .owner
            .schema(&definition.component)
            .ok_or(EditorSourceToolError::Schema)?;
        let admitted = basis.schema.as_ref().ok_or(EditorSourceToolError::Schema)?;
        if admitted.component_schema_owner(&definition.component)
            != Some(definition.owner.owner_id())
            || admitted.schema(&definition.component) != Some(schema)
        {
            return Err(EditorSourceToolError::Schema);
        }
        let mut fields = BTreeMap::new();
        let mut bytes: usize = 0;
        let mut nodes: usize = 0;
        for field in schema.fields() {
            bytes = bytes
                .checked_add(field.id.as_str().len())
                .filter(|bytes| *bytes <= VALUE_BYTES)
                .ok_or(EditorSourceToolError::Limit)?;
            let property = ComponentPropertyPath::new(field.id.clone());
            let value = match &intent {
                EditorSourceToolIntent::Edit { entity } => {
                    let limits = EditorPropertyReadLimits::new(
                        4096,
                        VALUE_NODES.saturating_sub(nodes),
                        VALUE_BYTES.saturating_sub(bytes),
                    )
                    .map_err(EditorSourceToolError::QueryBasis)?;
                    match self
                        .read_document_property(EditorPropertyRead {
                            target: EditorPropertyTarget {
                                basis: basis.clone(),
                                entity: entity.clone(),
                                component: definition.component.clone(),
                                property,
                            },
                            limits,
                        })
                        .map_err(EditorSourceToolError::QueryBasis)?
                        .value
                    {
                        EditorPropertyValue::Present(value)
                        | EditorPropertyValue::Defaulted(value) => Cow::Owned(value),
                        _ => return Err(EditorSourceToolError::Schema),
                    }
                }
                _ => Cow::Borrowed(
                    resolve_component_property(
                        schema,
                        schema.version(),
                        schema
                            .authoring_initial_value()
                            .ok_or(EditorSourceToolError::Definition)?,
                        &property,
                    )
                    .map_err(|_| EditorSourceToolError::Schema)?
                    .value,
                ),
            };
            let cost = value
                .cost_up_to(
                    VALUE_NODES.saturating_sub(nodes),
                    VALUE_BYTES.saturating_sub(bytes),
                )
                .ok_or(EditorSourceToolError::Limit)?;
            bytes += cost.logical_bytes();
            nodes += cost.nodes();
            fields.insert(field.id.as_str().to_owned(), value.into_owned());
        }
        if parameters.len() > MAX_FIELDS {
            return Err(EditorSourceToolError::Limit);
        }
        for (key, value) in &parameters {
            bytes = bytes
                .checked_add(key.len())
                .filter(|bytes| *bytes <= VALUE_BYTES)
                .ok_or(EditorSourceToolError::Limit)?;
            let cost = value
                .cost_up_to(
                    VALUE_NODES.saturating_sub(nodes),
                    VALUE_BYTES.saturating_sub(bytes),
                )
                .ok_or(EditorSourceToolError::Limit)?;
            bytes += cost.logical_bytes();
            nodes += cost.nodes();
        }
        let (operation, name) = match &intent {
            EditorSourceToolIntent::Attach { .. } => ("attach", String::new()),
            EditorSourceToolIntent::Edit { .. } => ("edit", String::new()),
            EditorSourceToolIntent::Spawn { name } if name.len() <= 4096 => ("spawn", name.clone()),
            _ => return Err(EditorSourceToolError::Limit),
        };
        let sequence = self.source_tools.next_sequence()?;
        let task = self.spawn_source_tool(
            &definition.command,
            &WireRequest {
                protocol_version: 1,
                request: sequence,
                operation,
                name,
                fields,
                parameters,
            },
        )?;
        self.source_tools.invocation = Some(PendingInvocation {
            invocation: Invocation {
                activation,
                request,
                basis: basis.clone(),
                intent,
            },
            sequence,
            task,
        });
        Ok(())
    }

    fn spawn_source_tool(
        &mut self,
        command: &EditorSourceToolCommand,
        request: &WireRequest,
    ) -> Result<ProcessTask, EditorSourceToolError> {
        let input = serde_json::to_vec(request).map_err(|_| EditorSourceToolError::Protocol)?;
        if input.len() > WIRE_BYTES {
            return Err(EditorSourceToolError::Limit);
        }
        let process = process::ProcessRequest::new(command.clone(), input);
        #[cfg(test)]
        let process = process.with_test_gates(std::mem::take(&mut self.source_tools.next_gates));
        self.authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(request.request, TOOL_DOMAIN),
                move |token| process.run(token),
            )
            .into_handle()
            .map_err(|_| EditorSourceToolError::TaskRejected)
    }

    pub(super) fn advance_source_tools(&mut self) {
        self.source_tools
            .retiring
            .retain(|task| task.return_observer().state() == TaskReturnState::Pending);
        if self
            .source_tools
            .probe
            .as_ref()
            .is_some_and(|probe| probe.task.return_observer().state() != TaskReturnState::Pending)
        {
            let mut probe = self.source_tools.probe.take().expect("observed a probe");
            let result = take_reply(&mut probe.task, probe.sequence)
                .and_then(|reply| {
                    let WireResult::Description {
                        catalog,
                        integer_property,
                    } = reply
                    else {
                        return Err(EditorSourceToolError::Protocol);
                    };
                    let bytes = serde_json::to_vec(&catalog)
                        .map_err(|_| EditorSourceToolError::Protocol)?;
                    let actual = match probe.definition.owner.predecessor() {
                        Some(predecessor) => {
                            ComponentSchemaCatalog::from_json_bytes_with_predecessor(
                                &bytes,
                                predecessor,
                                Default::default(),
                            )
                        }
                        None => ComponentSchemaCatalog::from_json_bytes(&bytes),
                    }
                    .map_err(|_| EditorSourceToolError::Declaration)?;
                    if &actual != probe.definition.owner.current() {
                        return Err(EditorSourceToolError::Declaration);
                    }
                    let integer_property = integer_property
                        .map(|property| {
                            EditorSourceToolIntegerProperty::new(
                                ComponentFieldId::new(property.field),
                                property.parameter,
                                property.label,
                                property.initial,
                            )
                        })
                        .transpose()?;
                    let active = EditorSourceToolActive::admit(
                        EditorSourceToolActivationId::issue(),
                        probe.definition.owner.clone(),
                        probe.definition.component.clone(),
                        integer_property,
                    )?;
                    Ok(active)
                })
                .and_then(|active| self.ensure_source_tool_admission().map(|()| active));
            match result {
                Ok(active) => {
                    self.source_tools.retire_invocation();
                    self.source_tools.latest = None;
                    self.source_tools.active = Some(Activation {
                        active,
                        definition: probe.definition,
                    });
                    self.source_tools.request_error = None;
                }
                Err(error) => self.source_tools.attempt_error = Some(error),
            }
        }
        if self
            .source_tools
            .invocation
            .as_ref()
            .is_some_and(|pending| {
                pending.task.return_observer().state() != TaskReturnState::Pending
            })
        {
            let mut pending = self
                .source_tools
                .invocation
                .take()
                .expect("observed an invocation");
            let result = take_reply(&mut pending.task, pending.sequence)
                .and_then(|reply| self.prepare_source_tool_result(pending.invocation, reply));
            match result {
                Ok(reviewed) => self.source_tools.reviewed = Some(reviewed),
                Err(error) => self.source_tools.request_error = Some(error),
            }
        }
    }

    fn prepare_source_tool_result(
        &self,
        invocation: Invocation,
        reply: WireResult,
    ) -> Result<ReviewedOperation, EditorSourceToolError> {
        self.ensure_source_tool_admission()?;
        let active = self
            .source_tools
            .active
            .as_ref()
            .ok_or(EditorSourceToolError::Disabled)?;
        if active.active.id() != invocation.activation.active.id() {
            return Err(EditorSourceToolError::StaleActivation);
        }
        if self.source_tools.latest.as_ref() != Some(&invocation.request) {
            return Err(EditorSourceToolError::StaleRequest);
        }
        let mut draft = self.begin_authoring_construction(&invocation.basis)?;
        let WireResult::Proposal { title, writes } = reply else {
            return Err(EditorSourceToolError::Protocol);
        };
        if writes.is_empty() || writes.len() > MAX_WRITES || title.len() > 4096 {
            return Err(EditorSourceToolError::Limit);
        }
        let title: Arc<str> = title.into();
        let mut bytes = 0;
        let mut nodes = 0;
        for write in &writes {
            let cost = write
                .value
                .cost_up_to(
                    VALUE_NODES.saturating_sub(nodes),
                    VALUE_BYTES.saturating_sub(bytes),
                )
                .ok_or(EditorSourceToolError::Limit)?;
            bytes += cost.logical_bytes();
            nodes += cost.nodes();
        }
        let created = match &invocation.intent {
            EditorSourceToolIntent::Spawn { .. } => Some(
                draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .map_err(|_| EditorSourceToolError::Draft)?,
            ),
            _ => None,
        };
        let entity = match &invocation.intent {
            EditorSourceToolIntent::Attach { entity } | EditorSourceToolIntent::Edit { entity } => {
                ConstructionEntity::Existing(entity.clone())
            }
            _ => ConstructionEntity::Local(created.clone().expect("a Spawn creates one local")),
        };
        let component = invocation.activation.definition.component.clone();
        if !matches!(&invocation.intent, EditorSourceToolIntent::Edit { .. }) {
            draft
                .add_component(entity.clone(), component.clone())
                .map_err(|_| EditorSourceToolError::Draft)?;
        }
        if let EditorSourceToolIntent::Spawn { name } = &invocation.intent {
            let name_component = ComponentTypeId::new("nara.scene.Name");
            draft
                .add_component(entity.clone(), name_component.clone())
                .map_err(|_| EditorSourceToolError::Draft)?;
            draft
                .set_property(
                    entity.clone(),
                    name_component,
                    ComponentPropertyPath::new(ComponentFieldId::new("value")),
                    ComponentValue::String(name.clone()),
                )
                .map_err(|_| EditorSourceToolError::Draft)?;
        }
        for write in writes {
            draft
                .set_property(
                    entity.clone(),
                    component.clone(),
                    write.property,
                    write.value,
                )
                .map_err(|_| EditorSourceToolError::Draft)?;
        }
        let prepared = self.prepare_authoring_construction(&invocation.basis, &draft)?;
        Ok(ReviewedOperation {
            activation: invocation.activation.active.id().clone(),
            request: invocation.request,
            basis: invocation.basis,
            intent: invocation.intent.clone(),
            title,
            created,
            prepared,
        })
    }
}

fn take_reply(task: &mut ProcessTask, sequence: u64) -> Result<WireResult, EditorSourceToolError> {
    let bytes = match task.try_take() {
        Some(TaskTerminal::Completed(bytes)) => bytes.map_err(|error| {
            use process::ProcessFailure as Failure;
            EditorSourceToolError::Process(match error {
                Failure::Spawn => "spawn",
                Failure::Io => "io",
                Failure::WorkerPanic => "worker-panic",
                Failure::Cancelled => "cancelled",
                Failure::TimedOut => "timed-out",
                Failure::StdoutLimit => "stdout-limit",
                Failure::StderrLimit => "stderr-limit",
                Failure::NonzeroExit => "nonzero-exit",
            })
        })?,
        _ => return Err(EditorSourceToolError::Process("task-terminal")),
    };
    if bytes.len() > WIRE_BYTES {
        return Err(EditorSourceToolError::Limit);
    }
    let reply: WireReply =
        serde_json::from_slice(&bytes).map_err(|_| EditorSourceToolError::Protocol)?;
    if reply.protocol_version != 1 || reply.request != sequence {
        return Err(EditorSourceToolError::Protocol);
    }
    Ok(reply.result)
}
