"""Record the normal application check/build without an engine feature flag."""
import argparse
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--build', action='store_true')
p.add_argument('--release', action='store_true', help='Use the normal application release profile.')
p.add_argument('--link-map', action='store_true', help='Build the normal application with MSVC linker-map evidence; no engine feature is selected.')
p.add_argument('--contract', action='store_true')
p.add_argument('--check-tests', action='store_true', help='Type-check every existing test target before linking the full suite.')
p.add_argument('--all-tests', action='store_true')
p.add_argument('--no-fail-fast', action='store_true', help='Run every selected target even when one reports a failure.')
p.add_argument('--oracle', action='store_true', help='Enable the isolated development oracle for contract tests.')
p.add_argument('--test', action='append')
p.add_argument('--filter', help='Run a focused existing test-name filter.')
a = p.parse_args()
if a.check_tests:
    a.contract = True
assert not a.oracle or a.contract
assert not a.link_map or (a.build and not a.contract)
assert not a.release or (a.build and not a.contract)
d = Path(__file__).resolve().parent
r = d.parents[2]
profile = 'release' if a.release else 'debug'
out = d / (a.stage + ('-song' if a.contract else '-application') + '-windows-' + profile)
out.mkdir(exist_ok=False)
assert os.name == 'nt'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
def sources():
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others',
        '--exclude-standard', '-z', '--', *[f'crates/{name}' for name in [
        'deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
        'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell',
        'deadsync-notefield']]], cwd=r).decode().split('\0')
    paths = {r / name for name in names if name and Path(name).suffix in {'.rs', '.lua', '.toml'}}
    paths.update(r / name for name in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
    return {p.relative_to(r).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
before = sources()
(out / 'sources.json').write_text(json.dumps(before, indent=2) + '\n')
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(r / 'target/rust-adapters'), CARGO_INCREMENTAL='0',
    CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_RELEASE_DEBUG='0',
    CARGO_BUILD_JOBS='4')
tests = (['--tests'] if a.check_tests or a.all_tests else
    [arg for test in (a.test or ['production_routing']) for arg in ['--test', test]])
cmd = (['cargo', 'check' if a.check_tests else 'test', '--locked', '-p', 'deadsync-song-lua', *tests,
        *(['--features', 'legacy-oracle'] if a.oracle else []),
        *(['--no-fail-fast'] if (a.all_tests or a.no_fail_fast) and not a.check_tests else []),
        *([] if a.check_tests else ['--', *([a.filter] if a.filter else []), '--nocapture', '--test-threads=1'])] if a.contract else
       ['cargo', 'build' if a.build else 'check', '--locked', '-p', 'deadsync', '--bin', 'deadsync'])
if a.release:
    cmd.append('--release')
map_file = out.resolve() / 'application.map'
if a.link_map:
    # Extra rustc arguments apply only to the application crate. Dependencies
    # retain the normal features/profile; a map does not select a backend.
    cmd = ['cargo', 'rustc', '--locked', '-p', 'deadsync', '--bin', 'deadsync',
           *(['--release'] if a.release else []),
           '--', '-C', f'link-arg=/MAP:{map_file}']
print(a.stage + (' oracle contracts started' if a.oracle else ' normal production routing started'), flush=True)
started = time.monotonic()
with (out / 'run.log').open('wb') as stream:
    result = subprocess.run(cmd, cwd=r, env=env, stdout=stream, stderr=subprocess.STDOUT)
report = dict(command=cmd, exit_code=result.returncode, seconds=time.monotonic()-started,
              log_sha256=sha(out / 'run.log'), sources_unchanged=before == sources())
if a.build and not a.contract and result.returncode == 0:
    binary = r / 'target/rust-adapters' / profile / 'deadsync.exe'
    report['artifact'] = dict(path=str(binary), sha256=sha(binary))
if a.link_map:
    analysis = dict(exists=map_file.is_file())
    if map_file.is_file():
        lines = map_file.read_text(errors='replace').splitlines()
        forbidden = re.compile(
            r'deadlib[-_]lua[-_]legacy|(?:^|[\\/\s:])(?:lib)?mlua[-_.]|'
            r'lua[-_]src|luajit|(?:^|\s)(?:__imp_)?_?lua(?:L_|open_|_[A-Za-z0-9_]|[A-Z][A-Za-z0-9_]*)', re.I)
        analysis.update(sha256=sha(map_file),
            rust_engine_symbols_found=any('deadlib_lua' in line for line in lines),
            forbidden_lines=[line for line in lines if forbidden.search(line)])
    analysis['validation_passed'] = bool(analysis.get('rust_engine_symbols_found') and
                                          not analysis.get('forbidden_lines'))
    report['link_map'] = analysis
if a.contract:
    log = (out / 'run.log').read_text(errors='replace')
    report.update(tests_passed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed;', log))),
                  tests_failed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. \d+ passed; (\d+) failed;', log))))
    report['check_only'] = a.check_tests
(out / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
if result.returncode:
    print((out / 'run.log').read_text(errors='replace')[-5000:])
    raise SystemExit(result.returncode)
assert report['sources_unchanged']
if a.link_map:
    assert report['link_map']['validation_passed'], report['link_map']
