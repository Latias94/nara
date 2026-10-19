# Image Lifecycle Contract Repairs

Status: completed within the R0 scope; see the
[verification evidence](../knowledge/engineering/verification/2026-10/2026-10-19T021717Z-image-lifecycle-contract-repairs-95901fe367a149bc90ec922ad0384a73.md).
This plan executes R0 from the
[TileSet dependency and Editor loading research](../research/2026-10-18-tileset-dependency-and-editor-loading-research.md).
The completed component/plugin/asset identity work remains the baseline. No new TileSet loading,
streaming, generic importer executor or tile-authoring feature is included.

## Outcome

1. Preserve metadata dirtiness when source/meta events coalesce while retaining the latest source
   existence and remove/create behavior.
2. Recover a previously admitted Image after removal and recreation under the same valid source
   identity. First load without an admitted recipe and identity conflicts still reject.
3. Admit explicit Import/Reimport while bounded read-only hydration is pending. Resolve same-source
   work at the owning lane, preserving stale-result rejection and physical cleanup.
4. Revoke invalid/drifted authoring Image identity in the managed typed runtime and content receipt,
   so UI choices and newly generated Scene preview packets agree. Recoverable decode failure
   under the same admitted identity still preserves last-good.

## Execution

| Unit | Owner | Implementation and acceptance |
|---|---|---|
| I1: source facts and recipe recovery | `nara_asset/reload`, `nara_image` | Add failing event-order and split-update recreation regressions; implement owning corrections and preserve existing first-load/removal/type/slot guards |
| I2: explicit lane priority | Editor Image transaction/hydration owner | Add queued/running hydration and explicit request regression; prioritize user admission without competing publication or a second task runtime |
| I3: shared Image revocation | Managed Editor authoring runtime and projections | Regress one Image consumed by Scene/UI; invalidate typed publication/identity and content stamp before new projections; cover late completion and last-good distinction |
| I4: integration and records | Root integration owner | Independent review; focused serial nextest and affected build/format checks; update current research/HTML, ADR coverage and verification evidence with actual results |

I1-I4 are implemented. Final owner libraries pass 393 tests; focused Windows/DX12 Scene import and
Canvas Save/Reopen, the independent public consumer and feature-minimal checks pass. Two review
findings were closed: exact resolver state continuity and terminal catalog-revocation failure.
Counts, run IDs, final follow-up checks and platform limits belong in the linked evidence.

The runtime lane may edit only its assigned asset/image files. The root owns Editor integration.
Research/review lanes are read-only. Cargo runs serially with `CARGO_BUILD_JOBS=1`, using `target`.
Existing unstaged and unrelated files are preserved. No commit, push or PR is part of this scope.

## Verification

Start with named regressions in existing owner suites, confirm their expected failures, and record
run IDs. After fixes, run the complete affected Asset/Image owners and root authoring/preview
selection. Exercise separately compiled public Image consumers if a public contract changes.
Shared Editor content/preview changes require the relevant selected Windows/DX12 trajectory under
the [native procedure](../verification/native-editor-qualification.md); do not claim full release
qualification from a focused development run. Finish with relevant feature builds,
`cargo fmt --all -- --check` and `git diff --check`.

## Limits

Keep filesystem authority in the Host and decoding in Image. Identity revocation must not be
implemented as a frontend-only hidden choice. A retained allocation does not preserve authority.
Never turn the recovery repair into unrestricted first-load behavior, and never treat a failed
decode under unchanged identity as revocation. Significant contradictory existing contract
evidence must be reconciled before changing that behavior.
