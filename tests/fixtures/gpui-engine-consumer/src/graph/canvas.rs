//! Finite, viewport-local graph gestures without document or UI ownership.

use super::{NodeId, VALUE_LIMIT, preview::GraphPreviewBasis};

const PAN_LIMIT: f64 = 15_000.0;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 2.0;
const DRAG_THRESHOLD_SQUARED: f64 = 9.0;
const LINE_PIXELS: f64 = 32.0;
const WHEEL_ZOOM_SCALE: f64 = 0.0015;
const WHEEL_EXPONENT_LIMIT: f64 = 2.0 * std::f64::consts::LN_2;

/// Maps authored node positions to fixed viewport coordinates as `pan + position * zoom`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasTransform {
    /// Translation in logical viewport pixels.
    pub pan: [f32; 2],
    /// Uniform scale of authored coordinates.
    pub zoom: f32,
}

impl Default for CanvasTransform {
    fn default() -> Self {
        Self {
            pan: [0.0; 2],
            zoom: 1.0,
        }
    }
}

/// The button that starts and must finish a gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasButton {
    /// Moves a node in authored coordinates.
    Primary,
    /// Pans the local view.
    Middle,
}

/// The native scroll delta's unit before normalization to logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollUnit {
    /// Each line represents 32 logical pixels.
    Lines,
    /// Deltas already represent logical pixels.
    Pixels,
}

/// A finished node move for the caller to validate against its original document basis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeMove {
    /// The exact basis supplied when the gesture began.
    pub basis: GraphPreviewBasis,
    /// The authored node selected at the start of the gesture.
    pub node: NodeId,
    /// The final authored position, clamped to the graph's position bounds.
    pub position: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NodeDrag {
    basis: GraphPreviewBasis,
    node: NodeId,
    original: [f32; 2],
    position: [f32; 2],
    pointer: [f32; 2],
    zoom: f32,
    crossed_threshold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PanDrag {
    original: [f32; 2],
    pointer: [f32; 2],
    crossed_threshold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Gesture {
    Node(NodeDrag),
    Pan(PanDrag),
}

impl Gesture {
    fn button(self) -> CanvasButton {
        match self {
            Self::Node(_) => CanvasButton::Primary,
            Self::Pan(_) => CanvasButton::Middle,
        }
    }
}

/// Owns one local view transform and at most one transient gesture.
///
/// All pointers and zoom anchors must use logical coordinates relative to the same fixed
/// viewport. The caller captures its displayed origin; moving descendants do not change it.
/// Node previews never mutate documents or history. Motion starts at three logical pixels
/// and, once started, follows every finite sample, including samples outside the viewport.
/// Invalid numeric inputs preserve both the view and any active gesture.
#[derive(Debug, Default)]
pub struct GraphCanvas {
    transform: CanvasTransform,
    gesture: Option<Gesture>,
}

impl GraphCanvas {
    /// Returns the current local view transform.
    #[must_use]
    pub fn transform(&self) -> CanvasTransform {
        self.transform
    }

    /// Reports whether a node or pan gesture is held, including before its drag threshold.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.gesture.is_some()
    }

    /// Returns the held node, including before its preview starts.
    #[must_use]
    pub fn active_node(&self) -> Option<NodeId> {
        match self.gesture {
            Some(Gesture::Node(drag)) => Some(drag.node),
            _ => None,
        }
    }

    /// Starts a primary-button node gesture, refusing non-finite inputs or another gesture.
    pub fn begin_node(
        &mut self,
        basis: GraphPreviewBasis,
        node: NodeId,
        position: [f32; 2],
        pointer: [f32; 2],
    ) -> bool {
        if self.is_active() || !finite(position) || !finite(pointer) {
            return false;
        }
        self.gesture = Some(Gesture::Node(NodeDrag {
            basis,
            node,
            original: position,
            position,
            pointer,
            zoom: self.transform.zoom,
            crossed_threshold: false,
        }));
        true
    }

    /// Starts a middle-button pan gesture, refusing non-finite input or another gesture.
    pub fn begin_pan(&mut self, pointer: [f32; 2]) -> bool {
        if self.is_active() || !finite(pointer) {
            return false;
        }
        self.gesture = Some(Gesture::Pan(PanDrag {
            original: self.transform.pan,
            pointer,
            crossed_threshold: false,
        }));
        true
    }

    /// Updates a held gesture from its starting pointer, returning whether its state changed.
    ///
    /// Node displacement is divided by the captured starting zoom and added to the original
    /// authored position. Repeated samples never accumulate displacement or rounding error.
    pub fn motion(&mut self, pointer: [f32; 2]) -> bool {
        if !finite(pointer) {
            return false;
        }
        let before = (self.transform, self.gesture);
        match self.gesture.as_mut() {
            Some(Gesture::Node(drag)) => {
                let delta = displacement(pointer, drag.pointer);
                drag.crossed_threshold |= crossed_threshold(delta);
                if drag.crossed_threshold {
                    drag.position = translated(
                        drag.original,
                        delta.map(|value| value / f64::from(drag.zoom)),
                        VALUE_LIMIT,
                    );
                }
            }
            Some(Gesture::Pan(drag)) => {
                let delta = displacement(pointer, drag.pointer);
                drag.crossed_threshold |= crossed_threshold(delta);
                if drag.crossed_threshold {
                    self.transform.pan = translated(drag.original, delta, PAN_LIMIT);
                }
            }
            None => {}
        }
        (self.transform, self.gesture) != before
    }

    /// Applies the final sample and clears only a matching-button gesture with a finite pointer.
    ///
    /// A wrong button or non-finite pointer preserves the gesture. A click, a node returned
    /// exactly to its original position, and every pan produce no domain move.
    pub fn finish(&mut self, button: CanvasButton, pointer: [f32; 2]) -> Option<NodeMove> {
        if !finite(pointer) || self.gesture?.button() != button {
            return None;
        }
        self.motion(pointer);
        match self.gesture.take()? {
            Gesture::Node(drag) if drag.crossed_threshold && drag.position != drag.original => {
                Some(NodeMove {
                    basis: drag.basis,
                    node: drag.node,
                    position: drag.position,
                })
            }
            _ => None,
        }
    }

    /// Cancels a held gesture without emitting a domain move, returning whether one existed.
    ///
    /// Node cancellation drops its preview. Pan cancellation restores its original pan, or
    /// the constrained pan immediately after the most recent wheel rebase during that drag.
    pub fn cancel(&mut self) -> bool {
        let Some(gesture) = self.gesture.take() else {
            return false;
        };
        if let Gesture::Pan(drag) = gesture {
            self.transform.pan = drag.original;
        }
        true
    }

    /// Returns this node's transient authored position, or the caller's unchanged fallback.
    #[must_use]
    pub fn node_position(&self, node: NodeId, fallback: [f32; 2]) -> [f32; 2] {
        match self.gesture {
            Some(Gesture::Node(drag)) if drag.node == node => drag.position,
            _ => fallback,
        }
    }

    /// Pans by normalized wheel deltas, or zooms by `exp(0.0015 * normalized_delta_y)`.
    ///
    /// Zoom preserves the authored point at `pointer` wherever the pan bounds permit it.
    /// A nonzero relevant wheel delta cancels a node preview. During a pan it rebases the
    /// starting pointer and cancellation pan to the actual pointer and constrained result,
    /// preserving a stationary release even after a limit was reached. Zero relevant delta
    /// does nothing. Returns whether the view or gesture changed.
    pub fn scroll(
        &mut self,
        pointer: [f32; 2],
        delta: [f32; 2],
        unit: ScrollUnit,
        zoom: bool,
    ) -> bool {
        if !finite(pointer) || !finite(delta) {
            return false;
        }
        let scale = match unit {
            ScrollUnit::Lines => LINE_PIXELS,
            ScrollUnit::Pixels => 1.0,
        };
        let delta = delta.map(|value| f64::from(value) * scale);
        if (zoom && delta[1] == 0.0) || (!zoom && delta == [0.0; 2]) {
            return false;
        }
        let before = (self.transform, self.gesture);
        self.transform = if zoom {
            let exponent =
                (delta[1] * WHEEL_ZOOM_SCALE).clamp(-WHEEL_EXPONENT_LIMIT, WHEEL_EXPONENT_LIMIT);
            zoomed(self.transform, exponent.exp(), pointer)
        } else {
            CanvasTransform {
                pan: translated(self.transform.pan, delta, PAN_LIMIT),
                ..self.transform
            }
        };
        self.gesture = match self.gesture {
            Some(Gesture::Pan(drag)) => Some(Gesture::Pan(PanDrag {
                original: self.transform.pan,
                pointer,
                ..drag
            })),
            _ => None,
        };
        (self.transform, self.gesture) != before
    }

    /// Cancels a held gesture, then applies a finite positive zoom factor around `anchor`.
    ///
    /// Zoom is clamped to `[0.5, 2.0]`; pan is clamped to `[-15000, 15000]` per axis.
    /// Cancellation restores a pan's baseline before applying the view change. Invalid
    /// inputs do not cancel. Returns whether the view or gesture changed.
    pub fn zoom_by(&mut self, factor: f32, anchor: [f32; 2]) -> bool {
        if !factor.is_finite() || factor <= 0.0 || !finite(anchor) {
            return false;
        }
        let cancelled = self.cancel();
        let next = zoomed(self.transform, f64::from(factor), anchor);
        let changed = self.transform != next;
        self.transform = next;
        cancelled || changed
    }

    /// Cancels a held gesture, then adds a finite logical-pixel delta within the pan bounds.
    ///
    /// Cancellation restores a pan's baseline before applying the delta. Invalid inputs do
    /// not cancel. Returns whether the view or gesture changed.
    pub fn pan_by(&mut self, delta: [f32; 2]) -> bool {
        if !finite(delta) {
            return false;
        }
        let cancelled = self.cancel();
        let pan = translated(self.transform.pan, delta.map(f64::from), PAN_LIMIT);
        let changed = self.transform.pan != pan;
        self.transform.pan = pan;
        cancelled || changed
    }
}

fn finite(point: [f32; 2]) -> bool {
    point.into_iter().all(f32::is_finite)
}

fn displacement(pointer: [f32; 2], start: [f32; 2]) -> [f64; 2] {
    std::array::from_fn(|axis| f64::from(pointer[axis]) - f64::from(start[axis]))
}

fn crossed_threshold(delta: [f64; 2]) -> bool {
    delta[0] * delta[0] + delta[1] * delta[1] >= DRAG_THRESHOLD_SQUARED
}

fn translated(origin: [f32; 2], delta: [f64; 2], limit: f64) -> [f32; 2] {
    std::array::from_fn(|axis| (f64::from(origin[axis]) + delta[axis]).clamp(-limit, limit) as f32)
}

fn zoomed(transform: CanvasTransform, factor: f64, anchor: [f32; 2]) -> CanvasTransform {
    let zoom =
        (f64::from(transform.zoom) * factor).clamp(f64::from(MIN_ZOOM), f64::from(MAX_ZOOM)) as f32;
    if zoom == transform.zoom {
        return transform;
    }
    let ratio = f64::from(zoom) / f64::from(transform.zoom);
    CanvasTransform {
        pan: std::array::from_fn(|axis| {
            let anchor = f64::from(anchor[axis]);
            (anchor - (anchor - f64::from(transform.pan[axis])) * ratio)
                .clamp(-PAN_LIMIT, PAN_LIMIT) as f32
        }),
        zoom,
    }
}

#[cfg(test)]
mod tests;
