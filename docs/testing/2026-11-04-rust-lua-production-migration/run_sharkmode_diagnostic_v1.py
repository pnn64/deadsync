"""Expose the original Sharkmode initialization error in an isolated oracle tool."""
from pathlib import Path
import hashlib, json, os, subprocess, time

root = Path(__file__).resolve().parents[3]
ledger = Path(__file__).resolve().parent
out = ledger / 'production-sharkmode-diagnostic-v1'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
watched = json.loads((ledger / 'production-live-observer-regressions-v1-song-windows-debug/sources.json').read_text())
before = {name: sha(root / name) for name in watched}
helpers = {str(p.relative_to(root)): sha(p) for p in [out/'Cargo.toml', out/'src/main.rs']}
(out/'sources.json').write_text(json.dumps(dict(runtime=before, helper=helpers), indent=2)+'\n')
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(root/'target/rust-adapters'), CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='2')
command = ['cargo','run','--offline','--manifest-path',str(out/'Cargo.toml'),
           '--bin','production-sharkmode-control']
started = time.monotonic()
with (out/'run.log').open('wb') as stream:
    result = subprocess.run(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT)
changed = [name for name,value in before.items() if sha(root/name) != value]
changed += [name for name,value in helpers.items() if sha(root/name) != value]
report = dict(command=command,exit_code=result.returncode,seconds=time.monotonic()-started,
              log_sha256=sha(out/'run.log'),sources_unchanged=not changed,changed_sources=changed,
              note='Initialization diagnosis only; execution success is not a regression or performance acceptance claim.')
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
print((out/'run.log').read_text(errors='replace')[-5000:])
assert not changed
raise SystemExit(result.returncode)
