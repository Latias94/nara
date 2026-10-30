---
type: "Verification Evidence"
title: "Retained Runtime pack and independent encoded residency qualification"
description: "Verification Evidence for Retained Runtime pack and independent encoded residency qualification."
timestamp: 2026-10-30T18:48:47Z
record_id: "386d2e5e9c124b66bc4390f69b796053"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-retained-runtime-pack-plan.md"
git_branch: "refactor/foundation"
git_commit: "c0a8325"
---

# Verification

Qualified the bounded CP4 production replacement from baseline c0a8325 on Windows 11 with
Rust +1.95.0, CARGO_BUILD_JOBS=1 and root target. All Cargo commands were serial. The complete
foundation rebaseline Goal remains active.

Normalized Git source state for the nine changed Rust files:
`1a3137cea395f7903fdd1221f1269267f46fb0b777a615763d619d7390e36f6e`.
This is SHA-256 of sorted `path + NUL + git hash-object blob + newline` entries, not a
performance measurement or a digest of the whole checkout.

# Result

- Captured-directory and retained-pack policies share original catalogue/executable admission,
  actual Scene/asset decoding, current demand, deep validation and native publication owners.
  There is no second dependency interpreter, resolver, codec path or encoded cache.
- `open_pack` consumes one exact FileCapability; checked extents derive from the admitted
  catalogue. Each member read reserves raw work before allocation and authenticates the exact
  private bytes passed to its decoder. Bytes drop before their reservation on success, error
  and caught panic. Decoded results retain their original plan/base and output charges.
- `write_pack` preserves original raw catalogue bytes and every admitted member without domain
  decoding. Its standard Write sink accepts partial writes and returns failures without claiming
  destination publication or durability. Transport is canonical version1; no legacy reader.
- Pack mode keeps only catalogue/backing metadata between acquisitions. The exact retained-cost
  comparison removes every captured member length and the Vec-versus-offset table difference.
  Repeated reads observe mutation instead of returning cached encoding.
- Held physical preparation remains Cancelling, unavailable for prepared access/acknowledgment
  and Busy for another request until the real worker returns. Old/new bases and escaped Image
  pixels keep their original owners after Scene retirement and pathname replacement.
- Complete captured mode still succeeds after loose inputs change or disappear. Pack mode can
  reject future affected reads after mutation/truncation; prior decoded results remain valid.
  A file handle is not represented as a portable immutable-byte proof.

# Evidence

| Check | Actual result |
| --- | --- |
| First focused pack owner run | 3/3 passed; fb137668-f35f-4700-b128-baf64f77a312 |
| Runtime content owner cohort before final panic/sink additions | 58/58 passed; de5e7a84-0250-46b7-9a79-05ff64cbc1f6 |
| Final Root lib, project-workflow-cli/tooling/presentation-2d union | 1022 passed, 4 existing skipped; d7d49ffc-3d59-4604-be84-ae720532afdb; run 239.096s |
| Independent Image and TileSet delivery binaries | 22/22 passed; dac3ffd5-8172-43ec-9144-ed21adddf60b |
| Independent Runtime base, processed Scene references and processed Host binaries | 28/28 passed; 364e9721-5999-4a66-89f5-3d97691924cb |
| Root all-features/all-targets check | Passed |
| Root all-features/all-targets correctness Clippy | Passed; non-correctness warnings retained |
| Final runtime-2d/serde minimal lib check | Passed |
| Root and independent workspace fmt checks, whitespace and selected local links | Passed |

The final Root run includes all new owner oracles: exact/one-short raw plus decoder/dependency
work, complete preparation and deep validation, no retained member encoding, refusal and panic
refund, changed bytes and late truncation, malformed header/catalogue/extent and partial sink
failure. No test was newly ignored.

The independent tests use actual temporary pack files and production codecs. They cover concurrent
Scene reads through independent cursors, original noncanonical catalogue-byte preservation,
same-digest separate plan instances, compound CPU products, shared atlas and heavy later demand,
actual Scene requests/cancellation/commit, Image/TileSet publication and escaped prepared pixels.
The original eager controls remain. The Image/TileSet run preceded the final source-compatible
`Write + ?Sized` widening and generic I/O Display wording; its exercised behavior is unchanged.
The later Scene/Host and final Root/Clippy runs include those edits.

Representative commands:

```text
cargo +1.95.0 nextest run -p nara --no-default-features --features project-workflow-cli,tooling,presentation-2d --locked --lib --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --locked --test runtime_image_delivery --test runtime_tileset_delivery --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-asset-domain-consumer --locked --test runtime_base --test processed_scene_references --test processed_host --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
cargo +1.95.0 check -p nara --all-features --all-targets --locked
cargo +1.95.0 clippy -p nara --all-features --all-targets --locked -- -D clippy::correctness
cargo +1.95.0 check -p nara --no-default-features --features runtime-2d,serde --lib --locked
```

Independent read-only production review by Kuhn reported zero confirmed P1/P2 findings for the
design and admission/backing/acquisition seam. It checked reservation/drop order, exact verified
bytes, original authority and portable availability. Maxwell and Turing wrote disjoint actual
consumer tests; parent inspected their diffs, formatted and ran verification. All three agents
were closed; no worker ran Cargo/fmt/commit.

# Follow-up

Two initially selected smaller lib-test feature unions failed in existing unrelated test cfg
imports: project-workflow/tooling lacked project-tooling test modules, and project-tooling alone
lacked tooling/persistence test modules. Reduced production lib checks passed; the established
test union then passed the complete final Root run. This unit does not qualify every possible
lib-test feature union or conceal those build attempts as passing.

No GPU execution path changed or was newly qualified. Actual HeadlessRun resource/prepared-pixel
consumers are distinct from Windows/DX12 display evidence. Linux/macOS execution, OS-enforced byte
freezing, filesystem removal behavior beyond the stated policy, RSS/throughput, whole-game
streaming, mmap/compression, remote providers, cache policy, overlays, target cooking and the
packaged Player remain separate. CP4 does not change qualified CP3 demand semantics.

The parent plan continues through its dependency-ordered units; this bounded content comparison
does not complete the foundation Goal, rendering or the custom-drawn Editor.

# Citations

- [Backing design](../../../../architecture/runtime-content-backing-and-residency-design.md)
- [Unit plan](../../../../plans/2026-10-31-retained-runtime-pack-plan.md)
- [Pinned source study](../../../../research/2026-10-31-retained-pack-and-resource-lifetimes.md)
- [In-progress capture](../../progress/2026-10/2026-10-30T182551Z-retained-local-pack-ownership-replacement-and-public-consumer-qualification-in-progress-5d5a3464bbcd436581a379fcee22e3e1.md)
