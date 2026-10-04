# Modifier targets with zero approach speed

The semantic trace records requested Song-level modifier targets. The
modifier comparator previously inspected Current after a large update
delta, assuming that every target would settle. A zero approach speed
prevents Current from moving, regardless of the elapsed time.

Mawaru8 requests zero Drunk and Tipsy targets at speed zero at the ends
of its short modifier sequences. Twelve comparisons reported its frozen
Current values as missing targets. ITGmania's `PlayerOptions::Approach`
multiplies the update delta by the requested approach speed, and
`fapproach` moves by zero in that case. DeadSync correctly preserves
Current as well.

Evaluate the compiled target windows with the existing production
`apply_song_lua_attack_eases` API at the recorded timestamp. This replaces
the assumption that settled Current always equals the requested target.
The production gameplay state and approach behavior are unchanged.

The small `zero-approach` fixture first requests Drunk 1 at speed 100,
then Drunk 0 at speed 0. Its local native capture supplies the targets.
The test verifies complete semantic and target parity, separately checks
that the production Current value remains 1 after the zero-speed write,
and confirms that removing that write still produces two failed target
checks. The original audit failed 2 of 17 comparisons for this fixture.

All 102 regular semantic tests pass. This target audit does not measure
the full Current approach timeline; the separate runtime tests exercise
Current behavior with explicit elapsed time.

Against the same existing Mawaru8 capture, all 274 modifier target checks
now pass and the complete audit reports 96,731/97,433. The remaining
geometry and five speculative message-capture gaps are still failures.
MAIN also passes all 102 regular semantic tests.

Capture provenance: clean ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`, embedded bundled Lua, seed 1,
dance-single Challenge, both players, 854x480, 0.125-beat samples.
The JSON SHA-256 is
`e49b227fbf3e7fe708e52be4ebdafcf7ea78b7896953e70d34dccc9d1dfbd22e`.
The resources and capture were generated locally.
