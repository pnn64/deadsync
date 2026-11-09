from pathlib import Path
import subprocess,sys
d=Path(__file__).parent
targets=['rust_live_actors','rust_live_frames','rust_song_multitap','rust_song_updates','rust_song_update_output','rust_song_compiler','rust_song_observations','rust_song_diagnostics','rust_actors','rust_player_parts','rust_generated_players','rust_notefields','rust_wrappers','rust_transitions','rust_scene_retirement','rust_statistics']
cmd=[sys.executable,str(d/'checks.py'),'song-live-actors-regressions-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--no-fail-fast','--jobs','4']
for target in targets:cmd+=['--test',target]
raise SystemExit(subprocess.call(cmd))
