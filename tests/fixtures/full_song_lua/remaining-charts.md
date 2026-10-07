# Remaining song Lua captures

Audited all 501 simfiles in `lua-songs`. The harness repair recovered 71 additional complete archives: 483 simfiles now have indexed captures and 18 remain pending.

Only captures with zero runtime errors, zero dropped observations, and the full native endpoint were published. Archive and member hashes, dependencies, original simfile bytes, and unchanged contents after level-22 recompression were verified. All existing fixtures were preserved. DeadSync fixture tests were not run.

The repair includes 18 valid simfiles with no Lua references; their complete archives have empty actor captures. Both `.sm` and `.ssc` variants were audited separately. Song sources were left unchanged.

Loop and Space Creation Theory were captured on **Edit**, matching their bundled multitap data. Fiji was captured on **Hard**; its Challenge path still requires unsupported `SongUtil.GetPlayableSteps` theme context. These choices are recorded in each capture. Megalovania's file generation ran in an isolated corpus copy, and its archive retains original source inputs.

Remaining categories: 6 capture_timeout, 6 missing_song_lua, 5 source_error, 1 unsupported_theme_context. Missing files and source errors require song input repairs. Metronogik needs full theme HUD support. Timeout captures remain incomplete; no diagnostic or partial archive was published.

See [capture-report.json](capture-report.json) for capture provenance, contexts, and exact statuses. Diagnostic paths are relative to the sibling `../itgmania-harness` repository and remain under its ignored `target/song-lua-repair` directory.

| Simfile | Category | Reason |
|---|---|---|
| 308-MISC-[lv.Quest] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/ArrowQuest.sm | capture_timeout | native capture exceeded 600 seconds |
| 308-MISC-[lv.Quest] [Tech Spectrum Super]/ArrowQuest.sm | capture_timeout | native capture exceeded 600 seconds |
| [12] Ascendanz/Ascendanz.ssc | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/[12] Ascendanz/lua/default.lua: No such file or directory |
| Ascendanz/Ascendanz.ssc | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/Ascendanz/lua/default.lua: No such file or directory |
| Collaboration with Paul J Kim/Something.sm | source_error | corpus:/Collaboration with Paul J Kim/Nothing/default.lua:6: attempt to index global 'SREENMAN' (a nil value) |
| Cursed Metamorph - [Zaia]/saved/Cursed Metamorph.ssc | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/Cursed Metamorph - [Zaia]/saved/lua/default.lua: No such file or directory |
| Dongo Bongo/dongobongo2.ssc | source_error | corpus:/Dongo Bongo/despair/default.lua:198: attempt to index global 'SREENMAN' (a nil value) |
| james/james.sm | capture_timeout | native capture exceeded 600 seconds |
| james/james.ssc | capture_timeout | native capture exceeded 600 seconds |
| Lesson by DJ/Lesson by DJ.sm | source_error | ITGmania oracle failed: ITGmania loaded no charts; source #NOTES header lacks its colon |
| Lots of Spices - [tari]/spices.ssc | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/Lots of Spices - [tari]/template/default.lua: No such file or directory |
| mawaru9/mawaru9.sm | capture_timeout | native capture exceeded 900 seconds |
| Metronogik/Song.ssc | unsupported_theme_context | corpus:/Metronogik/code/update.lua:439: bad argument #1 to 'ipairs' (table expected, got nil); full Simply Love HUD text actors are not instantiated by this host |
| mr sandman/mr sandman.sm | capture_timeout | native capture exceeded 900 seconds |
| Pokemon Gym Battle Remix/pokes.ssc | source_error | ITGmania Lua host failed: corpus:/Pokemon Gym Battle Remix/jokes/default.lua:18: attempt to perform arithmetic on global 'counter' (a nil value) |
| Rayc Fire 2/Rayc Fire 2.sm | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/Rayc Fire 2/lua/LUA.lua: No such file or directory |
| Rayc Fire 2/Rayc Fire 2.ssc | missing_song_lua | ITGmania Lua host failed: cannot open corpus:/Rayc Fire 2/lua/LUA.lua: No such file or directory |
| Sequential Dreambox - [Zaia]/Sequential Dreambox.ssc | source_error | ITGmania Lua host failed: corpus:/Sequential Dreambox - [Zaia]/lua/FastEnd.lua:47: '<eof>' expected near 'local' |
