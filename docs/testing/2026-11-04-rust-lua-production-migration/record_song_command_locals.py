"""Record verified command-local probe migration evidence without rewriting history."""
from pathlib import Path
import json,hashlib
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
review=read(base/'song-command-locals-review.json')
assert review['watched_dependency_sources_match_checks'] and review['song_clippy_baseline_unchanged']
stages=['song-command-locals-regressions-v2-song-linux-debug','song-command-locals-lint-v1-song-linux-debug']
reg,lint=[read(base/stage/'result.json')for stage in stages]
assert reg['tests_passed']==309 and reg['tests_failed']==0 and len(reg['test_results'])==22
findings=read(base/'song-command-locals-findings.json')
for stage,r in zip(stages,[reg,lint]):
 assert hashlib.sha256((base/stage/'run.log').read_bytes()).hexdigest()==r['log_sha256']
 findings['results'][stage]=r
findings['final_review']='song-command-locals-review.json'
(base/'song-command-locals-findings.json').write_text(json.dumps(findings,indent=2)+'\n')
before=read(base/'song-command-locals-docs-before.json');p=base/'README.md';old=p.read_bytes().decode();assert hashlib.sha256(old.encode()).hexdigest()==before[p.relative_to(root).as_posix()]['sha256']
append=f"""
## Command-local state inside actor probes (2026-11-06)

This continues the fixed production migration. The command-local portion of an
actor-aware action scope now preserves Lua closures first reached through
playcommand, queues, broadcasts, group/course commands and updates, including
zero-delta startup updates. Ordinary execution leaves the scope inactive.

The existing script snapshot traversal also runs in a nested host callback with
one native-stack lease. Saved cells, function identities, aliases, cycles and
referenced table contents stay rooted through the runtime registry's private
host-state chain. Locals-only capture does not shallow-copy global or custom
environment bindings; the outer shared-script snapshot retains that separate
responsibility. Native host functions have no Lua upvalue cells to rewind.
Nested scopes merge encountered functions/snapshots into their owner, keeping
locals advanced between stability attempts; the outermost scope restores in
reverse snapshot order so earliest values win. RNG restores at every boundary.
Failed snapshot construction releases its attachment. Static review also added
release of remaining saved attachments after an early restoration error.

- [Final regressions]({stages[0]}/result.json): **309 passed, zero failed across
  22 targets**, exit 0 in {reg['seconds']} seconds; sources unchanged.
  Log SHA-256: `{reg['log_sha256']}`.
  Activation has 93 cases with the exact prior 88-case prefix unchanged. Five
  native comparisons cover dynamically reached command closures, custom
  environment binding persistence versus table rewind, shared cells/cycles and
  detached commands with GC/retirement, callback and instruction-limit recovery,
  and nested local/RNG boundaries. The course target adds its existing 11 cases
  because its dispatch also uses the new command-call wrapper.
- [Clippy]({stages[1]}/result.json): exactly the existing 166 warnings and
  cmd.rs:349 error, exit 101 in {lint['seconds']} seconds; no new diagnostics.
  Log SHA-256: `{lint['log_sha256']}`.
- [Source review](song-command-locals-review.json): all {review['source_count']:,}
  watched inputs match both final checks. Formatting passes for nine modified
  adapter/test files, the new scope module, and the isolated test-only oracle
  append. All earlier native production bytes are unchanged. The oracle delegates
  to the actual existing action-scope begin/restore functions. Core, manifests
  and backend selection are unchanged in this step. The [test dependency graph](song-command-locals-test-dependencies.json),
  [source diff](song-command-locals-source-diff.patch) and edit/documentation
  baselines preserve the exact review boundary.
- [Findings](song-command-locals-findings.json) preserve the first compile failure
  in the test string accessor, the five-case focused pass and interim 309-case
  group. The full group was rerun after static review tightened attachment
  cleanup. No failing coverage was removed, weakened or ignored. Durations are
  verification costs, not representative application performance measurements.

The command-local layer is implemented; full actor-aware probing still requires
lazy touched-actor semantic snapshots/capture-buffer reset, default message
parameters and action/message orchestration. Full tree/wrapper/global-reference
extraction, proxy remapping, remaining derived output, production preparation
compiler and retained live/session/navigation lifecycle, automatic teardown,
fallback/cache and unconditional song cutover remain pending. Original content/
final-source regressions, normal application/render, dependency/link and
preparation/live/memory gates remain required. Normal production songs still use
legacy Lua. The interpreter and C API removal milestones remain closed; this
production migration remains active and incomplete.
"""
if '\r\n' in old:append=append.replace('\n','\r\n')
p.write_bytes((old+append).encode());assert p.read_bytes().decode().startswith(before[p.relative_to(root).as_posix()]['text'])
print('Recorded final command-local evidence and fixed remaining scope')
