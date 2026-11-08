# And Drugs whole-song parity

Frozen chart: `d79aed6b06cb7912`, `And Drugs/and drugs.ssc`, dance-single
Challenge, description `[FX] XMOD XO FS+ BR+ SS DS DT BU`. The local chart matches the
frozen identity. All files and resources come from the local workspace;
nothing was downloaded, and the frozen project manifest remains unchanged.

## Whole-song reference

The frozen report listed 14,839/14,867 comparisons. The complete current native
trace passes **29,680/29,680**, including all 29,662 runtime modifier targets
and 12 player transform ranges. Compilation and layer checks also pass.
The existing shared fixes cover this chart. The retained regression keeps
every observation and the existing tolerances; removing the modifier targets
must fail its countercheck.

The local reference covers both players using cel at 854x480, seed 1, 60 FPS,
and beat step 0.125, through beat 265 (127.20000776367236 seconds). It has
47,478 events, one Lua layer, zero runtime errors, and zero dropped events.
The 4,619,661-byte JSON compresses to 310,109 bytes with an exact round trip.
Its raw SHA-256 is
`f37ce1114ccace7b7863c0d1ee839050db2cfc0e154399beeddbe89bcf164f42`.
Provenance pins 30 local reference, harness, and song files.

## Validation and publication

Rework and MAIN independently pass all 129 semantic tests, including the
retained whole-song pass and the missing-modifier countercheck. The library code
is unchanged since Igaku's independently passing 2,572 package tests per repo;
those checks remain valid without repeating the same build. The local harness
passes 142 tests. MAIN is version `0.5.1756`, exactly one patch above `0.5.1755`,
with Cargo.toml and Cargo.lock included in the curated commit. Rework retains
the changes without committing them.

These checks cover no-input headless execution and the production modifier
evaluator. They do not constitute an interactive GPU screenshot or audible
playback check. Frozen/local identity discrepancies remain recorded in
[`song-lua-mawaru6-progress.md`](song-lua-mawaru6-progress.md) for user correction.
