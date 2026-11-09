"""Validate the final arrow snapshot sequentially on the existing target."""
import subprocess,sys
from pathlib import Path
d=Path(__file__).parent
base=[sys.executable,str(d/'checks.py')]
tail=['song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--jobs','4']
def run(args,code=0):
 result=subprocess.run(args);assert result.returncode==code,(args,result.returncode,code)
run([*base,'song-arrows-focus-v5',*tail,'--test','rust_arrows','--test','rust_clock','--no-fail-fast'])
run([*base,'song-arrows-lint-v3',*tail,'--clippy'],101)
run([*base,'song-arrows-library-v1',*tail,'--lib','--filter','host::tests::arrow_','--no-fail-fast'])
run([sys.executable,str(d/'check_song_arrows_without_test_support.py')])
run([sys.executable,str(d/'run_song_arrows_regressions.py')])
run([sys.executable,str(d/'review_song_arrows.py'),'song-arrows-regressions-v1-song-linux-debug','song-arrows-focus-v5-song-linux-debug','song-arrows-lint-v3-song-linux-debug','song-arrows-library-v1-song-linux-debug'])
print('Final ArrowEffects source and validation review passed.',flush=True)
