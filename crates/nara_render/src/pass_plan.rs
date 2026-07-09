use std::collections::{BTreeMap, BTreeSet};

use crate::{ExtractedViews, RenderPhaseLabel, RenderTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderPassStepLabel {
    Clear,
    Phase(RenderPhaseLabel),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderPassNodeId {
    pub view_index: usize,
    pub label: RenderPassStepLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPassStep {
    pub node: RenderPassNodeId,
    pub view_order: i32,
    pub target: RenderTarget,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RenderPassPlan {
    steps: Vec<RenderPassStep>,
}

impl RenderPassPlan {
    #[must_use]
    pub fn new(steps: Vec<RenderPassStep>) -> Self {
        Self { steps }
    }

    #[must_use]
    pub fn steps(&self) -> &[RenderPassStep] {
        &self.steps
    }

    pub fn for_view(&self, view_index: usize) -> impl Iterator<Item = &RenderPassStep> {
        self.steps
            .iter()
            .filter(move |step| step.node.view_index == view_index)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn validate_dependencies(
        &self,
        dependencies: &[RenderPassDependency],
    ) -> Result<(), RenderPassDependencyError> {
        validate_render_pass_dependencies(self, dependencies)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPhaseInput {
    pub view_index: usize,
    pub phase: RenderPhaseLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPassDependency {
    pub before: RenderPassNodeId,
    pub after: RenderPassNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderPassDependencyError {
    #[error("render pass dependency references an unknown node: {node:?}")]
    UnknownNode { node: RenderPassNodeId },
    #[error("render pass dependency graph contains a cycle")]
    Cycle,
}

#[must_use]
pub fn build_render_pass_plan(
    views: &ExtractedViews,
    phases: impl IntoIterator<Item = RenderPhaseInput>,
) -> RenderPassPlan {
    let mut phases_by_view = BTreeMap::<usize, BTreeSet<RenderPhaseLabel>>::new();
    for input in phases {
        if input.view_index < views.len() {
            phases_by_view
                .entry(input.view_index)
                .or_default()
                .insert(input.phase);
        }
    }

    let mut ordered_views = views
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, view)| (view.order, index, view.target))
        .collect::<Vec<_>>();
    ordered_views.sort_by_key(|(order, index, _)| (*order, *index));

    let mut steps = Vec::new();
    for (view_order, view_index, target) in ordered_views {
        steps.push(RenderPassStep {
            node: RenderPassNodeId {
                view_index,
                label: RenderPassStepLabel::Clear,
            },
            view_order,
            target,
        });

        let mut phases = phases_by_view
            .remove(&view_index)
            .map(|phases| phases.into_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        phases.sort_by_key(|phase| render_phase_order(*phase));
        for phase in phases {
            steps.push(RenderPassStep {
                node: RenderPassNodeId {
                    view_index,
                    label: RenderPassStepLabel::Phase(phase),
                },
                view_order,
                target,
            });
        }
    }

    RenderPassPlan::new(steps)
}

#[must_use]
pub fn render_phase_order(phase: RenderPhaseLabel) -> u16 {
    if phase == RenderPhaseLabel::OPAQUE_2D {
        10
    } else if phase == RenderPhaseLabel::TILEMAP_2D {
        20
    } else if phase == RenderPhaseLabel::TRANSPARENT_2D {
        30
    } else if phase == RenderPhaseLabel::UI {
        40
    } else if phase == RenderPhaseLabel::GIZMO {
        50
    } else {
        u16::MAX
    }
}

fn validate_render_pass_dependencies(
    plan: &RenderPassPlan,
    dependencies: &[RenderPassDependency],
) -> Result<(), RenderPassDependencyError> {
    let nodes = plan
        .steps()
        .iter()
        .map(|step| step.node)
        .collect::<BTreeSet<_>>();
    let mut edges = BTreeMap::<RenderPassNodeId, Vec<RenderPassNodeId>>::new();

    for dependency in dependencies {
        if !nodes.contains(&dependency.before) {
            return Err(RenderPassDependencyError::UnknownNode {
                node: dependency.before,
            });
        }
        if !nodes.contains(&dependency.after) {
            return Err(RenderPassDependencyError::UnknownNode {
                node: dependency.after,
            });
        }
        edges
            .entry(dependency.before)
            .or_default()
            .push(dependency.after);
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for node in nodes {
        if has_cycle(node, &edges, &mut visiting, &mut visited) {
            return Err(RenderPassDependencyError::Cycle);
        }
    }

    Ok(())
}

fn has_cycle(
    node: RenderPassNodeId,
    edges: &BTreeMap<RenderPassNodeId, Vec<RenderPassNodeId>>,
    visiting: &mut BTreeSet<RenderPassNodeId>,
    visited: &mut BTreeSet<RenderPassNodeId>,
) -> bool {
    if visited.contains(&node) {
        return false;
    }
    if !visiting.insert(node) {
        return true;
    }
    if let Some(next_nodes) = edges.get(&node) {
        for next in next_nodes {
            if has_cycle(*next, edges, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(&node);
    visited.insert(node);
    false
}

#[cfg(test)]
mod tests {
    use nara_core::{Color, Vec2};
    use nara_ecs::Entity;

    use super::*;
    use crate::{ExtractedView, ViewportRect};

    fn views() -> ExtractedViews {
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 100, 100).unwrap(),
            world_position: Vec2::ZERO,
            viewport_height: 100.0,
            order: 0,
            clear_color: Color::BLACK,
        });
        views
    }

    #[test]
    fn pass_plan_orders_clear_world_ui_and_gizmo_per_view() {
        let views = views();
        let plan = build_render_pass_plan(
            &views,
            [
                RenderPhaseInput {
                    view_index: 0,
                    phase: RenderPhaseLabel::UI,
                },
                RenderPhaseInput {
                    view_index: 0,
                    phase: RenderPhaseLabel::TRANSPARENT_2D,
                },
                RenderPhaseInput {
                    view_index: 0,
                    phase: RenderPhaseLabel::GIZMO,
                },
                RenderPhaseInput {
                    view_index: 0,
                    phase: RenderPhaseLabel::TILEMAP_2D,
                },
            ],
        );

        assert_eq!(
            plan.steps()
                .iter()
                .map(|step| step.node.label)
                .collect::<Vec<_>>(),
            vec![
                RenderPassStepLabel::Clear,
                RenderPassStepLabel::Phase(RenderPhaseLabel::TILEMAP_2D),
                RenderPassStepLabel::Phase(RenderPhaseLabel::TRANSPARENT_2D),
                RenderPassStepLabel::Phase(RenderPhaseLabel::UI),
                RenderPassStepLabel::Phase(RenderPhaseLabel::GIZMO),
            ]
        );
    }

    #[test]
    fn pass_plan_sorts_multiple_views_by_camera_order() {
        let mut views = ExtractedViews::default();
        for (order, width) in [(2, 300), (-1, 100), (1, 200)] {
            views.push(ExtractedView {
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::PrimaryWindow,
                viewport: ViewportRect::new(0, 0, width, 100).unwrap(),
                world_position: Vec2::ZERO,
                viewport_height: 100.0,
                order,
                clear_color: Color::BLACK,
            });
        }

        let plan = build_render_pass_plan(&views, []);

        assert_eq!(
            plan.steps()
                .iter()
                .map(|step| step.node.view_index)
                .collect::<Vec<_>>(),
            vec![1, 2, 0]
        );
    }

    #[test]
    fn dependency_validation_reports_unknown_nodes_and_cycles() {
        let views = views();
        let plan = build_render_pass_plan(
            &views,
            [RenderPhaseInput {
                view_index: 0,
                phase: RenderPhaseLabel::UI,
            }],
        );
        let clear = RenderPassNodeId {
            view_index: 0,
            label: RenderPassStepLabel::Clear,
        };
        let ui = RenderPassNodeId {
            view_index: 0,
            label: RenderPassStepLabel::Phase(RenderPhaseLabel::UI),
        };
        let missing = RenderPassNodeId {
            view_index: 9,
            label: RenderPassStepLabel::Clear,
        };

        assert!(matches!(
            plan.validate_dependencies(&[RenderPassDependency {
                before: clear,
                after: missing,
            }]),
            Err(RenderPassDependencyError::UnknownNode { node }) if node == missing
        ));
        assert_eq!(
            plan.validate_dependencies(&[
                RenderPassDependency {
                    before: clear,
                    after: ui,
                },
                RenderPassDependency {
                    before: ui,
                    after: clear,
                },
            ]),
            Err(RenderPassDependencyError::Cycle)
        );
    }
}
