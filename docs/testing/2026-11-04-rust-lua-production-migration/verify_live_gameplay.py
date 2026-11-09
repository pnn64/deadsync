"""Run final checks sequentially against one immutable source snapshot."""
import subprocess, sys
from pathlib import Path
d=Path(__file__).parent
base=[sys.executable,str(d/'checks.py')]
tail=['song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--jobs','4']
def run(args,code=0):
    result=subprocess.run(args);assert result.returncode==code,(args,result.returncode,code)
run([*base,'song-live-gameplay-callbacks-v4',*tail,'--test','rust_live_gameplay','--no-fail-fast'])
run([*base,'song-live-gameplay-lint-v2',*tail,'--clippy'],101)
run([*base,'song-live-gameplay-library-v1',*tail,'--lib','--filter','live::','--no-fail-fast'])
run([sys.executable,str(d/'check_live_gameplay_without_test_support.py')])
run([sys.executable,str(d/'run_live_gameplay_regressions.py')])
run([sys.executable,str(d/'review_live_gameplay.py'),'song-live-gameplay-regressions-v1-song-linux-debug','song-live-gameplay-callbacks-v4-song-linux-debug','song-live-gameplay-lint-v2-song-linux-debug','song-live-gameplay-library-v1-song-linux-debug'])
print('Final gameplay source and validation review passed.',flush=True)
