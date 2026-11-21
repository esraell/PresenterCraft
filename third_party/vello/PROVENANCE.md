# Where this copy of Vello comes from, and what was changed

This folder is the `vello` crate, version 0.12.0, as published on crates.io on 2026-11-11
(<https://crates.io/crates/vello/0.12.0>), built from commit
`b7e8f523b45c07a5edb806bb7241ef5afe34a882` of <https://github.com/linebender/vello>, where it
lives in `research/vello_research`. `.cargo_vcs_info.json` is the record crates.io ships with it.

Vello is copyright the Vello Authors and licensed under Apache-2.0 OR MIT; both licences are
here as `LICENSE-APACHE` and `LICENSE-MIT`, unchanged. SoftPresenter's changes are offered
under the same two licences, so the folder stays usable on Vello's own terms.

The workspace's `Cargo.toml` puts this copy in place of the published crate with
`[patch.crates-io]`. `vello_encoding` and `vello_shaders` are still the published 0.12.0
crates, unchanged; no shader is changed.

## Why

Vello's compute stages write into scratch buffers (lines, tiles, segment counts, segments,
per-tile command lists) whose sizes must be set before a frame goes to the GPU. Vello 0.12
gives them constant sizes, "hand picked to accommodate the vello test scenes as well as
paris-30k" (`vello_encoding/src/config.rs`): about 165 MB, which every `Renderer` keeps for
good. It also never checks whether a frame outgrew them: such a frame is simply not drawn, and
`render_to_texture` still returns `Ok`.

For SoftPresenter that was wrong both ways. An ordinary slide needs a small part of that
memory, and the app has two renderers. And a slide full of small outlined text on a 4K screen
needs more lines and segments than the constants allow, so it was not drawn at all.

Upstream has nothing newer to move to: on 2026-11-20, `main` had the same constants and the
same unfinished check ("TODO: apply logic to determine whether we need to rerun coarse"), and
Vello's README still lists it among its limitations
(<https://github.com/linebender/vello/issues/366>).

## What was changed

Everything is marked `SoftPresenter:` in the source. Removed from the published crate:
`Cargo.lock`, `Cargo.toml.orig` and `.cargo-ok`, which only crates.io's packaging uses.

- **`src/scratch.rs` (new, SoftPresenter's own):** the sizes of the scratch buffers for one
  frame (a resting size, what recent frames used, and a guess from the scene's path count for
  one not drawn yet), how they are judged against the counters the GPU kept, how they grow
  after a frame that ran out of room (never past 1 GiB for one frame), how long a heavy
  frame's sizes are remembered, and which scenes are known to fit already. Its tests run with
  `cargo test -p vello --lib scratch`.
- **`src/lib.rs`:**
  - `Renderer::render_to_texture` now draws in two submissions. The coarse stages run first
    with scratch sizes taken from what recent frames used; their counters are read back
    (waiting for this frame's own submission only, by asking the device again and again and
    never waiting inside it, where a lock would hold up another thread presenting a window);
    if anything ran out of room the frame is
    run again with more, and only a frame that fitted goes on to the fine stage. `Ok` now
    means the scene was drawn. A scene that was drawn before at the same size and fitted (a
    playing video, frame after frame) is not waited for: the same scene always counts the
    same.
  - Vello's own single pass is kept as `Renderer::render_to_texture_unchecked`, and is still
    what a renderer with `use_cpu` runs.
  - New: `Renderer::rest` (give back what a heavy frame took, when drawing stops),
    `Renderer::set_fixed_scratch`, `Renderer::spare_bytes`, `Renderer::scratch_wanted`,
    `Renderer::scratch_reruns`, `Renderer::scratch_unwaited`; `Error::SceneTooHeavy` and
    `Error::GpuWait`; the types `ScratchSizes`, `ScratchPlan` and `ScratchRoom`.
- **`src/render.rs`:** `Render::render_encoding_coarse` takes the scratch room for the frame
  and writes it over the constants `RenderConfig::new` starts with, and takes a fingerprint
  of the scene as it is sent to the GPU; `Render::abandon_fine` frees a frame that will be
  run again; `Render::scratch_plan` and `Render::scene_print`.
- **`src/wgpu_engine.rs`:**
  - `run_recording` returns the submission it made, so that a caller can wait for it.
  - The buffer pool notes when each spare buffer was handed back and can give buffers back to
    the GPU (`release_idle`, `release_larger_than`, `release_spare`, `spare_bytes`). Upstream
    keeps every buffer it ever made.
  - `SIZE_CLASS_BITS` is 2, not 1: a buffer is at most a quarter larger than asked for, not a
    half, which matters for buffers of tens of megabytes.
- **`src/util.rs`:** `RenderContext` asks the GPU for its largest storage buffer sizes (up to
  1 GiB) in place of the default 128 MiB, so that a heavy frame's buffers are allowed.

## Updating

To move to a newer Vello, check first whether it sizes these buffers itself (then this copy
can go). If not, copy the new crate here and carry the changes above across; the picture tests
in `crates/sp-render/tests/golden.rs` say whether heavy slides still draw.
