use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_ecs::{Entity, Mut, Resource, World};
use nara_hierarchy::{Children, HierarchyOrder};
use nara_input::{
    InputIngressError, InputPresentationEpoch, InputSeatId, InputSurfaceId, InputTimeline,
    InputWindowId,
};
use nara_scene::{PrefabProjectionPath, SceneEntitySource};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiDispatchBatch, UiDispatchError,
    UiEnvironment, UiEnvironmentError, UiFrame, UiFrameJournal, UiInputEvent, UiInteractionSession,
    UiJournalError, UiKey, UiKeyError, UiLimits, UiLocalKey, UiOpaqueId, UiPersistentIdentity,
    UiPresentationReceipt, UiProjectionPath, UiProjectionStep, UiRootKey, UiRouteStamp, UiSeatId,
    UiSemanticSnapshot, UiSurfaceId,
};
use nara_ui_widgets::{
    UiButton as KernelButton, UiButtonState, UiButtonVisualState, UiIntentBatch,
};
use thiserror::Error;

use crate::{UiButton, UiNode, UiRoot, UiRuntimeKey, UiSemantics, UiText, UiTheme, UiVisual};

const MAX_ADAPTER_DIAGNOSTICS: usize = 64;
const MAX_DIAGNOSTIC_BYTES: usize = 1_024;
const MAX_PROJECTION_DEPTH: usize = 64;
const MAX_PROJECTED_NODES: usize = 4_096;

/// Host-owned metrics and input-route binding for one stable UI surface lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceDescriptor {
    surface_id: UiSurfaceId,
    input_window: InputWindowId,
    input_seat: InputSeatId,
    logical_width: f32,
    logical_height: f32,
    scale_factor: f32,
    route_revision: u64,
    presentation_revision: u64,
    font_revision: u64,
}

impl UiSurfaceDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        surface_id: UiSurfaceId,
        input_window: InputWindowId,
        input_seat: InputSeatId,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
        route_revision: u64,
        presentation_revision: u64,
        font_revision: u64,
    ) -> Result<Self, UiEnvironmentError> {
        let descriptor = Self {
            surface_id,
            input_window,
            input_seat,
            logical_width,
            logical_height,
            scale_factor,
            route_revision,
            presentation_revision,
            font_revision,
        };
        descriptor.environment(1)?;
        Ok(descriptor)
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn input_window(self) -> InputWindowId {
        self.input_window
    }

    #[must_use]
    pub const fn input_seat(self) -> InputSeatId {
        self.input_seat
    }

    #[must_use]
    pub const fn logical_size(self) -> (f32, f32) {
        (self.logical_width, self.logical_height)
    }

    #[must_use]
    pub const fn scale_factor(self) -> f32 {
        self.scale_factor
    }

    #[must_use]
    pub const fn route_revision(self) -> u64 {
        self.route_revision
    }

    #[must_use]
    pub const fn presentation_revision(self) -> u64 {
        self.presentation_revision
    }

    #[must_use]
    pub const fn font_revision(self) -> u64 {
        self.font_revision
    }

    fn environment(self, theme_revision: u64) -> Result<UiEnvironment, UiEnvironmentError> {
        UiEnvironment::new(
            self.surface_id,
            self.logical_width,
            self.logical_height,
            self.scale_factor,
            self.route_revision,
            self.presentation_revision,
            theme_revision,
            self.font_revision,
        )
    }
}

/// One bounded adapter diagnostic without runtime entity identity leakage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAdapterDiagnostic {
    code: &'static str,
    message: String,
}

impl UiAdapterDiagnostic {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Current bounded projection diagnostics from the most recent compile pass.
#[derive(Debug, Default, Resource)]
pub struct UiAdapterDiagnostics {
    entries: Vec<UiAdapterDiagnostic>,
}

impl UiAdapterDiagnostics {
    #[must_use]
    pub fn as_slice(&self) -> &[UiAdapterDiagnostic] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn replace(&mut self, entries: Vec<UiAdapterDiagnostic>) {
        self.entries = entries;
    }
}

#[derive(Debug, Error)]
pub enum UiAdapterError {
    #[error("UI runtime adapter is not installed")]
    RuntimeUnavailable,
    #[error("UI surface {surface} was retired and cannot be reused")]
    RetiredSurface { surface: u64 },
    #[error("UI surface {surface} is unknown")]
    UnknownSurface { surface: u64 },
    #[error("UI surface limit was exceeded")]
    SurfaceLimitExceeded,
    #[error("the submitted frame is not the adapter's latest frame for surface {surface}")]
    FrameNotLatest { surface: u64 },
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
    #[error(transparent)]
    Journal(#[from] UiJournalError),
    #[error(transparent)]
    Input(#[from] InputIngressError),
    #[error(transparent)]
    Dispatch(#[from] UiDispatchError),
    #[error("UI widget interaction failed: {0}")]
    Widget(String),
}

#[derive(Debug, Error)]
enum ProjectionError {
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Widget(#[from] nara_ui_widgets::UiWidgetError),
}

struct RuntimeButton {
    description: KernelButton,
    state: UiButtonState,
}

#[derive(Resource)]
pub(crate) struct UiRuntimeState {
    compiler: UiCompiler,
    journal: UiFrameJournal,
    interaction: UiInteractionSession,
    surfaces: BTreeMap<UiSurfaceId, UiSurfaceDescriptor>,
    retired_surfaces: BTreeSet<UiSurfaceId>,
    latest_frames: BTreeMap<UiSurfaceId, Arc<UiFrame>>,
    buttons: BTreeMap<UiKey, RuntimeButton>,
    next_source_revision: u64,
}

#[derive(Resource)]
pub(crate) struct UiDefaultTheme(pub(crate) UiTheme);

impl UiRuntimeState {
    pub(crate) fn new() -> Result<Self, String> {
        let limits = UiLimits::default();
        let font = bundled_font_face().map_err(|error| error.to_string())?;
        let compiler = UiCompiler::new(font, limits).map_err(|error| error.to_string())?;
        let journal = UiFrameJournal::with_limits(limits).map_err(|error| error.to_string())?;
        let interaction =
            UiInteractionSession::with_limits(limits).map_err(|error| error.to_string())?;
        Ok(Self {
            compiler,
            journal,
            interaction,
            surfaces: BTreeMap::new(),
            retired_surfaces: BTreeSet::new(),
            latest_frames: BTreeMap::new(),
            buttons: BTreeMap::new(),
            next_source_revision: 1,
        })
    }

    fn rebuild(&mut self, world: &World) -> Vec<UiAdapterDiagnostic> {
        let mut diagnostics = Vec::new();
        if nara_hierarchy::__private::completed_topology_generation(world).is_none() {
            push_diagnostic(
                &mut diagnostics,
                "hierarchy-incomplete",
                "UI compilation requires a completed structural hierarchy generation",
            );
            return diagnostics;
        }
        let Some(order) = world.get_resource::<HierarchyOrder>() else {
            push_diagnostic(
                &mut diagnostics,
                "hierarchy-order-missing",
                "UI compilation requires the hierarchy order projection",
            );
            return diagnostics;
        };
        let ordered_entities = order.as_slice().to_vec();
        let theme = select_theme(world, &ordered_entities, &mut diagnostics);
        let source_revision = self.next_source_revision;
        let Some(next_revision) = source_revision.checked_add(1) else {
            push_diagnostic(
                &mut diagnostics,
                "source-revision-exhausted",
                "UI source revision space was exhausted",
            );
            return diagnostics;
        };

        let mut builder = UiAuthoringSnapshotBuilder::new(source_revision);
        let mut next_buttons = BTreeMap::new();
        let mut projected_nodes = 0usize;
        for entity in ordered_entities.iter().copied() {
            let Some(root) = world.get::<UiRoot>(entity).copied() else {
                continue;
            };
            match self.project_root(
                world,
                entity,
                root,
                &mut next_buttons,
                &mut projected_nodes,
                &mut diagnostics,
            ) {
                Ok(Some(root)) => builder = builder.with_root(root),
                Ok(None) => {}
                Err(error) => push_diagnostic(
                    &mut diagnostics,
                    "root-projection-rejected",
                    &error.to_string(),
                ),
            }
        }

        let snapshot = match builder.build() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                push_diagnostic(&mut diagnostics, "snapshot-rejected", &error.to_string());
                return diagnostics;
            }
        };

        let mut compiled = BTreeMap::new();
        for (surface_id, descriptor) in &self.surfaces {
            let environment = match descriptor.environment(theme.revision().get()) {
                Ok(environment) => environment,
                Err(error) => {
                    push_diagnostic(
                        &mut diagnostics,
                        "surface-environment-rejected",
                        &error.to_string(),
                    );
                    continue;
                }
            };
            match self
                .compiler
                .compile(&snapshot, environment, theme.kernel())
            {
                Ok(frame) => {
                    compiled.insert(*surface_id, frame);
                }
                Err(error) => push_compile_diagnostic(&mut diagnostics, &error),
            }
        }
        self.latest_frames.extend(compiled);
        self.buttons = next_buttons;
        self.next_source_revision = next_revision;
        diagnostics
    }

    #[allow(clippy::too_many_arguments)]
    fn project_root(
        &self,
        world: &World,
        entity: Entity,
        root: UiRoot,
        next_buttons: &mut BTreeMap<UiKey, RuntimeButton>,
        projected_nodes: &mut usize,
        diagnostics: &mut Vec<UiAdapterDiagnostic>,
    ) -> Result<Option<nara_ui_kernel::UiRoot>, ProjectionError> {
        let Some(node) = world.get::<UiNode>(entity).copied() else {
            push_diagnostic(
                diagnostics,
                "root-node-missing",
                "Every UiRoot entity must also carry UiNode authoring data",
            );
            return Ok(None);
        };
        let Some(root_key) = project_root_key(world, entity)? else {
            push_diagnostic(
                diagnostics,
                "root-key-missing",
                "A runtime UiRoot requires UiRuntimeKey; a persistent UiRoot requires SceneEntitySource",
            );
            return Ok(None);
        };
        *projected_nodes = projected_nodes.saturating_add(1);
        if *projected_nodes > MAX_PROJECTED_NODES {
            push_diagnostic(
                diagnostics,
                "node-limit-exceeded",
                "UI projection exceeded the adapter node ceiling",
            );
            return Ok(None);
        }

        if world.get::<UiButton>(entity).is_some() {
            push_diagnostic(
                diagnostics,
                "button-root-rejected",
                "A controlled UiButton must be a root-scoped descendant, not the UiRoot entity",
            );
            return Ok(None);
        }
        let mut projected =
            nara_ui_kernel::UiRoot::new(root_key.clone(), root.layer(), node.style());
        if let Some(visual) = world.get::<UiVisual>(entity) {
            projected = projected.with_visual(visual.kernel());
        }
        if let Some(text) = world.get::<UiText>(entity) {
            projected = projected.with_text(text.kernel());
        }
        if let Some(semantics) = world.get::<UiSemantics>(entity) {
            projected = projected.with_semantics(semantics.kernel());
        }
        let children = self.project_children(
            world,
            entity,
            &root_key,
            next_buttons,
            projected_nodes,
            diagnostics,
            1,
        )?;
        Ok(Some(projected.with_children(children)))
    }

    #[allow(clippy::too_many_arguments)]
    fn project_children(
        &self,
        world: &World,
        parent: Entity,
        root_key: &UiRootKey,
        next_buttons: &mut BTreeMap<UiKey, RuntimeButton>,
        projected_nodes: &mut usize,
        diagnostics: &mut Vec<UiAdapterDiagnostic>,
        depth: usize,
    ) -> Result<Vec<nara_ui_kernel::UiNode>, ProjectionError> {
        if depth > MAX_PROJECTION_DEPTH {
            push_diagnostic(
                diagnostics,
                "projection-depth-exceeded",
                "UI projection exceeded the adapter depth ceiling",
            );
            return Ok(Vec::new());
        }
        let Some(children) = world.get::<Children>(parent) else {
            return Ok(Vec::new());
        };
        let mut projected = Vec::new();
        for child in children.iter() {
            if world.get::<UiRoot>(child).is_some() {
                continue;
            }
            let Some(node) = world.get::<UiNode>(child).copied() else {
                continue;
            };
            *projected_nodes = projected_nodes.saturating_add(1);
            if *projected_nodes > MAX_PROJECTED_NODES {
                push_diagnostic(
                    diagnostics,
                    "node-limit-exceeded",
                    "UI projection exceeded the adapter node ceiling",
                );
                break;
            }
            let local_key = project_local_key(world, child)?;
            let descendants = self.project_children(
                world,
                child,
                root_key,
                next_buttons,
                projected_nodes,
                diagnostics,
                depth + 1,
            )?;
            let authored = if let Some(button) = world.get::<UiButton>(child) {
                let Some(local_key) = local_key else {
                    push_diagnostic(
                        diagnostics,
                        "button-key-missing",
                        "A runtime UiButton requires UiRuntimeKey; a persistent UiButton requires SceneEntitySource",
                    );
                    continue;
                };
                let key = UiKey::scoped(root_key.clone(), local_key)?;
                let Some(visual) = world.get::<UiVisual>(child) else {
                    push_diagnostic(
                        diagnostics,
                        "button-visual-missing",
                        "A controlled UiButton requires an explicit UiVisual component",
                    );
                    continue;
                };
                if world.get::<UiText>(child).is_some() || world.get::<UiSemantics>(child).is_some()
                {
                    push_diagnostic(
                        diagnostics,
                        "button-authoring-conflict",
                        "UiButton owns its label and semantics; do not add UiText or UiSemantics to the same entity",
                    );
                    continue;
                }
                let description =
                    KernelButton::new(key.clone(), button.label(), button.action().clone())?
                        .with_enabled(button.enabled());
                let state = self
                    .buttons
                    .get(&key)
                    .map_or_else(UiButtonState::default, |runtime| runtime.state);
                let visual = button_visual(visual, state.visual_state(&description));
                let authored = description
                    .authored_node(root_key, node.style(), visual, button.logical_text_size())?
                    .with_children(descendants);
                next_buttons.insert(key, RuntimeButton { description, state });
                authored
            } else {
                let mut authored = nara_ui_kernel::UiNode::new(node.style());
                if let Some(local_key) = local_key {
                    authored = authored.with_key(local_key);
                }
                if let Some(visual) = world.get::<UiVisual>(child) {
                    authored = authored.with_visual(visual.kernel());
                }
                if let Some(text) = world.get::<UiText>(child) {
                    authored = authored.with_text(text.kernel());
                }
                if let Some(semantics) = world.get::<UiSemantics>(child) {
                    authored = authored.with_semantics(semantics.kernel());
                }
                authored.with_children(descendants)
            };
            projected.push(authored);
        }
        Ok(projected)
    }

    pub(crate) fn dispatch(
        &mut self,
        route: UiRouteStamp,
        input: UiInputEvent,
    ) -> Result<(UiDispatchBatch, UiIntentBatch), UiAdapterError> {
        let dispatch = self.interaction.dispatch(&self.journal, route, input)?;
        let mut keys = BTreeSet::new();
        keys.extend(dispatch.events().iter().map(|event| event.target().clone()));
        let mut intents = UiIntentBatch::default();
        for key in keys {
            if let Some(runtime) = self.buttons.get_mut(&key) {
                runtime
                    .state
                    .reduce_batch(&runtime.description, &dispatch, &mut intents)
                    .map_err(|error| UiAdapterError::Widget(error.to_string()))?;
            }
        }
        Ok((dispatch, intents))
    }
}

pub(crate) fn compile_ui_frames(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let diagnostics = state.rebuild(world);
        world
            .resource_mut::<UiAdapterDiagnostics>()
            .replace(diagnostics);
    });
}

/// Installs or updates one active Host-owned surface without reusing retired identities.
pub fn install_ui_surface(
    world: &mut World,
    descriptor: UiSurfaceDescriptor,
) -> Result<(), UiAdapterError> {
    let theme_revision = world
        .get_resource::<UiDefaultTheme>()
        .map_or_else(UiTheme::default, |theme| theme.0)
        .revision()
        .get();
    descriptor.environment(theme_revision)?;
    let Some(mut state) = world.get_resource_mut::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    let surface_id = descriptor.surface_id();
    if state.retired_surfaces.contains(&surface_id) {
        return Err(UiAdapterError::RetiredSurface {
            surface: surface_id.get(),
        });
    }
    if !state.surfaces.contains_key(&surface_id) && state.surfaces.len() == 16 {
        return Err(UiAdapterError::SurfaceLimitExceeded);
    }
    state.surfaces.insert(surface_id, descriptor);
    Ok(())
}

/// Retires one surface and its retained presented-frame history.
pub fn retire_ui_surface(
    world: &mut World,
    surface_id: UiSurfaceId,
) -> Result<bool, UiAdapterError> {
    let Some(mut state) = world.get_resource_mut::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    let removed = state.surfaces.remove(&surface_id).is_some();
    state.latest_frames.remove(&surface_id);
    state.journal.retire_surface(surface_id);
    if removed {
        state.retired_surfaces.insert(surface_id);
    }
    Ok(removed)
}

/// Returns the latest compiled immutable frame for one surface.
pub fn latest_ui_frame(
    world: &World,
    surface_id: UiSurfaceId,
) -> Result<Option<Arc<UiFrame>>, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    Ok(state.latest_frames.get(&surface_id).map(Arc::clone))
}

/// Commits backend acceptance of the exact current frame and publishes its input route.
pub fn commit_ui_presentation(
    world: &mut World,
    frame: Arc<UiFrame>,
    presentation_epoch: u64,
) -> Result<UiPresentationReceipt, UiAdapterError> {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let surface_id = frame.environment().surface_id();
        let descriptor =
            state
                .surfaces
                .get(&surface_id)
                .copied()
                .ok_or(UiAdapterError::UnknownSurface {
                    surface: surface_id.get(),
                })?;
        let Some(latest) = state.latest_frames.get(&surface_id) else {
            return Err(UiAdapterError::FrameNotLatest {
                surface: surface_id.get(),
            });
        };
        if !Arc::ptr_eq(latest, &frame) {
            return Err(UiAdapterError::FrameNotLatest {
                surface: surface_id.get(),
            });
        }
        let mut journal = state.journal.clone();
        let receipt = journal.submit(frame, presentation_epoch)?;
        let epoch = InputPresentationEpoch::new(presentation_epoch)
            .ok_or(UiJournalError::ZeroPresentationEpoch)?;
        world
            .resource_mut::<InputTimeline>()
            .publish_presented_route(
                descriptor.input_window(),
                InputSurfaceId::new(surface_id.get()),
                descriptor.input_seat(),
                epoch,
            )?;
        state.journal = journal;
        Ok(receipt)
    })
}

/// Projects accessibility semantics from the latest submitted frame and interaction state.
pub fn ui_semantic_snapshot(
    world: &World,
    surface_id: UiSurfaceId,
    presentation_epoch: u64,
    seat: UiSeatId,
) -> Result<UiSemanticSnapshot, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    state
        .interaction
        .semantic_snapshot(
            &state.journal,
            UiRouteStamp::new(surface_id, presentation_epoch),
            seat,
        )
        .map_err(UiAdapterError::from)
}

fn select_theme(
    world: &World,
    ordered_entities: &[Entity],
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
) -> UiTheme {
    let fallback = world
        .get_resource::<UiDefaultTheme>()
        .map_or_else(UiTheme::default, |theme| theme.0);
    let mut selected: Option<UiTheme> = None;
    for entity in ordered_entities.iter().copied() {
        let Some(theme) = world.get::<UiTheme>(entity).copied() else {
            continue;
        };
        match selected {
            None => selected = Some(theme),
            Some(current) if current == theme => {}
            Some(_) => push_diagnostic(
                diagnostics,
                "theme-conflict",
                "All persistent UiTheme components in one runtime projection must be identical",
            ),
        }
    }
    selected.unwrap_or(fallback)
}

fn project_root_key(world: &World, entity: Entity) -> Result<Option<UiRootKey>, UiKeyError> {
    if let Some(source) = world.get::<SceneEntitySource>(entity) {
        return persistent_identity(source)
            .and_then(UiRootKey::persistent)
            .map(Some);
    }
    world
        .get::<UiRuntimeKey>(entity)
        .map(|key| UiRootKey::runtime(key.domain(), key.local()))
        .transpose()
}

fn project_local_key(world: &World, entity: Entity) -> Result<Option<UiLocalKey>, UiKeyError> {
    if let Some(source) = world.get::<SceneEntitySource>(entity) {
        return persistent_identity(source)
            .and_then(UiLocalKey::persistent)
            .map(Some);
    }
    world
        .get::<UiRuntimeKey>(entity)
        .map(|key| UiLocalKey::runtime(key.domain(), key.local()))
        .transpose()
}

fn persistent_identity(source: &SceneEntitySource) -> Result<UiPersistentIdentity, UiKeyError> {
    let source_entity = UiOpaqueId::new(source.source_entity_id.as_str())?;
    match source.projection_path.as_ref() {
        None => Ok(UiPersistentIdentity::local(source_entity)),
        Some(path) => project_prefab_path(path)
            .map(|projection| UiPersistentIdentity::projected(source_entity, projection)),
    }
}

fn project_prefab_path(path: &PrefabProjectionPath) -> Result<UiProjectionPath, UiKeyError> {
    let mut projected = UiProjectionPath::new(UiOpaqueId::new(path.root_anchor().as_str())?);
    for step in path.source_chain() {
        projected = projected.with_step(UiProjectionStep::new(
            UiOpaqueId::new(step.source_document().to_string())?,
            UiOpaqueId::new(step.source_entity().as_str())?,
        ))?;
    }
    Ok(projected)
}

fn button_visual(visual: &UiVisual, state: UiButtonVisualState) -> nara_ui_kernel::UiVisual {
    use nara_ui_kernel::{UiBorder, UiBrush, UiColorRole};

    match state {
        UiButtonVisualState::Idle => visual.kernel(),
        UiButtonVisualState::Hovered => nara_ui_kernel::UiVisual::role(UiColorRole::Accent),
        UiButtonVisualState::Pressed => nara_ui_kernel::UiVisual::role(UiColorRole::Positive),
        UiButtonVisualState::Focused => UiBorder::new(2.0, UiBrush::Role(UiColorRole::Focus))
            .map_or_else(
                |_| visual.kernel(),
                |border| visual.kernel().with_border(border),
            ),
        UiButtonVisualState::Disabled => nara_ui_kernel::UiVisual::role(UiColorRole::Disabled),
    }
}

fn push_compile_diagnostic(diagnostics: &mut Vec<UiAdapterDiagnostic>, error: &UiCompileError) {
    push_diagnostic(diagnostics, "compile-rejected", &error.to_string());
}

fn push_diagnostic(diagnostics: &mut Vec<UiAdapterDiagnostic>, code: &'static str, message: &str) {
    if diagnostics.len() == MAX_ADAPTER_DIAGNOSTICS {
        return;
    }
    diagnostics.push(UiAdapterDiagnostic {
        code,
        message: truncate_message(message),
    });
}

fn truncate_message(message: &str) -> String {
    if message.len() <= MAX_DIAGNOSTIC_BYTES {
        return message.to_string();
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}
