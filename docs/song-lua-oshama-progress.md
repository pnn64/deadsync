# Oshama Scramble whole-song parity

Frozen/local chart `b2f5af049eeeb8f9` matches
`Oshama Scramble! (Cranky Remix)/Oshama Scramble! (Cranky Remix).ssc`,
dance-single Challenge, `MODS BR+ HS FS BU-(24ths)`. The Edit chart shares its
note hash but takes a different branch. This audit selects Challenge explicitly
and retains both background and foreground layers. No downloads, song edits
or frozen-manifest changes were made.

The initial fresh audit failed four modifier values at the opening warp and
270 modifier observations that treated invalid index-zero names as targets.
ITGmania's Init/On run at beat zero; its first zero-delta gameplay update uses
the actual song position, beat 12 after the opening warp. DeadSync now advances
the clock before that initial update and begins chronological replay at the
same position, replacing the hardcoded beat-zero replay origin. A small fixture
checks the initial Dark and Stealth values without advancing a positive frame,
and confirms startup still observes beat zero.

Local PlayerOptions::FromOneModString accepts indexed columns 1 through 16.
The song also writes confusionoffset0, movex0 and movey0; native parsing ignores
these names. The harness now retains copy-and-compare no-op evidence and queries
the native affected global/active-column fields. The audit compares those
values instead of inventing an option from the invalid requested level. Old
captures without evidence still report a gap. A native fixture checks indices
0 and 17 against existing nonzero values.

The last note is beat 608; the native specified endpoint is about beat 618.526.
That hint includes the two-player bonk at 610 and the ending graphic at 617.
The final indexed modifier eases run from 613 through 621, so the retained
reference explicitly extends its minimum endpoint through beat 624 and the
following quiet frames. It does not simulate ScreenGameplay's transition.

The complete local reference has 133,803 events, no Lua errors or dropped
events, 8,338 update frames and 325 projected samples across eight drawables.
Its 780 invalid-index calls retain 3,380 queried native field values. The
whole-song audit retains 304,077 comparisons, including 133,408 alpha/visibility
checks across every drawable update. A corrupted ending alpha track must fail.

Another 2,440 comparisons exercise production image draw geometry/tint after
normal local PNG decoding verifies source dimensions. Eight fully off-screen
ending-cat observations check empty output after viewport culling; other
off-screen draw lists retain geometry/tint checks. Native projected corners
capture the nominal pose without random vibration translation, so the draw
adapter supplies that same nominal pose to the production builder. Separate
native effect comparisons retain active vibration magnitudes and state; these
draw checks do not claim identical random offsets or framebuffer pixels.

The 15,696,593-byte reference compresses to 501,185 bytes with an exact round
trip. Raw SHA-256:
`85e9b1e94f164e9723e8cbaced499aa050a6d379b529771b7e74b2371b210114`.

REWORK and MAIN each independently pass 140 semantic tests and 965 song-Lua
package tests; the reference harness passes 149 tests. MAIN is `0.5.1762`,
exactly one patch above `0.5.1761`, with both Cargo files in the curated
commit. REWORK retains its changes uncommitted.

Audible playback, interactive input and GPU screenshots remain outside this
headless audit. Frozen/local hash discrepancies remain unchanged in
[the inventory report](song-lua-mawaru6-progress.md).
