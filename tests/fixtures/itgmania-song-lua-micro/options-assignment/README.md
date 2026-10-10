This complete native options control checks empty assignment, numeric-only
assignment, explicit skin reset, and explicit skin replacement against
compiled ITGmania PlayerOptions and ModsGroup code. Reference sources and
the original native trace are unchanged.

The previous runtime dropped startup numeric targets for a static actor.
The comparison also missed resets omitted by a replacement options string.
Both faults are corrected: all 5,333 full-control comparisons pass, including
eight numeric target checks and all 16 noteskin API checks. The regression
also rejects deletion of a replacement reset window.

The separate Current/Song option progression gap remains open. The broader
modifier suite has three unchanged failures, reproduced using the previous
committed binary; the Lua crate has the same two existing unit failures.
See provenance.json for exact binaries, source contracts, and results.

No original chart, source Lua, tolerance, archive index, or prior archive is
removed or weakened. This control does not establish whole-corpus parity.
