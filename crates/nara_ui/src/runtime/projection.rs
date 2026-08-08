use std::collections::{BTreeMap, BTreeSet};

use nara_ecs::{Entity, World};
use nara_hierarchy::Children;
use nara_scene::{PrefabProjectionPath, SceneEntitySource};
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiKey, UiKeyError, UiLocalKey, UiOpaqueId,
    UiPersistentIdentity, UiProjectionPath, UiProjectionStep, UiRootDiagnostic, UiRootKey,
};
use nara_ui_widgets::{
    UiButton as KernelButton, UiButtonState, UiButtonVisualState, UiWidgetError,
};
use thiserror::Error;

use crate::{
    UiButton, UiButtonVisuals, UiNode, UiRoot, UiRuntimeKey, UiSemantics, UiText, UiTheme, UiVisual,
};

use super::{UiAdapterDiagnostic, push_diagnostic};

const MAX_PROJECTION_DEPTH: usize = 64;
const MAX_PROJECTED_NODES: usize = 4_096;

#[derive(Debug, Error)]
enum ProjectionError {
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Widget(#[from] UiWidgetError),
    #[error("UI root projection exceeded the adapter depth ceiling {maximum}")]
    DepthLimitExceeded { maximum: usize },
    #[error("UI root projection exceeded its remaining node budget {maximum}")]
    NodeLimitExceeded { maximum: usize },
    #[error("a controlled UI button is missing its stable runtime or persistent key")]
    ButtonKeyMissing,
    #[error("a controlled UI button is missing its explicit visual authoring")]
    ButtonVisualMissing,
    #[error("a controlled UI button conflicts with separately authored text or semantics")]
    ButtonAuthoringConflict,
    #[error("a controlled UI button cannot be authored on the UiRoot entity")]
    RootButtonUnsupported,
}

/// One surface-specific authored snapshot and the behavior descriptions compiled with it.
pub(super) struct UiProjectedAuthoring {
    snapshot: UiAuthoringSnapshot,
    buttons: BTreeMap<UiKey, KernelButton>,
}

impl UiProjectedAuthoring {
    pub(super) fn into_parts(self) -> (UiAuthoringSnapshot, BTreeMap<UiKey, KernelButton>) {
        (self.snapshot, self.buttons)
    }
}

pub(super) fn project_authoring(
    world: &World,
    ordered_entities: &[Entity],
    source_revision: u64,
    reducer_states: &BTreeMap<UiKey, UiButtonState>,
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
) -> Option<UiProjectedAuthoring> {
    let mut roots = Vec::new();
    let mut root_keys = BTreeSet::new();
    for entity in ordered_entities.iter().copied() {
        let Some(root) = world.get::<UiRoot>(entity).copied() else {
            continue;
        };
        let root_key = match project_root_key(world, entity) {
            Ok(Some(key)) => key,
            Ok(None) => {
                push_diagnostic(
                    diagnostics,
                    "root-key-missing",
                    "A runtime UiRoot requires UiRuntimeKey; a persistent UiRoot requires SceneEntitySource",
                );
                return None;
            }
            Err(error) => {
                push_diagnostic(diagnostics, "root-key-rejected", &error.to_string());
                return None;
            }
        };
        if !root_keys.insert(root_key.clone()) {
            push_diagnostic(
                diagnostics,
                "root-key-duplicate",
                "Every projected UiRoot must have a unique stable root key",
            );
            return None;
        }
        roots.push((entity, root, root_key));
    }

    let mut preflighted_roots = Vec::with_capacity(roots.len());
    let mut candidate_nodes = 0usize;
    for (entity, root, root_key) in roots {
        let remaining_nodes = MAX_PROJECTED_NODES.saturating_sub(candidate_nodes);
        let root_nodes = match count_projected_root_nodes(world, entity, remaining_nodes) {
            Ok(count) => count,
            Err(error) => {
                push_diagnostic(
                    diagnostics,
                    "snapshot-capacity-rejected",
                    &error.to_string(),
                );
                return None;
            }
        };
        candidate_nodes = candidate_nodes.saturating_add(root_nodes);
        preflighted_roots.push((entity, root, root_key, root_nodes));
    }

    let mut builder = UiAuthoringSnapshotBuilder::new(source_revision);
    let mut buttons = BTreeMap::new();
    for (entity, root, root_key, source_node_count) in preflighted_roots {
        let Some(node) = world.get::<UiNode>(entity).copied() else {
            let message = "Every UiRoot entity must also carry UiNode authoring data";
            push_diagnostic(diagnostics, "root-node-missing", message);
            builder = nara_ui_kernel::__private::with_rejected_root(
                builder,
                root_key,
                root.layer(),
                0,
                UiRootDiagnostic::AuthoringRejected {
                    message: message.to_owned(),
                },
            );
            continue;
        };
        let mut root_buttons = BTreeMap::new();
        let mut root_nodes = 0usize;
        match project_root(
            world,
            entity,
            root,
            node,
            &root_key,
            reducer_states,
            &mut root_buttons,
            &mut root_nodes,
            source_node_count,
            diagnostics,
        ) {
            Ok(root) => {
                if root_nodes != source_node_count {
                    push_diagnostic(
                        diagnostics,
                        "root-node-count-mismatch",
                        "UI root projection did not consume its preflighted node count",
                    );
                    return None;
                }
                buttons.extend(root_buttons);
                builder = builder.with_root(root);
            }
            Err(error) => {
                let message = error.to_string();
                push_diagnostic(diagnostics, "root-projection-rejected", &message);
                if matches!(error, ProjectionError::NodeLimitExceeded { .. }) {
                    return None;
                }
                builder = nara_ui_kernel::__private::with_rejected_root(
                    builder,
                    root_key,
                    root.layer(),
                    source_node_count,
                    UiRootDiagnostic::AuthoringRejected { message },
                );
            }
        }
    }

    match builder.build() {
        Ok(snapshot) => Some(UiProjectedAuthoring { snapshot, buttons }),
        Err(error) => {
            push_diagnostic(diagnostics, "snapshot-rejected", &error.to_string());
            None
        }
    }
}

fn count_projected_root_nodes(
    world: &World,
    root: Entity,
    maximum_nodes: usize,
) -> Result<usize, ProjectionError> {
    let mut projected_nodes = 0usize;
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        if entity != root && world.get::<UiRoot>(entity).is_some() {
            continue;
        }
        if world.get::<UiNode>(entity).is_none() {
            continue;
        }
        admit_projected_node(&mut projected_nodes, maximum_nodes)?;
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().rev());
        }
    }
    Ok(projected_nodes)
}

pub(super) fn select_theme(
    world: &World,
    ordered_entities: &[Entity],
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
) -> UiTheme {
    let fallback = world
        .get_resource::<super::UiDefaultTheme>()
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

#[allow(clippy::too_many_arguments)]
fn project_root(
    world: &World,
    entity: Entity,
    root: UiRoot,
    node: UiNode,
    root_key: &UiRootKey,
    reducer_states: &BTreeMap<UiKey, UiButtonState>,
    buttons: &mut BTreeMap<UiKey, KernelButton>,
    projected_nodes: &mut usize,
    maximum_nodes: usize,
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
) -> Result<nara_ui_kernel::UiRoot, ProjectionError> {
    admit_projected_node(projected_nodes, maximum_nodes)?;

    if world.get::<UiButton>(entity).is_some() {
        push_diagnostic(
            diagnostics,
            "button-root-rejected",
            "A controlled UiButton must be a root-scoped descendant, not the UiRoot entity",
        );
        return Err(ProjectionError::RootButtonUnsupported);
    }
    let mut projected = nara_ui_kernel::UiRoot::new(root_key.clone(), root.layer(), node.style());
    if let Some(visual) = world.get::<UiVisual>(entity) {
        projected = projected.with_visual(visual.kernel());
    }
    if let Some(text) = world.get::<UiText>(entity) {
        projected = projected.with_text(text.kernel());
    }
    if let Some(semantics) = world.get::<UiSemantics>(entity) {
        projected = projected.with_semantics(semantics.kernel());
    }
    let children = project_children(
        world,
        entity,
        root_key,
        reducer_states,
        buttons,
        projected_nodes,
        maximum_nodes,
        diagnostics,
        1,
    )?;
    Ok(projected.with_children(children))
}

#[allow(clippy::too_many_arguments)]
fn project_children(
    world: &World,
    parent: Entity,
    root_key: &UiRootKey,
    reducer_states: &BTreeMap<UiKey, UiButtonState>,
    buttons: &mut BTreeMap<UiKey, KernelButton>,
    projected_nodes: &mut usize,
    maximum_nodes: usize,
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
    depth: usize,
) -> Result<Vec<nara_ui_kernel::UiNode>, ProjectionError> {
    if depth > MAX_PROJECTION_DEPTH {
        return Err(ProjectionError::DepthLimitExceeded {
            maximum: MAX_PROJECTION_DEPTH,
        });
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
        admit_projected_node(projected_nodes, maximum_nodes)?;
        let local_key = project_local_key(world, child)?;
        let descendants = project_children(
            world,
            child,
            root_key,
            reducer_states,
            buttons,
            projected_nodes,
            maximum_nodes,
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
                return Err(ProjectionError::ButtonKeyMissing);
            };
            let key = UiKey::scoped(root_key.clone(), local_key)?;
            let Some(visual) = world.get::<UiVisual>(child) else {
                push_diagnostic(
                    diagnostics,
                    "button-visual-missing",
                    "A controlled UiButton requires an explicit UiVisual component",
                );
                return Err(ProjectionError::ButtonVisualMissing);
            };
            if world.get::<UiText>(child).is_some() || world.get::<UiSemantics>(child).is_some() {
                push_diagnostic(
                    diagnostics,
                    "button-authoring-conflict",
                    "UiButton owns its label and semantics; do not add UiText or UiSemantics to the same entity",
                );
                return Err(ProjectionError::ButtonAuthoringConflict);
            }
            let description =
                KernelButton::new(key.clone(), button.label(), button.action().clone())?
                    .with_enabled(button.enabled());
            let state = reducer_states.get(&key).copied().unwrap_or_default();
            let button_visuals = world
                .get::<UiButtonVisuals>(child)
                .copied()
                .unwrap_or_default();
            let (visual, text_brush) = button_visual(
                visual,
                button_visuals,
                state,
                state.visual_state(&description),
                &description,
            )
            .map_err(UiWidgetError::Style)?;
            let authored = description
                .authored_node(
                    root_key,
                    node.style(),
                    visual,
                    text_brush,
                    button.logical_text_size(),
                )?
                .with_children(descendants);
            buttons.insert(key, description);
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

fn admit_projected_node(
    projected_nodes: &mut usize,
    maximum_nodes: usize,
) -> Result<(), ProjectionError> {
    let candidate = projected_nodes.saturating_add(1);
    if candidate > maximum_nodes {
        return Err(ProjectionError::NodeLimitExceeded {
            maximum: maximum_nodes,
        });
    }
    *projected_nodes = candidate;
    Ok(())
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

fn button_visual(
    visual: &UiVisual,
    visuals: UiButtonVisuals,
    interaction: UiButtonState,
    background_state: UiButtonVisualState,
    button: &KernelButton,
) -> Result<(nara_ui_kernel::UiVisual, nara_ui_kernel::UiBrush), nara_ui_kernel::UiStyleError> {
    use nara_ui_kernel::{UiBorder, UiBrush};

    let mut visual = visual.kernel();
    let background = match background_state {
        UiButtonVisualState::Hovered => Some(visuals.hovered_background()),
        UiButtonVisualState::Pressed => Some(visuals.pressed_background()),
        UiButtonVisualState::Disabled => Some(visuals.disabled_background()),
        UiButtonVisualState::Idle | UiButtonVisualState::Focused => None,
    };
    if let Some(background) = background {
        visual = visual.with_background(UiBrush::Role(background));
    }
    if interaction.focused() {
        visual = visual.with_border(UiBorder::new(
            visuals.focus_border_width(),
            UiBrush::Role(visuals.focused_border()),
        )?);
    }
    let text_role = if !button.enabled() {
        visuals.disabled_text()
    } else if background_state == UiButtonVisualState::Pressed {
        visuals.pressed_text()
    } else if interaction.focused() {
        visuals.focused_text()
    } else if background_state == UiButtonVisualState::Hovered {
        visuals.hovered_text()
    } else {
        visuals.normal_text()
    };
    Ok((visual, UiBrush::Role(text_role)))
}
