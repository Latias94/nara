//! A third-party canvas component built from neutral meshes and ordinary public controls.

use lyon_path::{Path, math::point};
use lyon_tessellation::{
    BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use nara_ui_kernel::{
    UiActionId, UiBrush, UiButtonPhase, UiCaptureContinuity, UiColor, UiDispatchBatch, UiEdges,
    UiInteraction, UiKey, UiLayer, UiLocalKey, UiMesh, UiMeshVertex, UiNode, UiOverflow, UiPoint,
    UiPointerButton, UiPointerButtons, UiPointerId, UiPointerInput, UiPointerInputKind, UiRoot,
    UiRootKey, UiRoutePhase, UiRoutedEventKind, UiScrollDelta, UiSeatId, UiSemanticRole,
    UiSemantics, UiStyle, UiVisual,
};
use nara_ui_widgets::UiButton;

use crate::StatusLabelError;

/// A finite curve viewport with a selectable node and an independently operable output port.
///
/// The caller owns selected state, button reducers and event admission. This component authors
/// immutable descriptions; [`CurveCanvasState`] optionally owns its local pan/zoom gesture.
/// The two buttons expose their exact keys/actions for independently reduced control behavior.
pub struct CurveCanvas {
    root: UiRootKey,
    viewport_key: UiKey,
    curve_key: UiKey,
    button: UiButton,
    port_button: UiButton,
}

impl CurveCanvas {
    pub const WIDTH: f32 = 320.0;
    pub const HEIGHT: f32 = 100.0;

    pub fn new(root: UiRootKey) -> Result<Self, StatusLabelError> {
        Ok(Self {
            viewport_key: UiKey::scoped(
                root.clone(),
                UiLocalKey::runtime("external-curve", "viewport")?,
            )?,
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
    /// This is the unit-zoom convenience entry point for [`Self::authored_view`].
    pub fn authored_root(&self, pan: UiPoint, selected: bool) -> Result<UiRoot, StatusLabelError> {
        self.authored_view(pan, 1.0, selected)
    }

    /// Authors a transformed world inside the fixed 320 by 100 point viewport.
    ///
    /// Mesh positions, stroke thickness, control positions, control dimensions and padding use
    /// `world * zoom + pan`. Labels keep their fixed 14/10 point font sizes. Colors are straight
    /// alpha. This fixture retessellates on composition and makes no caching/performance claim.
    /// Pan and zoom use the same finite bounds as [`CurveCanvasState::set_view`].
    pub fn authored_view(
        &self,
        pan: UiPoint,
        zoom: f32,
        selected: bool,
    ) -> Result<UiRoot, StatusLabelError> {
        validate_view(pan, zoom)?;

        let curve = UiNode::new(UiStyle::absolute(0.0, 0.0, Self::WIDTH, Self::HEIGHT))
            .with_key(UiLocalKey::runtime("external-curve", "connection")?)
            .with_mesh(tessellate_curve(pan, zoom)?);
        let node_color = if selected {
            UiColor::rgba(0.95, 0.45, 0.12, 1.0)
        } else {
            UiColor::rgba(0.25, 0.30, 0.36, 1.0)
        };
        let button = self
            .button
            .authored_node(
                &self.root,
                UiStyle::absolute(
                    112.0 * zoom + pan.x(),
                    30.0 * zoom + pan.y(),
                    112.0 * zoom,
                    36.0 * zoom,
                )
                .with_padding(UiEdges::all(5.0 * zoom)?),
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
            UiStyle::absolute(
                244.0 * zoom + pan.x(),
                12.0 * zoom + pan.y(),
                72.0 * zoom,
                24.0 * zoom,
            )
            .with_padding(UiEdges::all(3.0 * zoom)?),
            UiVisual::solid(UiColor::rgba(0.16, 0.80, 0.40, 1.0)),
            UiBrush::Solid(UiColor::rgba(0.0, 0.0, 0.0, 1.0)),
            10.0,
        )?;
        let viewport = UiNode::new(
            UiStyle::absolute(0.0, 0.0, Self::WIDTH, Self::HEIGHT).with_overflow(UiOverflow::Clip),
        )
        .with_key(UiLocalKey::runtime("external-curve", "viewport")?)
        .with_visual(UiVisual::solid(UiColor::rgba(0.04, 0.07, 0.11, 1.0)))
        .with_semantics(UiSemantics::new(UiSemanticRole::Group, "Curve viewport")?)
        .with_interaction(
            UiInteraction::focus_target()
                .with_pointer_motion(true)
                .with_scroll(true)
                .with_pointer_buttons(UiPointerButtons::new(true, true, true))
                .with_capture_continuity(UiCaptureContinuity::TargetGeometry),
        )
        .with_children([curve, button, port_button]);

        Ok(UiRoot::new(self.root.clone(), UiLayer::Base, UiStyle::fill()).with_child(viewport))
    }

    /// Returns the exact target for local canvas gestures, distinct from either child button.
    #[must_use]
    pub const fn viewport_key(&self) -> &UiKey {
        &self.viewport_key
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

/// Local desired view state for one exact [`CurveCanvas`] viewport.
///
/// The caller supplies only batches already admitted by its original journal/session. This
/// reducer does not grant replay or presentation authority and retains no frame, receipt or
/// epoch. Consecutive admitted inputs update the desired view before the caller presents it.
/// Selection, button actions, documents and history remain with their existing owners.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveCanvasState {
    viewport_key: UiKey,
    pan: UiPoint,
    zoom: f32,
    drag: Option<CurveDrag>,
}

impl CurveCanvasState {
    /// Minimum local view scale accepted by this component.
    pub const MIN_ZOOM: f32 = 0.25;
    /// Maximum local view scale accepted by this component.
    pub const MAX_ZOOM: f32 = 4.0;
    /// Per-axis pan bound, leaving room for the complete world at maximum zoom within mesh limits.
    pub const MAX_PAN: f32 = 15_000.0;

    /// Starts at zero pan and unit zoom, without an active gesture.
    #[must_use]
    pub fn new(canvas: &CurveCanvas) -> Self {
        Self {
            viewport_key: canvas.viewport_key().clone(),
            pan: UiPoint::default(),
            zoom: 1.0,
            drag: None,
        }
    }

    /// Returns the current desired pan in authored logical points.
    #[must_use]
    pub const fn pan(&self) -> UiPoint {
        self.pan
    }

    /// Returns the current desired positive world scale.
    #[must_use]
    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns whether this local state is tracking an admitted viewport press.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Explicitly replaces the desired view and ends any local drag.
    ///
    /// Non-finite values, pan outside `-MAX_PAN..=MAX_PAN`, and zoom outside
    /// `MIN_ZOOM..=MAX_ZOOM` fail without changing state. The result reports only a view change;
    /// the caller still owns input cancellation and presentation of the replacement.
    pub fn set_view(&mut self, pan: UiPoint, zoom: f32) -> Result<bool, StatusLabelError> {
        validate_view(pan, zoom)?;
        let changed = self.pan != pan || self.zoom != zoom;
        self.pan = pan;
        self.zoom = zoom;
        self.drag = None;
        Ok(changed)
    }

    /// Atomically reduces an already admitted batch and reports whether the desired view changed.
    ///
    /// Only Target events for the exact viewport participate. A child button's press never starts
    /// pan through ancestor observation. Primary, middle and secondary drags use their original
    /// authored surface anchor even when motion leaves the viewport. Matching release, cancellation
    /// and viewport focus loss end the local gesture. Hover loss alone does not.
    ///
    /// Wheel zoom preserves the cursor's world point and clamps to the component's view bounds.
    /// Lines use `exp(delta_y * 0.1)`; Pixels use `exp(delta_y * 0.002)`. Shift instead pans
    /// horizontally, using 16 authored points per line or one per pixel unit. Bounds may limit
    /// exact anchor preservation. No publication, document mutation or external effect occurs.
    pub fn reduce(&mut self, batch: &UiDispatchBatch) -> Result<bool, StatusLabelError> {
        let mut candidate = self.clone();
        for event in batch.events() {
            if event.phase() != UiRoutePhase::Target || event.target() != &self.viewport_key {
                continue;
            }
            match event.kind() {
                UiRoutedEventKind::Pressed => {
                    let input = pointer_input(event.pointer_input())?;
                    let UiPointerInputKind::Button {
                        button,
                        phase: UiButtonPhase::Pressed,
                    } = input.kind()
                    else {
                        return Err(invalid_input(
                            "CurveCanvas press has no pressed-button sample",
                        ));
                    };
                    if !UiPointerButtons::new(true, true, true).contains(button) {
                        return Err(invalid_input(
                            "CurveCanvas does not support this drag button",
                        ));
                    }
                    let anchor = pointer_position(input)?;
                    if candidate.drag.is_some() {
                        return Err(invalid_input("CurveCanvas already has an active drag"));
                    }
                    candidate.drag = Some(CurveDrag {
                        seat: input.seat(),
                        pointer: input.pointer(),
                        button,
                        anchor,
                        pan_x: f64::from(candidate.pan.x()),
                        pan_y: f64::from(candidate.pan.y()),
                    });
                }
                UiRoutedEventKind::PointerMoved => {
                    let input = pointer_input(event.pointer_input())?;
                    if input.kind() != UiPointerInputKind::Moved {
                        return Err(invalid_input("CurveCanvas motion has no motion sample"));
                    }
                    let position = pointer_position(input)?;
                    if let Some(drag) = &candidate.drag
                        && drag.owns(input)
                    {
                        candidate.pan = drag.pan_at(position);
                    }
                }
                UiRoutedEventKind::Released { .. } => {
                    let input = pointer_input(event.pointer_input())?;
                    let UiPointerInputKind::Button {
                        button,
                        phase: UiButtonPhase::Released,
                    } = input.kind()
                    else {
                        return Err(invalid_input(
                            "CurveCanvas release has no released-button sample",
                        ));
                    };
                    if let Some(drag) = &candidate.drag
                        && drag.owns(input)
                        && drag.button == button
                    {
                        candidate.pan = drag.pan_at(pointer_position(input)?);
                        candidate.drag = None;
                    }
                }
                UiRoutedEventKind::Scrolled => {
                    let input = pointer_input(event.pointer_input())?;
                    let UiPointerInputKind::Scroll { delta } = input.kind() else {
                        return Err(invalid_input("CurveCanvas scroll has no scroll sample"));
                    };
                    candidate.scroll(input, delta)?;
                }
                UiRoutedEventKind::Cancelled
                | UiRoutedEventKind::FocusChanged { focused: false } => {
                    candidate.drag = None;
                }
                _ => {}
            }
        }
        let changed = candidate.pan != self.pan || candidate.zoom != self.zoom;
        *self = candidate;
        Ok(changed)
    }

    fn scroll(
        &mut self,
        input: UiPointerInput,
        delta: UiScrollDelta,
    ) -> Result<(), StatusLabelError> {
        let position = pointer_position(input)?;
        let (delta, zoom_rate, pan_rate) = match delta {
            UiScrollDelta::Lines(delta) => (delta, 0.1, 16.0),
            UiScrollDelta::Pixels(delta) => (delta, 0.002, 1.0),
        };
        if !delta.x().is_finite() || !delta.y().is_finite() {
            return Err(invalid_input(
                "CurveCanvas scroll displacement must be finite",
            ));
        }
        if input.modifiers().shift() {
            self.pan = bounded_pan(
                f64::from(self.pan.x()) + (f64::from(delta.x()) + f64::from(delta.y())) * pan_rate,
                f64::from(self.pan.y()),
            );
        } else {
            let logarithm = (f64::from(self.zoom).ln() + f64::from(delta.y()) * zoom_rate).clamp(
                f64::from(Self::MIN_ZOOM).ln(),
                f64::from(Self::MAX_ZOOM).ln(),
            );
            let zoom = logarithm.exp() as f32;
            let ratio = f64::from(zoom) / f64::from(self.zoom);
            self.pan = bounded_pan(
                f64::from(position.x())
                    - (f64::from(position.x()) - f64::from(self.pan.x())) * ratio,
                f64::from(position.y())
                    - (f64::from(position.y()) - f64::from(self.pan.y())) * ratio,
            );
            self.zoom = zoom;
        }
        if let Some(drag) = &mut self.drag {
            drag.anchor = position;
            drag.pan_x = f64::from(self.pan.x());
            drag.pan_y = f64::from(self.pan.y());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CurveDrag {
    seat: UiSeatId,
    pointer: UiPointerId,
    button: UiPointerButton,
    anchor: UiPoint,
    pan_x: f64,
    pan_y: f64,
}

impl CurveDrag {
    fn owns(self, input: UiPointerInput) -> bool {
        self.seat == input.seat() && self.pointer == input.pointer()
    }

    fn pan_at(self, position: UiPoint) -> UiPoint {
        bounded_pan(
            self.pan_x + f64::from(position.x()) - f64::from(self.anchor.x()),
            self.pan_y + f64::from(position.y()) - f64::from(self.anchor.y()),
        )
    }
}

fn bounded_pan(pan_x: f64, pan_y: f64) -> UiPoint {
    let maximum = f64::from(CurveCanvasState::MAX_PAN);
    UiPoint::new(
        pan_x.clamp(-maximum, maximum) as f32,
        pan_y.clamp(-maximum, maximum) as f32,
    )
}

fn validate_view(pan: UiPoint, zoom: f32) -> Result<(), StatusLabelError> {
    if !pan.x().is_finite()
        || !pan.y().is_finite()
        || pan.x().abs() > CurveCanvasState::MAX_PAN
        || pan.y().abs() > CurveCanvasState::MAX_PAN
    {
        return Err(invalid_input(
            "CurveCanvas pan must be finite and within 15000 points per axis",
        ));
    }
    if !zoom.is_finite()
        || !(CurveCanvasState::MIN_ZOOM..=CurveCanvasState::MAX_ZOOM).contains(&zoom)
    {
        return Err(invalid_input(
            "CurveCanvas zoom must be finite and within 0.25..=4",
        ));
    }
    Ok(())
}

fn pointer_input(input: Option<&UiPointerInput>) -> Result<UiPointerInput, StatusLabelError> {
    input
        .copied()
        .ok_or_else(|| invalid_input("CurveCanvas event has no pointer sample"))
}

fn pointer_position(input: UiPointerInput) -> Result<UiPoint, StatusLabelError> {
    input
        .position()
        .filter(|position| position.x().is_finite() && position.y().is_finite())
        .ok_or_else(|| invalid_input("CurveCanvas event has no finite pointer position"))
}

fn invalid_input(message: &'static str) -> StatusLabelError {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into()
}

fn tessellate_curve(pan: UiPoint, zoom: f32) -> Result<UiMesh, StatusLabelError> {
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
                UiPoint::new(position.x * zoom + pan.x(), position.y * zoom + pan.y()),
                UiColor::rgba(0.10, 0.75, 1.0, 0.80),
            )
        }),
    )?;
    Ok(UiMesh::new(geometry.vertices, geometry.indices)?)
}
