# Startup self-queue

`actor-input.json` is executed by the harness actor-conformance command,
which calls compiled ITGmania Actor::Sleep, QueueCommand and UpdateTweening.
The song Lua actor adapter is not used for these observations.

SelfQueue queues Move again from Move. Native command counts at 0, 1/60,
2/60, .05 and .25 seconds are 0, 2, 4, 7 and 35. FiniteMove uses five
queued commands to produce x positions 1, 3, 5, 6 and 6 at those times.
The DeadSync startup_self_queue_moves regression uses a self-queued Lua
Move with GetX()+1 and stops at x=6; it checks the same sampled cadence.

The latest local Actor.cpp and Actor.h are byte-identical to the compiled
vendor sources. Original native numbers, queues and observations are
retained unchanged in native.json.
