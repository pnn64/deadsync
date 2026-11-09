"""Record verified coordinator progress without claiming application cutover."""
import json
from pathlib import Path
d=Path(__file__).parent;r=d.resolve().parents[2]
x=json.loads((d/'song-live-scene-review.json').read_text())
assert not x['goal_complete'] and not x['production_cutover']
p=r/'docs/lua-architecture.md';s=p.read_text()
old='records its comparisons. Full live-scene assembly and application routing\nremain pending.'
new='''records its comparisons. A retained Rust live-scene coordinator now assembles
those phases with input, draw, outro, health, output collection and End; the
[coordinator record](testing/2026-11-04-rust-lua-production-migration/README.md#retained-live-scene-coordination-2026-11-07)
records its tests. Production live compilation/routing, synchronous gameplay
hooks, ArrowEffects integration, visual publication and complete session reuse
remain pending.'''
assert old in s;s=s.replace(old,new,1);p.write_text(s)
p=d/'README.md';s=p.read_text();heading='## Retained live-scene coordination (2026-11-07)';assert heading not in s
reg=x['results']['song-live-scene-regressions-v1-song-linux-debug']
focus=next(v for k,v in x['results'].items() if 'focus' in k)
s+=f'''

{heading}

The staged `rust_engine::SongLiveScene` owns its Rust VM and rooted loader,
overlay/tracked actor lists, selected statistics, frame phases and layer phases.
It coordinates preparation, actor updates, foreground/background scheduling,
success/failure outro ordering, current render states, post-state health,
input, custom Draw, startup output and End. `begin_live_song_load` clears old
live-player feedback before Init; selected statistics become authoritative for
SetLife/AddLife/RecordLife writes. A failed background preparation clears the
prepared mark so a retry repeats the host phase. Health failures retain the
existing report-once boundary. Current-state reads use owned tween/render fields,
including when authored raw pose fields have invalid types.

Explicit broadcasts capture their source parameters after the history write and
before listeners can mutate them. The snapshot retains omitted/explicit nil,
function provenance, tables, actor identity gaps, cycle/depth/entry limits,
unsupported value gaps, path normalization and theme-source eligibility. This
stream observes delivery; it does not replace message delivery. Live draw
collection now mirrors the existing boundary: immediate submissions are drained,
while normal draw frames remain internal until the next Draw traversal. The
generic preparation draw collector remains available with its original behavior.
End preserves compiled Sound owner identity and prevents duplicate retirement
stops during the explicit handoff back to the retained runtime.

Validation against the final source:

- {reg['tests_passed']} passed, zero failures across {x['test_targets']} test targets;
  {x['live_scene_cases']} new live-scene cases pass with no ignored/filtered tests.
  The tests compare actual legacy compiler/live scenes with Rust assembly,
  scalar observations, complete current overlay/tracked poses, output streams,
  normalized Sound owner identities and draw frames. They include explicit GC
  between retained phases and after End, background retry, selected statistics,
  health report-once, gameplay-noted outro and handoff cleanup.
- Rustfmt and `cargo check --locked -p deadsync-song-lua --features rust-engine
  --lib` pass. The Clippy command still exits 101 at the unchanged
  `src/cmd.rs:349` immutable-loop error; its entire log is byte-identical to the
  previous layer-phase baseline. No new Clippy diagnostic was introduced.
- Existing tests, native algorithms and `deadlib-lua` source are byte-unchanged
  from the preserved pre-coordinator snapshot. Only the staged Rust adapter,
  its test target and this migration evidence changed.

Focused final run: `{focus['stage']}`, {focus['seconds']:.3f} seconds, log SHA-256
`{focus['log_sha256']}`. Broader run: `{reg['stage']}`,
{reg['seconds']:.3f} seconds, log SHA-256 `{reg['log_sha256']}`.
These are validation durations, not the required final performance comparison.

Development runs remain archived in `song-live-scene-focus-v1` through
`song-live-scene-focus-v9`. Failures identified compiler mistakes, a missing
appended screen actor in the harness, invalid fixture syntax, a request issued
during Update(0), and the live draw-drain defect. Positive cases now require
the real fixture actors; malformed syntax has its own dummy-fallback control.
The original unconditional startup request remains a separate native 128-reload
rejection control. It does not establish the Rust live facade's reload behavior.
The handoff fixture originally expected an End stop for a Sound without a file;
the native compiler excludes that owner. That exact fileless source remains an
explicit native comparison, and the positive handoff case supplies a real WAV.
The shared test builder extraction preserved every existing comparison.

Evidence: `song-live-scene-edit-baseline.json`,
`song-live-scene-before-sources.json`, `review_live_scene.py`,
`song-live-scene-review.json`, `song-live-scene-reviewed-sources.json`,
`song-live-scene-shipping-cfg/result.json` and final Windows hash audit.

The migration remains active. This coordinator is a staging API: the normal song
compiler and application still use legacy. Production live compilation/routing,
same-VM synchronous gameplay hooks, post-health ArrowEffects cache updates,
presentation target/content publication, complete next-load/session reset,
reference command-probe eligibility and imported message-identity coverage gaps,
connections for the existing compiled input/interaction snapshot entry points,
fallback/cache cutover and removal of production legacy dependencies remain.
Final downstream/content checks, normal application build and rendered smoke,
dependency/link verification and matched preparation/live/memory measurements
also remain. The interpreter/C API removal milestones stay closed; no new
language parity, C API or baking work was added.
'''
p.write_text(s)
print(json.dumps({'documented_tests':reg['tests_passed'],'goal_complete':False,'production_cutover':False}))
