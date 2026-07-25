# Reference-Game Immutable Pre-release Preparation

This document records only the local U12 publisher preparation for the carried RGF-U21 contract.
It records no tag creation, approval, draft upload, Release mutation, hosted smoke result, public
download, or announcement. The workflow has not been dispatched by this preparation.

## Release Inputs

`reference-game-release.yml` accepts only a canonical U11 `Publish` approval pinned by all of:

- protected-`main` approval commit;
- Git blob object ID;
- SHA-256 of the exact canonical approval bytes;
- SHA-256 of the independently reviewed publisher workflow bytes;
- existing protected annotated `vX.Y.Z` tag;
- one approved candidate workflow run ID and the exact Linux and Windows Actions artifact IDs.

Dispatch fields can locate records, but cannot replace the reviewed verifier, schemas, smoke helper,
package helper, package layout, candidate workflow identity, candidate archive identity, protected
branch state, tag target, or Release policy. The credential-free verifier fetches the reviewed
release helper and schemas by a fixed commit/blob/SHA-256 triple, derives Actions and Git facts, and
rejects a changed approval, candidate, tag, tag rule, source revision, workflow definition, retention
deadline, or pre-release version.

The candidate transport download has the narrow `actions: read` grant required by GitHub's old
artifact download endpoint. It runs only pinned GitHub artifact actions and no repository helper or
candidate code. The next verifier process has `permissions: {}`, receives no GitHub token, validates
the downloaded transport manifests and archives without extraction, then stages only manifest-bound
raw candidate ZIP bytes.

## Authorization Boundaries

The workflow is a protected-`main`, manual, first-attempt-only path. Its run name and tag-scoped
concurrency key make an earlier dispatch for the same tag fail closed. A tag is never created by the
workflow. The intended one-shot sequence is:

1. An authorized maintainer creates the protected annotated version tag after U11 records `Publish`.
2. The `reference-game-immutable-policy` environment supplies a policy-only token with
   Administration read access. It must prove that repository immutable releases are already enabled;
   the workflow never enables or disables that setting.
3. The `reference-game-draft-upload` environment approves the first and only content-write stage.
   It creates one draft pre-release and uploads only two verified candidate ZIPs plus `SHA256SUMS`.
4. Read-only Linux and Windows draft-smoke jobs download their exact draft assets, verify
   size/digest/identity, and execute them only after the authenticated download phase has ended.
5. The `reference-game-release-finalize` environment approves the read-only binding of both draft
   smoke receipts.
6. The distinct `reference-game-release-mutation` environment approves the second and only
   content-write stage. It rechecks immutable-release policy immediately before switching that
   verified draft to public pre-release state.
7. Anonymous Linux and Windows public-smoke jobs retrieve the public assets without an
   authorization header, repeat digest/archive/runtime smoke, and emit one read-only verdict
   artifact. The workflow performs no announcement.

`NARA_RELEASE_POLICY_TOKEN` is an environment secret, not a general repository secret. It must be
restricted to Administration read and be available only to the immutable-policy and
release-mutation environments. Candidate bytes, the pinned verifier, the smoke helper, and candidate
child processes never receive it. The draft/upload and finalization jobs never checkout the
repository, extract a candidate archive, invoke a repository helper, or execute a candidate.

## Failure Rules

The workflow refuses a rerun attempt and refuses a fresh dispatch whose tag has already appeared in
the release workflow history. It also refuses an existing Release for the tag, a mutable-release
setting, a lightweight or moved tag, an unprotected tag rule, any draft asset substitution, a missing
or failed smoke receipt, or a public Release that does not immediately report `immutable: true`.

No rebuild occurs in publication. A candidate expiry, provenance mismatch, failed draft smoke, failed
public smoke, or any failure after tag creation leaves that version unannounced and requires a new
U11/U12 version. An external operator must retain the authorization and run identities in the final
evidence record; local policy preparation is not evidence that any of those stages ran.

## Non-Claims

- This file and the workflow do not prove GitHub environments, required reviewers, tag rulesets,
  immutable-release policy, or the policy-only token are configured.
- They do not create a tag, dispatch a workflow, create a draft, upload an asset, publish a Release,
  download a public asset, or send an announcement.
- They do not replace U8 hosted CI, U9 baseline evidence, U10 candidate evidence, U11 review,
  clean-room journey, approval commit, or the independently required security and bug/regression
  review of the final permission-bearing workflow revision.
