"""Run final song checks once focused initial-state parity passes."""
from pathlib import Path
import json,re,subprocess,tomllib
base=Path(__file__).resolve().parent;root=base.parents[2]
r=json.loads((base/'song-initial-focused-v5-song-linux-debug/result.json').read_text())
assert r['exit_code']==0 and r['sources_unchanged']
common=['--target-dir','/var/tmp/deadsync-rust-adapters-target','--features','rust-engine,test-support']
def run(stage,*args,expected=0):
    p=subprocess.run(['python3',str(base/'checks.py'),stage,'song',*common,*args],cwd=root)
    assert p.returncode==expected,(stage,p.returncode)
run('song-initial-lint-v1','--clippy',expected=101)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
a=diagnostics(base/'song-initial-lint-v1-song-linux-debug/run.log');b=diagnostics(base/'song-layers-lint-v1-song-linux-debug/run.log')
assert a==b,{'new':list(set(a)-set(b)),'missing':list(set(b)-set(a))}
targets=sorted(t['name']for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test']if t['name'].startswith('rust_'))
run('song-initial-adapters-v1',*[a for t in targets for a in ['--test',t]],'--no-fail-fast')
subprocess.run(['python3',str(base/'review_song_initial.py')],cwd=root,check=True)
