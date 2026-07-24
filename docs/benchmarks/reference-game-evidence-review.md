# Reference-Game Evidence Review

This document defines the local-preparation evidence path for the RGD U11 successor of RGF-U20.
It records no measurement result, approval, release candidate, or Publish/Redirect/Stop decision.
Those facts may be added only after the hosted candidate, clean-room journey, complete review, and
separate protected-branch authorization gates have completed.

## Trust Flow

The evidence path has three intentionally separate stages:

1. A credential-free collector may execute the candidate and emit an untrusted U22 envelope plus
   restricted raw-log references.
2. The read-only evidence-ingest workflow obtains the fixed helper and normalized schema by exact
   reviewed commit, blob object ID, and SHA-256. It constructs a canonical
   `nara.reference-game.evidence-trusted-input-v1` record from independently obtained Actions and
   candidate facts. `build-expectation` reads the untrusted envelope only to bind its encoded byte
   count and SHA-256; `normalize` then compares every identity, environment, context-receipt,
   raw-log, and candidate field against that expected record. Neither step executes candidate or
   product code.
3. A separately reviewed PR/CI step feeds the exact raw transfer and normalized evidence through
   the Rust U22 oracle. Only this semantic gate can make the normalized record eligible for an
   approval record. Outer-transfer success alone is deliberately insufficient.

The `trusted-input` record is an authority boundary, not collector output. A workflow must derive
it from read-only GitHub Actions/run/artifact metadata, pinned workflow facts, and explicit
review-owned context. Dispatch inputs may select an already immutable candidate or evidence
artifact identity, but may not override the pinned helper/schema identity or fabricate the
candidate workflow, source revision, environment, or raw-log facts.

## Candidate Startup Collection

The local `measure_headless_iteration.py` and `measure_desktop_product.py` helpers prepare one raw
component observation at a time. Each invocation requires an expected platform, expected source
revision, positive sample index, fresh empty work root, and new output destination. It validates
and extracts the fixed candidate archive, replaces the candidate home, working directory, and
temporary directory with clean-room paths, and enables the opt-in startup marker.

Each helper runs one unmeasured warmup process followed by one measured process. A failed warmup is
preserved and prevents the measured process from starting; a failed measured process is also
preserved. Candidate output has a shared 64 KiB limit, a finite process deadline, empty-stderr
requirement, and an exact success shape:

- Headless accepts one `headless_first_authoritative_tick` marker followed by the canonical,
  terminal wave summary.
- Desktop accepts one `desktop_first_playable_present` marker followed by the render-probe success
  line. The packaged probe traverses the product render and present path, but it is not evidence of
  manual desktop playability or input ergonomics.

The output directory contains one canonical observation plus four bounded raw stream files for the
warmup and sample. The observation records only the stream byte counts and SHA-256 digests; raw
process output remains a restricted transfer outside repository evidence. It declares
`decision: not_evaluated` and never emits a U22 envelope.

Headless and desktop durations with the same sample index are still independent raw components.
Only the Rust semantic gate may verify their candidate, environment, context-receipt, and index
bindings, take the slower component for each pair, admit at least ten warm pairs, calculate the
frozen population P95, and evaluate `candidate.startup_p95_ns`. Collector success is not metric
success.

## Committed Workflow Preparation

`.github/workflows/reference-game-evidence-ingest.yml` is deliberately a preparation-only
workflow. It runs only from a protected `main` manual dispatch, fetches the reviewed
`ingest_evidence.py` helper and normalized schema by exact source revision, blob ID, and SHA-256
into the runner temporary directory, then runs `verify-policy` against those bytes.

It does not checkout the repository, obtain a candidate or evidence artifact, construct a
`trusted-input` record, normalize an envelope, publish an Actions artifact, or make an approval
decision. This avoids treating a workflow dispatch identity or an untrusted envelope as the
missing independent source of candidate, environment, context-receipt, and restricted raw-log
facts.

The workflow may be extended into a real ingest execution only after U8, U9, and U10 have supplied
the final candidate evidence and a separately reviewed, read-only producer contract can construct
all required trusted-input fields. That extension must preserve the constraints below and receive
its own policy review before an authorized evidence-ingest dispatch.

The reviewed source revision must remain an ancestor of the protected dispatch revision. If
integration rewrites that identity, for example through a squash, the workflow fails closed until
the commit/blob/SHA-256 pins are refreshed against the resulting `main` history and reviewed again.

## Future Evidence Ingestion Constraints

The future execution path remains one bounded, cancellable, protected-main `workflow_dispatch`
path. It has no checkout, no repository or Release mutation, no OIDC, no user or environment
secret, and no candidate/product execution. If private-repository access is required, its ephemeral
job token is restricted to Actions/Contents read only and is not persisted or passed to a child
process.

The workflow must reject before parsing evidence when any of these is true:

- The fixed helper or schema cannot be fetched by the reviewed commit/blob/SHA-256 triple.
- A transfer is linked, aliased, special, missing, unexpected, oversized, or out of retention.
- GitHub run/artifact metadata, candidate receipt, source revision, workflow definition, archive
  size/digest, or raw-log reference differs from the independently expected record.
- The trusted-input record is noncanonical, has an unknown field, contains a path escape, or binds
  a candidate to another source revision.
- The envelope fails an outer budget, exact identity/environment/context comparison, or canonical
  encoding check.

The workflow may publish one immutable normalized artifact and its digest to an Actions artifact.
It may not commit the artifact, create an approval record, tag, create a release, upload a draft,
or classify a result as approved.

## Semantic Admission

The review/CI semantic gate preserves the U22 ordering:

1. Reconstruct the exact raw evidence transfer and verify that the normalized `evidence` object
   has not changed its canonical bytes.
2. Validate protocol, record catalogue, fields, populations, required context, source revision,
   environment equivalence, payload digest, limits, and decision inputs through the existing Rust
   oracle.
3. Aggregate only the admitted records under the frozen `candidate_gate` rules.
4. Keep a failing or incomplete suite as `Redirect` or `Stop`; no workflow may synthesize a
   passing summary from percentile-only data or a different environment.

The committed policy test intentionally includes a shape-valid envelope with a false
`payload_digest`: outer normalization accepts its independently rebound transfer, while the U22
semantic gate rejects it. This guards the boundary against a future workflow treating successful
normalization as approval.

## Future Approval Record

The versioned canonical approval record belongs under
`docs/benchmarks/data/approvals/v1/`. It will bind all of the following before a release workflow
may consume it:

- the reviewed source revision and frozen protocol digest;
- the immutable normalized-evidence path, byte count, and SHA-256;
- the exact raw-envelope and U22 semantic-validation identities;
- final Windows and Linux candidate workflow/run/artifact/archive identities, digests, sizes, and
  retention deadlines;
- the clean-room Rust-author journey identity and result;
- the complete pre-publication review status, including no unresolved P0/P1 finding;
- a `Publish`, `Redirect`, or `Stop` decision plus a non-activating next-slice rule.

Changing any bound field allocates a new versioned record and reruns its affected evidence. An
outer-only normalized artifact, a reused authorization, an expired candidate, or any non-Accepted
authority is not a valid approval input.

## Non-Claims

- Local helper and policy-test preparation do not prove hosted candidate execution, a clean-room
  journey, semantic evidence, review clearance, an approval commit, or publication readiness.
- This path does not add a production benchmark service, generic evidence API, signing system, or
  package manager.
- The U11 decision remains blocked until U8, U9, U10, and the required independent reviews have
  produced their exact evidence at one final reviewed revision.
