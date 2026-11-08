# waltz current result (0.5.1802)

The frozen project chart `(R6) Waltz Capriccio/waltz_capriccio.ssc` passes **148,479/148,479** existing
semantic and runtime-modifier checks. Pin hash `0d1c59154eff02b7`,
dance-single Challenge, description `TaroNuke (has mods)`,
and seed 1.

The original completion audit reports 520,436/520,967 against
the superseded reference. The complete current local headless capture passes
without production changes, comparator changes or tolerance changes. The
reference records the current native float beat and countdown behavior;
delete the superseded fixture and retain every emitted observation.
The capture preserves the previous full beat/time horizon and 60 Hz update
frequency.

Passing sections:

```text
  compile info          8/8 ok
  layer order           3/3 ok
  final render          38/38 ok
  player proxy sources  12/12 ok
  sprite textures       6/6 ok
  render persistence    980/980 ok
  update values         6184/6184 ok
  player ranges         4/4 ok
  projected geometry    1177/1177 ok
  draw colors           2328/2328 ok
  draw crops            1216/1216 ok
  draw shadows          1824/1824 ok
  projected vibration   304/304 ok
  timeline              1/1 ok
  runtime modifiers     134394/134394 ok
```

Retain 8,462 update frames, ending at beat
230 and 141.0108074295209 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`66882f61a3993b7bc700d01bfda4373117a5a232d10cf73edb4b64c7f95caade`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The direct current-trace comparison and complete pinned corpus test pass
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1801
to 0.5.1802.
