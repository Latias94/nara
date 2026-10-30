//! Finite immutable action definitions shared by code and native source files.

use std::{any::Any, error::Error, fmt, mem::size_of, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};

use crate::{ActionBinding, ActionContext, InputBinding};

pub const MAX_ACTION_IDENTIFIER_BYTES: usize = 128;
pub const MAX_ACTION_CONTEXTS: usize = 64;
pub const MAX_ACTION_BINDINGS: usize = 1_024;
pub const MAX_BINDINGS_PER_INPUT: usize = 32;

/// Logical allocation costs, including compile scratch and shared ownership metadata.
///
/// These are conservative admission units, not allocator-resident measurements. Runtime state
/// covers context and retained lifetime snapshots; it excludes physical ingress and action output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionMapCompileCost {
    work: usize,
    retained: usize,
}

impl ActionMapCompileCost {
    #[must_use]
    pub const fn work_bytes(self) -> usize {
        self.work
    }

    #[must_use]
    pub const fn retained_bytes(self) -> usize {
        self.retained
    }

    #[must_use]
    pub const fn runtime_state_bytes(self) -> usize {
        crate::ActionMap::required_state_bytes()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionMapLimits {
    bindings: ItemLimit,
    contexts: ItemLimit,
    identifier_bytes: ByteLimit,
    bindings_per_input: ItemLimit,
    work_bytes: ByteLimit,
    retained_bytes: ByteLimit,
}

impl Default for ActionMapLimits {
    fn default() -> Self {
        Self {
            bindings: ItemLimit::new(MAX_ACTION_BINDINGS).unwrap(),
            contexts: ItemLimit::new(MAX_ACTION_CONTEXTS).unwrap(),
            identifier_bytes: ByteLimit::new(MAX_ACTION_IDENTIFIER_BYTES).unwrap(),
            bindings_per_input: ItemLimit::new(MAX_BINDINGS_PER_INPUT).unwrap(),
            work_bytes: ByteLimit::new(2 * 1024 * 1024).unwrap(),
            retained_bytes: ByteLimit::new(1024 * 1024).unwrap(),
        }
    }
}

impl ActionMapLimits {
    #[must_use]
    pub const fn bindings(self) -> ItemLimit {
        self.bindings
    }
    #[must_use]
    pub const fn contexts(self) -> ItemLimit {
        self.contexts
    }
    #[must_use]
    pub const fn identifier_bytes(self) -> ByteLimit {
        self.identifier_bytes
    }
    #[must_use]
    pub const fn bindings_per_input(self) -> ItemLimit {
        self.bindings_per_input
    }
    #[must_use]
    pub const fn work_bytes(self) -> ByteLimit {
        self.work_bytes
    }
    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }

    #[must_use]
    pub const fn with_bindings(mut self, value: ItemLimit) -> Self {
        self.bindings = value;
        self
    }
    #[must_use]
    pub const fn with_contexts(mut self, value: ItemLimit) -> Self {
        self.contexts = value;
        self
    }
    #[must_use]
    pub const fn with_identifier_bytes(mut self, value: ByteLimit) -> Self {
        self.identifier_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_bindings_per_input(mut self, value: ItemLimit) -> Self {
        self.bindings_per_input = value;
        self
    }
    #[must_use]
    pub const fn with_work_bytes(mut self, value: ByteLimit) -> Self {
        self.work_bytes = value;
        self
    }
    #[must_use]
    pub const fn with_retained_bytes(mut self, value: ByteLimit) -> Self {
        self.retained_bytes = value;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionMapLimitKind {
    Bindings,
    Contexts,
    IdentifierBytes,
    BindingsPerInput,
    WorkBytes,
    RetainedBytes,
    RuntimeStateBytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionMapError {
    LimitExceeded {
        kind: ActionMapLimitKind,
        required: usize,
        limit: usize,
    },
    DuplicateContext(ActionContext),
    UnknownContext(ActionContext),
    DuplicateBinding,
    CostOverflow,
}

impl fmt::Display for ActionMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded {
                kind,
                required,
                limit,
            } => write!(f, "action map {kind:?} requires {required}, limit {limit}"),
            Self::DuplicateContext(context) => write!(f, "duplicate action context {context}"),
            Self::UnknownContext(context) => write!(f, "undeclared action context {context}"),
            Self::DuplicateBinding => f.write_str("duplicate complete action binding"),
            Self::CostOverflow => f.write_str("action map cost overflow"),
        }
    }
}
impl Error for ActionMapError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContextDefinition {
    pub context: ActionContext,
    pub enabled: bool,
}

/// Owned, immutable compiled data. Sharing is available only through an actual definition owner.
#[derive(Debug)]
pub struct ActionMapDefinition {
    data: Arc<CompiledActionMap>,
}

#[derive(Debug)]
struct CompiledActionMap {
    contexts: Box<[ActionContextDefinition]>,
    bindings: Box<[ActionBinding]>,
    by_input: Box<[(InputBinding, usize)]>,
    context_slots: Box<[usize]>,
    cost: ActionMapCompileCost,
}

impl AsRef<ActionMapDefinition> for ActionMapDefinition {
    fn as_ref(&self) -> &ActionMapDefinition {
        self
    }
}

impl ActionMapDefinition {
    #[must_use]
    pub fn contexts(&self) -> &[ActionContextDefinition] {
        &self.data.contexts
    }
    #[must_use]
    pub fn bindings(&self) -> &[ActionBinding] {
        &self.data.bindings
    }
    #[must_use]
    pub fn cost(&self) -> ActionMapCompileCost {
        self.data.cost
    }

    #[must_use]
    pub fn into_bindings(self) -> ActionMapBindings {
        ActionMapBindings {
            definition: self,
            owner: None,
        }
    }

    pub(crate) fn context_index(&self, context: &ActionContext) -> Option<usize> {
        self.data
            .contexts
            .iter()
            .position(|entry| &entry.context == context)
    }

    pub(crate) fn binding_context_index(&self, index: usize) -> Option<usize> {
        self.data.context_slots.get(index).copied()
    }

    pub(crate) fn indices_for_input(
        &self,
        input: InputBinding,
    ) -> impl Iterator<Item = usize> + '_ {
        let start = self.data.by_input.partition_point(|(key, _)| *key < input);
        self.data.by_input[start..]
            .iter()
            .take_while(move |(key, _)| *key == input)
            .map(|(_, index)| *index)
    }
}

/// A cheap shared handle that captures one immutable definition and its actual owner together.
///
/// Source owners keep their definition and original reservation together. Processed owners can
/// project an actual typed acquisition once with `AsRef`; later projection changes cannot change
/// these bindings. The compiled allocation and its sharing operation remain private.
pub struct ActionMapBindings {
    definition: ActionMapDefinition,
    owner: Option<Arc<dyn Any + Send + Sync>>,
}

impl Clone for ActionMapBindings {
    fn clone(&self) -> Self {
        Self {
            definition: ActionMapDefinition {
                data: Arc::clone(&self.definition.data),
            },
            owner: self.owner.clone(),
        }
    }
}

impl fmt::Debug for ActionMapBindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ActionMapBindings")
            .field(self.definition())
            .finish()
    }
}

impl ActionMapBindings {
    #[must_use]
    pub fn from_owner<T>(owner: Arc<T>) -> Self
    where
        T: AsRef<ActionMapDefinition> + Send + Sync + 'static,
    {
        let definition = ActionMapDefinition {
            data: Arc::clone(&owner.as_ref().as_ref().data),
        };
        Self {
            definition,
            owner: Some(owner),
        }
    }

    #[must_use]
    pub fn definition(&self) -> &ActionMapDefinition {
        &self.definition
    }

    #[must_use]
    pub fn shares_owner_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.definition.data, &other.definition.data)
            && match (&self.owner, &other.owner) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }
}

/// Bounded authoring input. Context declarations are always explicit, including `gameplay`.
#[derive(Debug)]
pub struct ActionMapBuilder {
    limits: ActionMapLimits,
    contexts: Vec<ActionContextDefinition>,
    bindings: Vec<ActionBinding>,
    string_bytes: usize,
}

impl ActionMapBuilder {
    #[must_use]
    pub fn new(limits: ActionMapLimits) -> Self {
        Self {
            limits,
            contexts: Vec::new(),
            bindings: Vec::new(),
            string_bytes: 0,
        }
    }

    pub fn declare_context(
        &mut self,
        context: ActionContext,
        enabled: bool,
    ) -> Result<&mut Self, ActionMapError> {
        check_limit(
            ActionMapLimitKind::Contexts,
            self.contexts.len() + 1,
            self.limits.contexts.get().min(MAX_ACTION_CONTEXTS),
        )?;
        self.check_identifier(context.as_str())?;
        if self.contexts.iter().any(|entry| entry.context == context) {
            return Err(ActionMapError::DuplicateContext(context));
        }
        let string_bytes = self
            .string_bytes
            .checked_add(context.as_str().len())
            .ok_or(ActionMapError::CostOverflow)?;
        self.admit(self.contexts.len() + 1, self.bindings.len(), string_bytes)?;
        self.contexts
            .push(ActionContextDefinition { context, enabled });
        self.string_bytes = string_bytes;
        Ok(self)
    }

    pub fn bind(&mut self, binding: ActionBinding) -> Result<&mut Self, ActionMapError> {
        check_limit(
            ActionMapLimitKind::Bindings,
            self.bindings.len() + 1,
            self.limits.bindings.get().min(MAX_ACTION_BINDINGS),
        )?;
        self.check_identifier(binding.action.as_str())?;
        self.check_identifier(binding.context.as_str())?;
        if !self
            .contexts
            .iter()
            .any(|entry| entry.context == binding.context)
        {
            return Err(ActionMapError::UnknownContext(binding.context));
        }
        if self.bindings.contains(&binding) {
            return Err(ActionMapError::DuplicateBinding);
        }
        let fanout = self
            .bindings
            .iter()
            .filter(|entry| entry.input == binding.input)
            .count()
            + 1;
        check_limit(
            ActionMapLimitKind::BindingsPerInput,
            fanout,
            self.limits
                .bindings_per_input
                .get()
                .min(MAX_BINDINGS_PER_INPUT),
        )?;
        let string_bytes = self
            .string_bytes
            .checked_add(binding.action.as_str().len())
            .and_then(|bytes| bytes.checked_add(binding.context.as_str().len()))
            .ok_or(ActionMapError::CostOverflow)?;
        self.admit(self.contexts.len(), self.bindings.len() + 1, string_bytes)?;
        self.bindings.push(binding);
        self.string_bytes = string_bytes;
        Ok(self)
    }

    pub fn compile(self) -> Result<ActionMapDefinition, ActionMapError> {
        let cost = self.admit(self.contexts.len(), self.bindings.len(), self.string_bytes)?;
        let mut by_input = self
            .bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.input, index))
            .collect::<Vec<_>>();
        by_input.sort_unstable();
        let context_slots = self
            .bindings
            .iter()
            .map(|binding| {
                self.contexts
                    .iter()
                    .position(|entry| entry.context == binding.context)
                    .expect("builder admits only declared contexts")
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(ActionMapDefinition {
            data: Arc::new(CompiledActionMap {
                contexts: self.contexts.into_boxed_slice(),
                bindings: self.bindings.into_boxed_slice(),
                by_input: by_input.into_boxed_slice(),
                context_slots,
                cost,
            }),
        })
    }

    fn check_identifier(&self, id: &str) -> Result<(), ActionMapError> {
        check_limit(
            ActionMapLimitKind::IdentifierBytes,
            id.len(),
            self.limits
                .identifier_bytes
                .get()
                .min(MAX_ACTION_IDENTIFIER_BYTES),
        )
    }

    fn admit(
        &self,
        contexts: usize,
        bindings: usize,
        strings: usize,
    ) -> Result<ActionMapCompileCost, ActionMapError> {
        // Output includes owned strings, boxed tables, indices and both shared-owner words.
        let retained = size_of::<CompiledActionMap>()
            .checked_add(size_of::<ActionMapDefinition>() + size_of::<ActionMapBindings>())
            .and_then(|bytes| bytes.checked_add(4 * size_of::<usize>()))
            .and_then(|bytes| {
                contexts
                    .checked_mul(size_of::<ActionContextDefinition>() + 2 * size_of::<usize>())
                    .and_then(|extra| bytes.checked_add(extra))
            })
            .and_then(|bytes| {
                bindings
                    .checked_mul(
                        size_of::<ActionBinding>()
                            + size_of::<(InputBinding, usize)>()
                            + size_of::<usize>()
                            + 4 * size_of::<usize>(),
                    )
                    .and_then(|extra| bytes.checked_add(extra))
            })
            .and_then(|bytes| bytes.checked_add(strings))
            .ok_or(ActionMapError::CostOverflow)?;
        // Geometric builder capacity and shrink-to-box peaks coexist with the final output.
        let work = retained
            .checked_mul(3)
            .ok_or(ActionMapError::CostOverflow)?;
        check_limit(
            ActionMapLimitKind::RetainedBytes,
            retained,
            self.limits.retained_bytes.get(),
        )?;
        check_limit(
            ActionMapLimitKind::WorkBytes,
            work,
            self.limits.work_bytes.get(),
        )?;
        Ok(ActionMapCompileCost { work, retained })
    }
}

pub(crate) fn check_limit(
    kind: ActionMapLimitKind,
    required: usize,
    limit: usize,
) -> Result<(), ActionMapError> {
    if required > limit {
        Err(ActionMapError::LimitExceeded {
            kind,
            required,
            limit,
        })
    } else {
        Ok(())
    }
}
