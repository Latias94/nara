use std::{
    any::Any,
    sync::{Arc, OnceLock},
};

use nara_app::RuntimeCandidateScope;
use nara_core::ByteLimit;
use nara_ecs::{Resource, World};

use crate::{
    ActionBinding, ActionContext, ActionMapBindings, ActionMapBuilder, ActionMapDefinition,
    ActionMapError, ActionMapLimitKind, ActionMapLimits, InputBinding, MAX_ACTION_CONTEXTS,
    MAX_INPUT_TIMELINE_ENTRIES, MAX_RETAINED_BUTTON_TRANSITIONS, definition::check_limit,
    semantic::ActionMapRevision,
};

/// Independently mutable World-local context and mapping state. It is deliberately not cloneable
/// or serializable; immutable bindings share their actual data owner instead.
#[derive(Resource)]
pub struct ActionMap {
    bindings: ActionMapBindings,
    enabled: Box<[bool]>,
    pub(crate) revision: Arc<ActionMapRevision>,
    explicit: bool,
    _state_owner: Option<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ActionMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionMap")
            .field("bindings", &self.bindings)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl Default for ActionMap {
    fn default() -> Self {
        let bindings = ActionMapBuilder::new(ActionMapLimits::default())
            .compile()
            .expect("empty definition fits default limits")
            .into_bindings();
        Self::new(
            bindings,
            ByteLimit::new(Self::required_state_bytes()).unwrap(),
        )
        .expect("exact state allowance")
    }
}

impl ActionMap {
    /// Fixed logical allowance for context state and all retained revision/staging snapshots.
    ///
    /// The three finite edge logs can retain distinct snapshots. A fixed maximum-context allowance
    /// remains charged across replacement while old snapshots survive. Ingress endpoints, held
    /// semantic contributions and transient action output have separate ownership and are excluded.
    #[must_use]
    pub const fn required_state_bytes() -> usize {
        let snapshots = MAX_INPUT_TIMELINE_ENTRIES + 2 * MAX_RETAINED_BUTTON_TRANSITIONS + 8;
        snapshots * (128 + MAX_ACTION_CONTEXTS * (4 * size_of::<usize>()))
            + size_of::<Self>()
            + MAX_ACTION_CONTEXTS
    }

    pub fn new(
        bindings: ActionMapBindings,
        state_limit: ByteLimit,
    ) -> Result<Self, ActionMapError> {
        Self::create(bindings, state_limit, None)
    }

    fn create(
        bindings: ActionMapBindings,
        state_limit: ByteLimit,
        state_owner: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Result<Self, ActionMapError> {
        check_limit(
            ActionMapLimitKind::RuntimeStateBytes,
            Self::required_state_bytes(),
            state_limit.get(),
        )?;
        let enabled = bindings
            .definition()
            .contexts()
            .iter()
            .map(|entry| entry.enabled)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let revision = Arc::new(ActionMapRevision::new_with_retention_owner(
            enabled.len(),
            state_owner.clone(),
        ));
        Ok(Self {
            bindings,
            enabled,
            revision,
            explicit: true,
            _state_owner: state_owner,
        })
    }

    /// Consumes newly admitted state retention. Unlike bindings, the state is never shared or
    /// copied from another World; Hosts keep this original reservation until the state is dropped.
    pub fn new_with_retention_owner<T: Any + Send + Sync>(
        bindings: ActionMapBindings,
        state_limit: ByteLimit,
        owner: Arc<T>,
    ) -> Result<Self, ActionMapError> {
        Self::create(bindings, state_limit, Some(owner))
    }

    #[must_use]
    pub fn definition(&self) -> &ActionMapDefinition {
        self.bindings.definition()
    }

    #[must_use]
    pub fn bindings_handle(&self) -> ActionMapBindings {
        self.bindings.clone()
    }

    /// Installs an already compiled definition and starts a new mapping lifetime, including when
    /// the same bindings return. Candidate compilation can fail without touching this state.
    /// The fixed maximum-context allowance covers replacement and all retained old snapshots.
    pub fn replace_bindings(&mut self, bindings: ActionMapBindings) {
        let enabled = bindings
            .definition()
            .contexts()
            .iter()
            .map(|entry| entry.enabled)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let revision = Arc::new(ActionMapRevision::new_with_retention_owner(
            enabled.len(),
            self._state_owner.clone(),
        ));
        self.bindings = bindings;
        self.enabled = enabled;
        self.revision = revision;
        self.explicit = true;
    }

    pub fn disable_context(&mut self, context: ActionContext) -> Result<bool, ActionMapError> {
        self.set_context_enabled(&context, false)
    }

    pub fn enable_context(&mut self, context: &ActionContext) -> Result<bool, ActionMapError> {
        self.set_context_enabled(context, true)
    }

    pub fn set_context_enabled(
        &mut self,
        context: &ActionContext,
        enabled: bool,
    ) -> Result<bool, ActionMapError> {
        let slot = self
            .definition()
            .context_index(context)
            .ok_or_else(|| ActionMapError::UnknownContext(context.clone()))?;
        self.explicit = true;
        if self.enabled[slot] == enabled {
            return Ok(false);
        }
        ActionMapRevision::context_changed(&mut self.revision, slot);
        self.enabled[slot] = enabled;
        Ok(true)
    }

    #[must_use]
    pub fn is_context_enabled(&self, context: &ActionContext) -> bool {
        self.definition()
            .context_index(context)
            .is_some_and(|slot| self.enabled[slot])
    }

    #[must_use]
    pub fn bindings(&self) -> &[ActionBinding] {
        self.definition().bindings()
    }

    pub fn binding_indices_for_input(
        &self,
        input: InputBinding,
    ) -> impl Iterator<Item = usize> + '_ {
        self.definition().indices_for_input(input)
    }

    #[must_use]
    pub fn binding_at(&self, index: usize) -> Option<&ActionBinding> {
        self.bindings().get(index)
    }

    pub(crate) fn pristine() -> Self {
        Self {
            explicit: false,
            ..Self::default()
        }
    }
}

#[derive(Resource)]
pub(crate) struct InputInstallation {
    pristine: Option<Arc<ActionMapRevision>>,
    finalized: bool,
}

impl InputInstallation {
    pub(crate) fn new(map: &ActionMap) -> Self {
        Self {
            pristine: (!map.explicit).then(|| Arc::clone(&map.revision)),
            finalized: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSelectionError {
    MissingInputOwner,
    MissingActionMap,
    AlreadyFinalized,
    CompetingConfiguration,
}

impl std::fmt::Display for InputSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "input selection rejected: {self:?}")
    }
}
impl std::error::Error for InputSelectionError {}

/// Finalizes the Host's global selection once in the unpublished candidate admission phase.
/// `None` preserves code-first configuration. The scope type prevents post-Startup installation.
pub fn finalize_input_selection(
    scope: &mut RuntimeCandidateScope<'_>,
    selection: Option<ActionMap>,
) -> Result<(), InputSelectionError> {
    let result = Arc::new(OnceLock::new());
    let output = Arc::clone(&result);
    scope.apply_command(move |world: &mut World| {
        let value = receive_selection(world, selection);
        output
            .set(value)
            .expect("immediate selection command runs once");
    });
    result
        .get()
        .copied()
        .expect("candidate commands apply synchronously")
}

fn receive_selection(
    world: &mut World,
    selection: Option<ActionMap>,
) -> Result<(), InputSelectionError> {
    let installation = world
        .get_resource::<InputInstallation>()
        .ok_or(InputSelectionError::MissingInputOwner)?;
    if installation.finalized {
        return Err(InputSelectionError::AlreadyFinalized);
    }
    let current = world
        .get_resource::<ActionMap>()
        .ok_or(InputSelectionError::MissingActionMap)?;
    if selection.is_some()
        && (current.explicit
            || !installation
                .pristine
                .as_ref()
                .is_some_and(|pristine| Arc::ptr_eq(pristine, &current.revision)))
    {
        return Err(InputSelectionError::CompetingConfiguration);
    }
    if let Some(map) = selection {
        world.insert_resource(map);
    }
    let mut installation = world.resource_mut::<InputInstallation>();
    installation.finalized = true;
    installation.pristine = None;
    Ok(())
}
