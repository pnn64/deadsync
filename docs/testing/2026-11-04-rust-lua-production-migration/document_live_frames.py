import base64,hashlib,json
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
x=json.loads((d/'song-live-frames-review.json').read_text());assert x['tests_passed']>0 and x['without_test_support_compiles']
paths=[r/'docs/lua-architecture.md',d/'README.md']
saved={p.relative_to(r).as_posix():{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'base64':base64.b64encode(p.read_bytes()).decode()} for p in paths}
p=d/'song-live-frames-documentation-before.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
p=paths[0];b=p.read_bytes();nl='\r\n' if b'\r\n' in b else '\n'
old='cover these fields alongside production output. This entry point rejects live contexts.'
new='''cover these fields alongside production output. This entry point rejects live contexts.
A Rust live-frame owner now coordinates retained statistics, song/player clocks,
uptime, attack activation, option advancement and post-actor health notifications;
the [live host-phase record](testing/2026-11-04-rust-lua-production-migration/README.md#live-frame-host-phases-2026-11-07)
tracks its differential checks. The full live-scene coordinator and application
routing remain pending.'''
assert b.count(old.encode())==1;p.write_bytes(b.replace(old.encode(),new.replace('\n',nl).encode()))
rows=[]
for stage,v in x['results'].items():
 label='Affected adapter suites' if 'regressions' in stage else 'Native live-phase comparisons' if 'focus' in stage else 'Clippy (baseline defect remains)'
 result=f"{v['tests_passed']} passed; {v['tests_failed']} failed" if 'lint' not in stage else '166 warnings and existing `cmd.rs:349` error; exact prior diagnostic set'
 rows.append(f"| {label} | `{stage}` | {result} |")
text=f'''

## Live frame host phases (2026-11-07)

`rust_engine::SongLiveFrames` now owns the existing live GameState/player host
phases. It retains the statistics tables selected through the actual Lua methods
and the native Current option owners in a rooted private batch. Scene-level
construction selects owners before resetting health publication, matching the
legacy constructor. Life meters bind to those retained statistics. Released and
foreign-runtime owners reject further use without consuming another VM's roots.

Frame preparation preserves statistics-before-clock ordering, independent uptime,
global Song timing versus each selected Steps timing, f64/f32 conversion points,
ordinary versus raw accesses and partial mutation boundaries. Attack activation
uses the prior PlayerState audio position and strict endpoints, with retained
schedule identity and the existing outro gate. Options then advance with the
supplied frame delta or independent Song/P1/P2 deltas. Preparing before synchronous
Player callbacks is idempotent; resuming after them refreshes feedback and
statistics without reapplying uptime, attacks or option advancement.

The post-actor health phase reads current life and fail settings for each enabled
player, publishes a changed status before notifying listeners, and preserves
cross-player life writes and partial failures. Callback error reporting remains
the responsibility of the full live-scene coordinator. Health publication resets
for a new owner while the retained VM and its process globals survive.

`tests/rust_live_frames.rs` adds {x['live_phase_cases']} cases against the actual legacy
live scene's prepare/update methods and its unchanged private health function.
They compare clock/position fields, all relevant option levels, statistics, meter
life, controller/health queries and notification order using exact scalar values.
Cases include rate/timing separation, frozen music, prepared-frame feedback,
attack replacement, retained tables through global replacement and GC, nested
life writes, health thresholds/fail-off, failed option clocks and recovery,
constructor health reset, release and foreign VM rejection.

New test harness mistakes were corrected without changing existing coverage:
both engines now execute the same literal chunk (`call`, without the native
`eval` expression rewrite), with separate explicit-return checks. The nested
life-write fixture uses Player's existing `SetLife` method; its original invalid
LifeMeter call remains an error/partial-state control. Both enabled-player and
disabled-player cases remain. Earlier failed runs and pre-edit test files are
preserved. The fail-off case additionally verifies zero life remains Alive.

| Check | Evidence directory | Result |
| --- | --- | --- |
'''+ '\n'.join(rows)+f'''
| Adapter without test support | `song-live-frames-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The regression run covers {x['test_targets']} targets and {x['tests_passed']} passing tests,
with no failed, ignored or filtered tests. It includes the unchanged clock,
statistics, player, song-option, game-state, screen-manager, message, player-part,
generated-player, underlay, reload, retirement and compiler-observation controls.
Source snapshots match the final checks. Formatting passes; Clippy has no new
diagnostic relative to the recorded baseline. These are adapter checks, not a
normal application build or before/after performance measurement.

`review_live_frames.py` checks all {x['source_count']} watched source hashes,
saved pre-edit bytes, existing test preservation, test-only manifest additions,
exact native-source prefix preservation, formatting, final result snapshots and
baseline lint diagnostics. The only edit to native `live.rs` is an appended
test-only delegate to its original health helper. The shared engine is unchanged.
`song-live-frames-review.json` records the review; `audit_live_frames.ps1`
independently verifies Windows file hashes and the unchanged objective.

The full production live-scene coordinator remains pending: layer entry/update
ordering, actor/native-feedback phases, draw and output assembly, synchronous
gameplay callback bridges, retained-session ownership, fallback and cache wiring.
The load boundary must clear prior live-player feedback before new Init callbacks;
the native scene constructor resets health publication later. Authored selection
of statistics owners must also govern live player writes. These remain existing
production semantics required by the fixed objective. Normal routing, legacy
normal/build dependency removal, final content/downstream/application/render
checks and matched preparation/live/memory measurements remain required.
The interpreter and C API removal milestones remain closed.
'''
p=paths[1];b=p.read_bytes();assert b'Live frame host phases (2026-11-07)' not in b
p.write_bytes(b+text.replace('\n','\r\n' if b'\r\n' in b else '\n').encode())
print('Updated active architecture and live host-phase evidence ledger')
