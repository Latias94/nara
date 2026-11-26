use std::{any::Any, collections::BTreeMap, sync::Arc};

use nara_core::ByteLimit;
use nara_ecs::Resource;

use crate::{
    ActionBinding, ActionContext, ActionMapBindings, ActionMapBuilder, ActionMapDefinition,
    ActionMapError, ActionMapLimitKind, ActionMapLimits, InputBinding, InputSeatId,
    MAX_ACTION_CONTEXTS, definition::check_limit,
};

/// Independently mutable context and mapping state for one seat. It is deliberately not cloneable
/// or serializable; immutable bindings share their actual data owner instead.
pub struct ActionMap {
    bindings: ActionMapBindings,
    enabled: Box<[Option<Arc<()>>]>,
    pub(crate) revision: Arc<()>,
    _state_owner: Option<Arc<dyn Any + Send + Sync>>,
}

/// Maximum independently configured action seats in one World.
pub const MAX_SEAT_ACTION_MAPS: usize = 64;

/// The World's finite owner of independently mutable seat action maps.
///
/// An unconfigured seat has no gameplay mapping. Raw button observations and UI routing remain
/// independent of this policy. The default map occupies [`InputSeatId::PRIMARY`].
#[derive(Debug, Resource)]
pub struct ActionMaps {
    maps: BTreeMap<InputSeatId, ActionMap>,
}

impl Default for ActionMaps {
    fn default() -> Self {
        Self::new(ActionMap::default())
    }
}

impl ActionMaps {
    /// Installs an owned map for the default seat.
    #[must_use]
    pub fn new(primary: ActionMap) -> Self {
        Self {
            maps: BTreeMap::from([(InputSeatId::PRIMARY, primary)]),
        }
    }

    #[must_use]
    pub fn get(&self, seat: InputSeatId) -> Option<&ActionMap> {
        self.maps.get(&seat)
    }

    #[must_use]
    pub fn get_mut(&mut self, seat: InputSeatId) -> Option<&mut ActionMap> {
        self.maps.get_mut(&seat)
    }

    /// Transfers one map into this owner, returning the previous map when replacing a seat.
    ///
    /// Capacity refusal returns the supplied map without changing either installed state or its
    /// retention ticket. Replacing an existing seat requires no additional map slot.
    pub fn insert(
        &mut self,
        seat: InputSeatId,
        map: ActionMap,
    ) -> Result<Option<ActionMap>, ActionMapInstallError> {
        if self.maps.len() == MAX_SEAT_ACTION_MAPS && !self.maps.contains_key(&seat) {
            return Err(ActionMapInstallError { seat, map });
        }
        Ok(self.maps.insert(seat, map))
    }

    /// Returns the original owned state. The next input resolution releases its active actions.
    pub fn remove(&mut self, seat: InputSeatId) -> Option<ActionMap> {
        self.maps.remove(&seat)
    }
}

/// A refused seat installation retaining the original supplied map.
#[derive(Debug)]
pub struct ActionMapInstallError {
    seat: InputSeatId,
    map: ActionMap,
}

impl ActionMapInstallError {
    #[must_use]
    pub const fn seat(&self) -> InputSeatId {
        self.seat
    }

    #[must_use]
    pub fn into_map(self) -> ActionMap {
        self.map
    }
}

impl std::fmt::Display for ActionMapInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "action maps cannot install seat {} beyond the {}-map limit",
            self.seat.get(),
            MAX_SEAT_ACTION_MAPS,
        )
    }
}

impl std::error::Error for ActionMapInstallError {}

impl std::fmt::Debug for ActionMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionMap")
            .field("bindings", &self.bindings)
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
    /// Compiles code-defined bindings for the default `gameplay` context.
    pub fn from_bindings(
        bindings: impl IntoIterator<Item = ActionBinding>,
    ) -> Result<Self, ActionMapError> {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder.declare_context(ActionContext::gameplay(), true)?;
        for binding in bindings {
            builder.bind(binding)?;
        }
        Self::new(
            builder.compile()?.into_bindings(),
            ByteLimit::new(Self::required_state_bytes()).expect("the state allowance is nonzero"),
        )
    }

    /// Fixed allowance for the live map and its configurable contexts.
    #[must_use]
    pub const fn required_state_bytes() -> usize {
        size_of::<Self>() + (3 * MAX_ACTION_CONTEXTS + 2) * size_of::<usize>()
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
        Ok(Self {
            enabled: enabled_contexts(&bindings),
            bindings,
            revision: Arc::new(()),
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
    pub fn replace_bindings(&mut self, bindings: ActionMapBindings) {
        self.enabled = enabled_contexts(&bindings);
        self.bindings = bindings;
        self.revision = Arc::new(());
    }

    pub fn disable_context(&mut self, context: &ActionContext) -> Result<bool, ActionMapError> {
        self.set_context_enabled(context, false)
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
        if self.enabled[slot].is_some() == enabled {
            return Ok(false);
        }
        self.enabled[slot] = enabled.then(|| Arc::new(()));
        Ok(true)
    }

    #[must_use]
    pub fn is_context_enabled(&self, context: &ActionContext) -> bool {
        self.context_token(context).is_some()
    }

    pub(crate) fn context_token(&self, context: &ActionContext) -> Option<&Arc<()>> {
        self.enabled[self.definition().context_index(context)?].as_ref()
    }

    pub(crate) fn binding_context_token(&self, index: usize) -> Option<&Arc<()>> {
        self.enabled[self.definition().binding_context_index(index)?].as_ref()
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
}

fn enabled_contexts(bindings: &ActionMapBindings) -> Box<[Option<Arc<()>>]> {
    bindings
        .definition()
        .contexts()
        .iter()
        .map(|entry| entry.enabled.then(|| Arc::new(())))
        .collect()
}
