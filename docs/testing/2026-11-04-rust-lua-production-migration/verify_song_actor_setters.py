"""Verify the shared actor changes against every existing Rust adapter target."""
import json, subprocess, sys, tomllib
from pathlib import Path
d=Path(__file__).resolve().parent; r=d.parents[2]
def run(command, expected=0):
    result=subprocess.run(command)
    assert result.returncode==expected,(command,result.returncode,expected)
focus=json.loads((d/'song-actor-setters-focus-v4-song-linux-debug/result.json').read_text())
assert focus['exit_code']==0 and focus['tests_passed']==45 and focus['tests_failed']==0 and focus['sources_unchanged']
run([sys.executable,str(d/'review_song_actor_setters.py')])
base=[sys.executable,str(d/'checks.py')]
tail=['song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--jobs','4']
run([*base,'song-actor-setters-lint-v1',*tail,'--clippy'],101)
run([*base,'song-actor-setters-library-v1',*tail,'--lib','--filter','scalar_method_identities','--no-fail-fast'])
run([sys.executable,str(d/'check_song_actor_setters_without_test_support.py'),'-v1'])
manifest=tomllib.loads((r/'crates/deadsync-song-lua/Cargo.toml').read_text())
targets=[t['name'] for t in manifest['test'] if t['name'].startswith('rust_')]
assert len(targets)==len(set(targets)) and 'rust_actor_setters' in targets
print('All Rust adapter targets: '+str(len(targets)),flush=True)
command=[*base,'song-actor-setters-regressions-v1',*tail,'--no-fail-fast']
for target in targets:command+=['--test',target]
run(command)
run([sys.executable,str(d/'review_song_actor_setters.py')])
print('Actor-setter regression checks finished; production migration remains active.',flush=True)
