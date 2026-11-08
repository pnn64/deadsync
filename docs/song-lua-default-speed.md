# Initial speed modifier queries

Mawaru8 discovers each player's starting speed by querying
`GAMESTATE:PlayerIsUsingModifier` for `0x` through `10x` in 0.01 increments.
DeadSync's default 1x getter worked, but its initial option state omitted the
active speed fields. Every speed query consequently returned false and the
song retained its 2.5x fallback. ITGmania discovers 1x.

Initialize the active XMod value for 1x, while retaining the native omission
of an implicit 1x from `GetPlayerOptionsString`. Explicit setters still include
it. This replaces the special case that left the default speed uninitialized.

The regression uses both numeric player IDs, executes Mawaru8's rounded
speed-detection loop, verifies that querying does not mutate options, and checks
the implicit modifier string. The local ITGmania harness executes the same
speed-detection loop through its native option parser.

Validation: 781 song-Lua unit tests and 100 regular semantic comparisons pass.
This addresses the starting-speed gap; Mawaru8's full-song audit still has
other rendering and callback differences under investigation.
Against the same existing capture, its total improves from 89,665/95,993 to
94,751/95,993. The magnetic receptor position mismatches and all starting-speed
comparisons clear; this is not a claim that the complete song passes.

Local chart hashes differ from the frozen project metadata. These resources
are being tested as supplied, without downloading replacements:

| Song | Frozen project hash | Local Challenge hash |
| --- | --- | --- |
| mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Get Into It and Rhythm Hell were not found in the local song resources.
The project metadata remains unchanged for the owner to correct.
