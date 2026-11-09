"""Verify the authored layer construction checkpoint after focused parity passes."""
from pathlib import Path
import json,re,subprocess,tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
r=json.loads((base/'song-layers-focused-v5-song-linux-debug/result.json').read_text())
assert r['exit_code']==0 and r['sources_unchanged'] and r['tests_passed']==52
common=['--target-dir','/var/tmp/deadsync-rust-adapters-target','--features','rust-engine,test-support']
def run(stage,*args,expected=0):
    p=subprocess.run(['python3',str(base/'checks.py'),stage,'song',*common,*args],cwd=root)
    assert p.returncode==expected,(stage,p.returncode)
run('song-layers-lint-v1','--clippy',expected=101)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
actual=diagnostics(base/'song-layers-lint-v1-song-linux-debug/run.log')
previous=diagnostics(base/'scene-retirement-lint-v2-song-linux-debug/run.log')
assert actual==previous,{'new':list(set(actual)-set(previous)),'missing':list(set(previous)-set(actual))}
targets=sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
run('song-layers-adapters-v1',*[a for t in targets for a in ['--test',t]],'--no-fail-fast')
subprocess.run(['python3',str(base/'review_song_layers.py')],cwd=root,check=True)
