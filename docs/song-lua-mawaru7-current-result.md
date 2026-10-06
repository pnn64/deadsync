# mawaru7 current result (0.5.1805)

The frozen project chart `mawaru7/mawaru7.sm` passes **1,454,202/1,454,202** existing
semantic and runtime-modifier checks. Pin hash `f7fdd8fafaee6188`,
dance-single Challenge, description `TaroNuke (converted by MrThatKid4)`,
and seed 1.

The original completion audit reports 874,663/874,665 against
the superseded reference. The current full capture reproduces two visibility failures and adds two
crop failures at idle boundaries. Sparse render tracks had closed a held
interval with the actor state already changed by its next command. This
created a ramp throughout the idle interval. Preserve the last captured
render value until the preceding update frame, then retain the command
state at its original dispatch frame before appending the next write. Replace
the two endpoint paths that used the changed state. This fixes hidden
grade sprites fading toward the next appearance and cropped doors
slowly opening while idle. The four native queue/update regressions pass.
Comparator code and tolerances are unchanged. The
reference records the current native float beat and countdown behavior;
retain every emitted observation in the current selected capture. Keep
the shared micro fixture: its unchanged standalone regression also passes
874,665/874,665 checks after the actual sparse-track fix. The targeted
idle-boundary regression passes 154/154 checks.
The capture preserves the previous full beat/time horizon and 60 Hz update
frequency.

Passing sections:

```text
  compile info         12/12 ok
  layer order          4/4 ok
  final render         1204/1204 ok
  judgment textures    8/8 ok
  sprite textures      566/566 ok
  render persistence   55466/55466 ok
  update values        163718/163718 ok
  player ranges        8/8 ok
  projected geometry   205778/205778 ok
  draw colors          395720/395720 ok
  draw crops           229532/229532 ok
  draw shadows         344298/344298 ok
  projected vibration  57383/57383 ok
  timeline             178/178 ok
  message commands     195/195 ok
  runtime modifiers    132/132 ok
```

Retain 17,086 update frames, ending at beat
816 and 284.7352863536562 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`c8b74a49a46838cf0085873289c81211c59f30eaa4cfb75ec645568ecd68d309`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The complete pinned corpus test passes
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1804
to 0.5.1805.
