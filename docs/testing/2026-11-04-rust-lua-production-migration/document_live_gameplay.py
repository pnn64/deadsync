"""Publish only the completed callback phase, preserving the migration finish line."""
import hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
review=json.loads((d/'song-live-gameplay-review.json').read_text())
assert review['formatting_passed'] and review['clippy_diagnostics_match_prior_baseline'] and review['without_test_support_compiles']
heading='## Rust retained gameplay callbacks (2026-11-07)'
text=f'''
{heading}

The Rust live scene now installs the five existing synchronous gameplay hooks:
DidTapNote, Step, tap-row Judgment, hold Judgment and avoided-mine Judgment.
Gameplay retains closures sharing the scene's one Rust VM, selected statistics
and timing. Scene reloads retain that VM even while old hooks still exist.
The scene/runtime accessors now return a scoped VM guard. Lua host functions use
HostCall for nested Lua execution; gameplay writes are drained and applied after
releasing the VM guard. Dropped hook roots are queued for release on the next
idle VM access, and the final owner destroys the VM without a HostFn/VM cycle.

Event callbacks refresh selected statistics, the owned current music rate,
song beat/music time and live players without advancing uptime, option groups,
attacks or the outer prepared frame. Native messages bypass authored Judgment
normalization. Row payloads include current notes and preceding active hold
heads, with retained note/result methods and routine participant ownership.
DidTapNote retains its selected field/callback across GC, preserves independent
optional column/score/brightness returns, recomputes brightness with the mine
exception and preserves the existing once-per-owner error flags. Player writes
before an error still apply. Pending/output writes and Song FailType precedence
also use the existing gameplay methods. New actors created by event callbacks
are adopted by the current screen even when the callback reports an error.

Final-source evidence:

- {review['tests_passed']} tests passed across {review['test_targets']} regression targets, with
  zero failures, ignored cases or filtered cases. This retains the preceding 31
  compiler/scene/actor/binding targets and adds {review['gameplay_cases']} gameplay cases.
  They use real GameplayRuntimeState instances, both installed closure calls and
  the gameplay judgment pipeline, native payload/return/output comparisons,
  nested Lua messages, malformed-rate and callback errors, prepared-frame reuse,
  GC, old hooks across reload, worker/playback thread transfer and final VM
  destruction after the last retained hook drops.
- {review['library_tests']} library tests passed under the explicit `live::` filter; the
  entire library test target compiled. Shipping cfg without test-support also
  compiles. Applicable Rustfmt checks pass. Clippy matches the preceding baseline,
  including the unrelated immutable-loop error at src/cmd.rs:349; no new
  diagnostic is accepted.
- [Source review](song-live-gameplay-review.json) verifies the {review['source_count']}
  watched sources, the archived pre-edit bytes, the manifest addition, the
  unchanged generic interpreter and native algorithms. The only changes to
  existing test files bind or release the new scoped runtime guard; their
  assertions and coverage are preserved. Independent final hashes are in
  [the final audit](song-live-gameplay-final-state.json).

The initial test compile used the wrong public write-type path and omitted the
native reload factory argument; both fixture declarations were corrected. All
development logs remain archived. The tap test exposed precise closure capture
of a copied root ID instead of its owner; the closure now calls through the
retained owner, keeping its root alive until callback destruction. This adapter
lifetime defect was fixed without changing the generic VM. Run durations are validation costs, not the
matched preparation/live/memory measurements still required by the objective.

This is progress on the [unchanged production migration objective](objective.txt),
not completion. The normal song facade still uses legacy. Post-health
ArrowEffects, presentation/content publication, compiled input snapshots,
remaining reference probes and retained-handle/session auditing remain pending.
Production routing, fallback/cache cutover, legacy isolation/removal and the
final content/downstream, normal-build, rendered-smoke, graph/link and matched
performance/memory gates remain required. The interpreter and C API removal
milestones stay closed.
'''
p=d/'README.md';s=p.read_text();assert heading not in s;p.write_text(s.rstrip()+'\n\n'+text.strip()+'\n')
p=r/'docs/lua-architecture.md';s=p.read_text();assert heading not in s
s+='\n'+heading+'\n\n'
s+=f'''The Rust song live scene now provides all five synchronous gameplay callback
installers over the same retained VM: DidTapNote, Step, tap-row Judgment, hold
Judgment and avoided-mine Judgment. It also exposes pending/output player writes
and Song FailType preparation. Event callbacks preserve selected statistics,
owned option reads, message/return payloads, error boundaries and prepared-frame
ordering. VM guards serialize host entry; nested Lua uses HostCall, and gameplay
writes apply after the guard is released. Hook owners keep roots alive across GC,
reload and scene destruction and release the VM after the final hook drops.

Final-source validation passes **{review['tests_passed']} tests across {review['test_targets']} targets**, including
{review['gameplay_cases']} gameplay differential/lifetime cases, plus {review['library_tests']} filtered library tests.
Formatting and shipping cfg pass; Clippy retains the recorded pre-existing
diagnostics. See the [migration ledger](testing/2026-11-04-rust-lua-production-migration/README.md)
and [source review](testing/2026-11-04-rust-lua-production-migration/song-live-gameplay-review.json).

Normal application song routing still uses legacy. ArrowEffects,
presentation/content and input snapshot integration, remaining probes/session
audits, fallback/cache cutover, legacy dependency removal and the objective's
final application/content/performance gates remain unfinished. The interpreter
and C API removal milestones remain closed.
'''
p.write_text(s)
print(json.dumps({'documents':['docs/lua-architecture.md',str((d/'README.md').relative_to(r))],'goal_complete':False}))
