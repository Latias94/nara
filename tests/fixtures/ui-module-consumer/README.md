# Independent UI module consumer

This small project exercises only the public `nara_text`, `nara_ui_kernel` and `nara_ui_widgets`
interfaces. Its tests shape and rasterize a label, compile an immutable status panel, and compile
a controlled button with focus and semantic actions. A fourth test constructs complete binary
persistent identities without Scene types: distinct contexts stay separate, rebuilding the same
root/local composition preserves keys, and both buttons retain focus and semantic actions. The
independent Image consumer compiles an opaque Product-shaped name and two provider-scoped external
tokens into a retained frame without importing Asset, ECS or renderer types. It creates no native
window or GPU surface and cannot manufacture presentation authority or persistent asset bindings.

The editable-text consumer creates a bounded field through ordinary public text/widget APIs,
cancels preedit without changing its committed buffer, rejects input from an earlier focus,
performs local Undo/Redo and compiles the exact committed semantic value. It does not admit native
focus or qualify an Inspector, OS input-method delivery or platform accessibility.

The font-reconfiguration consumers explicitly replace/reorder real font faces while keeping a
field's key, focus token, draft, selection, preedit and local history. They compile the resulting
fractionally scaled/clipped field, verify committed semantics and exact glyph/raster provenance,
then use its new geometry for point editing. Different resource limits refuse without replacing
the editor; matching limits permit a retry. Adding the fallback changes a missing intersection
glyph into supported output. These callers use public `UiTextField::focus`, not private journal
submission, so they qualify local field/compile consumption rather than presented input routing
or live native Editor font switching.

The separate Cargo workspace and lockfile keep this dependency graph independent of the engine
and the broader module-consumer project. Building alongside their ECS, Scene or renderer
dependencies would weaken this isolation check.

`keyed_editors` is an independently authored reusable row composite. Two fields share the
accessible label `Value` but retain state by parent/item/property identity. Reorder, insertion
and repeat composition preserve the exact draft, selection, preedit, focus token and local
history. The chosen filter policy evicts omitted fields; a restored row starts fresh and old
scoped input cannot reach same-position replacements. Binding/schema mismatch preserves the
previous membership until explicit replacement. The component retains no journal, GPU, document
owner or native input method. Local tests use public field focus; presented input routing is
separately exercised by the native output consumer importing this actual library.

Run from the repository root, using the shared `target` directory through `CARGO_TARGET_DIR`:

```text
cargo nextest run --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --locked --test-threads=1
```

Run Cargo commands serially. The [fixture guide](../README.md) describes the other consumer projects.
