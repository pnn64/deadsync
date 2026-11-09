import base64,hashlib,json
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
x=json.loads((d/'song-observations-review.json').read_text())
assert x['tests_passed']>0 and x['all_five_fields_compared'] and x['without_test_support_compiles']
paths=[r/'docs/lua-architecture.md',d/'README.md']
saved={p.relative_to(r).as_posix():{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'base64':base64.b64encode(p.read_bytes()).decode()} for p in paths}
dest=d/'song-observations-documentation-before.json';assert not dest.exists();dest.write_text(json.dumps(saved,indent=2)+'\n')
p=paths[0];b=p.read_bytes();nl=b'\r\n' if b'\r\n' in b else b'\n'
old=b'record the supported cases. Reference-only setter observations still need'+nl+b'compiler wiring. This entry point rejects live contexts.'
assert b.count(old)==1
new=('record the supported cases. The finite compiler now also captures all five\n'
     'reference-only setter/call histories through existing Rust bindings;\n'
     '[compiler observation checks](testing/2026-11-04-rust-lua-production-migration/README.md#song-compiler-setter-observations-2026-11-07)\n'
     'cover these fields alongside production output. This entry point rejects live contexts.').replace('\n',nl.decode()).encode()
p.write_bytes(b.replace(old,new))
rows=[]
for stage,v in x['results'].items():
 label='Broad adapter regression' if 'regressions' in stage else 'Focused compiler regression' if 'focus' in stage else 'Clippy (baseline defect remains)'
 result=f"{v['tests_passed']} passed; {v['tests_failed']} failed" if 'lint' not in stage else '166 warnings and existing `cmd.rs:349` immutable-loop error; exact prior diagnostic set'
 rows.append(f"| {label} | `{stage}` | {result} |")
text=f'''

## Song compiler setter observations (2026-11-07)

The finite Rust compiler now populates the five existing test-support fields:
`boolean_writes`, `speed_writes`, `noteskin_writes`, `state_calls` and
`overlay_writes`. This closes the comparison-wiring gap recorded in the prior
compiler/callback sections. It does not switch the application's backend.

The four option/manager streams share Rust-owned observation state through
existing binding payloads. The actor stream retains setter arguments separately
from interpolated rendering tracks, including equal writes, same-frame overwrite
rules, queued-command suppression and the `UpdateCommand` exception. Startup,
reload, discovery, chronological replay and deferred probes keep the native
capture boundaries. `FromString` includes its existing speed-history record;
queued immediate setters preserve their last same-frame value. All capture code
is gated by `test-support`, with no added Lua globals or shared-engine changes.

`tests/rust_song_observations.rs` adds {x['observation_cases']} cases covering the full
production payload and all five reference fields, with separate rejection and
diagnostic controls. Cases include all option levels, dynamically republished
owner identity, accepted/rejected noteskins, partial speed failures, parsed
options, nested broadcasts, chart metadata, reloads, GC, multi-layer index
remapping and queued/direct/message setter ordering.

The first run exposed incorrect assumptions in newly authored fixtures:
non-frame actors cannot install update functions, Init screen requests are
discarded, and the published option owner is the Song level. Corrected positive
fixtures retain every original Lua source in explicit controls. The original
nil-parameter message probe still checks its diagnostic count, command label and
authored cause, and compares all remaining output; only the engine-specific
rendering of that one error is normalized. No pre-existing test body was edited,
removed, ignored or filtered. Earlier failed check directories remain preserved.

| Check | Evidence directory | Result |
| --- | --- | --- |
'''+ '\n'.join(rows)+f'''
| Adapter without observation code | `song-observations-shipping-cfg-v2` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The broad run covers {x['test_targets']} targets with {x['tests_passed']} passing tests,
including all 212 activation controls. Source snapshots match across the final
checks. Formatting passes. Clippy's diagnostic set matches the preceding
callback phase, including its recorded pre-existing error; no new lint failure
is accepted. The adapter check without test support is not an application build.

`review_song_observations.py` checks all {x['source_count']} watched source hashes,
saved pre-edit bytes, unchanged existing tests/native algorithms/shared engine,
test-only manifest additions, fixture retention, formatting, exact result
snapshots and baseline lint diagnostics. `song-observations-review.json` and
`song-observations-reviewed-sources.json` record that review;
`audit_song_observations.ps1` independently verifies current Windows file hashes
and the original objective before writing `song-observations-final-state.json`.

The migration remains active. Live/session/fallback/cache integration,
unconditional song routing, legacy normal/build dependency removal, final
downstream/content regressions, application/render smoke and matched preparation,
live and memory measurements are still required by the unchanged objective.
The interpreter and C API removal milestones remain closed.
'''
p=paths[1];b=p.read_bytes();assert b'Song compiler setter observations (2026-11-07)' not in b
p.write_bytes(b+text.replace('\n','\r\n' if b'\r\n' in b else '\n').encode())
print('Updated active architecture and migration evidence ledger')
