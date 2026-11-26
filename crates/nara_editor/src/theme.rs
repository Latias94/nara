use open_gpui::{Div, StyleRefinement, Styled, div, px, rgb};
use open_gpui_base::Button;
use open_gpui_docking::{DockspaceColors, GpuiDockspaceConfig};

pub const CANVAS: u32 = 0x171a1f;
pub const PANEL: u32 = 0x1d2128;
pub const RAISED: u32 = 0x252b34;
pub const INPUT: u32 = 0x14171b;
pub const HOVER: u32 = 0x2a323c;
pub const BORDER: u32 = 0x303741;
pub const TEXT: u32 = 0xe6eaf0;
pub const MUTED: u32 = 0xa4adb9;
pub const ACCENT: u32 = 0x5ec8bf;
pub const SELECTED: u32 = 0x264442;
pub const ERROR: u32 = 0xf08f95;
pub const WARNING: u32 = 0xe4bf7b;

pub fn panel() -> Div {
    div()
        .size_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .bg(rgb(PANEL))
        .text_color(rgb(TEXT))
        .text_size(px(13.0))
        .line_height(px(20.0))
}

pub fn toolbar() -> Div {
    div()
        .h(px(34.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .bg(rgb(RAISED))
        .border_b_1()
        .border_color(rgb(BORDER))
}

pub fn button(id: &'static str, label: &str) -> Button {
    Button::new(id, label.to_owned())
        .h(px(26.0))
        .px_2()
        .rounded(px(4.0))
        .text_size(px(12.0))
        .bg(rgb(HOVER))
}

pub fn input() -> StyleRefinement {
    StyleRefinement::default()
        .bg(rgb(INPUT))
        .text_color(rgb(TEXT))
        .text_size(px(13.0))
        .h(px(28.0))
        .px_2()
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(BORDER))
}

pub fn docking() -> GpuiDockspaceConfig {
    let mut config = GpuiDockspaceConfig::default();
    config.colors = DockspaceColors {
        background: rgb(CANVAS).into(),
        pane: rgb(PANEL).into(),
        tab_bar: rgb(CANVAS).into(),
        tab: rgb(CANVAS).into(),
        selected_tab: rgb(RAISED).into(),
        hovered_tab: rgb(HOVER).into(),
        text: rgb(TEXT).into(),
        muted_text: rgb(MUTED).into(),
        border: rgb(BORDER).into(),
        splitter: rgb(BORDER).into(),
        contained_title: rgb(RAISED).into(),
        accent: rgb(ACCENT).into(),
        ..config.colors
    };
    config
}
