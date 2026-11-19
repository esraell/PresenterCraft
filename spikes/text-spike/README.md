# text-spike (throwaway)

Not product code. Written 2026-11-18 to answer one question before the design: can a native GPU
renderer draw a bilingual slide with outlined, shadowed Latin and Cyrillic text, off screen and
fullscreen?

Stack tried: winit 0.30.13, wgpu 30 (through vello), vello 0.12.0, parley 0.13.0.

## Result: yes

- `result-1920x1080.png` is the off-screen render, read back from the GPU. English, Russian and
  Ukrainian letters (ї, ґ, є) all drew from a system font, centred, with a black outline and a
  soft shadow.
- Off-screen path (the one NDI would use): build scene 15 ms, scene + first render + readback
  104 ms for the first frame, including one-time setup. Steady-state readback cost not measured.
- Windowed 2560x1440: about 153 frames per second. Borderless fullscreen 3024x1898: about 115
  frames per second, rebuilding the whole scene every frame (no caching).
- Only the built-in display was connected, so "fullscreen on a chosen second display" was
  exercised only as "fullscreen on display 0". The display list and the choose-by-index code
  path ran; a real second display is still untested.

## What it taught us

- Outline: stroke the glyph outlines (round joins) under the fill. Works directly.
- Shadow: the renderer has no blur for glyphs. The spike fakes a soft shadow with six widening,
  faint strokes. Good enough to look right; a true blur would need our own pass later.
- The renderer draws through a compute pass into an intermediate texture, then copies to the
  window. That intermediate texture is also what we would read back for NDI.
- The renderer can register a GPU texture as an image (`register_texture`), which is the hook
  for putting video frames under text.
- egui-wgpu 0.37 uses the same wgpu 30 and winit 0.30.13, so an egui operator interface can
  share one GPU device with this renderer.
- These crates change their APIs often (four call sites differed from the docs I remembered).
  Pin exact versions.

## Run

    cargo run --release -- --png out.png
    cargo run --release -- --list
    cargo run --release -- --window            # Esc quits
    cargo run --release -- --display 0         # Esc quits
