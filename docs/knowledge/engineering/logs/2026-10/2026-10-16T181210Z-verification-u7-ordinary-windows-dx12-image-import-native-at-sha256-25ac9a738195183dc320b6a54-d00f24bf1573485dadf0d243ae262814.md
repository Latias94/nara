---
type: "Memory Event"
title: "Verification: U7 ordinary Windows DX12 image-import native at SHA256 25ac9a738195183dc320b6a54"
description: "U7 ordinary Windows DX12 image-import native at SHA256 25ac9a738195183dc320b6a545710aa1455f21f4bb514051cd6232ad769fa752 passes Browser filte"
timestamp: 2026-10-16T18:12:10Z
record_id: "d00f24bf1573485dadf0d243ae262814"
producer_id: "codex-root"
run_id: "editor-foundation-u7"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_commit: "0ee59e9"
event_kind: "Verification"
---

# Event

U7 ordinary Windows DX12 image-import native at SHA256 25ac9a738195183dc320b6a545710aa1455f21f4bb514051cd6232ad769fa752 passes Browser filter, Prefab Open, Canvas exact-image node picking, Back to Scene, recipe popup, sRGB-to-Linear Reimport and invalid-PNG failure retaining 1x1 last-good Linear image, with exit 0; copied fixture/screenshots target/u7-native-x_kw7emh. Previous attempt target/u7-native-g3gafi91 aborts at Dear cursor-boundary assert when UI Canvas first paints; ui_authoring.rs now submits Dummy after restoring cursor, and rebuilt native succeeds. Generic continuous/structural UI editing remains owner-declared read-only; trusted Canvas manipulation is still pending. Full U7 parity/review and U8 committed external acceptance remain open.

# Impact

# Citations
