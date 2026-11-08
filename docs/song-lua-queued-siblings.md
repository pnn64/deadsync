# Queued commands on later actors

Mawaru8 broadcasts `ShowGame2`, then sets `mawaru_curgame` to 2 in the
same controller command. The receiving actor queues `Start`; its child
starts the recurring rerave reader only when that variable is 2.

DeadSync drained the receiver's zero-time queue inside the broadcast.
`Start` consequently read the previous game number and never scheduled
the reader. The note pool was populated, but all 48 used note sprites
remained hidden. ITGmania waits until the receiving actor's own
`Actor::UpdateTweening` phase before dispatching its queued command.

Defer a ready queue on a later actor until that actor advances. Keep its
command and queue clock intact, allowing the broadcasting Lua body to
finish updating shared state. Direct commands retain their existing
dispatch behavior.

The small `delayed-pool` fixture reproduces the broadcast, variable change,
delayed registration, conditional reader startup, note fade and hide.
Its local ITGmania capture contains 63 comparisons, all passing after
the fix. Before the fix, the actors stayed hidden and retained alpha 1
instead of completing their fade to 0. The existing 100 regular semantic
tests also pass.

MAIN validation passes 781 song-Lua unit tests and all 101 regular semantic
tests, including the new fixture.

Capture provenance: ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`, clean reference tree; embedded
bundled Lua; seed 1; dance-single Challenge; both players; 854x480;
0.125-beat samples. The local harness uses native float tween arithmetic,
checked against the linked ITGmania actor oracle. The JSON SHA-256 is
`f6013347e569d0f04535bf3a7f92d537f5bb9078665806312319a8376afb0181`.

Against the same pre-existing Mawaru8 capture, the whole-song audit now
reports 96,719/97,433, compared with 94,751/95,993 before this fix. The
additional comparisons cover the newly visible sprites. All 1,088 final
actor-state checks pass. Geometry, random placement, modifier target
checks and five speculative message captures still require investigation;
this does not mark Mawaru8 as a complete pass.

The supplied local chart hash is `8224fb7e0b05040f`; the frozen project
lists `cadefe09888e9ab8`. No replacement resources were downloaded and
the project metadata is unchanged.
