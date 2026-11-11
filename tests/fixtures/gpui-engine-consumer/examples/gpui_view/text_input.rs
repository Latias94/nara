//! A local single-line draft field using GPUI's public native text-input contract.
//!
//! Adapted from the open-gpui input examples. Text has no field-level length limit;
//! line breaks become spaces and long lines scroll to keep the active caret visible.

use std::{ops::Range, rc::Rc};

use open_gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, PointerCaptureHandle, PrepaintPublicationId, ShapedLine, SharedString,
    TargetedEvent, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, actions, canvas,
    div, fill, point, prelude::*, px, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

actions!(
    nara_draft_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

const KEY_CONTEXT: &str = "NaraDraftInput";

pub(super) struct DraftInput {
    focus_handle: FocusHandle,
    pointer_capture: PointerCaptureHandle,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    composition: Option<(SharedString, Range<usize>, bool)>,
    layout: Option<Rc<DraftLayout>>,
    publication: PrepaintPublicationId,
}

impl DraftInput {
    /// Creates an empty field in the caller's existing window without taking focus.
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();
        cx.on_blur(&focus_handle, window, |this, window, cx| {
            this.release_pointer(window);
            this.cancel_composition();
            cx.notify();
        })
        .detach();

        Self {
            focus_handle,
            pointer_capture: window.new_pointer_capture_handle(),
            content: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            composition: None,
            layout: None,
            publication: PrepaintPublicationId::new(),
        }
    }

    /// Registers editing actions once during application initialization.
    pub(super) fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
            KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
            KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
            KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
            KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
            KeyBinding::new("end", End, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-home", SelectHome, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-end", SelectEnd, Some(KEY_CONTEXT)),
        ]);

        #[cfg(target_os = "macos")]
        cx.bind_keys([
            KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-c", Copy, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-x", Cut, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-v", Paste, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-left", Home, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-right", End, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-left", SelectHome, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-right", SelectEnd, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some(KEY_CONTEXT)),
        ]);

        #[cfg(not(target_os = "macos"))]
        cx.bind_keys([
            KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-insert", Copy, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-insert", Paste, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-delete", Cut, Some(KEY_CONTEXT)),
        ]);
    }

    /// Returns the current UTF-8 draft, including any in-progress IME composition.
    pub(super) fn text(&self) -> &str {
        self.content.as_ref()
    }

    /// Distinguishes committed text from an in-progress IME composition for smoke checks.
    pub(super) fn is_composing(&self) -> bool {
        self.marked_range.is_some()
    }

    /// Rebinds an owned field after its selected source value changes.
    pub(super) fn rebind(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.composition = None;
        self.layout = None;
        window.notify_text_input_changed(&self.focus_handle);
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, window, cx);
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.notify_selection(window, cx);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), window, cx);
    }

    fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, window, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(previous, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(next, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &TargetedEvent<MouseDownEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(position) = event.target_layout_position() else {
            return;
        };
        self.focus_handle.focus(window, cx);
        window.request_virtual_keyboard();
        if let Err(error) = window.capture_pointer(&self.pointer_capture, MouseButton::Left) {
            eprintln!("draft input pointer capture failed: {error}");
        }
        let Some(offset) = self.index_for_position(position) else {
            return;
        };
        if event.window_event().modifiers.shift {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
    }

    fn on_mouse_up(
        &mut self,
        _: &TargetedEvent<MouseUpEvent>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.release_pointer(window);
    }

    fn on_mouse_move(
        &mut self,
        event: &TargetedEvent<MouseMoveEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pointer_capture.is_captured(window)
            && let Ok(position) = event.target_layout_position()
            && let Some(offset) = self.index_for_position(position)
        {
            self.select_to(offset, window, cx);
        }
    }

    fn release_pointer(&self, window: &mut Window) {
        if let Err(error) = window.release_pointer(&self.pointer_capture) {
            eprintln!("draft input pointer release failed: {error}");
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let read = cx.read_from_clipboard_async();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(item) = read.await? {
                let _ = cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        if this.focus_handle.is_focused(window)
                            && window.is_focus_handle_rendered(&this.focus_handle)
                        {
                            EntityInputHandler::paste(this, item, window, cx);
                        }
                    })
                });
            }
            Ok::<_, open_gpui::ClipboardReadError>(())
        })
        .detach_and_log_err(cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_owned(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.copy(&Copy, window, cx);
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.notify_selection(window, cx);
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        let anchor = if self.selection_reversed {
            self.selected_range.end
        } else {
            self.selected_range.start
        };
        self.selected_range = anchor.min(offset)..anchor.max(offset);
        self.selection_reversed = offset < anchor;
        self.notify_selection(window, cx);
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn current_layout(&self) -> Option<&DraftLayout> {
        self.layout
            .as_deref()
            .filter(|layout| layout.content == self.content)
    }

    fn index_for_position(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.current_layout()?;
        let index = layout
            .line
            .closest_index_for_x(position.x - layout.origin().x);
        Some(index.min(self.content.len()))
    }

    fn notify_selection(&self, window: &Window, cx: &mut Context<Self>) {
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace(&mut self, range_utf16: Option<Range<usize>>, text: &str) -> Range<usize> {
        let range = range_utf16
            .map(|range| byte_range(&self.content, range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let text = text.replace(['\r', '\n'], " ");
        let mut content = self.content.to_string();
        content.replace_range(range.clone(), &text);
        self.content = content.into();
        let inserted = range.start..range.start + text.len();
        self.selected_range = inserted.end..inserted.end;
        self.selection_reversed = false;
        self.marked_range = None;
        inserted
    }

    fn cancel_composition(&mut self) {
        if let Some((content, selection, reversed)) = self.composition.take() {
            self.content = content;
            self.selected_range = selection;
            self.selection_reversed = reversed;
            self.layout = None;
        }
        self.marked_range = None;
    }
}

fn byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut utf16 = 0;
    for (byte, character) in text.char_indices() {
        if utf16 >= utf16_offset {
            return byte;
        }
        utf16 += character.len_utf16();
    }
    text.len()
}

fn byte_range(text: &str, range_utf16: Range<usize>) -> Range<usize> {
    let start = byte_offset(text, range_utf16.start);
    start..byte_offset(text, range_utf16.end).max(start)
}

fn utf16_range(text: &str, range: Range<usize>) -> Range<usize> {
    text[..range.start].encode_utf16().count()..text[..range.end].encode_utf16().count()
}

impl EntityInputHandler for DraftInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = byte_range(&self.content, range_utf16);
        *adjusted_range = Some(utf16_range(&self.content, range.clone()));
        Some(self.content[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: utf16_range(&self.content, self.selected_range.clone()),
            reversed: self.selection_reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = byte_range(&self.content, range_utf16);
        self.selection_reversed = false;
        self.notify_selection(window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| utf16_range(&self.content, range.clone()))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composition.is_some() {
            self.cancel_composition();
            window.notify_text_input_changed(&self.focus_handle);
            self.notify_selection(window, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range_utf16, text);
        self.composition = None;
        window.notify_text_input_changed(&self.focus_handle);
        self.notify_selection(window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composition.is_none() {
            self.composition = Some((
                self.content.clone(),
                self.selected_range.clone(),
                self.selection_reversed,
            ));
        }
        let inserted = self.replace(range_utf16, text);
        if let Some(selected_range_utf16) = selected_range_utf16 {
            let relative = byte_range(&self.content[inserted.clone()], selected_range_utf16);
            self.selected_range = inserted.start + relative.start..inserted.start + relative.end;
        }
        self.marked_range = (!inserted.is_empty()).then_some(inserted);
        window.notify_text_input_changed(&self.focus_handle);
        self.notify_selection(window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.current_layout()?;
        Some(layout.range_bounds(byte_range(&self.content, range_utf16)))
    }

    fn selection_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.current_layout()
            .map(|layout| vec![layout.range_bounds(byte_range(&self.content, range_utf16))])
            .unwrap_or_default()
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.current_layout()?;
        if !layout.bounds.contains(&position) {
            return None;
        }
        let index = self.index_for_position(position)?;
        Some(self.content[..index].encode_utf16().count())
    }
}

struct DraftLayout {
    content: SharedString,
    line: ShapedLine,
    bounds: Bounds<Pixels>,
    scroll_x: Pixels,
}

impl DraftLayout {
    fn origin(&self) -> Point<Pixels> {
        point(self.bounds.left() - self.scroll_x, self.bounds.top())
    }

    fn range_bounds(&self, range: Range<usize>) -> Bounds<Pixels> {
        let left = self.origin().x + self.line.x_for_index(range.start);
        let right = self.origin().x + self.line.x_for_index(range.end);
        Bounds::new(
            point(left.min(right), self.bounds.top()),
            size((right - left).abs().max(px(2.0)), self.bounds.size.height),
        )
    }
}

impl Render for DraftInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.content.clone();
        let selected_range = self.selected_range.clone();
        let marked_range = self.marked_range.clone();
        let cursor = self.cursor_offset();
        let previous_scroll = self
            .layout
            .as_ref()
            .map_or(px(0.0), |layout| layout.scroll_x);
        let focus_handle = self.focus_handle.clone();
        let input = cx.entity();
        let committed_input = input.clone();
        let discarded_input = input.clone();
        let publication = self.publication;
        let text_element = canvas(
            move |bounds, window, _| {
                let style = window.text_style();
                let run = TextRun {
                    len: content.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let runs = if let Some(marked) = marked_range {
                    vec![
                        TextRun {
                            len: marked.start,
                            ..run.clone()
                        },
                        TextRun {
                            len: marked.len(),
                            underline: Some(UnderlineStyle {
                                color: Some(rgb(0xfbbf24).into()),
                                thickness: px(1.0),
                                wavy: false,
                            }),
                            ..run.clone()
                        },
                        TextRun {
                            len: content.len() - marked.end,
                            ..run
                        },
                    ]
                    .into_iter()
                    .filter(|run| run.len > 0)
                    .collect::<Vec<_>>()
                } else {
                    vec![run]
                };
                let line = window.text_system().shape_line(
                    content.clone(),
                    style.font_size.to_pixels(window.rem_size()),
                    &runs,
                    None,
                );
                let visible_width = (bounds.size.width - px(2.0)).max(px(0.0));
                let cursor_x = line.x_for_index(cursor);
                let max_scroll = (line.width() - visible_width).max(px(0.0));
                let mut scroll_x = previous_scroll.min(max_scroll).max(px(0.0));
                if cursor_x < scroll_x {
                    scroll_x = cursor_x;
                } else if cursor_x > scroll_x + visible_width {
                    scroll_x = cursor_x - visible_width;
                }
                let layout = Rc::new(DraftLayout {
                    content,
                    line,
                    bounds,
                    scroll_x,
                });
                let committed_layout = layout.clone();
                window.record_prepaint_window_transaction(
                    publication,
                    move |_, _, cx| {
                        committed_input.update(cx, |input, _| {
                            input.layout = Some(committed_layout.clone());
                        });
                    },
                    move |_, _, cx| {
                        discarded_input.update(cx, |input, _| input.layout = None);
                    },
                );
                layout
            },
            move |bounds, layout, window, cx| {
                window.handle_input(&focus_handle, ElementInputHandler::new(bounds, input), cx);
                if !selected_range.is_empty() {
                    window.paint_quad(fill(
                        layout.range_bounds(selected_range.clone()),
                        rgba(0x3b82f680),
                    ));
                }
                if let Err(error) = layout.line.paint(
                    layout.origin(),
                    bounds.size.height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                ) {
                    eprintln!("draft input text painting failed: {error}");
                }
                if focus_handle.is_focused(window) && selected_range.is_empty() {
                    window.paint_quad(fill(layout.range_bounds(selected_range), rgb(0x58a6ff)));
                }
            },
        )
        .size_full();

        div()
            .id("nara-draft-input")
            .w_full()
            .min_w(px(0.0))
            .flex_shrink_0()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x526680))
            .bg(rgb(0x253247))
            .text_color(rgb(0xf0f4fa))
            .text_size(px(18.0))
            .line_height(px(28.0))
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .tab_index(0)
            .track_pointer_capture(&self.pointer_capture)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(
                div()
                    .w_full()
                    .h(px(28.0))
                    .overflow_hidden()
                    .child(text_element),
            )
    }
}

impl Focusable for DraftInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
