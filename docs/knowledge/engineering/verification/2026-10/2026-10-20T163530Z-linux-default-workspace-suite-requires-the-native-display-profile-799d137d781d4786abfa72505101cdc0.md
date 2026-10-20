---
type: "Verification Evidence"
title: "Linux default workspace suite requires the native display profile"
description: "Verification Evidence for Linux default workspace suite requires the native display profile."
timestamp: 2026-10-20T16:35:30Z
record_id: "799d137d781d4786abfa72505101cdc0"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-ci-portability-repair-plan.md"
git_branch: "main"
git_commit: "56840a4435c2d2997fa8e90aa905fdd6acac6b8d"
---

# Verification

Hosted run [39449651006](https://github.com/Latias94/nara/actions/runs/39449651006)
at `56840a4435c2d2997fa8e90aa905fdd6acac6b8d` completes Linux Reference Game
(83 default and 151 all-feature tests), both independent module-consumer jobs, Windows Reference
Game and portable Wasm successfully. Linux Root job `118720886175` completes its default suite:
2,656 of 2,657 tests pass, with 12 skips, in 956.381 seconds. Its nextest run is
`25d19aea-743b-4e44-bf0b-cfc78d66c291`.

# Result

The only Linux Root failure is `generated_starter_reaches_ready_stop_and_clean_editor_exit`,
which reaches Faulted/Start after 226.749 seconds. The default workspace includes this native
desktop child test because `nara-editor` has desktop dependencies even with empty default
features; the test is OS-gated, not feature-gated. The CI default command was outside Xvfb,
while only the later all-feature command supplied the native display profile. That later command
did not run after the default failure. Reference Game gates the equivalent test on `editor` and
runs it in its Xvfb-wrapped all-feature cohort.

# Evidence

Root and an independent read-only agent inspect the actual job commands, feature declarations,
test gating and DesktopRun failure projection. The retained child tail reports primary reason
`runner`, secondary `project.desktop.presentation-missing`, and handshake EOF before READY.
The secondary diagnostic is appended when the runner returns before verification; it is not
the separate presentation timeout. Bounded stderr retention has already evicted the exact native
cause, so this record does not claim to have observed a specific X11 error.

The correction wraps both serial Linux root cohorts with the existing Xvfb configuration;
Windows commands and all test assertions remain intact. PyYAML parses the edited workflow, and
a focused structural check confirms both Linux commands use Xvfb. No environment allowlist,
timeout or product failure rule is broadened.

# Follow-up

Hosted verification of the edited workflow is pending. Windows Root at the observed source is
still running; neither complete green CI nor the later Linux all-feature suite is claimed.
No Linux Cargo workload is run concurrently with this checkout's ongoing P1 content validation.

# Citations

- [Workflow](../../../../../.github/workflows/ci.yml)
- [Prior Linux generation storage correction](2026-10-20T152924Z-reflection-declaration-checks-and-linux-supervised-play-storage-corrections-efc879a2a8974c989725df994b86e5db.md)
- Local sanitized log: `target/ci-root-linux-56840a4-job-clean.log` (untracked)
