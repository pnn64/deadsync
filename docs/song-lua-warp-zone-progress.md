# Warp Zone whole-song parity

Frozen chart: `9fc2acb0fcec215b`, `(R10) Warp Zone/warp zone.ssc`,
dance-single Challenge, description `MODS BR++ XO+ FS`. The local identity
matches. All song files and resources come from the local workspace; nothing
was downloaded, and the frozen project manifest remains unchanged.

## Whole-song reference

The frozen report listed 3,909/3,926 comparisons. The new complete local native
trace passes **9,201/9,201**, including all 7,584 runtime modifier targets,
482 projected geometry checks, 960 color checks, 122 vibration checks,
12 update values, eight persistence checks, and six player proxy sources.
Compilation, layers, final rendering, and the message timeline also pass.
The existing shared fixes cover this chart; this change retains its full
regression at the current tolerances. Removing its captured portal updates
must fail the projected geometry countercheck.

The reference covers both players using cel at 854x480, seed 1, 60 FPS,
and beat step 0.125, through beat 334 (156.5625 seconds). It has 63,361
events, one Lua layer, zero runtime errors, and zero dropped events.
The 4,234,663-byte JSON compresses to 252,351 bytes with an exact round trip.
Its raw SHA-256 is
`7841afa3645492a0c7a8c11ee2c15e9ea05bc5a3e3cadb9e17dbdff3ec29b415`.
Provenance pins the local native reference, harness, and song resources.

## Validation and publication

Rework and MAIN independently pass all 128 semantic tests, including the
retained whole-song pass and the missing-portal countercheck. The library code
is unchanged since Igaku's independently passing 2,572 package tests per repo;
those checks remain valid without repeating the same build. The local harness
passes 142 tests. MAIN is version `0.5.1755`, exactly one patch above `0.5.1754`,
with Cargo.toml and Cargo.lock included in the curated commit. Rework retains
the changes without committing them.

These checks cover a no-input headless whole-song trace and production
composition. They do not constitute an interactive GPU screenshot or audible
playback check. Frozen/local identity discrepancies remain recorded in
[`song-lua-mawaru6-progress.md`](song-lua-mawaru6-progress.md) for user correction.
