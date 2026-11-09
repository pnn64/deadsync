"""Independently verify sources while the completed test runner waits on DrvFS."""
import datetime
import hashlib
import json
import re
import subprocess
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
stage = d / 'actor-input-snapshots-contract-v1-song-linux-debug'
out = d / 'actor-input-snapshots-contract-v1-host-source-proof.json'
assert not out.exists()
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = json.loads((stage / 'sources.json').read_text())
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
          'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell',
          'deadsync-notefield']
names = subprocess.check_output([
    'git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
    *[f'crates/{name}' for name in crates]], cwd=r).decode().split('\0')
paths = {r / name for name in names if name and Path(name).suffix in {'.rs', '.lua', '.toml'}}
paths.update(r / name for name in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(r).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert current == before, 'watched sources changed after the test run'
log = (stage / 'run.log').read_text(errors='replace')
results = re.findall(r'test result: .*', log)
assert len(results) == 4
assert all('test result: ok.' in result and '0 failed;' in result for result in results)
assert [int(re.search(r'ok\. (\d+) passed;', result)[1]) for result in results] == [17, 0, 17, 9]
out.write_text(json.dumps({
    'recorded_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'stage': stage.name,
    'runner_pid': 3088905,
    'observed_runner_wait': 'p9_client_rpc',
    'runner_result': 'Still pending at the time of this independent host source check.',
    'watched_sources': len(current),
    'sources_unchanged': True,
    'source_manifest_sha256': sha(stage / 'sources.json'),
    'log_sha256': sha(stage / 'run.log'),
    'test_results': results,
    'tests_passed': 43,
    'tests_failed': 0,
    'windows_render_validation': 'Not exercised: actor_input_render is Windows-only and ran zero tests under Linux.',
    'objective_sha256': sha(d / 'objective.txt'),
}, indent=2) + '\n')
print(json.dumps({'watched_sources': len(current), 'sources_unchanged': True,
                  'tests_passed': 43, 'runner_result_still_pending': True}), flush=True)
