import subprocess,sys
from pathlib import Path
d=Path(__file__).parent
targets=['rust_live_frames','rust_clock','rust_statistics','rust_player_state','rust_song_options','rust_game_state','rust_screen_manager','rust_messages','rust_player_parts','rust_generated_players','rust_gameplay_underlay','rust_gameplay_reload','rust_scene_retirement','rust_song_observations']
cmd=[sys.executable,str(d/'checks.py'),'song-live-frames-regressions-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support']
for target in targets:cmd+=['--test',target]
cmd+=['--no-fail-fast','--jobs','4']
raise SystemExit(subprocess.call(cmd))
