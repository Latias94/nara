# Editor Image Metadata Publication Cleanup Custody

Status: Completed at `bae0440`, after owned startup implementation `eb1ee98`.

## Confirmed Gap

The Scene/Prefab lifetime correction does not register Image metadata cleanup. After terminal
metadata publication failure, `EditorImageImportHost::publication_cleanup` is the sole owner of a
FilePublicationHost with retained TemporaryFileCleanup. Session Drop only cancels pending image
work and drops that field. Explicit close retries it, but its authoring runtime can already become
Stopped because no runtime participant accounts for that cleanup. Dropping either session then
loses identity-bound retry authority. The failed-cleanup object intentionally does not retry on Drop.

Keep the completed Scene/Prefab and startup qualifications within their recorded scope. CLI schema
publication separately reports temporary identity/name for explicit manual cleanup; it does not
claim the Editor's retained runtime-close contract and is not the same finding.

## Intended Repair

Extend the existing private finite Host custody to the concrete metadata publication Host. Register
authority before mutation where the actual publication flow permits it; successful publication
retires that registration, while failed cleanup retains it independently of the image projection,
operation terminal and session lifetime. Keep current source identity, metadata validation, image
admission and publication receipts. Do not retain the complete image Host or add an async framework.

Explicit close cannot certify a stopped runtime while registered metadata cleanup remains pending.
The successful content-runtime transfer must include this authority without allowing the old runtime
to close a live Host. New image work remains blocked after session close begins.

## Verification

- Terminal failed metadata publication -> direct Drop, and failed cleanup during explicit close ->
  Drop: independent cleanup release control, actual target bytes and temporary entry counts, retained
  quarantine until release, then retirement to the original count.
- Preserve existing cleanup blocker/retry, source conflict, last-good image and metadata invariants.
- Check success/failed/cancelled publication and any real runtime-replacement intersection.
- Independently challenge ownership, run focused engine/Editor/native evidence serially, update the
  owning ADR/ledger and external HTML, record actual limits and commit.

## Lane Map

Mode: execute_direct. Baseline implementation `eb1ee98`. Shared branch
`refactor/foundation-contract-hardening`; all lanes share this checkout and preserve other edits.

| Lane | Writable scope | Responsibility |
| --- | --- | --- |
| Root | `file_publication.rs` test controls, Editor forwarding, test registration, plan/contracts | Integrate shared fault control, serialize Cargo/fmt/stage/commit and document qualification |
| Retirement owner | `runtime/editor/persistence/retirement.rs` | Add the concrete metadata publication registration and retained cleanup contract |
| Image transaction | `runtime/editor/image_import/transaction.rs` and narrowly required private children | Register before mutation, retire after publication, retain read-only completion authority |
| Compound tests | New `runtime/tests/image_publication_custody.rs` | Direct/close/unwind abandonment, external release, source bytes and actual cleanup evidence |

Review is read-only after implementation; each lane reports ownership and actual verification.
Reuse target with CARGO_BUILD_JOBS=1; preserve concurrent files and vendor/. No push or PR.

## Outcome

The concrete FilePublicationHost is registered before mutation, with a transaction handle retained
before borrowing it. Terminal failure, task shutdown, session Drop and unwind through the real Host
mutex preserve cleanup authority in runtime custody. Successful publication retires the writer and
keeps only the existing fresh-read source and binding for late identity/digest checks. The helper
is a private transaction child, not a generic executor or importer abstraction.

Focused engine/public 116, Editor/application 186 and actual Windows/DX12 native 2 tests pass;
workspace all-features/all-targets, formatting and diff checks pass. Two read-only integration
reviews report no remaining confirmed blocking defect in this change. Image mutation currently
uses Authoring execution, while content-runtime replacement uses trusted compiled execution;
there is no real consumer intersection to claim. The finite registration transfer covers the
concrete variant; existing Scene/Prefab replacement evidence retains its original scope.

The [qualification](../knowledge/engineering/verification/2026-10/2026-10-20T035244Z-editor-image-metadata-publication-cleanup-custody-qualification-c097c716d6f3480692d800285ec53544.md)
records commands, run identities, independent review and limits. This closes the confirmed Image
cleanup finding; unmeasured consumer costs and future capability work remain separate.
