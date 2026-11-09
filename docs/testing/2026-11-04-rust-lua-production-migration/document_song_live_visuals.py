"""Record verified live publication work while preserving the migration finish line."""
import json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
x=json.loads((d/'song-live-visuals-review.json').read_text())
assert x['formatting_passed'] and x['reader_refactors_verified'] and x['clippy_diagnostics_match_prior_baseline'] and x['without_test_support_compiles'] and x['reentry_guard_verified']
heading='## Rust live visual and content publication (2026-11-08)'
ledger=d/'README.md';architecture=r/'docs/lua-architecture.md'
old_ledger=ledger.read_text();old_architecture=architecture.read_text()
assert heading not in old_ledger and heading not in old_architecture
body=f'''{heading}

The Rust live scene now supplies bind_targets and write_visuals for the
existing prepared visual slots. Frozen actor poses publish in binding order,
followed by tracked player/judgment/combo, song-foreground and screen-layer
poses and hidden-player flags. Mesh vertices and texture paths, text and
attributes/stroke, and sprite paths/frame arrays retain the native field order.
Earlier fields and slots stay published if a later read fails. Proxy targets
resolve current actor identity to a local slot in the proxy's own layer;
cross-layer and missing targets retain the native sentinel. Invalid global
bindings leave the previous binding map intact; duplicate identities use the
last binding. Test-support visual paths and draw bindings are also available.

Preparation and live publication share the existing text-attribute and sprite
frame loops through visitors. Their read/conversion/error order is preserved;
the source review reconstructs both refactors from the archived input and
compares the complete formatted files. Live arrays decode once, preserve
bitwise float meaning and keep their Arc when values are unchanged, including
in-place edits, shortened arrays and NaNs. Retained render snapshots keep their
original immutable values. Each protected read owns its request and partial
result; no store or output mutex spans authored getters, collection or nested
actor calls. Lua actor/registry/helper values are traced captures.

Lua debug inspection can obtain a host caller. Publication callbacks therefore
have an atomic single-use guard: recursive and saved calls return a Lua error
without taking the active request or trying to consume an absent payload. The
dedicated test recovers the original getinfo from Def.Actor's captured upvalues
(the public compiler debug wrapper hides raw frames), obtains the real caller, calls it recursively
and after publication, forces collection, and verifies a fresh publication still
works. The initial 683-test source/review remains archived; only the new content
module and the appended test changed for this correction.
The initial reentry fixture inspected the synthetic public debug wrapper; its
failed run and diagnostic probe are retained. It now inspects the captured VM
primitive and keeps the recursive/saved-call error assertions.

Final-source evidence:

- **{x['tests_passed']} tests passed across {x['test_targets']} targets**, with no failures,
  ignored cases or filtered cases. The previous 38 targets remain present; the
  new live-visual target and existing mesh/proxy targets extend that run.
  The {x['visual_cases']} new tests compare real native and Rust compiled live scenes,
  complete prepared visual payloads, collecting getter order, frozen poses,
  tracked visibility, Arc identity, partial errors, layer-local proxy retargeting,
  invalid/missing bindings and immutable snapshots after session teardown.
  The focused five-target run passed {x['focused_tests']} tests.
- {x['library_tests']} native publication control tests passed with the explicit
  `publication` filter; the full library test target compiled. Rustfmt and the
  shipping configuration without test-support pass. Clippy matches its prior
  diagnostics, including the existing src/cmd.rs:349 immutable-loop error.
  Validation durations are not matched application performance measurements.
- [Source review](song-live-visuals-review.json) verifies {x['source_count']} watched
  sources, the five archived edit boundaries, the two reader refactors and
  final-source logs. The interpreter, native implementations, ArrowEffects and
  original regression files are unchanged. The initial compile caught a
  required mutable VM guard for object identities; the correction and failed
  build log are retained. [Final audit](song-live-visuals-final-state.json)
  records independent source, objective and document hashes.

The [unchanged production migration objective](objective.txt) remains active.
Normal production song routing still uses legacy. Existing input snapshot
integration, reference/eligibility/coverage probes and retained-handle/session
auditing, production fallback/cache cutover and legacy isolation/removal remain
required. Final interpreter/adapter/downstream/content checks, the normal
application build and rendered song/gameplay/noteskin smoke, graph/link proof
and matched preparation/live/memory measurements remain pending. The completed
interpreter and C API removal milestones remain closed.
'''
ledger.write_text(old_ledger.rstrip()+'\n\n'+body)
architecture.write_text(old_architecture.rstrip()+'\n\n'+heading+f'''

The Rust live scene now binds prepared visual slots and publishes frozen poses,
current mesh/text/sprite content, layer-local proxy targets and tracked visibility.
Publication preserves native read order and partial-error boundaries; shared
one-pass readers retain unchanged immutable arrays. Saved visual snapshots
remain valid after collection and session teardown.

Final-source checks pass **{x['tests_passed']} tests across {x['test_targets']} targets**, including
{x['visual_cases']} live differential tests, plus {x['library_tests']} filtered native publication controls.
Rustfmt and shipping cfg pass. Clippy retains its recorded baseline diagnostics.
See [the source review](testing/2026-11-04-rust-lua-production-migration/song-live-visuals-review.json)
and [migration ledger](testing/2026-11-04-rust-lua-production-migration/README.md).

Production song routing still uses legacy. Input snapshots, remaining reference/
session audits, fallback/cache cutover, legacy removal and the objective's final
application/content/performance gates remain required. The interpreter and
C API removal milestones remain closed; the migration finish line is unchanged.
''')
print(json.dumps({'goal_complete':False,'documents':[str(ledger.relative_to(r)),str(architecture.relative_to(r))]}))
