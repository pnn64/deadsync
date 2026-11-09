"""Run the relevant final-source gates without overlapping Cargo invocations."""
from pathlib import Path
import subprocess,sys
d=Path(__file__).parent
base=[sys.executable,str(d/'checks.py')]
common=['song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--jobs','4']
subprocess.run([*base,'song-live-compiler-focus-v5',*common,'--test','rust_live_compiler','--no-fail-fast'],check=True)
subprocess.run([sys.executable,str(d/'run_live_compiler_regressions.py')],check=True)
subprocess.run([*base,'song-live-compiler-library-v1',*common,'--lib','--filter','compile::','--no-fail-fast'],check=True)
subprocess.run([sys.executable,str(d/'check_live_compiler_without_test_support.py')],check=True)
subprocess.run([sys.executable,str(d/'review_live_compiler.py'),'song-live-compiler-regressions-v1-song-linux-debug','song-live-compiler-focus-v5-song-linux-debug','song-live-compiler-lint-v1-song-linux-debug','song-live-compiler-library-v1-song-linux-debug'],check=True)
