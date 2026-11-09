"""Record the bounded snapshot migration evidence and its unfinished goal gates."""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
out = d / 'actor-input-snapshots-final-state.json'
sources_out = d / 'actor-input-snapshots-reviewed-sources.json'
assert not out.exists() and not sources_out.exists()
objective = '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d / 'objective.txt') == objective
review = json.loads(subprocess.check_output(
    [sys.executable, str(d / 'review_actor_input_snapshots.py')], cwd=r))
assert review['sources'] == 1307 and review['runtime_owners_match_native']
assert review['interpreter_unchanged'] and review['existing_tests_unchanged']
assert review['existing_actor_update_unchanged']

checks = {}
for name, expected in [
    ('actor-input-snapshots-windows-v1-song-windows-debug', 0),
    ('actor-input-snapshots-windows-v2-song-windows-debug', 0),
    ('actor-input-snapshots-lint-windows-v2-song-windows-debug', 101),
    ('actor-input-snapshots-shipping-windows-v2', 0),
]:
    folder = d / name
    result = json.loads((folder / 'result.json').read_text())
    assert result['exit_code'] == expected and result['sources_unchanged']
    assert result['log_sha256'] == sha(folder / 'run.log')
    checks[name] = dict(result, result_sha256=sha(folder / 'result.json'))

behavior_name = 'actor-input-snapshots-windows-v2-song-windows-debug'
behavior = checks[behavior_name]
assert behavior['tests_passed'] == 78 and behavior['tests_failed'] == 0
assert all('0 ignored;' in line and '0 filtered out;' in line for line in behavior['test_results'])
tested = json.loads((d / behavior_name / 'sources.json').read_text())
current = json.loads((d / 'actor-input-snapshots-lint-windows-v2-song-windows-debug/sources.json').read_text())
shipping = json.loads((d / 'actor-input-snapshots-shipping-windows-v2/sources.json').read_text())
assert current == shipping and all(sha(r / name) == digest for name, digest in current.items())

# The final lint cleanup changes only one blank line and three scoped lint
# attributes. Prove complete-file identity after removing those exact changes.
archives = json.loads((d / 'actor-input-snapshots-lint-inputs.json').read_text())
changed = {name for name in tested if tested[name] != current[name]}
assert changed == {row['source'] for row in archives}
for row in archives:
    archive = d / row['archive']
    assert sha(archive) == row['sha256'] == tested[row['source']]
    expected = archive.read_text()
    actual = (r / row['source']).read_text()
    actual, count = re.subn(
        r'(?m)^[ \t]*#\[allow\(\s*clippy::too_many_arguments,\s*reason = "[^"\n]+"\s*\)\]\n',
        '', actual)
    assert count == 1, row['source']
    if row['source'].endswith('/input_snapshot.rs'):
        before = '/// Other scene lifecycle work is not part of this input-only snapshot.\n\npub struct'
        assert expected.count(before) == 1
        expected = expected.replace(before, before.replace('\n\n', '\n'), 1)
    assert actual == expected, 'executable snapshot source changed during lint cleanup'

before = (d / 'actor-input-snapshots-static-materials-before.rs.txt').read_text()
after = (r / 'crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs').read_text()
old = '.is_some_and(|actor| !actor.model_materials.is_empty())'
new = '.is_some_and(|actor| actor.model_materials.iter().any(Option::is_some))'
assert before.count(old) == 1 and before.replace(old, new, 1) == after
lint = (d / 'actor-input-snapshots-lint-windows-v2-song-windows-debug/run.log').read_text()
assert 'variables in the condition are not mutated in the loop body' in lint
assert re.search(r'src[\\/]cmd.rs:349:', lint)
assert 'generated 166 warnings' in lint and 'due to 1 previous error' in lint
assert not any(token in lint for token in ['input_snapshot.rs:', 'input_snapshot\\',
                                           'lifecycle\\snapshot.rs:', 'lifecycle/snapshot.rs:'])

sources_out.write_text(json.dumps(current, indent=2) + '\n')
report = dict(
    objective_sha256=objective,
    sources=len(current),
    reviewed_sources_sha256=sha(sources_out),
    source_review=review,
    checks=checks,
    behavior_tests=78,
    final_cleanup='Only one doc-comment blank line and three scoped Clippy attributes; complete executable source equivalence verified.',
    final_shipping_configuration_passed=True,
    clippy='166 warnings and the unchanged pre-existing cmd.rs:349 immutable-loop error; no snapshot diagnostics remain.',
    interrupted_linux_verification={name: sha(d / name) for name in [
        'actor-input-snapshots-contract-v1-host-source-proof.json',
        'actor-input-snapshots-contract-v1-interruption.txt']},
    documents={name: sha(r / name) for name in ['docs/lua-architecture.md',
                 d.relative_to(r).as_posix() + '/README.md']},
    interpreter_milestone='Closed',
    c_api_removal_milestone='Closed',
    production_song_engine='Still legacy; unconditional production cutover remains required.',
    migration_goal_complete=False,
    remaining='Production facade/fallback/cache cutover, legacy isolation, existing API and coverage gaps, saved-handle/session audit, final core/adapter/downstream/content tests, normal application build and rendered song/gameplay/noteskin smoke, dependency/link proof, matched preparation/live/memory measurements.',
)
out.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'sources': len(current), 'behavior_tests': 78,
                  'final_shipping_configuration_passed': True,
                  'production_migration_complete': False}), flush=True)
