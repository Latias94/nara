# Compiled Project Failure Projection

Status: implemented foundation R1/F3 contract under ADRs 0009/0068/0110, with
[scoped owner and real compiled-consumer qualification](../knowledge/engineering/verification/2026-10/2026-10-25T100841Z-compiled-domain-failure-projection-and-truthful-publication-observations-8b995496a82b411184e95f6f462b48f8.md).
This is not full R1 or foundation qualification. Baseline: `c6967ea`, 2026-10-25.

## Problem And Owner

The independent Text domain already returns distinct safe diagnostics through
`ProjectContentError`. `ProjectCheckError::content_error()` retains that report. The compiled
tool currently traverses Display/source strings instead, so `consumer.text.too-long` and
`consumer.text.settings-invalid` become the same broad asset-domain failure at the public
bootstrap and CLI. Fix this concrete transport boundary, not the domain trait or a new service.

One read-only `ProjectToolFailure` retains operation, a closed failure stage, static producer
classification, bounded diagnostic records and explicit projection loss. Bootstrap returns that
value; terminal formatting is a sink, not the only public representation. Failure remains failure
when the source report has no retained entries. The diagnostics are supplemental observations,
not repair authority, receipts, rollback evidence or the operation's control-flow database.

## Selected Projection

- Canonical unreleased tool protocol is `1`, with no reader for superseded string responses.
  Synchronize metadata admission, generated projects, current consumers and both endpoints.
  Product-observation and export-acceptance protocols are different contracts and do not change.
- Retain at most eight diagnostics in report order, with complete stable ASCII codes up to 96
  bytes, closed severity and up to 256 UTF-8 summary bytes. Truncate summaries only at scalar
  boundaries. Do not truncate identity or serialize a complete report and cut its JSON.
- Carry upstream incompleteness separately from additional omitted entries, omitted retained
  fields, truncated summaries and removed summary bytes. Read retained entries through `iter()`;
  report `len()` counts observations and is not a retained count.
- Project no general diagnostic field keys or values. Static safe code/summary come from the
  typed domain report; other failures use explicit static owner classifications, never arbitrary
  Error/Debug/source chains. Do not format a raw error merely to discard its string afterward.
- Preserve the existing missing-startup repair workflow through one explicit hint facet: selected
  Scene and Scene root admit through the existing project-relative diagnostic policy; a selected
  profile admits through the existing public-identifier policy. Unadmitted values are withheld and
  counted. A withheld selected profile renders a placeholder, never a command silently targeting
  the default profile. This is narrower than general field transport and carries no mutation right.
- Keep stdout acquisition at 64 KiB; a failure JSON plus newline is at most 16 KiB. Check actual
  encoded size without cutting JSON. Private input DTOs reject missing/unknown/duplicate named
  fields, invalid severity/identity, oversize and wrong request association. Remote strings remain
  bounded transport values; do not reconstruct static `SafeSummary`/`DiagnosticCode` by interning.
- Success JSON requires a successful process exit; failure JSON requires an unsuccessful exit.
  Invalid gameplay output is not tool agreement. Compiler/child stderr is outside this projection:
  trusted native project execution is not a sandbox or whole-process secret-sanitization claim.
- A static fallback must not erase an existing physical outcome. Schema refresh has a concrete
  typed owner error distinguishing rejected/failed/uncertain publication and independent pending
  cleanup. Lower those facts to static observations, preserving reopen/check-before-retry guidance.
  A published repair can fail acceptance after an ordinary Check succeeds; inspect the typed
  postcheck reason rather than claiming every unaccepted publication failed Check. Nested repair
  content failures reuse the same diagnostic projection without changing mutation authority.
- Admitted repair hints have read-only public accessors, including selected-but-withheld profile
  state. Terminal prose is not their only consumer. Public transport views have no unbounded text
  deserialization entry; only private bounded input DTOs construct them.

## Alternatives And Reference Evidence

Parsing prose at the CLI leaves independent public callers broken. An exhaustive domain error
enum couples the generic CLI to independently authored domains. Full report/field serialization
expands privacy and retention scope without a consumer. Use the small concrete owner projection.

Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` explicitly serializes debugger records
in `core/debugger/debugger_marshalls.cpp` and decodes them in
`editor/debugger/script_editor_debugger.cpp`; its arbitrary source/callstack strings do not meet
Nara's privacy contract. Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates error
context and sink formatting in `crates/bevy_ecs/src/error/handler.rs`; it does not supply this
compiled-project protocol. These local primary-source comparisons motivate separation, not parity
or newly measured performance claims.

## Proof And Boundaries

Strengthen the existing real generated-project/independent Text-domain CLI trajectory, including
two distinct failures and recovery into its original Check/Run/Repair/Export flow. Public Rust
callers must inspect the structured result, not parse Display. Narrow protocol tests cover safe
fallback canaries, retained loss, UTF-8/encoded limits, malformed input and process-exit agreement.
Keep retained temporary repair cleanup and publication-versus-acceptance facts distinct.

The first attempted CLI reproduction stopped at a stale fixture lacking stable native-source
sidecars. Repair that test's data, not production admission. A corrected fixture reaches the
missing-domain-code assertion. Neither test maintenance nor this bounded replacement completes
R1, the semantic tool, installable packages or the foundation goal.
