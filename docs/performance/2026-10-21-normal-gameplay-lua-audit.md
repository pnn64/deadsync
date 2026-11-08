# Ordinary gameplay: song-Lua isolation audit

Parent: `a7b93aa06cfc18938e52430caeca6bf779fe1a80`. Toolchain: Rust 1.98.1.

## Pass 1: loading and preparation

The application loader already returns an empty prepared result before constructing a compile context when both background and foreground Lua declarations are absent. Normal charts do not create a Lua VM. Empty runtime-window preparation already returns before building event/timing windows; its native Player placement defaults remain necessary to preserve existing layout.

`FrameScratch::new` now returns native field/HUD storage before constructing Lua overlay order, topology, timeline, proxy, capture, or mesh plans for ordinary charts. Font prewarming no longer reserves a Lua-only font hash set before encountering a Lua font. Live scenes explicitly enable scripted composition on attachment; background-only and foreground-only scripted songs retain their existing compositor.

## Pass 2: frame, note and input paths

Ordinary charts now use a native composer that shares notefield buffers and the segment iterator. It skips Lua actor timeline evaluation, proxy analysis, capture planning, actor styling and Lua camera setup. Native chart attacks, prepared Player placement, cover, HUD, foreground images and field cameras retain their existing behavior. Placement when logical viewport and display metrics differ was checked against the existing compositor and preserved.

Disabled position splines return the native position/direction before per-note spline sampling or offset arithmetic. Disabled receptor splines skip evaluation. Existing blend behavior is retained. Screens without a Lua scene return after the native input phase, before Lua pending-input, held-repeat and draw/transition bookkeeping. Lua uptime conversion is also conditional on a scene.

Judgment callbacks are optional and absent on native screens. Mine-avoid callback collection is already guarded; the normal scoring, attack, timing and noteskin paths remain active.

## Validation

- Gameplay unit tests: **818 passed, 0 failed, 2 ignored**.
- Notefield unit tests: **480 passed, 0 failed, 2 ignored**.
- Final relevant shell selection: **18 passed, 0 failed, 8 ignored**. Thirteen existing Lua fixture failures were excluded only after isolated execution and reproduction in the older cached shell executable.
- **48 exact RenderFrame comparisons**: dance and pump single/double/versus, P1/P2, HUD visible/hidden and song visuals enabled/disabled. These include unequal viewport/display dimensions, per-player cover and an activated native foreground image. Additional existing comparisons cover notes, holds, mines, feedback, reverse, models and autoplay.
- **60 update frames** verify absent VM/judgment hooks, empty Lua output, an unconsumed Lua input iterator and exactly one native input phase per frame.
- The attack-visibility fixture now supplies one second of elapsed approach time instead of asserting final attack targets after a zero-time update. The prior assertion also failed in the older executable.
- Final build and selected test process both returned zero. Source and executable SHA-256 values are in the accompanying JSON.

The full isolated shell regression group yielded 18 passing, 13 failing and 8 ignored cases. The 13 failures concern existing scripted Lua fixtures and reproduce in the cached October 19 executable; that executable is not an exact rebuild of the parent. Their names are recorded in the JSON. A normal in-process full run also suffers cascading poisoned-session-lock failures after its first failing Lua fixture. This audit does not claim a passing broader Lua suite.

These checks compare rendering data and simulation behavior; they do not replace real-GPU pixel capture or hardware performance measurements. No percentage speedup is claimed.
