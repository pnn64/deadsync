import json,subprocess,sys
from pathlib import Path
d=Path(__file__).parent
previous=json.loads((d/'song-live-gameplay-review.json').read_text())['results']['song-live-gameplay-regressions-v1-song-linux-debug']['command']
stage=sys.argv[1] if len(sys.argv)>1 else 'song-arrows-regressions-v1'
owners=['rust_catalog','rust_game_state','rust_player_options','rust_player_state'] if stage=='song-arrows-regressions-v3' else []
targets=['rust_arrows','rust_clock',*owners,*[previous[i+1] for i,v in enumerate(previous) if v=='--test']]
assert len(targets)==34+len(owners) and len(set(targets))==34+len(owners)
if stage=='song-arrows-regressions-v2':
 ownership=[sys.executable,str(d/'checks.py'),'song-arrows-option-ownership-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--test','rust_player_options','--filter','retained_scalar_method_owns_only_its_level_and_discarded_cycles_collect','--jobs','4']
 result=subprocess.call(ownership)
 if result:raise SystemExit(result)
cmd=[sys.executable,str(d/'checks.py'),stage,'song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--no-fail-fast','--jobs','4']
for t in targets:cmd+=['--test',t]
raise SystemExit(subprocess.call(cmd))
