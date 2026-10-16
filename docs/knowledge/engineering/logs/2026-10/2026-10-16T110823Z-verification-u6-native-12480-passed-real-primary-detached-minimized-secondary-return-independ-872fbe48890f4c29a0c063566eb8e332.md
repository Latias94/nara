---
type: "Memory Event"
title: "Verification: U6 native 12480 passed real primary/detached/minimized-secondary/return/independ"
description: "U6 native 12480 passed real primary/detached/minimized-secondary/return/independent Scene and Game close-reopen/device.destroy owner replace"
timestamp: 2026-10-16T11:08:23Z
record_id: "872fbe48890f4c29a0c063566eb8e332"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_commit: "bb6314e"
event_kind: "Verification"
---

# Event

U6 native 12480 passed real primary/detached/minimized-secondary/return/independent Scene and Game close-reopen/device.destroy owner replacement/paused refresh/Resume/Stop, four exact frames in each measured phase. Native device-loss regression first reproduced mapped-buffer panic in 73187; sticky loss test red 728b8b48-65c9-44e3-9a56-50ab8c565be6 then two tests green aced93ec-73d8-4cb0-9581-55ac5491f60a. Native recovery 33193 passed after explicitly driving a fresh paused capture. Text probe 1f0a72 passed physical Windows Pinyin Preedit and Commit Chinese, sentinel-checked Ctrl+V/C/X OS clipboard, at 150 percent DPI using explicit SimHei. UI geometry now scales with font metrics and preedit foreground belongs to its viewport at the actual caret. Layout persistence/reset wiring compiles in 47245; actual save/restore/reset, secondary IME, keyboard navigation/capture handoff, mixed monitor DPI, font updates, comparable idle/frame/resource measurements, final simplify/full review remain pending. All U6 work uncommitted. Goal remains active; U7/U8 not started.

# Impact

# Citations
