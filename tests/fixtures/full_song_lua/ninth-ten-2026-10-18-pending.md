# Remaining song Lua charts

**490 passed fixtures, 10 pending charts, 1 ignored chart (Mawaru9).** This batch adds ten complete, accepted fixtures. Every accepted replay was compared uncompressed, then packed and verified by its decoded hash. Oversized fixtures remain local and gitignored.

Attempts below count this sweep only. Rows with zero attempts retain their concrete prior failure. Lesson SSC and Metronogik reached the two-attempt limit; neither received a third full replay.

| Chart | Attempts | Concrete failure |
| --- | ---: | --- |
| ArrowQuest.sm, Tech Spectrum Super / EXTRA CHARTS | 0 | Native capture timed out after 2,134 seconds without a complete semantic trace. |
| ArrowQuest.sm, Tech Spectrum Super | 0 | Native capture timed out after 2,134 seconds without a complete semantic trace. |
| Cursed Metamorph, saved/Cursed Metamorph.ssc | 0 | The saved directory lacks the music and foreground assets referenced by the SSC; native staging cannot load the song. |
| Happiness/Happiness.ssc | 0 | Native diva/diva.lua:1092 indexes nil FAST_BREAKS[cur_break] in Loop1Command. |
| Haxo (Sharks Remix)/haxo.ssc | 0 | Native OnCommand rejects effectmagnitude(0.9): the authored call omits required arguments. |
| Lesson by DJ/Lesson by DJ.sm | 0 | The file has no #NOTES tags; native ITGmania loads no selectable charts. |
| Lesson by DJ/Lesson by DJ.ssc | 2 | Four startup command/query mismatches, including native versus owned child tween-time ordering. The shared Footer-order repair is committed and independently tested; the chart remains pending until a later sweep can replay it. |
| Metronogik/Song.ssc | 2 | 26,279 command/query mismatches at the end phase: 130,014 native events versus 129,950 owned events; the first retained HeaderText tween-time difference is at 124.2966 seconds. |
| james/james.sm | 1 | Native capture timed out after 900 seconds without a complete trace; observed resident memory was about 20 GB. |
| james/james.ssc | 1 | Incomplete native capture stopped after 404.5 seconds. Flushed events had reached only 4.026 song seconds after 101 wall seconds. Fixed global update timing did not change Foreground's independent music-time delta and recurring timer catch-up. |

Velvet passes all 7,316 frames with the declared initially stopped collector input. Its default automatic-collector command-order differences remain [missing coverage](ninth-ten-2026-10-18-coverage.json), separate from this pending-chart count. No production runtime Lua setting was enabled.

The [JSON ledger](ninth-ten-2026-10-18-pending.json) retains detailed failure observations and evidence paths. The [batch summary](ninth-ten-2026-10-18-summary.json) records fixture hashes and verification results.
