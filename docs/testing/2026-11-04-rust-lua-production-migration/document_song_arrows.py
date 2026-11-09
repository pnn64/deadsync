"""Record verified ArrowEffects work without changing the migration criteria."""
import json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
x=json.loads((d/'song-arrows-review.json').read_text())
assert x['formatting_passed'] and x['clippy_diagnostics_match_prior_baseline'] and x['without_test_support_compiles']
heading='## Rust ArrowEffects and post-health cache (2026-11-07)'
body=f'''
{heading}

The song adapter now installs the existing ArrowEffects functions on the Rust
VM: YOffset/YPos/XPos/ZPos, RotationX/Y/Z, Zoom, Alpha, Glow and explicit Update.
This ports the supported native implementation, including its zero-valued
Z/RotationY/Glow and the existing column-zero RotationX alias. It adds no new
StepMania effects. Native X/Y/zoom/alpha/rotation expressions are transferred
with protected host reads; `port_song_arrow_math.py --check` verifies that
transfer against the unchanged native source. Scoped neg_multiply allowances
retain the captured float expressions and nonfinite far-edge results. Integer
side partitioning uses equivalent div_ceil arithmetic.

Bindings borrow the runtime-owned catalog through a private weak-value table,
avoiding ownership of unrelated catalog entries or unselected option levels.
The last selected option owner lives in a traced private batch. Every getter preserves
GetCurrentPlayerOptions selection, normal method lookup, numeric conversion,
error and return behavior. The last selected options remain rooted across GC;
cached Tipsy uses that selection's timer/cosecant while activation, speed and
offset come from the current player's owned Current options. Beat factors,
Tipsy, bounds and inversion caches commit only after a successful update. No
cache mutex spans Lua execution. Binding functions are rooted until publication;
the public API table can be collected after its global and saved references drop,
while the private host updater remains available.

The live scene refreshes the private cache after frozen actor states and health
callbacks, matching the existing host phase. It bypasses an authored replacement
of the public ArrowEffects.Update function. Raw frame clocks are consumed only
on success; retries preserve them. Reload keeps the same binding table, VM and
saved option identities. The diagnostic clock tape now distinguishes domains
and reads stack provenance in a child host call, so repeated query clocks do not
attempt a second native-stack lease on their caller. Shipping code retains no
diagnostic tape state.

Final-source evidence:

- **{x['tests_passed']} tests passed across {x['test_targets']} targets**, with no failures,
  ignored cases or filtered cases. This preserves the prior 32 targets and adds
  the arrow target, existing clock target and four existing catalog/game-state/
  player-option/player-state targets. The {x['arrow_cases']} arrow tests compare
  all **246 existing captured native X/Y/zoom cases**, alpha/nonfinite boundaries,
  per-player rotations, raw clocks, atomic cache failure, reentrant getters,
  selected-option and public-table GC, health ordering, error/retry clock consumption and reload.
  Preparation and live timing also compare the existing pause/speed-ramp corpus
  across music rates and player/nil inputs. The original regression files and
  reference inputs are unchanged.
- {x['library_tests']} existing native host test passed with the explicit
  `host::tests::arrow_` filter; the full library test target compiled. Shipping
  cfg without test-support compiles. Applicable Rustfmt checks pass. Clippy
  matches its prior diagnostics, including the pre-existing src/cmd.rs:349
  immutable-loop error. These validation durations are not performance results.
- [Source review](song-arrows-review.json) checks {x['source_count']} watched sources,
  the five archived edit boundaries, native/interpreter/test preservation,
  exact math transfer, reference hashes, formatting and final-source logs.
  [The independent final audit](song-arrows-final-state.json) records final
  source, objective and documentation hashes.

Development logs remain archived. The initial standalone oracle omitted player
constants, and a health fixture initially did not cause a health-state change;
both setups were corrected without dropping their assertions. The first adapter
run exposed repeated native-stack leases during numeric reads. Protected value
conversion and isolated diagnostic stack reads fixed that adapter error without
changing deadlib-lua.

The first broad run passed 606 tests and failed two unchanged statistics GC
tests. Arrow functions had captured the complete catalog, retaining stage
statistics after retirement. Capturing the required player-option levels
fixed those two tests, but the unchanged isolated option-lifetime test then
showed that this still retained unselected levels. Borrowing through a private
weak catalog reference corrected both ownership errors; the runtime retains its
original catalog root and the last selected options stay strongly rooted. The
final focused run includes all seven statistics/arrow/clock/catalog/game-state/
player-option/player-state targets, and the full 38-target run includes their
unchanged lifetime checks. The failed logs, pre-correction sources and source
hashes remain archived; only arrows.rs changed between those failures and the
final source.

The [production migration objective](objective.txt) remains active. Normal song
routing still uses legacy. Presentation/content publication, existing compiled
input snapshots, remaining reference probes and saved-handle/session auditing,
production fallback/cache cutover and legacy isolation/removal remain required.
The final content/downstream suites, normal application build and rendered
song/gameplay/noteskin smoke, graph/link proof and matched preparation/live/memory
measurements are still pending. Interpreter and C API removal milestones remain
closed; this phase does not change the finish line.
'''
p=d/'README.md';s=p.read_text();assert heading not in s;p.write_text(s.rstrip()+'\n\n'+body.strip()+'\n')
p=r/'docs/lua-architecture.md';s=p.read_text();assert heading not in s
s+='\n'+heading+'\n\n'+f'''The Rust song adapter now provides the existing ArrowEffects bindings and
refreshes their cache after live actor state capture and health callbacks.
Selected option tables are rooted, cache commits preserve error boundaries and
raw clock retry behavior, and public Update overrides do not replace the host
phase. The generic interpreter and native implementations are unchanged.

Final-source validation passes **{x['tests_passed']} tests across {x['test_targets']} targets**, including
{x['arrow_cases']} arrow tests covering all 246 captured native X/Y/zoom cases and the
existing pause/speed-ramp corpus, plus {x['library_tests']} filtered native host test.
Formatting and shipping cfg pass; Clippy matches its recorded prior diagnostics.
See [the migration ledger](testing/2026-11-04-rust-lua-production-migration/README.md)
and [source review](testing/2026-11-04-rust-lua-production-migration/song-arrows-review.json).

Production song routing still uses legacy. Presentation/input integration,
remaining reference/session audits, fallback/cache cutover, legacy dependency
removal and the objective's final application/content/performance gates remain
unfinished. The interpreter and C API removal milestones remain closed.
'''
p.write_text(s)
print(json.dumps({'goal_complete':False,'documents':['docs/lua-architecture.md',str((d/'README.md').relative_to(r))]}))
