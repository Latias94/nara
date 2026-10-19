---
type: Verification Evidence
title: Foundation U13 external ordinary native runner and admitted Exit
description: A separately compiled panel runs through the public native entry and joins normal application retirement after authoritative Session publication.
timestamp: 2026-10-19T10:25:11Z
record_id: 31a77e0e4c4b47b3ad66a13f45d39466
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# External Native Entry

The external `editor-panel-consumer` now includes a binary whose main thread invokes ordinary
`EditorNativeComposition::run`. It creates no private Host or manually driven frontend. Its
source-built panel emits a real public navigation action, observes that exact selection on a
later native frame while retaining its own draft, verifies unchanged document revision/dirty/
history facts, and emits the existing workspace Exit action. The runner must return normally,
with one factory, at least two paints and one eventual panel Drop.

The parent ignored Windows test isolates APPDATA, the actual Editor preference source, plus XDG
directories in its exclusively created temporary artifact directory. It retains the normal user
profile and graphics-driver cache environment. It does not modify parent environment or actual
user preferences. A 60-second watchdog kills and waits only on failure; forced termination can
never produce a passing result. Logs and preferences remain in the owned artifact directory.
The real ordinary Editor window is briefly visible; no global input is injected.

# Failure Diagnosis And Owning Correction

Run `525dfb65-3389-4b6e-a1ba-36e8822ada2d` timed out without a panel callback while the fixture
also replaced USERPROFILE/HOME/LOCALAPPDATA. The observed driver cache files support an environment
interaction; an exact driver-level cause was not established. Those unrelated overrides were
removed because APPDATA alone isolates the real Editor preferences. No production workaround or
longer deadline was introduced for that observation.

Run `c182bf97-fd5b-4a58-bce7-afa14c43bae2` reached the real panel, published navigation, observed
the selection and requested Exit, then timed out. Source inspection found a real ownership gap:
the Session accepted workspace Exit, while Editor Application only joined process retirement
when its shell-specific request_exit route had preinstalled the retirement owner. An ordinary
panel action could reach Prepared without that application owner.

The correction adopts ExitProcess retirement after the Session owner drive publishes its accepted
pending/applied Exit. Existing CloseProject and project-replacement retirement remain authoritative;
the new path does not overwrite them, bypass prompts or mint a new close authorization. Rejected
batches cannot create an admitted Exit state. Cancel clears the existing retirement as before.

An initial implementation incorrectly inspected the immutable published frame immediately after
dispatch. Independent correctness and adversarial reviews identified that stale snapshot, and the
new regression also failed. That attempt was replaced by the post-drive publication boundary,
without adding a public live-state projection. A separate first assertion assumed standalone
WorkspaceIntent requests required the current frame; their existing contract intentionally does
not. The rejection test now includes a real frame-sensitive navigation action and proves atomic
rejection of that stale batch, preserving the established request semantics.

# Passing Evidence

Windows, Rust 1.95.0, CARGO_BUILD_JOBS=1, serialized Cargo and shared target:

- `bd5e30f7-9790-4f5a-afc2-18730fdc28db`: all 14 Editor application contract tests passed after
  the post-drive correction, including stale mixed-batch rejection and public Exit retirement.
- `2ff99d9d-a23d-410a-98a2-70b5300bb27b`: the external ignored native runner test passed in
  1.240 s using its independent workspace lock. Actual Windows/DX12 startup, real panel actions,
  later-frame acknowledgement and normal process/session/graphics retirement all executed.
- `eb01fe90-95a9-4806-bb25-62e493aab320`: all 15 Editor application contract tests passed,
  additionally covering public Exit -> dirty prompt -> Cancel -> acknowledged result -> new Exit
  -> Discard -> normal terminal cleanup. Existing shell exit, CloseProject and replacement tests
  also passed. No original proof test was weakened.

The external command was:

`cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --test native_runner --locked --run-ignored only --test-threads=1`

Together with [state/GPU reconstruction](2026-10-19-foundation-u13-stateful-panels.md) and
[independent embedded consumers](2026-10-19-foundation-public-consumer-integration.md), this closes
U13's focused external runner acceptance. It does not claim Linux/macOS, monitor scanout, dynamic
plugins or an unrestricted widget framework. Final integrated checks/review and precise commits
remain separate gates.
