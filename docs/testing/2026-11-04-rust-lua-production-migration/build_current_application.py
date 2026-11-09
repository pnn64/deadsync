"""Build the normal Rust application and retain source and linker evidence."""
from pathlib import Path
import argparse, hashlib, json, os, re, shutil, subprocess, time
from migration_guard import inputs as migration_inputs

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--target-dir', required=True)
p.add_argument('--jobs', type=int, default=2)
a = p.parse_args()
d = Path(__file__).resolve().parent
r = d.parents[2]
out = d / (a.stage + '-application-windows-release')
out.mkdir(exist_ok=False)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()

def sources():
    names = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z','--',
        'crates','src','Cargo.toml','Cargo.lock','.cargo/config.toml','build.rs'],cwd=r).decode().split('\0')
    return {name: sha(r/name) for name in sorted(set(names)) if name and (r/name).is_file()
        and (Path(name).suffix in {'.rs','.lua','.toml'} or name == 'Cargo.lock')}

before = sources()
(out/'sources.json').write_text(json.dumps(before,indent=2)+'\n')
migration_before = migration_inputs(before)
(out/'migration-sources.json').write_text(json.dumps(migration_before,indent=2)+'\n')
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=a.target_dir,CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',
    CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_RELEASE_DEBUG='0',CARGO_BUILD_JOBS=str(a.jobs))
map_path = (r/a.target_dir/'release/production-current-application.map').resolve()
command = ['cargo','rustc','--locked','-p','deadsync','--bin','deadsync','--release','--',
    '-C','link-arg=/MAP:'+str(map_path)]
start = time.monotonic()
with (out/'run.log').open('wb') as stream:
    result = subprocess.run(command,cwd=r,env=env,stdout=stream,stderr=subprocess.STDOUT)
after = sources()
migration_after = migration_inputs(after)
(out/'sources-after.json').write_text(json.dumps(after,indent=2)+'\n')
report = dict(command=command,exit_code=result.returncode,seconds=time.monotonic()-start,
    log_sha256=sha(out/'run.log'),sources_unchanged=before==after,
    changed_sources=[name for name in sorted(before.keys()|after.keys()) if before.get(name)!=after.get(name)],
    migration_sources_unchanged=migration_before==migration_after,
    migration_changed_sources=[name for name in sorted(migration_before.keys()|migration_after.keys())
        if migration_before.get(name)!=migration_after.get(name)])
if result.returncode == 0:
    artifact = (r/a.target_dir/'release/deadsync.exe').resolve()
    report['artifact'] = dict(path=str(artifact),sha256=sha(artifact))
    saved_map = out/'application.map'
    shutil.copyfile(map_path,saved_map)
    text = saved_map.read_text(errors='replace')
    pattern = r'deadlib_lua_legacy|mlua|luaJIT|\b_?lua_(?:newstate|close|gettop|settop|pcall|call|next|rawget|rawset|tolstring|pushcclosure)\b|\b_?luaL_(?:newstate|loadbuffer|loadfile|openlibs)\b'
    forbidden = [line for line in text.splitlines() if re.search(pattern,line)]
    rust = 'deadlib_lua' in text
    report['link_map'] = dict(exists=True,sha256=sha(saved_map),rust_engine_symbols_found=rust,
        forbidden_lines=forbidden,validation_passed=rust and not forbidden)
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
assert report['migration_sources_unchanged']
assert result.returncode == 0 and report['link_map']['validation_passed']
