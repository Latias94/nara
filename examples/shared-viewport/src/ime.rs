use std::ops::Range;

use open_gpui::{
    Bounds, Context, ElementInputHandler, EntityInputHandler, FocusHandle, KeyDownEvent,
    MouseButton, Pixels, Point, UTF16Selection, Window, canvas, div, prelude::*, px, rgb,
};

pub(crate) struct ImeField {
    focus: FocusHandle,
    text: String,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
}

impl ImeField {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            text: String::new(),
            selection: 0..0,
            marked: None,
        }
    }

    fn byte_offset(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (byte, character) in self.text.char_indices() {
            if units >= utf16 {
                return byte;
            }
            units += character.len_utf16();
        }
        self.text.len()
    }

    fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> Range<usize> {
        let range = range
            .or_else(|| self.marked.take())
            .unwrap_or_else(|| self.selection.clone());
        let start = self.byte_offset(range.start);
        let end = self.byte_offset(range.end).max(start);
        let start_utf16 = self.text[..start].encode_utf16().count();
        self.text.replace_range(start..end, text);
        let end_utf16 = start_utf16 + text.encode_utf16().count();
        self.selection = end_utf16..end_utf16;
        start_utf16..end_utf16
    }

    fn backspace(&mut self) {
        let mut range = self.selection.clone();
        if range.is_empty() {
            let before = &self.text[..self.byte_offset(range.start)];
            let Some(character) = before.chars().next_back() else {
                return;
            };
            range.start = range.start.saturating_sub(character.len_utf16());
        }
        self.replace(Some(range), "");
    }
}

impl Render for ImeField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let entity = cx.entity();
        let border = if self.marked.is_some() {
            0xedb45b
        } else if self.focus.is_focused(window) {
            0x89bbff
        } else {
            0x596b86
        };
        let text = if self.text.is_empty() {
            "Click and type with a CJK IME".to_owned()
        } else {
            self.text.clone()
        };
        div()
            .id("scene-name")
            .relative()
            .min_h(px(40.0))
            .p_2()
            .border_1()
            .border_color(rgb(border))
            .rounded_sm()
            .bg(rgb(0x25344c))
            .overflow_hidden()
            .track_focus(&self.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|field, _, window, cx| {
                    field.focus.focus(window, cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|field, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "backspace" && field.marked.is_none() {
                    field.backspace();
                    window.notify_text_input_changed(&field.focus);
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(text)
    }
}

impl EntityInputHandler for ImeField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let start = self.byte_offset(range.start);
        let end = self.byte_offset(range.end).max(start);
        *adjusted = Some(
            self.text[..start].encode_utf16().count()..self.text[..end].encode_utf16().count(),
        );
        Some(self.text[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.selection.clone(),
            reversed: false,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let length = self.text.encode_utf16().count();
        let start = range.start.min(length);
        self.selection = start..range.end.clamp(start, length);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.text.encode_utf16().count())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone()
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text);
        self.marked = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let marked = self.replace(range, text);
        if let Some(selection) = selection {
            let start = marked.start + selection.start.min(marked.len());
            let end = marked.start + selection.end.min(marked.len());
            self.selection = start..end.max(start);
        }
        self.marked = (!marked.is_empty()).then_some(marked);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
