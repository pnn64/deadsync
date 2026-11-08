from pathlib import Path
import hashlib, json, os, re, subprocess, time
root = Path.cwd(); png = root / 'crates/deadlib-image/png'; base = 'e30aac88a827ef45eb5928e557fb20ef74c1fadc'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
production = sorted((png / 'src').rglob('*.rs'))
inputs = sorted((png / 'tests').rglob('*.rs')) + sorted((png / 'examples').glob('*.rs')) + sorted((png / 'fuzz/fuzz_targets').glob('*.rs')) + [png / 'tests/oracle/oracle.c', png / 'tests/oracle/CMakeLists.txt', png / 'fuzz/scripts/archive_native_seeds.py', png / 'scripts/verify_independent.ps1', png / 'fuzz/Cargo.toml', png / 'fuzz/Cargo.lock']
assert not subprocess.check_output(['git', 'diff', '--name-only', base, '--', 'crates/deadlib-image/png/src']).strip()
oracle = root / 'target/deadlib-image-reference/oracle-scalar-fields-build/Release/deadlib_png_oracle.exe'
prior = json.loads((png / 'stored-growth-validation.json').read_text())
for row in prior['production_source_sha256']: assert sha(png / row['path']) == row['sha256']
for row in prior['test_source_sha256']:
    if row['path'] != 'examples/seed_fuzz.rs': assert sha(png / row['path']) == row['sha256'], row['path']
assert sha(png.parent / 'src/lib.rs') == prior['shared_source_sha256']
meta = {'baseline_png_commit': base, 'change': 'Independent complete-tree symbol/depth permutation fixtures for LL, empty-distance literals, distances and CL/repeat instructions; production decoder bytes unchanged.', 'production_source_sha256': [{'path': p.relative_to(png).as_posix(), 'sha256': sha(p)} for p in production], 'test_source_sha256': [{'path': p.relative_to(png).as_posix(), 'sha256': sha(p)} for p in inputs], 'shared_source_sha256': sha(png.parent / 'src/lib.rs'), 'oracle_binary': str(oracle), 'oracle_sha256': sha(oracle), 'libpng_commit': '2b978915d82377df13fcbb1fb56660195ded868a', 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(), 'commands': [], 'new_test_files': ['tests/huffman_permutations.rs', 'tests/support/huffman_permutations.rs'], 'modified_existing_test_inputs': ['examples/seed_fuzz.rs'], 'fixture_baseline': {'path': 'stored-growth-validation.json', 'sha256': sha(png / 'stored-growth-validation.json')}, 'runner_sha256': sha(Path(__file__)), 'tools': [{'path': name, 'sha256': sha(png / name)} for name in ['scripts/fuzz_native.py', 'fuzz/scripts/archive_native_seeds.py']], 'standalone_scope': 'Complete copied package; run huffman_permutations offline, print production tree and verify all copied input bytes.'}
manifest = png / 'huffman-permutations-validation.json'; log = png / 'huffman-permutations-validation.log'; assert not manifest.exists() and not log.exists()
env = os.environ.copy(); env.update(CARGO_BUILD_JOBS='4', DEADLIB_LIBPNG_ORACLE=str(oracle))
with log.open('wb') as output:
    def run(label, command):
        print(label, flush=True); output.write((label + '\nCOMMAND: ' + json.dumps(command) + '\n').encode()); output.flush(); start = time.monotonic()
        result = subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
        meta['commands'].append({'label': label, 'command': command, 'exit_code': result.returncode, 'wall_seconds': round(time.monotonic() - start, 3)})
        output.write(('\nEXIT: ' + str(result.returncode) + '\n\n').encode()); output.flush(); manifest.write_bytes((json.dumps(meta, indent=2) + '\n').encode())
        if result.returncode:
            print(log.read_bytes()[-6000:].decode(errors='replace'), flush=True); raise SystemExit(result.returncode)
    run('Strict main lint', ['cargo', 'clippy', '-p', 'deadlib-image', '-p', 'deadlib-image-png', '--all-targets', '--', '-D', 'warnings'])
    run('Strict fuzz lint', ['cargo', 'clippy', '--manifest-path', 'crates/deadlib-image/png/fuzz/Cargo.toml', '--all-targets', '--', '-D', 'warnings'])
    run('Scoped formatting', ['rustfmt', '--edition', '2024', '--check'] + [str(png / p) for p in meta['new_test_files'] + meta['modified_existing_test_inputs']])
    run('Normal release suite', ['cargo', 'test', '-p', 'deadlib-image', '-p', 'deadlib-image-png', '--release', '--', '--nocapture'])
    run('C zlib permutation batch', ['cargo', 'test', '-p', 'deadlib-image-png', '--release', '--test', 'huffman_permutations', 'c_zlib_symbol_permutations', '--', '--ignored', '--nocapture'])
    run('Independent offline permutation audit', ['powershell', '-NoProfile', '-File', str(png / 'scripts/verify_independent.ps1'), '-LogPath', str(png / 'huffman-permutations-independent.log'), '-Test', 'huffman_permutations'])
    run('Fresh permutation seed export', ['cargo', 'run', '-p', 'deadlib-image-png', '--release', '--example', 'seed_fuzz', '--', '--huffman-permutations-only', str(root / 'target/png-huffman-permutations-seeds-20261027-a')])
isolated = Path(re.search(r'Isolated workspace: (.+)', (png / 'huffman-permutations-independent.log').read_text(encoding='utf-8-sig'))[1].strip())
for row in meta['production_source_sha256'] + meta['test_source_sha256']:
    assert sha(png / row['path']) == row['sha256']
    copy = isolated / 'png' / row['path']
    if copy.exists(): assert sha(copy) == row['sha256']
assert sha(isolated / 'src/lib.rs') == meta['shared_source_sha256']
meta.update(independent_workspace=str(isolated), final_source_hashes_verified=True)
manifest.write_bytes((json.dumps(meta, indent=2) + '\n').encode()); print('All permutation validation and fresh export commands passed.', flush=True)
