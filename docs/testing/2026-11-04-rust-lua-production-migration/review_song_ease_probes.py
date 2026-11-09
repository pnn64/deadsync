"""Review automatic function-ease probing without changing the migration gates."""
from pathlib import Path
import difflib
import hashlib
import json
import re
import subprocess
import sys

base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
reg, lint = sys.argv[1:]
objective_sha = sha(base/'objective.txt')
assert objective_sha == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
          'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell',
          'deadsync-notefield']
names = subprocess.check_output([
    'git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached',
    '--others', '--exclude-standard', '-z', '--',
    *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root/n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root/n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
deps = read(base/'song-ease-probes-test-dependencies.json')
assert deps['exit_code'] == 0
assert sha(base/'song-ease-probes-test-dependencies.stdout') == deps['stdout_sha256']
graph = subprocess.run(deps['command'], cwd=root, capture_output=True, check=True)
assert hashlib.sha256(graph.stdout).hexdigest() == deps['stdout_sha256']

def unused(path):
    p = Path(path).parts
    return len(p) > 2 and p[0] == 'crates' and p[1] not in deps['packages']

results = {s: read(base/s/'result.json') for s in [reg, lint]}
differences = {}
for stage, result in results.items():
    assert sha(base/stage/'run.log') == result['log_sha256']
    checked = read(base/stage/'sources.json')
    differences[stage] = sorted(k for k in current.keys() | checked.keys()
                                if current.get(k) != checked.get(k))
    assert all(unused(k) for k in differences[stage]), (stage, differences[stage])
assert results[reg]['exit_code'] == 0
assert results[reg]['tests_passed'] == 426 and results[reg]['tests_failed'] == 0
assert len(results[reg]['test_results']) == 24
snapshot = read(base/'song-ease-probes-edit-baseline.json')
for path, value in snapshot.items():
    assert hashlib.sha256(value['text'].encode()).hexdigest() == value['sha256']
edited = sorted(p for p, v in snapshot.items() if current[p] != v['sha256'])
expected = ['crates/deadsync-song-lua/src/rust_engine.rs',
            'crates/deadsync-song-lua/src/rust_engine/actors.rs',
            'crates/deadsync-song-lua/src/rust_engine/song_easing.rs',
            'crates/deadsync-song-lua/src/rust_engine/test_support.rs',
            'crates/deadsync-song-lua/tests/rust_song_activation.rs']
expected += ['crates/deadsync-song-lua/src/rust_engine/actors/' + n + '.rs'
             for n in ['action_capture', 'effects', 'framebuffers', 'lifecycle',
                       'render', 'song_columns', 'song_pose', 'sound', 'sprites', 'text']]
assert edited == sorted(expected), edited
added = ['crates/deadsync-song-lua/src/rust_engine/actors/ease_probe.rs',
         'crates/deadsync-song-lua/src/rust_engine/song_ease_probe.rs']
test = 'crates/deadsync-song-lua/tests/rust_song_activation.rs'
text = (root/test).read_text()
assert text.startswith(snapshot[test]['text'].replace('\r\n', '\n').rstrip()+'\n')
assert len(re.findall(r'^#\[test\]', text, re.M)) == 193 and '#[ignore' not in text
assert len(re.findall(r'^#\[test\]\nfn function_ease_', text, re.M)) == 17
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config',
                'skip_children=true', *edited, *added], cwd=root, check=True,
               capture_output=True)

def diagnostics(path):
    return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',
                      path.read_text())

baseline = base/'song-command-locals-lint-v1-song-linux-debug/run.log'
assert results[lint]['exit_code'] == 101
assert diagnostics(base/lint/'run.log') == diagnostics(baseline)
prior = read(base/'song-ease-probes-before-sources.json')
changed = sorted(k for k in current.keys() & prior.keys() if current[k] != prior[k])
new = sorted(current.keys() - prior.keys())
removed = sorted(prior.keys() - current.keys())
assert set(edited) <= set(changed) and set(added) <= set(new)
other = sorted(k for k in changed+new+removed if k not in edited and k not in added)
shared = ['Cargo.toml', 'Cargo.lock']
for path in shared:
    assert current[path] == read(base/reg/'sources.json')[path]
    assert current[path] == read(base/lint/'sources.json')[path]
assert all(unused(k) or k in shared for k in other), other
runner = read(base/'song-ease-probes-runner-change.json')
assert sha(base/'song-ease-probes-checks-before.py') == runner['before_sha256']
assert sha(base/'checks.py') == runner['after_sha256']
diff = ''.join(''.join(difflib.unified_diff(
    snapshot[p]['text'].replace('\r\n', '\n').splitlines(keepends=True),
    (root/p).read_text().splitlines(keepends=True), fromfile='before/'+p, tofile=p))
    for p in edited)
diff += ''.join(''.join(difflib.unified_diff(
    [], (root/p).read_text().splitlines(keepends=True), fromfile='/dev/null', tofile=p))
    for p in added)
diff_path = base/'song-ease-probes-source-diff.diff'
diff_path.write_text(diff)
report = dict(goal_complete=False, production_cutover=False,
              acceptance_scope='objective.txt', objective_sha256=objective_sha,
              final_application_validation=False, source_count=len(current),
              adapter_changed=edited, added=added,
              native_production_source_engine_and_manifests_unchanged_by_adapter=True,
              other_source_changes_preserved=other,
              concurrent_workspace_manifests_match_both_final_checks=True,
              watched_dependency_sources_match_checks=True,
              out_of_dependency_graph_changes_since_checks=differences,
              earlier_176_activation_tests_unchanged=True, new_cases=17,
              adapter_formatting_passed=True, song_clippy_baseline_unchanged=True,
              runner_only_adds_optional_jobs_with_default_four=True,
              source_diff_sha256=sha(diff_path),
              results={s: {k: r[k] for k in ['exit_code', 'seconds', 'log_sha256',
                       'tests_passed', 'tests_failed', 'sources_unchanged']}
                       for s, r in results.items()})
(base/'song-ease-probes-reviewed-sources.json').write_text(json.dumps(current, indent=2)+'\n')
(base/'song-ease-probes-review.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report))
