---
type: "Verification Evidence"
title: "RGD-U10 Linux X11 consumer diagnosis and local repair"
description: "Records the second protected candidate failure, exact Ubuntu 24.04 X11 runtime diagnosis, and locally verified workflow repair without claiming hosted completion."
timestamp: 2026-07-26T08:37:59Z
record_id: "4e1b673441774fd99f3637586cb2df4f"
tags: ["rgd-u10", "candidate", "hosted", "linux", "x11", "windows", "partial"]
status: "partial"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "aa9b564821d88cb2ac01be577543f79dca9386cf"
verified_by: "codex-root"
---

# Verification

The second one-shot protected `Reference Game Candidate` dispatch ran from
`c55146c8428c0d6888bbff299da85c565673b95f` as GitHub Actions run
[`30186343288`](https://github.com/Latias94/nara/actions/runs/30186343288).
Both candidate builds and the Windows no-checkout consumer succeeded. The
Linux no-checkout consumer verified and extracted the exact transport, then
failed its desktop render probe before RGD-U10 could complete.

# Result

The Linux consumer job `89753212024` failed with
`desktop_render_probe.product_path_failed`; Linux build `89751694387`, Windows
build `89751694398`, and Windows consumer `89753212035` succeeded. The Linux
transport artifact was `23,244,039` bytes with artifact SHA-256
`7fbb241a32520a0acde559a9bbaa2c5ba3ab527612a6c87fd12cedf1980bd8f1`.
Its candidate archive was `23,167,150` bytes with SHA-256
`8bf613a227bdc6d6bc61aeed89d26194bb31e18bfb269155de44eab7e3283d60`,
expanded to `73,922,503` bytes across 17 files, and retained the expected source
revision.

The exact Linux candidate reproduced on an isolated Ubuntu Base 24.04.4
userspace with glibc 2.39, Xvfb, Mesa 25.2.8 llvmpipe, and the same sanitized
random home/cwd contract. Headless passed while both the packaged desktop and
render probe failed. Adding the X11 client libraries dynamically opened by
Winit 0.30 (`libX11`, `libX11-xcb`, `libXcursor`, `libXi`, `libxkbcommon`, and
`libxkbcommon-x11`) changed the same full bundle smoke to success without
changing candidate bytes. The completed receipt reported the same archive
identity, headless summary schema, file count, expanded bytes, and
`desktop_probe: completed`.

# Evidence

- Commit `aa9b564821d88cb2ac01be577543f79dca9386cf` makes the Linux consumer
  software profile explicit, includes Xauth/Xvfb and the Mesa Vulkan fallback,
  and uses `--no-install-recommends` so success no longer depends on incidental
  runner-image packages.
- `tests/ci_policy.rs` now rejects removal of `libxkbcommon-x11-0` and pins the
  exact ordered Linux consumer profile.
- `cargo nextest run --locked --test ci_policy --test artifact_package_policy
  --test-threads=1` passed 19/19 tests with one Cargo build job and the shared
  repository target directory.
- `cargo fmt --all -- --check` and `git diff --check` passed.

# Follow-up

The workflow/policy change invalidates the RGD-U8 hosted verdict at
`26009e4dc3294eafbf19b35915436b30e13f47e0`. A separately authorized push of
the repair must first produce a successful ordinary six-cell hosted CI run.
Run `30186343288` consumed the prior candidate dispatch authorization, so a
new explicit one-shot authorization is then required before the protected
candidate workflow may run again. This partial evidence authorizes no
approval, tag, Release mutation, or publication.

# Citations

- `.github/workflows/reference-game-candidate.yml`
- `tests/ci_policy.rs`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- GitHub Actions run `30186343288`
- Commit `aa9b564821d88cb2ac01be577543f79dca9386cf`
