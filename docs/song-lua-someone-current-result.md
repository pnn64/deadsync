# someone current result (0.5.1804)

The frozen project chart `Someone Special - hubert/Someone Special.ssc` passes **1,067,542/1,067,542** existing
semantic and runtime-modifier checks. Pin hash `59bba16f2331d939`,
dance-single Challenge, description `Hubert x Crash Cringle`,
and seed 1.

The original completion audit reports 1,256,674/1,257,106 against
the superseded reference. The current capture retains the local ArrowEffects Mini scaling behavior;
the previous trace had 400 stale XY observations during Mini changes.
Retain immediate Actor::Update(0) writes in the initial gameplay state
rather than deferring them through queued startup commands. This fixes
the 32 actual initial visibility/position failures. Replace the consuming
capture loop while keeping queued blocks on their existing path.
Queued-command, update-order, retargeting and zero-time-read native
regressions pass. Resolve native noteskin:/ texture paths against the
hash-verified local inventory, retaining strict path and render-key
equality. This fixes all 104 texture-binding comparisons without
changing tolerances or dropping checks. The headless loading-budget
regression and existing per-frame runaway guard also pass. The
reference records the current native float beat and countdown behavior;
delete the superseded fixture and retain every emitted observation.
The capture preserves the previous full beat/time horizon and 60 Hz update
frequency.

Passing sections:

```text
  compile info          4/4 ok
  layer order           2/2 ok
  final render          334/334 ok
  player proxy sources  4/4 ok
  sprite textures       104/104 ok
  player ranges         4/4 ok
  column splines        886/886 ok
  multitap zoom         974256/974256 ok
  multitap writes       90176/90176 ok
  projected geometry    105/105 ok
  draw crops            420/420 ok
  draw shadows          630/630 ok
  projected vibration   105/105 ok
  runtime modifiers     512/512 ok
```

Retain 8,720 update frames, ending at beat
352 and 145.30443146289736 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`eca595592f11aacd049c589d13591d2385e89fc4cc8432d4c2cec763a8953dd8`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The complete pinned corpus test passes
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1803
to 0.5.1804.
