# Manual Raw-App Ownership Baseline

## Purpose

This record freezes the smallest pre-Host counterfactual for RGF-U26. The executable subject is the
manual reference-game tracer completed at revision
`f90e1b9b3235a8c5a8544eca9ba442bb7b81fd9f`. Git history preserves that implementation; this
document records only the concepts and observable ownership behavior that RGF-U24 must compare.

The baseline is a test and architecture reference, not a supported author-facing startup API.

## Caller Work

The manual caller currently has to understand and sequence these concepts:

| Boundary | Manual responsibility |
|---|---|
| Project input | Open the project capability, ingest `nara.toml`, resolve the profile, and load one immutable startup-content snapshot. |
| Runtime construction | Create `App`, construct `TaskPools`, transfer them into `App::World`, and commit the resolved plugins and time resources. |
| Startup | Drive a zero-duration startup frame and verify that no fixed tick ran. |
| Persistent materialization | Spawn the snapshot through the U29 guarded persistent-apply path. |
| Semantic control | Decode the committed command fixture and submit one `GameplayCommandSubmission`. |
| Simulation | Drive exactly one fixed tick and capture the authoritative game and command-queue result. |
| Retirement | Close task pools, shut down plugins, inspect both results, and finally drop `App`. |

RGF-U24 must move the construction, admission, publication, and retirement choreography behind the
product action. The ordinary product caller may retain only project authority or selection, profile
or run intent, semantic control input, product outcome, and structured diagnostics.

## Ownership Transitions

1. Project capabilities, the resolved plan, and the immutable snapshot begin under
   `manual_caller` ownership.
2. `TaskPools` also begin under `manual_caller`, including their standalone close owner.
3. Inserting `TaskPools` transfers the resource and its close owner into `App::World`. The task
   plugin records the corresponding required shutdown obligation but does not create a second pool.
4. Scene entities, command queues, fixed time, and plugin state are owned by the same `App` after
   commit and startup.
5. Clean retirement borrows the App-owned pools to complete task shutdown, completes plugin
   shutdown, and then drops the App.
6. If task retirement is incomplete, the manual cut keeps the App alive, so `App::World` remains
   the explicit owner. It releases the stalled task, retries only the task close, verifies that all
   configured workers joined, completes plugin shutdown, and only then drops the App.

`nara_tasks` also has a process-owned reaper fallback for an unfinished owner that is dropped. That
lower-level contract is covered by `nara_tasks` tests; this U26 cut deliberately does not claim to
observe it.

The manual path never creates a `RuntimeCandidate`, `RuntimeInstance`, publication slot, or Host.

## Observable Cases

| Case | Diagnostic class | Scene/runtime visibility | Custody and cleanup |
|---|---|---|---|
| Success | None | The guarded scene is present in the App; no runtime is published. | Every configured task worker joins and plugin shutdown completes before App drop. |
| Project content fails before ownership | `ProjectContent(OpenManifest)` | No App, scene, or runtime exists. | No retirement owner exists and no shutdown is required. |
| Late persistent hook | `scene.persistent-apply-ineligible` with `lifecycle-hook` / `add` | Entity count is unchanged and no scene or runtime is published. | App-owned task pools and plugins retire completely. The hook is never invoked. |
| Stalled required task | `TaskShutdown` with the terminal incomplete phase `Join` | The same first tick and scene completed; no runtime is published. | Bounded shutdown reports timeout while `App::World` retains custody. The test releases the task, retries close, and verifies every worker joined before plugin shutdown and App drop. |

The success task is fixed by the committed manifest, startup scene, prefab, image, plugin-plan,
schema, content, and command digests asserted in
`reference-game/tests/manual_raw_app_baseline.rs`.

## RGF-U24 Comparison Contract

RGF-U24 runs the same success task and the same three failure cuts through its ordinary product
action. It must preserve the authoritative first-tick result, diagnostic class, publication
visibility, and cleanup outcome. The Host must retain an equally explicit retryable owner for
incomplete retirement, but it may not report a false terminal state, lose custody, hide a failure,
or expose manual candidate/admission/publication/retirement choreography to the ordinary caller.

Internal type equality and byte-identical call graphs are irrelevant. Observable ownership and
product complexity are the comparison.

## Non-Claims

- This is not a public compatibility surface or a complete authoring workflow.
- It is not a transitive source-closure manifest, AST policy, canonical evidence envelope, or
  mechanical rebinding system.
- The first bounded timeout does not prove worker retirement. The subsequent retry report proves
  that all configured workers joined before the manual owner is dropped.
- A timeout does not transfer ownership in this cut. `App::World` retains the pool and its close
  owner until retry completes.
- This cut does not prove process-reaper custody. Lower-level `nara_tasks` tests own that fallback
  and its complete drain assertions.
- This baseline does not measure performance, prove native platform driving, or authorize a
  universal Host/factory abstraction.

## Verification

```text
cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test manual_raw_app_baseline --test-threads=1
```
