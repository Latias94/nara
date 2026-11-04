//! A third-party canvas component built from neutral meshes and ordinary public controls.

use lyon_path::{Path, math::point};
use lyon_tessellation::{
    BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use nara_ui_kernel::{
    UiActionId, UiBrush, UiColor, UiEdges, UiKey, UiLayer, UiLocalKey, UiMesh, UiMeshVertex,
    UiNode, UiOverflow, UiPoint, UiRoot, UiRootKey, UiSemanticRole, UiSemantics, UiStyle, UiVisual,
};
use nara_ui_widgets::UiButton;

use crate::StatusLabelError;

/// A finite curve viewport with a selectable node and an independently operable output port.
///
/// The caller owns selected state, button reducers and event admission. This component only
/// authors immutable descriptions. Its two buttons expose their exact keys/actions so the same
/// component can be driven through a native Host without introducing another state store.
pub struct CurveCanvas {
    root: UiRootKey,
    curve_key: UiKey,
    button: UiButton,
    port_button: UiButton,
}

impl CurveCanvas {
    pub const WIDTH: f32 = 320.0;
    pub const HEIGHT: f32 = 100.0;

    pub fn new(root: UiRootKey) -> Result<Self, StatusLabelError> {
        Ok(Self {
            curve_key: UiKey::scoped(
                root.clone(),
                UiLocalKey::runtime("external-curve", "connection")?,
            )?,
            button: UiButton::new(
                UiKey::scoped(root.clone(), UiLocalKey::runtime("external-curve", "node")?)?,
                "Curve node",
                UiActionId::new("external-curve.toggle-node")?,
            )?,
            port_button: UiButton::new(
                UiKey::scoped(
                    root.clone(),
                    UiLocalKey::runtime("external-curve", "output-port")?,
                )?,
                "Output port",
                UiActionId::new("external-curve.activate-port")?,
            )?,
            root,
        })
    }

    /// Authors background, curve, node button and port button in that paint order.
    ///
    /// `pan` moves both the curve geometry and controls inside the fixed viewport. Geometry is
    /// tessellated in local coordinates, then translated before entering the kernel. Colors are
    /// straight alpha; in particular the curve's RGB channels are not multiplied by its alpha.
    /// This small fixture retessellates on composition and makes no caching/performance claim.
    pub fn authored_root(&self, pan: UiPoint, selected: bool) -> Result<UiRoot, StatusLabelError> {
        if !pan.x().is_finite() || !pan.y().is_finite() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "CurveCanvas pan must be finite",
            )
            .into());
        }

        let curve = UiNode::new(UiStyle::absolute(0.0, 0.0, Self::WIDTH, Self::HEIGHT))
            .with_key(UiLocalKey::runtime("external-curve", "connection")?)
            .with_mesh(tessellate_curve(pan)?);
        let node_color = if selected {
            UiColor::rgba(0.95, 0.45, 0.12, 1.0)
        } else {
            UiColor::rgba(0.25, 0.30, 0.36, 1.0)
        };
        let button = self
            .button
            .authored_node(
                &self.root,
                UiStyle::absolute(112.0 + pan.x(), 30.0 + pan.y(), 112.0, 36.0)
                    .with_padding(UiEdges::all(5.0)?),
                UiVisual::solid(node_color),
                UiBrush::Solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
                14.0,
            )?
            .with_semantics(
                UiSemantics::new(UiSemanticRole::Button, self.button.label())?
                    .with_action(self.button.action().clone())?
                    .with_selected(selected),
            );
        let port_button = self.port_button.authored_node(
            &self.root,
            UiStyle::absolute(244.0 + pan.x(), 12.0 + pan.y(), 72.0, 24.0)
                .with_padding(UiEdges::all(3.0)?),
            UiVisual::solid(UiColor::rgba(0.16, 0.80, 0.40, 1.0)),
            UiBrush::Solid(UiColor::rgba(0.0, 0.0, 0.0, 1.0)),
            10.0,
        )?;
        let viewport = UiNode::new(
            UiStyle::absolute(0.0, 0.0, Self::WIDTH, Self::HEIGHT).with_overflow(UiOverflow::Clip),
        )
        .with_key(UiLocalKey::runtime("external-curve", "viewport")?)
        .with_visual(UiVisual::solid(UiColor::rgba(0.04, 0.07, 0.11, 1.0)))
        .with_children([curve, button, port_button]);

        Ok(UiRoot::new(self.root.clone(), UiLayer::Base, UiStyle::fill()).with_child(viewport))
    }

    #[must_use]
    pub const fn button(&self) -> &UiButton {
        &self.button
    }

    #[must_use]
    pub const fn port_button(&self) -> &UiButton {
        &self.port_button
    }

    #[must_use]
    pub const fn curve_key(&self) -> &UiKey {
        &self.curve_key
    }
}

fn tessellate_curve(pan: UiPoint) -> Result<UiMesh, StatusLabelError> {
    let mut path = Path::builder();
    path.begin(point(16.0, 72.0));
    path.cubic_bezier_to(point(100.0, 72.0), point(216.0, 24.0), point(300.0, 24.0));
    path.end(false);
    let path = path.build();
    let mut geometry: VertexBuffers<UiMeshVertex, u32> = VertexBuffers::new();
    StrokeTessellator::new().tessellate_path(
        &path,
        &StrokeOptions::default()
            .with_line_width(6.0)
            .with_tolerance(0.1),
        &mut BuffersBuilder::new(&mut geometry, |vertex: StrokeVertex<'_, '_>| {
            let position = vertex.position();
            UiMeshVertex::new(
                UiPoint::new(position.x + pan.x(), position.y + pan.y()),
                UiColor::rgba(0.10, 0.75, 1.0, 0.80),
            )
        }),
    )?;
    Ok(UiMesh::new(geometry.vertices, geometry.indices)?)
}
