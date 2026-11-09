"""Run final checks sequentially to avoid Cargo target contention."""
from pathlib import Path
import subprocess,sys
d=Path(__file__).parent
subprocess.run([sys.executable,str(d/'run_live_scene_regressions.py')],check=True)
lint=subprocess.run([sys.executable,str(d/'checks.py'),'song-live-scene-lint-v2','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--clippy','--jobs','4'])
assert lint.returncode==101,lint.returncode
subprocess.run([sys.executable,str(d/'check_live_scene_without_test_support.py')],check=True)
subprocess.run([sys.executable,str(d/'review_live_scene.py'),'song-live-scene-regressions-v1-song-linux-debug','song-live-scene-focus-v10-song-linux-debug','song-live-scene-lint-v2-song-linux-debug'],check=True)
