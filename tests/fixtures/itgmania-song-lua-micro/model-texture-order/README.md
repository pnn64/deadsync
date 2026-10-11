This control compares production Model material clocks with an independent
native ITGmania actor tree across 121 updates at 60 Hz. The four Models cover
queued `setstate`, a Model's own hibernation, parent hibernation, and a parent
update rate of two. Both texture states use the same image with unequal delays
and different translations, so texture identity alone cannot hide an ordering
error.

`control.json` invokes compiled `Actor::Update`, native queued command dispatch,
`Model::SetState`, and `Model::Update`. Native Model updates run actor commands
before materials and retain the incoming delta during their own hibernation.
A sleeping parent does not update its children. Parent update rate scales the
incoming child delta. The native source is ITGmania `5c737928`; harness 0.1.41
at `b57bf3c` records the actual draws independently of its Lua song host.

Reproduce the native control with
`itgmania-harness-rs actor-conformance control.json --out native-raw.json`.
`native.json` retains all 844 native draws, input hashes, the executable pin,
and raw output hash. The paired `default.lua` runs through DeadSync's actual
compiler without overriding material clocks or supplying expected UVs.

The playback comparison covers both song Model and noteskin Model builders:
1,688 draws and 10,128 transformed UV coordinates at a 0.000001 tolerance.
It checks material history on every native draw after wakeup; draw suppression
itself is not compared by this playback test. The harness separately compares
both suppressed and visible draws with the native scene. Whole-song Model
parity remains pending and its acceptance guard stays closed.
