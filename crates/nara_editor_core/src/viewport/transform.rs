use std::collections::HashSet;

use nara_core::{Affine2, EntityUuid, Vec2};
use nara_reflect::{ComponentCodecError, ComponentSchemaVersion, ComponentTypeId, Schema};
use nara_scene::{
    authoring::{SceneComponent, SceneComponentError},
    spatial::Transform2d,
};

use super::{Gizmo2D, GizmoDelta2D};
use crate::{EditorCommand, EditorSession};

/// The authored transforms and parent mappings captured when a gizmo drag starts.
pub struct GizmoSelection2D {
    gizmo: Gizmo2D,
    targets: Vec<TransformTarget>,
    version: ComponentSchemaVersion,
}

struct TransformTarget {
    entity: EntityUuid,
    original: Option<Transform2d>,
    parent_from_world: Affine2,
    world_axes: [Vec2; 2],
    offset: Vec2,
}

impl GizmoSelection2D {
    /// Captures selected roots so children of another edited entity follow their parent once.
    /// A missing or singular parent mapping prevents the group from starting.
    pub fn capture(
        session: &EditorSession,
        gizmo: Gizmo2D,
        parent_affine: impl Fn(EntityUuid) -> Option<Affine2>,
    ) -> Result<Option<Self>, SceneComponentError> {
        let scene = session.preview_scene();
        let selection = session.selection().entities();
        let selected: HashSet<_> = selection.iter().copied().collect();
        let find = |id| {
            scene
                .entities
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| &scene.entities[index])
        };
        let mut targets = Vec::with_capacity(selection.len());
        for &entity in selection {
            let Some(row) = find(entity) else {
                return Ok(None);
            };
            let ancestors = std::iter::successors(row.parent, |id| find(*id)?.parent);
            if ancestors.into_iter().any(|id| selected.contains(&id)) {
                continue;
            }
            let Some(parent) = parent_affine(entity) else {
                return Ok(None);
            };
            let determinant = parent.matrix2.determinant();
            if !determinant.is_finite() || determinant == 0.0 {
                return Ok(None);
            }
            let parent_from_world = parent.inverse();
            if !parent_from_world.is_finite() {
                return Ok(None);
            }
            let authored = row.typed::<Transform2d>()?;
            let original = authored.unwrap_or_default();
            let axis = Vec2::from_angle(original.rotation);
            let world_axes = [axis, axis.perp()].map(|v| parent.transform_vector2(v));
            let offset = parent.transform_point2(original.translation) - gizmo.pivot().0;
            if world_axes.iter().any(|axis| !axis.is_finite()) || !offset.is_finite() {
                return Ok(None);
            }
            targets.push(TransformTarget {
                entity,
                original: authored,
                parent_from_world,
                world_axes,
                offset,
            });
        }
        let version = Transform2d::type_schema()?.version();
        Ok((!targets.is_empty()).then(|| Self {
            gizmo,
            targets,
            version,
        }))
    }

    /// Applies a world-space delta about the captured pivot. Rotation and scale also transform
    /// each entity basis. If the result contains local shear, keep the transformed X axis and
    /// the signed perpendicular component of the Y axis, which fit the authored TRS format.
    pub fn command(&self, delta: GizmoDelta2D) -> Result<EditorCommand, ComponentCodecError> {
        let component = ComponentTypeId::new(Transform2d::COMPONENT_ID);
        let axes_change = match delta {
            GizmoDelta2D::Rotation(angle) if angle != 0.0 => Some(Affine2::from_angle(angle)),
            GizmoDelta2D::Scale(factors) if factors != Vec2::ONE => {
                let basis = Affine2::from_angle(self.gizmo.angle());
                Some(basis * Affine2::from_scale(factors) * basis.inverse())
            }
            _ => None,
        };
        let mut edits = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            let original = target.original.unwrap_or_default();
            let mut transform = original;
            let local = target.parent_from_world;
            if let GizmoDelta2D::Translation(movement) = delta {
                transform.translation += local.transform_vector2(movement);
            }
            if let Some(axes_change) = axes_change {
                let [x, y] = target.world_axes.map(|axis| {
                    local
                        .transform_vector2(axes_change.transform_vector2(axis))
                        .as_dvec2()
                });
                let x_length = x.length();
                let x_axis = x / x_length;
                transform.rotation = x_axis.y.atan2(x_axis.x) as f32;
                transform.scale = Vec2::new(
                    (f64::from(original.scale.x) * x_length) as f32,
                    (f64::from(original.scale.y) * x_axis.perp_dot(y)) as f32,
                );
                let movement = axes_change.transform_vector2(target.offset) - target.offset;
                transform.translation += local.transform_vector2(movement);
            }
            let entities = vec![target.entity];
            edits.push(if target.original.is_none() && transform == original {
                EditorCommand::RemoveComponent {
                    entities,
                    component: component.clone(),
                }
            } else {
                EditorCommand::SetComponent {
                    entities,
                    component: component.clone(),
                    value: SceneComponent::known(self.version, transform.encode_component()?),
                }
            });
        }
        Ok(EditorCommand::Batch(edits))
    }
}
