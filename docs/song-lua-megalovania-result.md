# MEGALOVANIA current result

The supplied `Megalovania SM5 [TaroNuke]/megalovania.ssc` passes all 533,837/533,837 current comparisons.
The headless reference reaches beat 788 with no
Lua runtime errors or dropped events. MAIN independently checks this result
before its 0.5.1774 promotion commit. Passing charts stay out of the investigation
queue unless a regression is reported.

ITGmania clearall calls PlayerOptions::Init; it is not a numeric option.
The audit now checks the reset values of all options used by the trace,
including speed mode, perspective and timer defaults. A regression proves
that incorrect resets fail, while subsequent writes in the same call win.
This replaces the obsolete raw reference and the false clearall getter audit.

Frozen identity: `f8c29e15fbbe460e`. Supplied local identity: `f8c29e15fbbe460e`.
The frozen project manifest and song resources are unchanged. No files were
downloaded. The current headless result does not establish full native screen
pixels or interactive input branches.
