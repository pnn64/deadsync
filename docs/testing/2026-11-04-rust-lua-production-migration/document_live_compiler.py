"""Publish verified compiler progress and keep the full migration finish line."""
import json
from pathlib import Path
d=Path(__file__).parent;r=d.resolve().parents[2]
x=json.loads((d/'song-live-compiler-review.json').read_text());assert not x['goal_complete'] and not x['production_cutover']
p=r/'docs/lua-architecture.md';s=p.read_text()
old='''records its tests. Production live compilation/routing, synchronous gameplay
hooks, ArrowEffects integration, visual publication and complete session reuse
remain pending.'''
new='''records its tests. A unified Rust song compiler now returns preparation output
or a retained live scene, and its reload entry point reconstructs screens in the
same VM while preserving globals, RNG and outgoing End output. The
[live compiler record](testing/2026-11-04-rust-lua-production-migration/README.md#rust-song-compiler-and-retained-live-reload-2026-11-07)
records these comparisons. Normal application song routing, synchronous gameplay
hooks, ArrowEffects integration, visual publication and the remaining retained
handle/session audit remain pending.'''
assert old in s;s=s.replace(old,new,1);p.write_text(s)
reg=x['results']['song-live-compiler-regressions-v1-song-linux-debug'];focus=next(v for k,v in x['results'].items() if 'focus' in k)
lib=next(v for k,v in x['results'].items() if 'library' in k)
p=d/'README.md';s=p.read_text();heading='## Rust song compiler and retained live reload (2026-11-07)';assert heading not in s
s+=f'''

{heading}

`rust_engine::compile_song_lua_layers` now handles both existing preparation and
live contexts. Its live path stops after startup Update(0), reads the actual
assets/current poses and late sprite/proxy targets, appends ScreenGameplay,
collects tracked actors and transfers the VM into `SongLiveScene`. It uses the
existing layer splitting, wrapper ordering, parent remapping, live indices and
primary-layer ownership rules. Empty live sessions retain their screen. Failed
live files retain the existing dummy-actor fallback and prior Lua side effects.
No future callbacks are executed to extract live assets.

The shared payload is now `CompiledSongLuaData<Actor, Scene>`. The existing
`CompiledSongLua<Actor>` alias retains its legacy scene owner until normal
application routing is ready. Fields, defaults and Debug representation are
preserved. The finite Rust compiler body and native algorithms are unchanged;
only their payload scene type was generalized. The old finite Rust entry point
remains available, and the unified entry point uses the same preparation body.

`rust_engine::reload_song_lua` runs outgoing End, reads the selected context,
releases scene-owned roots and reconstructs in the same Rust VM. Globals,
functions, saved actor identities and the RNG survive. Outgoing output precedes
new startup output. The load boundary clears live player feedback and cached
execution/replay plans before replacement Init. Startup requests preserve the
native unsupported-screen rejection and 128-reconstruction limit. Startup End
does not add the later live scene's compiled Sound stop pass; runtime End keeps
that pass and avoids duplicate retirement stops.

The differential cases exposed the native epoch behavior of old actor handles:
after playback state is cleared, ordinary scalar getters read their retained Lua
backing fields. Rust now marks the old epoch, preserves those reads and publishes
subsequent scalar writes to that backing. Initial live scalar fields and rotation
getter fields retain their native publication boundaries. Cached base alignment
getters fall back to their Lua defaults after reload. This is verified for the
tested scalar methods; complete saved-handle content/reattachment behavior remains
part of the pending presentation/session audit. Startup test-only observation
histories are consumed and discarded on the native live branch; the Rust live
payload follows that rule while finite preparation retains its histories.

Final-source validation:

- {reg['tests_passed']} passed across {x['test_targets']} regression targets, with zero
  failures, ignored cases or filtered cases. All existing test files are
  byte-unchanged. The {x['live_compiler_cases']} new compiler cases compare every compiled
  payload field through the preserved Debug schema, complete current scene poses,
  startup/live output, source rows, Sound owner identities, reloads and rejection
  controls. Cases include multiple layers/wrappers/proxies/AFTs, empty sessions,
  fallback side effects, preserved globals/RNG/End order, stale player feedback,
  saved scalar handles across GC, the 128-reload bound and an actual loading-worker
  to playback-thread VM handoff.
- {lib['tests_passed']} library tests passed with the explicit `compile::` filter.
  The whole library test target compiled; the filter runs the relevant existing
  native compile/finalization/reload coverage rather than unrelated unit families.
- Rustfmt passes for changed Rust modules and the new shared declarations.
  `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passes
  without test-support. Clippy retains the recorded immutable-loop error at
  `src/cmd.rs:349` and its prior diagnostics. Shared declaration line shifts are
  mapped to the old locations for comparison; no new diagnostic is accepted.
- `review_live_compiler.py` checks all watched sources, the exact shared/native
  type transformations, the unchanged finite compiler body, payload schema/Debug,
  manifest addition, old test preservation and interpreter preservation. The
  independent Windows hash audit records the final state and documentation.

Focused final run: `{focus['stage']}`, {focus['seconds']:.3f} seconds, log SHA-256
`{focus['log_sha256']}`. Broad run: `{reg['stage']}`,
{reg['seconds']:.3f} seconds, log SHA-256 `{reg['log_sha256']}`.
These durations are validation costs, not the final preparation/live/memory
performance comparison required by the objective.

Development runs and original fixtures remain archived. The first live payload
comparison caught incorrectly retained reference histories. A saved-pose fixture
initially assumed 77 survived native replay reset; the direct native comparison
showed its Lua fallback was 0, and the Rust epoch defect was fixed. The statistics
fixture initially called unsupported `PlayerStageStats:GetLife`; its supported
calls now use `GetCurrentLife` and `LifeMeter:GetLife`, with an explicit control
preserving the unsupported-name rejection. No failing coverage was removed.

The migration remains active. The normal application song facade still uses
legacy. Same-VM synchronous gameplay hooks, post-health ArrowEffects updates,
presentation/content publication, existing compiled input/interaction snapshots,
remaining reference probes and retained-handle/session cleanup behavior still
need connection and verification. Normal routing, fallback/cache cutover and
legacy dependency isolation/removal remain, followed by final content/downstream
checks, a normal application build and rendered smoke, dependency/link proof and
matched preparation/live/memory measurements. The interpreter/C API removal
milestones remain closed; the objective and its finish line are unchanged.
'''
p.write_text(s)
print(json.dumps({'tests_passed':reg['tests_passed'],'library_tests':lib['tests_passed'],'goal_complete':False,'production_cutover':False}))
