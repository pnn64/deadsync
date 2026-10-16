# 7th Gear live proxy playback (0.5.1872)

Foreground `OnCommand` runs after the application prepares its visual slots.
7th Gear binds its NoteField, Combo, Judgment, Underlay and Overlay proxies there.
The retained Lua actors had the targets, but presentation kept the unbound
startup targets, leaving only the captured background visible.

Live visual writes now refresh proxy targets and resolve local actor identities
to their own visual layer. Source visibility follows live retargeting. Screen
entry also reserves enough direct-proxy destinations for a late whole-Player
binding, whose three source runs can exceed an initially unbound slot.

The new regression fails before the fix and passes afterward. It checks the
actual prepared visual slot after foreground activation, then retargets it to
a hidden local actor and a hidden screen layer. Fifteen distinct focused proxy
and foreground checks pass.

The normal Windows application reproduces the missing playfield before the fix.
The rebuilt application shows all four receptor arrows and the HUD through
7th Gear's three ActorFrameTexture stages. It completes 120 warmup and 120 measured
frames on Single/Challenge, chart hash `477fed829d289df2`, with gameplay preparation
at 1.243 seconds. A separate Single/Expert Fof Guys application check also
completes and shows its course, ball and countdown. The installed songs, sync
and user profiles are unchanged. Both checks use isolated data directories.

Hashes, screenshots and application results are recorded in
[`seventh-windows.json`](../tests/fixtures/interactive_song_lua/seventh-windows.json).
These checks establish the live rendering repair; development-build performance
and full native pixel parity are not certified.

## Earlier semantic result (0.5.1800)

The frozen project chart `7th Gear/7th Gear.ssc` passes **1,410,424/1,410,424** existing
semantic and runtime-modifier checks. Pin hash `477fed829d289df2`,
dance-single Challenge, description `FS+ XO BR- BU- MODS`,
and seed 1.

The original completion audit reports 228,018/228,019 against
the superseded reference. The complete current local headless capture passes
without production changes, comparator changes or tolerance changes. The
reference records the current native float beat and countdown behavior;
delete the superseded fixture and retain every emitted observation.

Passing sections:

```text
  compile info         4/4 ok
  layer order          2/2 ok
  final render         450/450 ok
  sprite textures      1/1 ok
  column splines       4584/4584 ok
  projected geometry   235075/235075 ok
  draw colors          469784/469784 ok
  draw crops           235624/235624 ok
  draw shadows         353436/353436 ok
  projected vibration  58906/58906 ok
  runtime modifiers    52558/52558 ok
```

Retain 7,815 update frames, ending at beat
238.75 and 130.22727272727272 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`662382dcb034b156b5bb2f0a3dc1dd0b57ae6032da39e7e1a0fda3575b743277`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The direct current-trace comparison and complete pinned corpus test pass
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1799
to 0.5.1800.
