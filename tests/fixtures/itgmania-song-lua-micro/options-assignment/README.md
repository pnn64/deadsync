This complete native options control checks empty assignment, numeric-only
assignment, explicit skin reset, and explicit skin replacement against
compiled ITGmania PlayerOptions and ModsGroup code. Original reference
sources are unchanged.

The old DeadSync binary fails the first preserve-skin assertion. The fixed
Lua runtime passes all assertions, the semantic regression, and all 16
noteskin API checks in the full comparator. The full control still has two
numeric playback/comparator failures. Those remain open in provenance.json;
this control is not claimed to have complete gameplay parity.

No tolerance, original chart, source Lua, archive index, or prior fixture is
removed or weakened. See provenance.json for the source contract, exact
binaries, and the two unchanged pre-existing unit failures.
