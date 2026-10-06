# kagetsu current result (0.5.1794)

The frozen project chart `[FULL SONG] Kagetsu no Yume [wrsw]/Kagetsu no Yume.ssc` passes
**62,008/62,008** existing semantic and runtime-modifier comparisons.
Its single Challenge chart, hash `813d600a24bdd875`, description
`MODS STR [24'7|1-15'8'7/7]`, and random seed 1 are pinned by the corpus test.

The previously retained reference reports 30,814/30,830
against the current implementation. It predates the native float song-beat
and actor-countdown corrections. A complete current local headless capture
matches without production workarounds or tolerance changes. Every emitted
observation is retained; differing totals reflect the native dispatch
schedule and captured writes. Delete the superseded raw reference and
retain the exact compressed current capture.

The capture has 13,358 update frames, ends at beat
511.9983825683594 and 222.6079924210258 seconds,
and has zero Lua errors or dropped events. Settings: cel, dance-single
Challenge, seed 1, beat step 0.125, max events 2,000,000. Raw SHA-256:
`f2a21dbc53d8774de46f4291ec3c0f266e1d4e225ed9940f26290a242cb6a152`. Compression has an exact verified byte round trip.
The manifest retains source, harness, executable and capture hashes. The
complete noteskin inventory and source hashes remain inside the trace.

Validation: the pinned corpus test passes in MAIN's isolated checkout.
No downloads, ITGmania windows, song edits or frozen-project edits. The
comparison scope remains the existing semantic and modifier audit; it
does not claim whole-game pixel or interactive-input parity. Both Cargo
files bump exactly once from 0.5.1793 to 0.5.1794 in this commit.
