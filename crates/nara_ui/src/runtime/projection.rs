use std::collections::BTreeMap;

use nara_ecs::{Entity, World, bevy_ecs::hierarchy::Children};
use thiserror::Error;

use crate::kernel::{
    self, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiInteraction, UiKey, UiLimits,
    UiLocalKey, UiRootKey,
};
use crate::{
    UiBorder, UiBrush, UiButton, UiButtonVisuals, UiColorRole, UiNode, UiRoot, UiRuntimeKey,
    UiSemanticRole, UiSemantics, UiText, UiTextAlignment, UiVisual,
};

use super::{SurfaceInteraction, UiButtonBinding};

#[derive(Debug, Error)]
pub(super) enum ProjectionError {
    #[error("UI tree exceeds the node or depth limit")]
    Capacity,
    #[error(transparent)]
    Key(#[from] kernel::UiKeyError),
    #[error(transparent)]
    Node(#[from] kernel::UiAuthoringError),
    #[error(transparent)]
    Style(#[from] kernel::UiStyleError),
}

pub(super) fn project(
    world: &World,
    roots: &[(Entity, UiRoot)],
    source_revision: u64,
    interaction: &SurfaceInteraction,
) -> Result<(UiAuthoringSnapshot, BTreeMap<UiKey, UiButtonBinding>), ProjectionError> {
    let mut projection = Projection {
        world,
        interaction,
        buttons: BTreeMap::new(),
        remaining: UiLimits::default().max_nodes,
    };
    let mut snapshot = UiAuthoringSnapshotBuilder::new(source_revision);
    for &(entity, root) in roots {
        let Some(node) = world.get::<UiNode>(entity) else {
            continue;
        };
        let key = match world.get::<UiRuntimeKey>(entity) {
            Some(key) => UiRootKey::runtime(key.domain(), key.local())?,
            None => UiRootKey::entity(entity.to_bits()),
        };
        let mut tree = kernel::UiRoot::new(key.clone(), root.layer(), node.style());
        tree.node = projection.node(entity, *node, &key, 0)?;
        snapshot = snapshot.with_root(tree);
    }
    Ok((snapshot.build()?, projection.buttons))
}

struct Projection<'a> {
    world: &'a World,
    interaction: &'a SurfaceInteraction,
    buttons: BTreeMap<UiKey, UiButtonBinding>,
    remaining: usize,
}

impl Projection<'_> {
    fn node(
        &mut self,
        entity: Entity,
        node: UiNode,
        root: &UiRootKey,
        depth: usize,
    ) -> Result<kernel::UiNode, ProjectionError> {
        if depth >= UiLimits::default().max_depth || self.remaining == 0 {
            return Err(ProjectionError::Capacity);
        }
        self.remaining -= 1;
        let named = self.world.get::<UiRuntimeKey>(entity);
        let button = self.world.get::<UiButton>(entity);
        let text = self.world.get::<UiText>(entity);
        let semantics = self.world.get::<UiSemantics>(entity);
        let visual = self.world.get::<UiVisual>(entity);
        let local = if depth > 0 {
            match named {
                Some(key) => Some(UiLocalKey::runtime(key.domain(), key.local())?),
                None if button.is_some() || text.is_some() || semantics.is_some() => {
                    Some(UiLocalKey::entity(entity.to_bits()))
                }
                None => None,
            }
        } else {
            None
        };

        let mut projected = kernel::UiNode::new(node.style());
        if let Some(button) = button {
            let key = match &local {
                Some(local) => UiKey::scoped(root.clone(), local.clone())?,
                None => UiKey::root(root.clone())?,
            };
            let binding = UiButtonBinding {
                entity,
                action: button.action().clone(),
            };
            let styles = self
                .world
                .get::<UiButtonVisuals>(entity)
                .copied()
                .unwrap_or_default();
            let (visual, brush) = self
                .interaction
                .button_visuals(&key, &binding, button, styles, visual)?;
            self.buttons.insert(key, binding);
            projected = projected
                .with_visual(visual)
                .with_text(
                    kernel::UiText::new(button.label(), button.logical_text_size())?
                        .with_alignment(UiTextAlignment::Center)
                        .with_brush(brush),
                )
                .with_interaction(UiInteraction::focus_target().with_enabled(button.enabled()))
                .with_semantics(
                    kernel::UiSemantics::new(UiSemanticRole::Button, button.label())?
                        .with_action(button.action().clone())?,
                );
        } else {
            if let Some(visual) = visual {
                projected = projected.with_visual(visual.kernel());
            }
            if let Some(text) = text {
                projected = projected.with_text(text.kernel());
            }
            if let Some(semantics) = semantics {
                projected = projected.with_semantics(semantics.kernel());
            }
        }
        if let Some(local) = local {
            projected = projected.with_key(local);
        }
        if let Some(children) = self.world.get::<Children>(entity) {
            for &child in children.iter() {
                if self.world.get::<UiRoot>(child).is_none()
                    && let Some(node) = self.world.get::<UiNode>(child)
                {
                    projected = projected.with_child(self.node(child, *node, root, depth + 1)?);
                }
            }
        }
        Ok(projected)
    }
}

impl SurfaceInteraction {
    fn button_visuals(
        &self,
        key: &UiKey,
        binding: &UiButtonBinding,
        button: &UiButton,
        styles: UiButtonVisuals,
        visual: Option<&UiVisual>,
    ) -> Result<(kernel::UiVisual, UiBrush), kernel::UiStyleError> {
        let matches = |target: &super::UiTarget| &target.key == key && &target.binding == binding;
        let hovered = self.hovered.as_ref().is_some_and(matches);
        let focused = button.enabled() && self.focused.as_ref().is_some_and(matches);
        let pressed = hovered
            && self
                .pressed
                .as_ref()
                .is_some_and(|press| matches(&press.target));
        let (background, text) = if !button.enabled() {
            (Some(styles.disabled_background()), styles.disabled_text())
        } else if pressed {
            (Some(styles.pressed_background()), styles.pressed_text())
        } else if hovered {
            (Some(styles.hovered_background()), styles.hovered_text())
        } else {
            (None, styles.normal_text())
        };
        let mut visual = visual.map_or_else(
            || kernel::UiVisual::role(UiColorRole::Surface),
            UiVisual::kernel,
        );
        if let Some(background) = background {
            visual = visual.with_background(UiBrush::Role(background));
        }
        if focused {
            visual = visual.with_border(UiBorder::new(
                styles.focus_border_width(),
                UiBrush::Role(styles.focused_border()),
            )?);
        }
        let text = if focused && !pressed {
            styles.focused_text()
        } else {
            text
        };
        Ok((visual, UiBrush::Role(text)))
    }
}
