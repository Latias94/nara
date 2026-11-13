use std::{
    any::Any,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use nara_app::RuntimeCandidateScope;
use nara_core::ByteLimit;
use nara_ecs::{Resource, World};

use crate::{
    ActionBinding, ActionContext, ActionMapBindings, ActionMapBuilder, ActionMapDefinition,
    ActionMapError, ActionMapLimitKind, ActionMapLimits, InputBinding, InputSeatId,
    MAX_ACTION_CONTEXTS, MAX_INPUT_TIMELINE_ENTRIES, definition::check_limit,
    semantic::ActionMapRevision,
};

/// Independently mutable context and mapping state for one seat. It is deliberately not cloneable
/// or serializable; immutable bindings share their actual data owner instead.
pub struct ActionMap {
    bindings: ActionMapBindings,
    enabled: Box<[bool]>,
    pub(crate) revision: Arc<ActionMapRevision>,
    explicit: bool,
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
    pub(crate) fn pristine() -> Self {
        Self::new(ActionMap::pristine())
    }

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

    /// Returns the original owned state; later staging observes that this seat has no mapping.
    pub fn remove(&mut self, seat: InputSeatId) -> Option<ActionMap> {
        self.maps.remove(&seat)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (InputSeatId, &ActionMap)> {
        self.maps.iter().map(|(seat, map)| (*seat, map))
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
    /// The single finite ingress log can retain distinct snapshots. A fixed maximum-context allowance
    /// remains charged across replacement while old snapshots survive. Ingress endpoints, held
    /// semantic contributions and transient action output have separate ownership and are excluded.
    #[must_use]
    pub const fn required_state_bytes() -> usize {
        let snapshots = MAX_INPUT_TIMELINE_ENTRIES + 8;
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
    pub(crate) fn new(maps: &ActionMaps) -> Self {
        Self {
            pristine: maps
                .get(InputSeatId::PRIMARY)
                .filter(|map| !map.explicit)
                .map(|map| Arc::clone(&map.revision)),
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

/// Finalizes the Host's PRIMARY selection once in the unpublished candidate admission phase.
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
    let maps = world
        .get_resource::<ActionMaps>()
        .ok_or(InputSelectionError::MissingActionMap)?;
    if selection.is_some() {
        let current = maps
            .get(InputSeatId::PRIMARY)
            .ok_or(InputSelectionError::MissingActionMap)?;
        if current.explicit
            || !installation
                .pristine
                .as_ref()
                .is_some_and(|pristine| Arc::ptr_eq(pristine, &current.revision))
        {
            return Err(InputSelectionError::CompetingConfiguration);
        }
    }
    if let Some(map) = selection {
        world
            .resource_mut::<ActionMaps>()
            .insert(InputSeatId::PRIMARY, map)
            .expect("the pristine PRIMARY slot is already installed");
    }
    let mut installation = world.resource_mut::<InputInstallation>();
    installation.finalized = true;
    installation.pristine = None;
    Ok(())
}
