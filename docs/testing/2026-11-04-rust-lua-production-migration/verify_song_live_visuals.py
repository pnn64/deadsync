"""Verify the final visual publication snapshot on the existing Cargo target."""
import json,subprocess,sys
from pathlib import Path
d=Path(__file__).parent
base=[sys.executable,str(d/'checks.py')]
tail=['song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--jobs','4']
def run(args,code=0):
 result=subprocess.run(args);assert result.returncode==code,(args,result.returncode,code)
focus='song-live-visuals-focus-v2-song-linux-debug'
x=json.loads((d/focus/'result.json').read_text());assert x['exit_code']==0 and x['tests_passed']==41 and x['tests_failed']==0 and x['sources_unchanged']
run([sys.executable,str(d/'review_song_live_visuals.py'),'--preflight'])
run([*base,'song-live-visuals-lint-v1',*tail,'--clippy'],101)
run([*base,'song-live-visuals-library-v1',*tail,'--lib','--filter','publication','--no-fail-fast'])
run([sys.executable,str(d/'check_song_live_visuals_without_test_support.py')])
run([sys.executable,str(d/'run_song_live_visuals_regressions.py')])
run([sys.executable,str(d/'review_song_live_visuals.py'),'song-live-visuals-regressions-v1-song-linux-debug',focus,'song-live-visuals-lint-v1-song-linux-debug','song-live-visuals-library-v1-song-linux-debug'])
print('Final live visual publication and source review passed.',flush=True)
