import json,subprocess,sys
from pathlib import Path
d=Path(__file__).parent
prior=json.loads((d/'song-callback-errors-regressions-v1-song-linux-debug/result.json').read_text())['command']
targets=[prior[i+1] for i,v in enumerate(prior) if v=='--test']
targets+=['rust_song_observations','rust_game_state','rust_player_state','rust_screen_manager','rust_utility_managers','rust_song_options','rust_top_screen']
assert len(targets)==len(set(targets))==28
cmd=[sys.executable,str(d/'checks.py'),'song-observations-regressions-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support']
for t in targets:cmd+=['--test',t]
cmd+=['--no-fail-fast','--jobs','4']
raise SystemExit(subprocess.call(cmd))
