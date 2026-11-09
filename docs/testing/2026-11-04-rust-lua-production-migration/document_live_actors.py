"""Record verified live actor progress without changing the migration finish line."""
import base64,hashlib,json
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
report=json.loads((d/'song-live-actors-review.json').read_text())
assert not report['goal_complete'] and not report['production_cutover']
paths=[r/'docs/lua-architecture.md',d/'README.md']
backup=d/'song-live-actors-documentation-before.json';assert not backup.exists()
backup.write_text(json.dumps({p.relative_to(r).as_posix():{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'base64':base64.b64encode(p.read_bytes()).decode()} for p in paths},indent=2)+'\n')
p=paths[0];raw=p.read_bytes()
old='''tracks its differential checks. The full live-scene coordinator and application
routing remain pending.'''
new='''tracks its differential checks. A Rust live actor phase now uses the complete
screen/player/song/transition traversal, with player hold/judgment animations and
Current Mini zoom after each player's subtree. Its traced actor-entry hook runs
before delta selection for future foreground coordination. The
[live actor record](testing/2026-11-04-rust-lua-production-migration/README.md#live-actor-traversal-and-player-feedback-2026-11-07)
tracks native comparisons, including current rendered tween poses. The full
live-scene coordinator, layer scheduling and application routing remain pending.'''
eol=b'\r\n' if b'\r\n' in raw else b'\n'
old=old.encode().replace(b'\n',eol);new=new.encode().replace(b'\n',eol)
assert raw.count(old)==1;p.write_bytes(raw.replace(old,new))
section=f'''

## Live actor traversal and player feedback (2026-11-07)

The Rust live actor phase now reuses the complete chronological job executor:
top screen, screen layers, player subtrees, authored song layers, transitions and
the final top-screen callback. Live-only jobs apply hold animation, judgment
animation and NoteField Mini zoom after each player's subtree, before song
callbacks. Offline compiler plans remain free of live feedback. Switching the
cached plan's mode invalidates its traced root through the existing mechanism.

`RustSongRuntime::advance_song_live_actors` requires an active, activated live
screen. It roots the current job plan and optional entry callback across author
callbacks, cache invalidation and GC; new actors are adopted even when a callback
fails. The entry callback receives each actor before inactivity, own-delta and
inherited-delta selection. This is the actor phase used by the pending scene
coordinator; it does not schedule layers, publish host clocks/health or assemble
live output by itself.

Native feedback preserves hold/judgment timestamp deduplication, MissedHold's
existing queue, 6/7/12/14-state judgment selection, FA+/autoplay distinctions,
early/late frames, tilt and actual Sprite tween methods. The three feedback
helpers re-read their respective inputs in order, so hold callbacks can change
later judgment/option inputs. Raw modifier/child tables and the shared hold color
remain rooted across methods that replace owners and collect. Mini uses the owned
Current options table, writes X/Y before reading OITG mode, and preserves Z in
OITG mode. Feedback runs even when the player's actor delta is inactive.

`tests/rust_live_actors.rs` adds {report['live_actor_cases']} cases against the unchanged native
live scene and production render reader. These compare exact scalar getter and
queue values, authored callback order, and current rendered judgment/hold poses.
They also cover wrappers, rates, hibernation, note-column input delta, prepared
feedback, runtime wrapper creation/cache invalidation, report-once actor errors,
entry-hook timing/recovery, partial feedback errors and foreign/retired owners.

The first new fixtures incorrectly called frame setters on declaration tables,
Combo/Debug/non-frame note columns, or wrote Mini into a level replaced by the
next option update. Positive fixtures now retain initialized roots in Init,
instrument the existing native callback fields where setters are absent, and
set all option levels when testing a persistent Mini value. Explicit rejection
controls retain the invalid non-frame setters. The original new fixture file and
all failed runs remain archived; every previously existing test is byte unchanged.
Rendered-pose assertions strengthen the animation comparisons.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Affected adapter suites | `song-live-actors-regressions-v1-song-linux-debug` | {report['tests_passed']} passed; 0 failed |
| Native live actor comparisons | `song-live-actors-focus-v4-song-linux-debug` | 16 passed; 0 failed |
| Clippy (baseline defect remains) | `song-live-actors-lint-v1-song-linux-debug` | 166 warnings and existing `cmd.rs:349` error; exact prior log |
| Adapter without test support | `song-live-actors-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The regression run covers {report['test_targets']} targets with no failed, ignored or filtered
tests. Shared chronological/compiler output, observations, diagnostics, generic
actors, player parts, generated players, NoteFields, wrappers, transitions,
statistics, retirement and prior live-frame controls pass against the final
source. Formatting passes. Clippy's log is byte identical to the prior live-frame
baseline; its existing unrelated error is not suppressed. These are adapter
checks, not a normal application build or final performance measurements.

`review_live_actors.py` checks all {report['source_count']} watched source hashes,
saved pre-edit bytes, existing test preservation, the test-only manifest addition,
formatting, final-source result snapshots and lint diagnostics.
`song-live-actors-review.json` records the result. Native implementations and the
shared engine are byte unchanged. The immediately previous goal turn only
acknowledged scope and made no progress; this turn implements and verifies the
live actor phase. The fixed migration goal remains active and incomplete.

Still required: background/foreground scheduling and entry activation in the
full live-scene coordinator; draw/output assembly, outro behavior and synchronous
gameplay callbacks; session ownership, fallback/cache wiring and unconditional
production routing; removal of legacy normal/build dependencies; final content,
downstream, application/render, graph/link and matched preparation/live/memory
validation. The new load boundary must clear prior live players before Init;
selected statistics owners must also govern script live writes. The interpreter
and C API removal milestones remain closed. These are the existing migration
criteria, not new completion requirements.
'''
p=paths[1];raw=p.read_bytes();assert b'## Live actor traversal and player feedback' not in raw
p.write_bytes(raw+section.encode())
print(json.dumps({'tests_passed':report['tests_passed'],'test_targets':report['test_targets'],'goal_complete':False,'documents':[p.relative_to(r).as_posix() for p in paths]}))
