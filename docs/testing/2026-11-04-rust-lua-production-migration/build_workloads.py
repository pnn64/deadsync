"""Build and preserve the existing production preparation/live measurement tools."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time
from migration_guard import inputs as migration_inputs

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('stage')
parser.add_argument('--target-dir', required=True)
parser.add_argument('--release', action='store_true')
parser.add_argument('--only', nargs='+', choices=['song_lua_execution', 'song_lua_performance', 'compare_lua_engines'])
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
profile = 'release' if args.release else 'debug'
platform = 'windows' if os.name == 'nt' else 'linux'
out = Path(__file__).resolve().parent / f'{args.stage}-workloads-{platform}-{profile}'
out.mkdir(exist_ok=False)
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=args.target_dir, CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
           CARGO_PROFILE_RELEASE_DEBUG='0', CARGO_BUILD_JOBS='4')
if os.name != 'nt':
    native = Path('/var/tmp/deadsync-rust-adapters-native-deps/root/usr')
    if native.is_dir():
        env.update(PKG_CONFIG_PATH=f'{native}/lib/x86_64-linux-gnu/pkgconfig:{native}/share/pkgconfig',
                   CPATH=f'{native}/include')
        env['PATH'] = f'{native}/bin:' + env.get('PATH', '')
        env['LD_LIBRARY_PATH'] = f'{native}/lib/x86_64-linux-gnu:' + env.get('LD_LIBRARY_PATH', '')
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
def sources():
    # Re-enumerate current files: the completed C API removal deleted sources
    # in the frozen inventory, and migration added Rust adapter modules. Keep
    # the frozen baseline reports intact while guarding the actual build input.
    names = subprocess.check_output([
        'git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
        'crates', 'Cargo.toml', 'Cargo.lock', '.cargo/config.toml',
        'examples/song_lua_performance.rs', 'tests/support',
    ], cwd=root).decode('utf-8').split('\0')
    paths = {root / name for name in names if name and
             (Path(name).suffix in {'.rs', '.toml', '.lua'} or name == 'Cargo.lock')}
    return {path.relative_to(root).as_posix(): sha(path)
            for path in sorted(paths) if path.is_file()}

before = sources()
migration_before = migration_inputs(before)
(out / 'migration-sources.json').write_text(json.dumps(migration_before, indent=2) + '\n')
(out / 'sources.json').write_text(json.dumps(before, indent=2) + '\n')
(out / 'environment.json').write_text(json.dumps({
    'platform': platform, 'profile': profile,
    'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=root).decode('utf-8'),
    'cargo_environment': {key: value for key, value in env.items()
                          if key.startswith('CARGO_')},
}, indent=2) + '\n')
results = []
for name, package, switches, subdir in [
    ('song_lua_execution', 'deadsync-shell', ['--bin', 'song_lua_execution'], ''),
    ('song_lua_performance', 'deadsync', ['--example', 'song_lua_performance'], 'examples'),
    ('compare_lua_engines', 'deadsync-noteskin',
     ['--example', 'compare_lua_engines'], 'examples'),
]:
    if args.only and name not in args.only:
        continue
    command = ['cargo', 'build', '--locked', '-p', package, *switches]
    if args.release:
        command.append('--release')
    log = out / f'{name}.log'
    print(f'building {name}', flush=True)
    started = time.monotonic()
    with log.open('wb') as stream:
        result = subprocess.run(command, cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT)
    row = dict(name=name, command=command, exit_code=result.returncode,
               seconds=time.monotonic()-started, log_sha256=sha(log))
    if result.returncode == 0:
        filename = name + ('.exe' if os.name == 'nt' else '')
        artifact = Path(args.target_dir) / profile / subdir / filename
        preserved = root / 'target' / f'rust-lua-production-{args.stage}' / 'artifacts' / platform / profile / filename
        preserved.parent.mkdir(parents=True, exist_ok=True)
        assert not preserved.exists(), preserved
        # Preserve executable bytes; copying Linux metadata onto DrvFs can fail
        # after the copy has already succeeded.
        shutil.copyfile(artifact, preserved)
        row.update(artifact=str(preserved), artifact_sha256=sha(preserved))
    else:
        print(log.read_text(errors='replace')[-3000:], flush=True)
    results.append(row)
    (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(row), flush=True)
after = sources()
changes = [path for path in sorted(before.keys() | after.keys())
           if before.get(path) != after.get(path)]
(out / 'changed-sources.json').write_text(json.dumps(changes, indent=2) + '\n')
migration_after = migration_inputs(after)
migration_changes = [path for path in sorted(migration_before.keys() | migration_after.keys())
                     if migration_before.get(path) != migration_after.get(path)]
(out / 'guard.json').write_text(json.dumps(dict(sources_unchanged=not changes,
    changed_sources=changes, migration_sources_unchanged=not migration_changes,
    migration_changed_sources=migration_changes,
    scope='Original inventory sites, seven original Lua regression packages, root application sources and all manifests; full workspace differences retained separately'), indent=2) + '\n')
assert not migration_changes, migration_changes
assert all(row['exit_code'] == 0 for row in results)
