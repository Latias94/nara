# U6 native text, clipboard and accessibility baseline

Date: 2026-10-16. Scope: source-grounded comparison for the U6 adoption gate,
not a native qualification receipt or permission to start U7.

## Findings

The current native egui Editor does **not** initialize or publish an AccessKit
widget tree to Windows. Its headless widget tests do generate AccessKit trees;
those are a separate capability. Dear ImGui's direct Winit/WGPU route therefore
does not remove an already integrated Editor AccessKit bridge. This finding does
not establish accessibility completion for either frontend, and does not apply
to Nara's independently integrated game/runtime UI. [S1, S2, S3, S9]

The text comparison has a real source-level difference: egui handles and paints
preedit text and its active range, whereas Dear Winit 0.20.0 discards preedit
events. Winit 0.30.13 on Windows also suppresses the default OS composition
window. Consequently the Dear backend comment that composition is handled by
the OS is insufficient for this route. Committed Unicode and candidate-area
forwarding exist, but neither establishes usable visible composition. [S2, S4,
S5]

Windows clipboard support is **implemented by default in the bundled C++ core**.
The absence of `Context::set_clipboard_backend` is not evidence of a missing OS
clipboard. Its actual Nara copy/cut/paste behavior still needs a native,
bidirectional check. This research performed no Cargo command, launched no
native application, and changed no personal Windows input or clipboard settings.
[S6, S7]

## Exact source basis

- Nara source was inspected at `bb6314efb34b4bf0665812306694367bed8e0599`
  with concurrent, uncommitted U6 qualification files present. This note describes
  the inspected ordinary egui Host; qualifications added concurrently require
  their own execution evidence.
- Nara selects egui/egui-winit 0.36.2 and Winit 0.30.13. The corresponding
  downloaded crate source was read directly; a lockfile dependency name was not
  used to infer integration. [S1, S2]
- Dear source authority is tag `v0.20.0`, commit
  `af2942c78651b280d18af621de55b5462bb1e10a`. The reference checkout is
  `v0.20.0-2-g391171d1`; its post-tag diff contains build tooling and documentation
  changes, not the core/Winit/WGPU runtime files discussed here. The tag's
  `dear-imgui-sys/third-party/cimgui` gitlink is
  `125f397e0982fe9466eb0a162de547be5efec849`. [S6, S7]
- The downloaded `dear-imgui-sys` 0.20.0 package supplies the bundled C++ source
  inspected here; `imgui.h:32-33` identifies Dear ImGui `1.92.9b` / `19291`.
  The local reference checkout's C++ submodules are not populated, so this note
  does not pretend that the empty reference directory supplied that source.

## Current native egui Editor

| Capability | Actually wired in the ordinary Editor | Evidence boundary |
| --- | --- | --- |
| Native AccessKit adapter | No | `State::new` starts with no adapter, and the Editor never calls `init_accesskit`. |
| egui AccessKit tree generation | No native activation path found | Default `Context` is used; no `enable_accesskit` call or AccessKit user-event handling exists in the Editor. |
| Test widget tree | Yes | `egui_kittest::Harness` explicitly enables AccessKit; this does not initialize a native adapter. |
| IME preedit and commit delivery | Yes, while Editor input owns the event | Host calls `on_window_event`; egui-winit maps both preedit and commit into egui events. |
| Preedit drawing and active range | Implemented by the selected TextEdit | TextEdit replaces transient composition text, tracks the composition range, and paints its visuals. |
| Candidate-area output | Wired | Host handles platform output; egui-winit calls `set_ime_allowed` and `set_ime_cursor_area`. Exact native placement remains unverified here. |
| OS text clipboard | Default Windows route is wired | egui-winit initializes its clipboard and processes copy output/paste keys; initialization or OS contention can still fail. |
| Configured CJK font | None found in the ordinary Host | The Host uses the default egui Context and does not install a CJK face. This is a font-coverage limit, separate from retaining Unicode in a string. |

The Host creates `egui_winit::State` in
`editor/src/platform.rs:2544-2570`, forwards ordinary-window input at
`:772-784`, and applies platform output at `:1097-1100` and `:1583-1586`.
Its `EditorUserEvent` has only `SupervisedPlayWake` at `:301-304`.
The source search covered `editor/src` and the used tooling-egui adapter;
no `init_accesskit`, `enable_accesskit`, native AccessKit event conversion or
action handler was found. [S1]

This is stronger evidence than a package graph: egui-winit's `accesskit` Cargo
feature is not in its default feature list; even if another consumer enables
that feature through unification, `State::new` still initializes `accesskit:
None`. `init_accesskit` installs the adapter, and platform output only publishes
when both an adapter and a generated tree update are present. egui generates
the tree only after its own `enable_accesskit` activation. [S2, S3]

The existing `editor/tests/authoring_loop.rs` and `first_game_workflow.rs` use
`egui_kittest`/AccessKit roles to locate widgets. Harness construction explicitly
calls `ctx.enable_accesskit()`. These tests prove test-tree querying and widget
semantics, not a Windows UI Automation provider. [S3]

egui-winit translates UTF-8 byte ranges from Winit preedit events to character
ranges. TextEdit inserts transient preedit into its editing buffer, draws the
composition/active-range visuals, and clears that text before inserting the
committed result. Its IME output includes both widget and cursor rectangles;
the selected egui-winit implementation forwards `ime.rect` to Winit, so this
research does not claim exact caret-aligned native candidate placement merely
because a `cursor_rect` field exists. [S2, S3]

No native Chinese-composition or clipboard acceptance for the Editor was found
in the inspected verification records. The U5 native record explicitly excludes
IME and accessibility acceptance. Existing native authoring evidence must not
be relabeled as a text-input qualification. [S10]

## Dear ImGui 0.20.0 direct route

### Composition and caret

`events.rs:142-162` ignores `Ime::Preedit`, queues non-control characters from
`Ime::Commit`, and does not maintain composition or active-range state.
`handle_keyboard_input` also forwards pressed-key `KeyEvent.text`; real IME
qualification must detect duplicate insertion or leaked confirmation keys,
rather than testing only `Io::add_input_character`. [S4]

`platform/window_state.rs` installs `Platform_SetImeDataFn`, resolves the owning
viewport/window, converts the ImGui caret position relative to that viewport,
and calls `Window::set_ime_cursor_area`. Automatic IME management enables owned
windows while `io.want_text_input()` is true. Those are useful implemented
paths, but the callback is output to the IME, not input of the preedit string or
its composition cursor. [S4]

The Windows Winit 0.30.13 handler converts `WM_IME_COMPOSITION` into preedit and
commit events and consumes the message. In `WM_IME_SETCONTEXT` it removes
`ISC_SHOWUICOMPOSITIONWINDOW` before calling `DefWindowProcW`. Therefore a claim
that ignoring preedit is safe because Windows will draw its default composition
window is contradicted by the selected Winit route. Whether an individual IME
still draws useful candidate UI is an independent native observation; candidate
UI does not replace an editable preedit range. [S5]

This is a concrete capability gap to resolve or explicitly disposition before
adoption. A bounded adapter/vendor correction may keep transient composition
and its character-range caret in the active window and render it near the
reported insertion position. It must not insert preedit into the committed
document value, double-insert on commit, or survive cancel/focus loss/retirement.
Do not expand this into a general text-edit or accessibility framework. The U6
gate requires a real IME edit and a concrete preedit/caret disposition; source
support alone does not complete that gate. [S11]

The current qualification Host initially installs only
`FontSource::default_font_with_size(16.0)` in
`editor/src/platform/imgui.rs`. A suitable trusted CJK face must be supplied to
qualify visible Chinese glyphs. Upstream's `ime_debug` example deliberately
merges an optional Noto Sans SC font and otherwise retains its Latin default.
The ordinary egui Host also has no configured CJK face, so record pre-existing
font limits separately from a frontend regression. [S1, S8]

### Clipboard defaults

The safe Rust `Context` owns a dummy Rust clipboard object initially, but its
constructor does not install that object as the C++ clipboard callbacks.
`Context::set_clipboard_backend` is the explicit override. The C++ context
initialization already assigns its default callbacks. On Windows, unless
`IMGUI_DISABLE_WIN32_FUNCTIONS` or
`IMGUI_DISABLE_WIN32_DEFAULT_CLIPBOARD_FUNCTIONS` is defined, these callbacks
open the native clipboard, exchange `CF_UNICODETEXT`, and convert UTF-8/UTF-16.
The disabling options are commented out in the bundled `imconfig.h`; the sys
build script also explicitly links the relevant Windows system libraries.
No default-clipboard-disabling build definition was found in the inspected
0.20.0 route. [S6, S7]

The safe helper documentation describing no configured backend as `None`/a
no-op is consequently too broad for the native Windows core defaults. Do not
add an OS clipboard dependency merely to satisfy that description. Conversely,
the default implementation returns early if `OpenClipboard` fails, and cannot
by itself prove a successful product copy/cut/paste. The upstream
`clipboard_and_ini` tests install an in-memory `TestClipboardBackend`; they
prove callback routing, not native interoperability. [S6, S7]

### Accessibility

The bounded source search found no OS widget accessibility-tree bridge in the
pinned core/Winit/WGPU direct route. `NAV_ENABLE_KEYBOARD` is keyboard
navigation, not screen-reader publication. The U6 disposition should therefore
state: no integrated native Editor widget tree in either inspected route;
complete accessibility remains deferred by the plan. Preserve actual keyboard
usability as its own mandatory capability. [S1, S4, S11]

`reference-game/src/bin/desktop_accessibility_probe.rs` exercises a different
consumer: the Nara game/runtime UI. It binds Windows UI Automation to an exact
process/HWND, invokes an exposed Retry button, and checks runtime journal and
successor presentation evidence. It is useful precedent for an exact-window
native observation, but is not an Editor egui baseline and must not be reused
as proof of an Editor provider. [S9]

## Bounded Windows qualification

Run the ordinary egui Editor and Dear qualification with the same disposable
project and one string field. Capture the exact source/binary, Windows version,
already installed IME identity, input mode, monitor scale, field/window and
result. Use existing user configuration; do not install an IME or alter global
language, registry, Narrator or personal settings to produce a passing result.

1. **Committed Unicode and editing:** enter a short mixed Latin/Chinese string;
   move Home/End/Left/Right, select with Shift, delete, undo/redo, commit through
   the ordinary Inspector action, Save and Reopen. Inspect the saved UTF-8 value
   as well as the visible glyphs. Exercise Tab/Shift-Tab between the field and
   other controls, and verify shortcuts/game input do not steal field input.
2. **Real IME composition:** activate an already installed Chinese IME for the
   test window and type its normal physical Latin key sequence. Before commit,
   observe candidate UI, visible preedit, its caret/active range and composition
   updates. Move inside composition, cancel with Escape, begin again and commit
   a candidate. Confirm exactly one final insertion and no committed preedit.
   Repeat focus transfer to another field/window and a detached field, then
   close the owning secondary window. A short recording or paired screenshots
   plus temporary, content-bounded event observations can identify where the
   behavior diverges.
3. **Native clipboard:** copy a known mixed Unicode sample from another native
   text application into each Editor field with Ctrl+V. Select and Ctrl+C/Ctrl+X
   from each Editor and paste back into that external application. Verify exact
   text and cut semantics, and repeat in a detached Dear field. An internal
   set/get roundtrip alone cannot distinguish an application-private fallback
   from the real OS clipboard. Use disposable text, avoiding personal clipboard
   data in evidence.
4. **Candidate position:** while composing, move/resize the owning window and
   change the field's position. On available mixed-DPI monitors, repeat after a
   native move. Record what was actually available; a single-scale monitor does
   not qualify mixed-DPI behavior. Candidate UI must follow the active owning
   window and remain usable.
5. **Accessibility baseline observation:** optionally query the exact ordinary
   Editor HWND with the existing Windows UI Automation client pattern. Record
   native frame/nonclient controls separately from application widgets. An OS
   title bar is not a published Inspector tree. No new accessibility framework
   or broad test fixture is needed to record the current absence.

Synthetic `Event::Text`, `Ime::Commit`, `Io::add_input_character`,
`KEYEVENTF_UNICODE`, direct buffer assignment, or clipboard paste can establish
Unicode insertion and editing. They cannot establish real IME composition,
candidate placement, cancel/commit sequencing or preedit cursor behavior.
Similarly, injecting an artificial `Ime::Preedit` is only a bounded adapter
check. If the machine lacks a usable installed Chinese IME or the interaction
cannot be performed, record **native IME unverified** and keep the dependent
adoption claim pending. Do not report a synthetic Unicode pass as the required
Chinese IME pass.

At this source-only checkpoint, the preedit-drop/Windows composition-window
combination is the concrete implementation gap; CJK font provisioning is a
separate rendering prerequisite. Clipboard, committed-text correctness, keyboard
navigation, candidate position and actual native focus transitions are
unverified qualification items, not confirmed runtime failures. Missing native
AccessKit publication is the actual pre-existing Editor capability limit, not a
newly discovered Dear-only regression. [S11]

## Primary sources

Line references are one-based in the inspected source versions. Downloaded
crate sources are under Cargo's `registry/src/.../<crate>-<version>/` directory;
these are source provenance, not dependencies inferred from Cargo.lock.

- **S1 — Nara ordinary Editor and selected dependency declarations:**
  `Cargo.toml:103-106`; `editor/Cargo.toml`; `editor/src/platform.rs:275-304,
  437-439,772-784,1097-1100,1583-1586,2544-2570`;
  `editor/src/platform/imgui.rs:249-267`; `editor/tests/authoring_loop.rs:17-18`;
  `editor/tests/first_game_workflow.rs`. Read at the Nara snapshot above.
- **S2 — egui-winit 0.36.2:** `src/lib.rs:138-210,717-763,1104-1201`;
  `src/clipboard.rs:3-6,30-53,56-120,147-161`; `Cargo.toml:55-79`.
  [Published State API](https://docs.rs/egui-winit/0.36.2/egui_winit/struct.State.html)
  and [exact published source](https://docs.rs/crate/egui-winit/0.36.2/source/src/lib.rs).
- **S3 — egui and egui_kittest 0.36.2:** egui
  `src/context.rs:412,520-530,3700-3707`;
  `src/widgets/text_edit/builder.rs:907-961,1287-1333,1353-1377`;
  egui_kittest `src/lib.rs:144`;
  epaint_default_fonts 0.36.2 `src/lib.rs` lists Hack, Noto Emoji, Ubuntu Light
  and emoji-icon-font, rather than a supplied CJK text face.
  [Published TextEdit source](https://docs.rs/crate/egui/0.36.2/source/src/widgets/text_edit/builder.rs)
  and [Harness source](https://docs.rs/crate/egui_kittest/0.36.2/source/src/lib.rs).
- **S4 — exact Dear Winit 0.20.0:**
  [events.rs](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-winit/src/events.rs#L142),
  `:17-39,142-162`;
  [platform/window_state.rs](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-winit/src/platform/window_state.rs#L18),
  `:18-85,147,192-193,225-254,286-293`.
- **S5 — Winit 0.30.13 native Windows implementation:**
  [event_loop.rs](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/windows/event_loop.rs#L1530),
  `:1530-1578,1614-1617`;
  [ime.rs](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/windows/ime.rs#L113),
  `:113-138` positions candidate/composition windows.
- **S6 — exact Dear core wrapper 0.20.0:**
  `dear-imgui/src/context/core.rs:230-262`;
  [context/clipboard.rs](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-imgui/src/context/clipboard.rs),
  `:8-70`; `dear-imgui/tests/clipboard_and_ini.rs:14-43`;
  [sys build.rs](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-imgui-sys/build.rs#L411),
  `:411-420`.
- **S7 — bundled C++ in dear-imgui-sys 0.20.0:**
  [published imgui.cpp](https://docs.rs/crate/dear-imgui-sys/0.20.0/source/third-party/cimgui/imgui/imgui.cpp),
  `:4566-4570,21913-21963`; adjacent `imconfig.h:41-45` and `imgui.h:32-33`.
- **S8 — exact Dear IME example:**
  [ime_debug.rs](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/examples/04-integration/ime_debug.rs#L28),
  `:28-90`, optional trusted CJK merge and default-only fallback.
- **S9 — existing runtime native probe:**
  `reference-game/src/bin/desktop_accessibility_probe.rs:10-15,51-69` and
  `desktop_accessibility_probe/platform.rs:55-98`. This runs through
  `desktop_run_with_probe`, not the standalone Editor Host.
- **S10 — prior native evidence:**
  `docs/knowledge/engineering/verification/2026-10/2026-10-16T094118Z-editor-u5-independent-scene-and-game-views-pass-native-concurrent-presentation-close-reopen-and-paused-refocus-ad10a59ba57e4fe7b45a5176d3311a33.md:29`.
- **S11 — owning adoption requirement:**
  `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md:91,
  207-209,312-331`, under Accepted ADR 0118. U6 explicitly separates native
  evidence from source feasibility and defers complete accessibility.
