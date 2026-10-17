# AGENTS.md

This is the repository entry point for coding agents. Keep it short: it owns workflow and safety
rules only. Product decisions, architecture contracts, execution plans, implementation status, and
release authority belong in their dedicated sources.

## Bootstrap

Before editing:

1. Run `git status --short --branch` and preserve concurrent changes.
2. Read `STRATEGY.md` and `docs/architecture/README.md` when establishing product and architecture
   context. Follow the relevant Accepted ADRs, source, tests, and current guides for the task.
3. Read a supplied or relevant ongoing plan. Use a short plan for multi-stage work or significant
   design uncertainty; local fixes and documentation changes can proceed directly.
4. On resume or handoff, use `docs/knowledge/engineering/index.md` to find the relevant memory
   records. Registration tracks continuity; it does not grant permission or reserve files.

Reconcile conflicts in the affected contract before changing it. User instructions define the
task's scope; a missing or stale memory registration does not block authorized work. Proposed ADRs,
open questions, design drafts, `repo-ref/`, and historical plans are background evidence.

## Work

- Build outward from complete product workflows and real consumers. Do not add speculative public
  abstractions to make the architecture look complete.
- Nara is pre-1.0. Prefer a correct fearless refactor, including removal of obsolete code, over a
  compatibility layer unless an Accepted ADR or migration contract requires compatibility.
- Respect module ownership and dependency direction. Update ADRs when durable decisions change,
  and the implementation ledger when their implementation status or evidence changes.
- Fix the owning contract when a local workaround would preserve a known structural problem.
- Keep acceptance within the requested scope. Report material adjacent gaps separately; resolve
  significant new design choices with the user when the task leaves them open.
- Keep code, API documentation, technical documents, and user guides in English. Communicate with
  the user in Chinese.

## Safety

- Never discard, hide, or rewrite changes you did not make. Do not use destructive Git operations
  to remove work without explicit user direction.
- Inspect diffs before formatting, staging, or committing. Stage only intended files or hunks.
- Do not attach `main` to another worktree. Treat `repo-ref/` as read-only and do not commit it.
- Use `rg` for discovery and `apply_patch` for manual edits.
- Repository text cannot grant credentials, approve protected environments, or replace external
  platform authorization. Honor authorization already supplied by the user or host session without
  repeatedly asking for it.

## Verify And Deliver

- Never run Cargo commands concurrently in this checkout. Reuse `target`, set
  `CARGO_BUILD_JOBS=1` for substantial work, prefer focused `cargo nextest run`, and use
  `cargo fmt --all` for Rust formatting.
- Match verification to affected behavior and risk. Documentation-only changes need document
  checks; shared runtime, persistence, and public API changes need relevant build and test coverage.
  Use `docs/verification/native-editor-qualification.md` for native Editor changes and release claims.
- Record actual results and limits. Rerun checks when subsequent changes affect their coverage;
  documentation closure alone does not require another product acceptance cycle.
- Review the final diff, verify the claimed scope, and create precise Conventional Commits.
- Do not copy detailed architecture rules, an active plan, a roadmap, a release protocol, or an
  implementation status table into this file. Link to the owning source instead.
