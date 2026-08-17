# AGENTS.md

Orientation for AI agents working in this repo. `CONTRIBUTING.md` is the source of
truth for the build model, coding standards and how to bump the vendored upstream —
read it before touching `build.rs` or anything under `vendor/`. This file only adds
what an agent needs that CONTRIBUTING.md does not say.

## Toolchain

The Rust channel is pinned in `rust-toolchain.toml` (1.88, with the `rustfmt` and
`clippy` components); `rustup` picks it up automatically. The native side needs
CMake 3.15+ and a C/C++ toolchain (clang/gcc, or Xcode CLT on macOS), plus the two
git submodules under `vendor/` (`vendor/parakeet.cpp`, and its own nested
`third_party/ggml`): `git submodule update --init --recursive` is mandatory,
`build.rs` aborts with a clear error if `vendor/parakeet.cpp` is empty.

## What CI covers

`.github/workflows/ci.yml` runs on push to `main`, on every PR, and on manual
dispatch:

- `lint`: `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`, Ubuntu only.
- `build`: a matrix over macOS/Ubuntu/Windows x {static, dynamic-backends}, each
  cell running `cargo build --release` then `cargo test --release` with that cell's
  feature flags (Vulkan on Ubuntu/Windows, Metal implicit on macOS).

It does not build or test the `cuda` or `hip` features — there is no GPU runner —
and it never runs the integration tests that load a real model (gated on
`PARAKEET_TEST_MODEL` and friends, see CONTRIBUTING.md), because those environment
variables are never set in CI. Don't report a CUDA/HIP change or a model-dependent
code path as CI-verified; both need a local run with the right flags or variables.

## Warm build dir across worktrees

`target/` sits at the workspace root and is not redirected anywhere in the repo
(one full build is already ~330 MB, mostly the vendored C++ build). If you are
running more than one worktree of this repo at once, point every worktree at the
same target dir instead of letting each compile the vendored library from scratch:
`export CARGO_TARGET_DIR=$(git -C <main-checkout> rev-parse --show-toplevel)/target`
before building — Cargo's target lock then serializes the heavy compiles. Each
worktree still needs its own `git submodule update --init --recursive`: submodule
checkouts are per-worktree, only the compiled artifacts are shareable.

## Branch flow

`main` is protected by two rulesets, `protect-default-branch` (no force-push, no
deletion) and `require-pull-request` (every change lands through a PR, squash is
the only allowed merge method). `delete_branch_on_merge` is on, so a merged branch
is gone from the remote on its own — no manual cleanup. There is no linked GitHub
Project, no milestones and no custom labels on this repo today: it does not use the
board conventions the SvelteKit projects (canonry, mastro) use.
