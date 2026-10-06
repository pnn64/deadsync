# Sharkmode comparison result

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
