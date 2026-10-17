# Archived Dear WGPU 0.20 Patch

This is historical provenance for the completed 0.20 Editor qualification. The 0.21 upgrade
removes the renderer fork and uses published crates. This note grants no current execution or
platform authority; use the implementation ledger and current native qualification procedure.

The former `vendor/dear-imgui-wgpu` directory was based on the released 0.20.0 package at upstream
`af2942c78651b280d18af621de55b5462bb1e10a`, under MIT/Apache-2.0. Its Nara delta added actual
TextureView sampling reports and paired secondary surface/presentation observations, checked
identity counters and native Winit window identity. The private Nara bridge joined these reports
to its own current image receipts before primary acquisition.

The last pre-upgrade Nara revision is `257dac0`. Retrieve the original provenance and complete
patch from Git when reproducing that qualification:

```text
git show 257dac0:vendor/dear-imgui-wgpu/NARA_PATCH.md
git ls-tree -r 257dac0 vendor/dear-imgui-wgpu
```

The 0.20 report did not prove GPU completion or scanout. Its exact surface lifetime/configuration
fields are not equivalent to 0.21 native registration identity. The current adapter uses explicit
delegated native-target receipts and upstream external binding revisions, while Nara retains
its own actual registration, device lineage and image-content receipt checks.

Historical package-specific test commands and the former workspace exclusion apply only to that
revision. Current verification is described in
[Native Editor Qualification](../../verification/native-editor-qualification.md).
