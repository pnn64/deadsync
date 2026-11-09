"""Run final source verification after the focused retirement tests pass."""
from pathlib import Path
import json,re,subprocess,tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
focused=json.loads((base/'scene-retirement-focused-v5-song-linux-debug/result.json').read_text())
assert focused['exit_code']==0 and focused['sources_unchanged'] and focused['tests_passed']==10
common=['--target-dir','/var/tmp/deadsync-rust-adapters-target']
def run(stage,suite,*args,expected=0):
    result=subprocess.run(['python3',str(base/'checks.py'),stage,suite,*common,*args],cwd=root)
    assert result.returncode==expected,(stage,result.returncode)
run('scene-retirement-core-lint-v1','core','--clippy')
run('scene-retirement-lint-v1','song','--features','rust-engine,test-support','--clippy',expected=101)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert diagnostics(base/'scene-retirement-lint-v1-song-linux-debug/run.log')==diagnostics(base/'gameplay-reload-lint-v1-song-linux-debug/run.log')
run('scene-retirement-core-v1','core')
targets=sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
run('scene-retirement-adapters-v1','song','--features','rust-engine,test-support',*[a for t in targets for a in ['--test',t]],'--no-fail-fast')
run('scene-retirement-noteskin-v1','noteskin')
run('scene-retirement-gifs-v1','gifs')
subprocess.run(['python3',str(base/'review_scene_retirement.py')],cwd=root,check=True)
