# Cooked Player Delivery

The finite delivery workflow joins concrete processed content with one explicitly built Player.
It produces one relocatable directory containing the actual captured executable, `content.pack`
and `package.json`. Source-carrying Export remains a separate workflow. The
[delivery plan](../plans/2026-10-31-cooked-player-delivery-plan.md) records current qualification;
this API guide alone does not establish platform support or Runtime readiness.

## Ownership And Build Order

Use the same game-owned `ProductRecipe` in the compiled cooker and Player. The cooker selects
the content target/configuration, actual importer/cook definitions and source observations.
Cargo package, binary, profile, default features, requested features, target and tools are a
separate explicit selection. A content profile need not equal a Cargo profile.

| Value | Guarantee and owner |
| --- | --- |
| `VerifiedArtifactGroup` | Original encoded member bytes, Product mappings, formats, dependencies and build facts; its original retained lease survives cache deletion. It can be historically valid without being the current selection. |
| `CookedContentBuilder` | Finite concrete assembly through Scene, Input and TileSet production encoders plus verified groups; no importer registry or directory publication. |
| `PreparedRuntimeContent` | Original admitted captured base and independent expected catalog facts; every delivered member is covered by deep validation. |
| `GeneratedPlayerBootstrap` | Exact Rust input bytes/path/digest carrying independent expected catalog identity and fixed pack location. The Host writes this ordinary source input before Cargo starts. |
| `CapturedCargoExecutable` | Actual selected executable bytes and checked finite code-input/tool provenance. Building does not execute it. |
| `PreparedPlayerPackage` | Exact content/bootstrap/code binding and bounded three-file candidate; no Runtime activation. |
| `PlayerPackageReceipt` | Actual complete directory publication and filesystem guarantee tiers. A later callback failure cannot erase this physical result. |

The order avoids a digest cycle: prepare content, generate independent expectation input, build
the Player, bind actual build provenance, then publish the whole directory. The catalog never
contains the final executable digest. The final manifest records code, catalog and pack facts,
and cannot authorize its own expected identity.

## Assemble Complete Content

The cooker uses `project_workflow::cooked_content` under the `project-workflow` feature.
Construct `CookedContentRequest` with the complete startup/travel Scene set, explicit addressable
roots, `RuntimeConfiguration` and `RuntimeContentTarget`. Add actual verified groups with
`add_group`, saved/expanded Scenes with `add_scene`, immutable global Input with `add_input`,
and concrete TileSet descriptors with `add_tileset`. Scene references are resolved through the
actual `AssetReferenceResolver`; encoded Runtime references use complete Products.

`finish` consumes the unpublished builder. Required and Reference dependencies must both be
delivered; Immediate/Deferred determines current preparation demand. Soft metadata alone does
not require membership. Add an actual addressable root when an otherwise unreferenced Product
belongs to this delivery. Surplus members reject rather than silently evade validation or become
implicit roots. A compound member preserves all of its Products without duplicating its bytes.

Deep validation runs the selected production decoders and actual reference inventories, including
Image/TileSet/Tilemap CPU constraints. A target Image transform that invalidates an atlas rejects;
the assembler does not repair authored data implicitly. This work publishes no native resource or
World. Pack startup later admits the catalog/extents and current demand, rather than repeating
the entire producer validation eagerly. Later original-file corruption can reject a Deferred
acquisition while preserving the current Scene.

## Bind The Player

Native construction and publication live in `project_tooling::player_package`:

1. Create `GeneratedPlayerBootstrap::new(&content, canonical_input_path, limits)`.
2. Write its bytes at that exact workspace-relative Rust path. Include the path in the
   `CargoBuildRequest` declared inventory along with the other actual code inputs.
3. Make the Player compile this module and use its constants with its own game recipe.
4. Call `build_player_package(content, bootstrap, &request, limits, control)`. The wrapper retains
   content through Cargo, reserves the next owner's work promise and binds the actual capture.

The binder compares the whole requested Cargo selection and the generated input's exact canonical
path/digest/length in actual captured provenance. Resolved compiler features and tool observations
remain separate facts. Old Player A cannot bind content B by sharing a startup Product or label.
The separately available `PreparedPlayerPackage::bind` accepts already captured historical facts
without asserting that mutable authoring inputs remain current.

The Player's inclusion and use of the generated module is a trusted Rust implementation contract.
The binder does not analyze Rust inclusion, scan executable code or prove executed branches.
Do not offer command-line or manifest overrides of the compiled expected catalog/target. This
finite workflow does not establish hermetic compilation, remote authenticity or a reusable
prebuilt-Player trust scheme.

## Keep Current Selection With The Host

Retain the actual finite source, metadata, auxiliary-read/presence and processed-group selection
envelope used to prepare this content. The control callback revalidates that original envelope
around Cargo, before staging and before publication. Do not create a fresh baseline inside the
callback: that would accept the changed selection being checked. Scene, Input and TileSet source
observations participate alongside import/cook inputs.

Code inputs and content observations are different inventories. Cargo revalidates its finite code
inventory; group validity alone cannot extend content freshness across the later Player build.
The Host retains content observation custody through publication. A failed fence returns
`SelectionChanged` or `Cancelled`; callback panics become finite package failures. The check
envelope does not prove simultaneous cross-file state, ABA detection or globally latest inputs.

## Publish And Run

Call `prepared.publish(&parent, &destination, control)` with an existing writable directory
capability. The owner streams the original admitted pack, writes captured executable and manifest
bytes, synchronizes files, checks the exact three-file regular-member inventory and verifies
staged digests before existing no-replace whole-directory publication. It does not scan Cargo
output for dependency libraries. Additional native libraries need explicit finite support before
that product can claim a complete package.

Handle every `PlayerPackageOutcome`. Clean rejection, orphaned nonempty candidate and unresolved
begin custody are distinct. No recursive name-based cleanup occurs. `Published { receipt, error }`
always means publication happened; an optional postpublication error does not make the destination
absent. `package.json` is finalized before publication; physical receipts stay in the return value.
Publication neither promotes executable last-good nor establishes Runtime Ready.

The Player needs only `runtime-2d + serde` for executable-relative opening, not `project-tooling`:

```rust
let expected = RuntimeContentExpectation::new(
    ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
    RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)?,
);
let base = open_executable_relative_runtime_pack(
    &RelativePath::new(generated::PACK_FILE)?,
    &expected,
    game::recipe(),
    RuntimeContentLimits::default(),
)?;
```

`project_host::open_executable_relative_runtime_pack` uses the actual executable parent, independent
of the working directory. It opens one original pack and delegates all admission to Runtime
content. Failure has no authoring-source, metadata, importer or cache fallback. Continue through
the existing processed `HeadlessRun`/`DesktopRun` Host rather than installing a second loader.

## Bounds And Evidence

`CookedContentLimits` accounts for live group custody, copied captured bytes, metadata and decoder
promises. `PlayerPackageLimits` separately bounds bootstrap/manifest encoding, retention, overlapping
work and the complete staged disk payload. Pack writing is streamed. Shared owners keep original
leases; equal digests do not merge independent allocations. Borrowed Host observations, escaped
acquisitions and compiled recipe construction keep their own existing owner allowances. These
logical ledgers do not measure allocator capacity, subprocess RSS or arbitrary trusted native code.

The [independent Cargo consumer](../../tests/fixtures/cargo-build-consumer/README.md) contains the
complete compiled cooker/Player workflow and explicit serial commands. It uses real tracked Image
import/target cook, an auxiliary compound Product, global Input with production driver-event
admission, shared Image/TileSet,
Deferred Scene travel, whole-directory relocation and unavailable source/cache/Cargo at execution.
Its actual qualification remains in the owning plan/evidence; foreign builds, native presentation,
server stripping and additional platforms require their separate consumers.
