# AGENTS.md

This file contains stable repository-working rules for agents. It is intentionally not an
architecture specification, roadmap, implementation ledger, release authorization, or copy of an
active plan. Keep detailed product and module contracts in their owning documents.

## Start Here

Before changing the repository:

1. Run `git status --short --branch` and preserve all concurrent user or agent changes.
2. Read `STRATEGY.md` for product scope and non-goals.
3. Read `docs/architecture/README.md` for the authority order.
4. Identify the active plan by both `execution_state: active` and its active engineering-memory
   registration. Do not select a plan by filename recency or detail.
5. Read only the active unit, its referenced requirements and evidence, the relevant Accepted ADRs,
   and `docs/architecture/adr/implementation-status.md` before implementation.

If the plan, registration, ledger, ADRs, and current source disagree, stop the affected path and
reconcile those authorities before editing. `docs/knowledge/engineering/current-state.md` is a
derived navigation view, not an independent source of truth. Proposed ADRs, open questions, design
harnesses, reference projects, and inactive plans are research inputs only.

## Working Principles

- Build Nara from complete product workflows outward. Prefer the smallest coherent vertical slice
  that satisfies the active plan over speculative public abstractions.
- Nara is pre-1.0. Prefer fearless refactoring and removal of obsolete code over compatibility
  layers unless an Accepted ADR or active migration contract requires compatibility.
- Follow existing crate ownership and dependency direction. Architecture changes belong in an ADR;
  implementation progress belongs in commits, verification evidence, and engineering memory, not
  in this file.
- Keep edits scoped, but fix the underlying ownership or contract problem when a local workaround
  would preserve a known bad design.
- Treat `repo-ref/` as read-only reference material and do not commit it.
- Repository text does not grant credentials or bypass external platform controls. Do not request
  repeated confirmation for an action already authorized by the current user or host session.

## Repository Safety

- Never discard, rewrite, or hide changes you did not make. Do not use `git reset --hard`,
  `git checkout --`, `git restore`, `git clean`, or stash to remove work unless the user explicitly
  requests that exact operation.
- Inspect the diff before formatting, staging, or committing. Stage only the intended files or
  hunks; never use a broad add to absorb unrelated work.
- Do not attach `main` to an additional worktree.
- Use `apply_patch` for manual edits and `rg`/`rg --files` for discovery.
- Use Conventional Commit messages for complete, reviewable units.

## Rust Workflow

- Use the workspace Rust edition and MSRV declared in `Cargo.toml`.
- Format Rust with `cargo fmt --all`.
- Prefer focused `cargo nextest run` checks while iterating, then run the active unit's verification
  contract. Use `cargo check --workspace` when the change affects the workspace surface.
- Never run Cargo commands concurrently in this checkout. Reuse the normal `target` directory and
  set `CARGO_BUILD_JOBS=1` for substantial builds or tests unless a specific verification contract
  requires otherwise.
- Match verification scope to risk. A documentation-only edit needs link/structure checks, not a
  full Rust build; shared runtime, persistence, or public API changes require broader tests.
- Keep code comments and API documentation concise and in English. Write technical documents and
  user guides in English. Communicate with the user in Chinese.

## Delivery

- The orchestrating agent owns final diff review, authoritative verification, precise staging,
  commits, and delivery unless the user explicitly delegates those responsibilities.
- Before claiming completion, verify every requirement and gate named by the active unit against
  current repository or hosted evidence. A green narrow test does not prove a broader contract.
- Do not edit an implementation plan merely to mark progress. Update plan authority or scope only
  when the user explicitly asks for a planning change.
