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

## Pull requests

One shape for every repo of mine: `skill://opening-a-pull-request`. The issue and its
neighbours before the branch, the branch name Linear renders on the issue, Conventional
Commits in the first person, the body's four sections from
`.github/PULL_REQUEST_TEMPLATE.md` (Screenshots is never deleted), an independent review
applied in a second commit, and the card closed only against evidence. What is true only
here:

- **Scopes** for the subject: the crate or surface a change actually touches, read from
  the log rather than invented: `sys` (`parakeet-cpp-sys`: `build.rs`, the CMake
  invocation, bindgen, bumping the `vendor/parakeet.cpp` pin), `wrapper` (`parakeet-cpp`:
  the safe API, `Model`, `Error`), `stream` (`StreamSession` and its two
  implementations), `spike` (the `spike` example and its harness), or a backend name
  (`dl`, `windows`) when a change is specific to one. Leave the scope off for something
  repo-wide (docs, CI, the workspace `Cargo.toml`) rather than force one on it.
- **Required check**: the ruleset names `ci` as the sole required status check, but no
  job in `.github/workflows/ci.yml` is actually named `ci` (its checks are
  `lint (rustfmt + clippy)` and the six `build-<os>-<static|dl>` matrix cells): measured
  on #6, where all seven passed and the merge stayed `BLOCKED` on `Required status check
  "ci" is expected.` `bypass_actors` is empty, so this also blocks an admin merge, not
  only an ordinary one. Every PR here is unmergeable through the UI or `gh` until either
  the ruleset's context is corrected to a real check name or the workflow gains an
  aggregate `ci` job the way `pitchbox` and `mastro` do; do not work around it with a
  ruleset edit to force a merge through.
- **Merge**: squash is the only allowed method (`allow_squash_merge` true, everything
  else false) and `allow_auto_merge` is off, so watch checks by hand
  (`gh pr checks <n> --watch`) and merge with `gh pr merge <n> --squash --delete-branch`.
  `delete_branch_on_merge` is already on remotely; local `main` still needs
  `git checkout main && git pull --ff-only` afterward.

`vendor/parakeet.cpp` is upstream's own code, pulled in as a submodule and bumped by the
procedure in `CONTRIBUTING.md`, never edited or reviewed here: this shape covers a
change to this repo's own Rust, its build script, its CI or its docs, never a change
inside that submodule.
