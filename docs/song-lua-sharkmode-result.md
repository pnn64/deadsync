# Sharkmode comparison result

## Compiled playback regression, 2026-10-22

The reported background-only screen used
`280-MODS-[MASTER] Sharkmode/Sharkmode.ssc`. The installed copy's Lua files
match archive `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b`;
its simfile has a local 9 ms offset adjustment.

Production gameplay now compiles the song on the load worker and releases the
Lua VM before playback. The previous production selector retained a live scene
and bypassed whole-song compilation. The compiler also rejected explicit draws
of native screen placeholders, discarding the compiled draw commands.

The general draw rules now follow ITGmania's `ActorFrame::DrawPrimitives`,
`Transition::EarlyAbortDraw`, and `BitmapText::EarlyAbortDraw`: traverse native
frames without compositor bindings, skip waiting transitions and empty text,
and omit the transparent fallback bitmap only when its blend/depth/mask state
cannot affect the output. Unsupported renderable leaves still report errors.
Material sample lookup tolerates one ULP of foreground-origin subtraction so
an exact frame boundary does not display the previous material frame.

Validation includes production-loader assertions that no Lua VM survives,
direct and texture-pass composition at beats 16, 74, 122, 260, 290 and 340,
and Vulkan framebuffer comparisons with the actual background at beats
16, 74, 122 and 290 using the `cel` noteskin. All 399 shell tests and 187
playback tests pass. The final normal executable also completed a Vulkan
smoke run (300 warmup frames, 100 measured) on the installed song with an
isolated configuration and a screenshot showing the restored notefield.
Logs and screenshots are under
`target/sharkmode-debug` and `target/sharkmode-pixels`.

The frozen full-song ITGmania audit is **not a full pass**: 303,071 of 304,427
comparisons pass. The remaining 1,356 comparisons concern the initial quad
pose/color and the order of native screen layers in custom draws. This patch
does not change or replace the frozen reference. The framebuffer checks prove
note visibility at the sampled sections, not full-chart visual equivalence.

## Earlier headless results

The supplied `Sharkmode [Ky_Dash]/Sharkmode.ssc` Challenge chart matches the
frozen identity `f13226ac1b5eedcc`. Its complete local headless capture reaches
beat 400, retaining 9,292 explicit draw frames and 1,351 In draws with no Lua
errors or dropped events. All 109,825 current comparisons pass. The compressed
capture and its local-source provenance are retained as a regular regression.

The fixes preserve explicit draw snapshots and caller transforms, bind AMV
meshes to their AFT resources, honor capture creation, viewport, float color
and depth settings, and retain Player camera changes. Theme notefield widgets
belong to Underlay; Players draw Combo, Field, then Judgment. Explicit copies
keep that order locally, and the persistent In stage label participates in
captures. The unused two-run proxy tail and replaced rendering paths are removed.

REWORK validation passes 980 song-Lua, 913 presentation, 463 notefield, 863
gameplay and 154 native harness tests, plus the explicit GPU overlap regression.
The existing 140 regular semantic tests pass. MAIN independently passes
all 3,332 tests in the six affected package suites with zero failures and
no compiler warnings. Its selected Sharkmode result uses the same retained
complete capture; the superseded raw fixture is removed.

These totals describe the current headless comparisons. Complete native screen
inventory, the initial In animation and full-chart framebuffer output remain
unverified. The native sprite markers establish placement and ownership, not
font glyph pixels. These limitations do not reopen the passing chart without
a reported regression.

No resources were downloaded or song files changed. Frozen chart hashes stay
unchanged; the supplied-resource mismatches remain in
[the hash summary](song-lua-default-speed.md).
