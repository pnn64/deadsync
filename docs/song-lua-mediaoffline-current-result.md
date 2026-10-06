# mediaoffline current comparison result

The supplied `[10] media offline (SM) [Snap]/media offline.ssc` passes 59,334/59,334 existing
comparisons in MAIN's isolated verification checkout.

The update actor matcher now applies the native collector's leaf Actor
exclusion on both sides. This chart retains a plain Actor as a Lua tween
variable. Counting it only on the DeadSync side caused a false topology
failure; no native value comparison was removed. A regression verifies
that a frame transform is still compared and an incorrect transform fails.

The existing reference is retained without regeneration. This is the
current headless comparison result, not a full gameplay pixel claim.
No song resources were downloaded, no ITGmania window was launched,
and the frozen project identities remain unchanged.

Promoted MAIN version: `0.5.1783`.
