use crate::UiLayer;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RootOrder {
    stacking_path: Vec<(i32, usize)>,
    fragment_index: u8,
}

impl RootOrder {
    pub(super) fn new(stacking_path: Vec<(i32, usize)>, fragment_index: u8) -> Self {
        Self {
            stacking_path,
            fragment_index,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RootOrdered<T> {
    pub(super) order: RootOrder,
    pub(super) value: T,
}

impl<T> RootOrdered<T> {
    pub(super) const fn new(order: RootOrder, value: T) -> Self {
        Self { order, value }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FrameOrder {
    layer: UiLayer,
    root_ordinal: usize,
    root: RootOrder,
}

impl FrameOrder {
    pub(super) fn bind(layer: UiLayer, root_ordinal: usize, root: &RootOrder) -> Self {
        Self {
            layer,
            root_ordinal,
            root: root.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FrameOrdered<T> {
    pub(super) order: FrameOrder,
    pub(super) value: T,
}

impl<T> FrameOrdered<T> {
    pub(super) const fn new(order: FrameOrder, value: T) -> Self {
        Self { order, value }
    }
}
