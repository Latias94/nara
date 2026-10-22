# Public Extension Ergonomics

## Outcome

Complete the source-domain Editor workflow and correct demonstrated public-interface friction
using the existing independent source package, Editor adapter and native runner. Compare the
owning mechanisms with fixed Bevy source rather than copying its complete asset pipeline.
Keep `bevy_ecs`, explicit compiled selection and the existing content/publication/runtime owners.

The [source-grounded comparison](../research/2026-10-22-bevy-public-extension-ergonomics.md)
identifies a forwarding wrapper forced by the current facet relationship, independently chosen
settings types, a two-step embedded composition trap, and lost importer selection in the trusted
runner. Those concrete consumers justify the following corrections.

## Work

1. Make the source domain name its settings type and let an independently owned Editor facet name
   that domain. Preserve identity-first preparation and actionable domain settings diagnostics;
   do not introduce a second parser policy or require source-only settings to be serializable.
2. Add a typed import-action constructor at the facet that already knows the domain and codec.
   Keep tooling DTOs frontend-neutral, and retain current-frame, size and publication admission.
3. Consume complete Editor composition in one `build` operation, reused by the ordinary native
   runner. Pass the same selected importers through the trusted compiled path and replacement
   Open Host. Retain the real late-panel installation workflow as an explicitly panel-only
   operation that rejects selected Host facets instead of silently dropping them.
4. Qualify independent Check/Play and Editor Import/Reimport, invalid settings, exact source
   freshness, close, replacement, trusted construction and native presentation. Reconcile the
   owning ADRs/guides, review the final diff and create precise local Conventional Commits.

## Verification

Use serial Cargo with one build job and the existing target. Retain the existing source-domain,
Image/metadata-owner, ordinary Editor and Windows/DX12 canary coverage. Add focused public
consumers for the actual composition omission and rejected late Host configuration. Record
executed cohorts, failures and platform limits in the
[source-import qualification](../verification/2026-10-21-editor-source-domain-import.md).

No new ECS, generic job/plugin kernel, scripting language, dynamic ABI, GPU importer or watcher
is part of this correction. A future optional scripting or reload adapter must demonstrate the
specific missing owner interface through its own complete consumer.

## Status

Steps 1-4 are complete in the commit containing this plan. The associated settings/facet,
typed action helper and consuming composition are exercised by 17 independent-consumer cases,
181 Editor/panel cases, 124 Host/source-export cases and one explicit Windows/DX12 native canary.
The trusted case asserts its initial source-index unavailability, then proves Import after
ordinary authoring replacement. Formatting, scoped lint and minimal-feature checks pass;
the [qualification record](../verification/2026-10-21-editor-source-domain-import.md) records
the retained main-workspace warnings, reproduced failures and native/platform limits.
