import json,subprocess,sys
from pathlib import Path
d=Path(__file__).parent
previous=json.loads((d/'song-arrows-review.json').read_text())['results']['song-arrows-regressions-v3-song-linux-debug']['command']
targets=['rust_live_visuals','rust_meshes','rust_proxies',*[previous[i+1] for i,v in enumerate(previous) if v=='--test']]
assert len(targets)==len(set(targets))==41
stage=sys.argv[1] if len(sys.argv)>1 else 'song-live-visuals-regressions-v1'
cmd=[sys.executable,str(d/'checks.py'),stage,'song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--no-fail-fast','--jobs','4']
for target in targets:cmd+=['--test',target]
raise SystemExit(subprocess.call(cmd))
