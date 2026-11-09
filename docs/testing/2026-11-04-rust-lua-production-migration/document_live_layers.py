"""Append verified layer scheduling progress; retain the fixed migration criteria."""
import base64,hashlib,json
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
x=json.loads((d/'song-live-layers-review.json').read_text());assert not x['goal_complete'] and not x['production_cutover']
paths=[r/'docs/lua-architecture.md',d/'README.md'];backup=d/'song-live-layers-documentation-before.json';assert not backup.exists()
backup.write_text(json.dumps({p.relative_to(r).as_posix():{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'base64':base64.b64encode(p.read_bytes()).decode()} for p in paths},indent=2)+'\n')
p=paths[0];raw=p.read_bytes();eol=b'\r\n' if b'\r\n' in raw else b'\n'
old='''tracks native comparisons, including current rendered tween poses. The full
live-scene coordinator, layer scheduling and application routing remain pending.'''
new='''tracks native comparisons, including current rendered tween poses. Rust live
layer scheduling now prepares background focus/reset/fading before Player
callbacks and activates/ends foregrounds at their actor traversal boundaries;
the [layer scheduling record](testing/2026-11-04-rust-lua-production-migration/README.md#live-background-and-foreground-scheduling-2026-11-07)
records its comparisons. Full live-scene assembly and application routing
remain pending.'''
old=old.encode().replace(b'\n',eol);new=new.encode().replace(b'\n',eol);assert raw.count(old)==1
raw=raw.replace(old,new)
old=b'before delta selection for future foreground coordination.';assert raw.count(old)==1
p.write_bytes(raw.replace(old,b'before delta selection for scheduled foreground coordination.'))
section=f'''

## Live background and foreground scheduling (2026-11-07)

`SongLiveLayers` retains the existing background and foreground owners through
traced Rust host callbacks and a private rooted batch. Rust activation cursors
hold no untraced Lua handles. Background preparation runs after host clocks and
options, before synchronous Player callbacks. It preserves two layers, stable
activation ties, current/fading order, LoseFocus/transition/reset/On/GainFocus
ordering, rate wrappers and plan invalidation after traversal reordering.

Foregrounds activate through the live executor's actor-entry hook after Player
feedback and before incoming delta selection. The phase preserves strict start
and stop boundaries, the crossed-start music delta, visibility-triggered On
restarts, the rate query after On, and retained selected option objects across
replacement/GC. The caller supplies the last completed music time and commits it
after its outro phase; layer phases preserve partial cursors and authored writes
when they fail. New constructor owners are adopted on both success and failure.
Release validates VM ownership before consuming the retained root.

The native background reset clears Actor pose/effect/queue fields while keeping
resources, dimensions, authored data, commands, callbacks, wrappers and the three
existing preserved render flags. Render comparisons caught an initial Rust reset
that also discarded dimensions; that adapter defect is fixed. The original
state-reader path remains token identical after removing the new explicit reset
branch, and the startup walker body is unchanged. No native algorithm or shared
engine source changed.

`rust_live_layers.rs` adds {x['live_layer_cases']} comparisons against the actual native live
scene and production render reader. They include two-layer/tie ordering, rates,
fades and reactivation, foreground entry/expiry/restarts, prepared frames, Model
depth/wrap defaults, Sprite animation reset, GC during ordinary method lookup,
partial failures, cleanup and foreign ownership. Positive updates reject
unexpected callback errors and compare complete local render states. The
original invalid preference assertion now has an explicit error control: the
option phase reads this preference even when no layers are scheduled. A no-layer
case verifies exactly that one read. The rejected screen-clock rewind also
remains checked before recovery with monotonic screen time. All earlier test
files and failed runs are preserved; previously existing tests are byte unchanged.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Affected adapter suites | `song-live-layers-regressions-v1-song-linux-debug` | {x['tests_passed']} passed; 0 failed |
| Native live layer comparisons | `song-live-layers-focus-v4-song-linux-debug` | 16 passed; 0 failed |
| Clippy (baseline defect remains) | `song-live-layers-lint-v1-song-linux-debug` | Exact prior log: 166 warnings and existing `cmd.rs:349` error |
| Without test support | `song-live-layers-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The {x['test_targets']}-target regression run has no failed, ignored or filtered tests. It
includes prior live phases, compilation and chronology, activation, observations,
diagnostics, render/effects, Sprite/Model/text, player parts, wrappers, transitions,
statistics and retirement. Formatting passes. `review_live_layers.py` verifies
all {x['source_count']} watched source hashes, preserved pre-edit bytes, final-source
check snapshots, unchanged existing reader/startup behavior and test preservation.
`song-live-layers-review.json` records the review; `audit_live_layers.ps1`
independently verifies Windows hashes and the unchanged objective. These checks
do not claim a normal application build, final rendering smoke or matched
performance measurements.

The previous goal turn was verified progress on live actor traversal and player
feedback; this turn implements and verifies live layer scheduling. The migration
remains active. Full live-scene assembly still needs draw/output, outro and
synchronous gameplay callback coordination, retained-session/fallback/cache
wiring, unconditional production routing and legacy dependency removal, followed
by final content/downstream/application/render/graph/link and representative
preparation/live/memory checks. The load boundary must clear prior live players
before Init; selected statistics owners must govern script live writes; failed
background preparation must not leave the outer frame marked prepared. These
are existing production semantics to preserve while assembling the coordinator.
The interpreter and C API removal milestones remain closed.
'''
p=paths[1];raw=p.read_bytes();assert b'## Live background and foreground scheduling' not in raw;p.write_bytes(raw+section.encode())
print(json.dumps({'tests_passed':x['tests_passed'],'test_targets':x['test_targets'],'goal_complete':False,'documents':[p.relative_to(r).as_posix() for p in paths]}))
