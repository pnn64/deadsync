# Fof Guys current result

The supplied `fof_guys/fof.ssc` passes all 39/39 current comparisons.
The headless reference reaches beat 368.0833435058594 with no
Lua runtime errors or dropped events. MAIN independently checks this result
before its 0.5.1771 promotion commit. Passing charts stay out of the investigation
queue unless a regression is reported.

The older headless reference misclassified the join sound as a Sprite.
The current local capture records the Sound actor and passes every section.
This result covers the supplied join-screen chart; it does not assert that
interactive minigame branches were exercised.

Frozen identity: `29216cf8f382a21d`. Supplied local identity: `29216cf8f382a21d`.
The frozen project manifest and song resources are unchanged. No files were
downloaded. The current headless result does not establish full native screen
pixels or interactive input branches.
