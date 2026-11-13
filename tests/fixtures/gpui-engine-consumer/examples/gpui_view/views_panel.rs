use super::engine::views::{EngineViewImage, EngineViewPresentation, ViewKey};
use nara_gpui_engine_consumer::graph::preview::GraphPreviewBasis;
use nara_render::{DisplayMapping2d, DisplayRect, DisplayScale};
use open_gpui::{
    AnyElement, App, Bounds, Context, Corners, Element, ElementId, GlobalElementId, ImageTexture,
    InspectorElementId, LayoutId, MouseButton, Pixels, Point, PrepaintPublicationId, Render, Role,
    Window, WindowId, canvas, div, point, prelude::*, px, rgb, size,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

pub struct ViewSnapshot {
    pub image: Rc<EngineViewImage>,
    pub mapping: DisplayMapping2d,
    pub presented: RefCell<Option<Rc<EngineViewPresentation>>>,
}

pub struct ViewPublication {
    pub generation: u64,
    pub snapshot: Rc<ViewSnapshot>,
}

#[derive(Clone)]
pub enum ViewsIntent {
    Edit(GraphPreviewBasis),
    Compile(GraphPreviewBasis),
    HoldNext,
    ReleaseHeld,
    RelocateA,
    PulseB,
    CloseA,
    Save(GraphPreviewBasis),
    Reopen(GraphPreviewBasis),
    CloseGraph(GraphPreviewBasis),
    Pick {
        snapshot: Rc<ViewSnapshot>,
        point: [f32; 2],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewControl {
    Edit,
    Compile,
    HoldNext,
    ReleaseHeld,
    RelocateA,
    PulseB,
    CloseA,
    Save,
    Reopen,
    CloseGraph,
}

impl ViewControl {
    const ALL: [Self; 10] = [
        Self::Edit,
        Self::Compile,
        Self::HoldNext,
        Self::ReleaseHeld,
        Self::RelocateA,
        Self::PulseB,
        Self::CloseA,
        Self::Save,
        Self::Reopen,
        Self::CloseGraph,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Edit => 0,
            Self::Compile => 1,
            Self::HoldNext => 2,
            Self::ReleaseHeld => 3,
            Self::RelocateA => 4,
            Self::PulseB => 5,
            Self::CloseA => 6,
            Self::Save => 7,
            Self::Reopen => 8,
            Self::CloseGraph => 9,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit graph",
            Self::Compile => "Compile preview",
            Self::HoldNext => "Hold next compile",
            Self::ReleaseHeld => "Release held compile",
            Self::RelocateA => "Relocate A",
            Self::PulseB => "Pulse B",
            Self::CloseA => "Close A",
            Self::Save => "Save graph",
            Self::Reopen => "Reopen graph",
            Self::CloseGraph => "Close graph",
        }
    }
}

#[derive(Default)]
struct ControlSlot {
    publication: PrepaintPublicationId,
    bounds: Rc<RefCell<Option<ControlGeometry>>>,
}

struct ControlGeometry {
    bounds: Bounds<Pixels>,
    basis: Option<GraphPreviewBasis>,
}

#[derive(Default)]
struct ViewSlot {
    publication: PrepaintPublicationId,
    snapshot: Rc<RefCell<Option<Rc<ViewSnapshot>>>>,
}

struct RecordedControl {
    child: AnyElement,
    publication: PrepaintPublicationId,
    presented: Rc<RefCell<Option<ControlGeometry>>>,
    basis: Option<GraphPreviewBasis>,
    window_id: WindowId,
}

impl Element for RecordedControl {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
        if window.window_handle().window_id() != self.window_id {
            return;
        }
        let visible = window
            .try_element_geometry(bounds)
            .ok()
            .map(|geometry| geometry.displayed_bounds().intersect(&window.clip_bounds()))
            .filter(|bounds| !bounds.is_empty());
        let committed = self.presented.clone();
        let discarded = self.presented.clone();
        let basis = self.basis;
        window.record_prepaint_window_transaction(
            self.publication,
            move |_, _, _| {
                *committed.borrow_mut() = visible.map(|bounds| ControlGeometry { bounds, basis });
            },
            move |_, _, _| {
                discarded.borrow_mut().take();
            },
        );
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

pub struct ViewsPanel {
    commands: Rc<RefCell<VecDeque<ViewsIntent>>>,
    candidate: Rc<RefCell<Vec<Rc<ViewSnapshot>>>>,
    publications: Rc<RefCell<VecDeque<ViewPublication>>>,
    images: [Option<(Rc<EngineViewImage>, ImageTexture)>; 2],
    current: Option<GraphPreviewBasis>,
    status: String,
    relocated: bool,
    window_id: WindowId,
    controls: [ControlSlot; 10],
    views: [ViewSlot; 2],
}

impl ViewsPanel {
    pub fn new(
        commands: Rc<RefCell<VecDeque<ViewsIntent>>>,
        candidate: Rc<RefCell<Vec<Rc<ViewSnapshot>>>>,
        publications: Rc<RefCell<VecDeque<ViewPublication>>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Self {
        Self {
            commands,
            candidate,
            publications,
            images: [None, None],
            current: None,
            status: String::new(),
            relocated: false,
            window_id: window.window_handle().window_id(),
            controls: std::array::from_fn(|_| ControlSlot::default()),
            views: std::array::from_fn(|_| ViewSlot::default()),
        }
    }

    pub fn synchronize(
        &mut self,
        images: [Option<(Rc<EngineViewImage>, ImageTexture)>; 2],
        current: Option<GraphPreviewBasis>,
        status: String,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .images
            .iter()
            .zip(&images)
            .any(|(old, new)| match (old, new) {
                (Some((old, _)), Some((new, _))) => !Rc::ptr_eq(old, new),
                (None, None) => false,
                _ => true,
            })
            || self.current != current
            || self.status != status;
        self.images = images;
        self.current = current;
        self.status = status;
        if changed {
            cx.notify();
        }
    }

    pub fn hit_center(&self, control: ViewControl) -> Option<Point<Pixels>> {
        self.controls[control.index()]
            .bounds
            .borrow()
            .as_ref()
            .map(|geometry| geometry.bounds.center())
    }

    pub fn presents_basis(&self, basis: Option<GraphPreviewBasis>) -> bool {
        self.controls[ViewControl::Edit.index()]
            .bounds
            .borrow()
            .as_ref()
            .is_some_and(|geometry| geometry.basis == basis)
    }

    pub fn presented_view(&self, key: ViewKey) -> Option<Rc<ViewSnapshot>> {
        self.views[key.index()].snapshot.borrow().clone()
    }

    pub fn relocated(&self) -> bool {
        self.relocated
    }

    fn activate(
        &mut self,
        control: ViewControl,
        basis: Option<GraphPreviewBasis>,
        cx: &mut Context<Self>,
    ) {
        let intent = match control {
            ViewControl::Edit => basis.map(ViewsIntent::Edit),
            ViewControl::Compile => basis.map(ViewsIntent::Compile),
            ViewControl::HoldNext => Some(ViewsIntent::HoldNext),
            ViewControl::ReleaseHeld => Some(ViewsIntent::ReleaseHeld),
            ViewControl::RelocateA => {
                self.relocated = !self.relocated;
                cx.notify();
                Some(ViewsIntent::RelocateA)
            }
            ViewControl::PulseB => Some(ViewsIntent::PulseB),
            ViewControl::CloseA => Some(ViewsIntent::CloseA),
            ViewControl::Save => basis.map(ViewsIntent::Save),
            ViewControl::Reopen => basis.map(ViewsIntent::Reopen),
            ViewControl::CloseGraph => basis.map(ViewsIntent::CloseGraph),
        };
        if let Some(intent) = intent {
            self.commands.borrow_mut().push_back(intent);
        }
    }

    fn button(&self, control: ViewControl, cx: &Context<Self>) -> RecordedControl {
        let index = control.index();
        let basis = self.current;
        let child = div()
            .id(("views-control", index))
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(control.label())
            .absolute()
            .left(px(24. + (index % 5) as f32 * 232.))
            .top(px(24. + (index / 5) as f32 * 44.))
            .w(px(220.))
            .h(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(0x456078))
            .rounded_sm()
            .bg(rgb(0x2d4258))
            .focus_visible(|style| style.border_color(rgb(0x9bd7ff)))
            .cursor_pointer()
            .overflow_hidden()
            .child(control.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.activate(control, basis, cx);
            }));
        RecordedControl {
            child: child.into_any_element(),
            publication: self.controls[index].publication,
            presented: self.controls[index].bounds.clone(),
            basis,
            window_id: self.window_id,
        }
    }

    fn image_canvas(
        &self,
        key: ViewKey,
        image: Rc<EngineViewImage>,
        texture: ImageTexture,
    ) -> impl IntoElement + use<> {
        let publication = self.views[key.index()].publication;
        let presented = self.views[key.index()].snapshot.clone();
        let publications = self.publications.clone();
        let candidate = self.candidate.clone();
        let window_id = self.window_id;
        canvas(
            move |bounds, window, _| {
                if window.window_handle().window_id() != window_id {
                    return None;
                }
                let geometry = window.try_element_geometry(bounds).ok()?;
                if geometry
                    .displayed_bounds()
                    .intersect(&window.clip_bounds())
                    .is_empty()
                {
                    return None;
                }
                let mapping = DisplayMapping2d::fit(
                    image.display.output().extent(),
                    DisplayRect {
                        min: [f32::from(bounds.origin.x), f32::from(bounds.origin.y)],
                        max: [
                            f32::from(bounds.origin.x + bounds.size.width),
                            f32::from(bounds.origin.y + bounds.size.height),
                        ],
                    },
                    window.scale_factor(),
                    DisplayScale::Fit,
                )?;
                let snapshot = Rc::new(ViewSnapshot {
                    image,
                    mapping,
                    presented: RefCell::new(None),
                });
                let accepted = snapshot.clone();
                let committed = presented.clone();
                let discarded = presented.clone();
                window.record_prepaint_window_transaction(
                    publication,
                    move |fence, _, _| {
                        *committed.borrow_mut() = Some(accepted.clone());
                        publications.borrow_mut().push_back(ViewPublication {
                            generation: fence.generation(),
                            snapshot: accepted.clone(),
                        });
                    },
                    move |_, _, _| {
                        discarded.borrow_mut().take();
                    },
                );
                Some(snapshot)
            },
            move |_, snapshot, window, _| {
                let Some(snapshot) = snapshot else {
                    return;
                };
                let content = snapshot.mapping.content_rect();
                let bounds = Bounds::new(
                    point(px(content.min[0]), px(content.min[1])),
                    size(
                        px(content.max[0] - content.min[0]),
                        px(content.max[1] - content.min[1]),
                    ),
                );
                window
                    .paint_image_texture(bounds, Corners::all(px(0.)), &texture)
                    .expect("engine view output remains in its original atlas");
                candidate.borrow_mut().push(snapshot);
            },
        )
        .size_full()
    }

    fn view(&self, key: ViewKey) -> impl IntoElement + use<> {
        let (left, top, width, height) = match key {
            ViewKey::A if self.relocated => (24., 424., 256., 144.),
            ViewKey::A => (24., 160., 576., 324.),
            ViewKey::B => (648., 160., 576., 324.),
        };
        let name = match key {
            ViewKey::A => "View A",
            ViewKey::B => "View B",
        };
        let image = self.images[key.index()].clone();
        let label = match &image {
            Some((image, _)) if self.current == Some(image.basis) => {
                format!("{name} · current source")
            }
            Some(_) => format!("{name} · passive last-good source"),
            None => format!("{name} · no engine image"),
        };
        let presented = self.views[key.index()].snapshot.clone();
        let commands = self.commands.clone();
        let mut panel = div()
            .id(("engine-graph-view", key.index()))
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x0e151f))
            .on_mouse_down(MouseButton::Left, move |event, _, _| {
                let Some(snapshot) = presented.borrow().clone() else {
                    return;
                };
                let position = event.window_event().position;
                commands.borrow_mut().push_back(ViewsIntent::Pick {
                    snapshot,
                    point: [f32::from(position.x), f32::from(position.y)],
                });
            });
        if let Some((image, texture)) = image {
            panel = panel.child(self.image_canvas(key, image, texture));
        }
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(width))
            .h(px(height))
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(-28.))
                    .h(px(28.))
                    .text_color(rgb(0xffffff))
                    .child(label),
            )
            .child(panel)
    }
}

impl Render for ViewsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.candidate.borrow_mut().clear();
        div()
            .id("engine-views-panel")
            .size_full()
            .relative()
            .bg(rgb(0x17212e))
            .font_family("Segoe UI")
            .text_size(px(16.))
            .text_color(rgb(0xf0f3f8))
            .children(
                ViewControl::ALL
                    .into_iter()
                    .map(|control| self.button(control, cx)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(112.))
                    .child(self.status.clone()),
            )
            .child(self.view(ViewKey::A))
            .child(self.view(ViewKey::B))
    }
}
