"""Run existing compiler, actor, lifecycle and output coverage with live scenes."""
from pathlib import Path
import subprocess,sys
d=Path(__file__).parent
targets=['rust_live_scene','rust_live_layers','rust_live_actors','rust_live_frames','rust_song_layers','rust_song_activation','rust_song_multitap','rust_song_updates','rust_song_update_output','rust_song_compiler','rust_song_observations','rust_song_diagnostics','rust_actors','rust_player_parts','rust_generated_players','rust_notefields','rust_wrappers','rust_transitions','rust_scene_retirement','rust_statistics','rust_render','rust_effects','rust_sprites','rust_models','rust_bitmap_text','rust_draw','rust_input','rust_messages','rust_screen_manager','rust_sound']
cmd=[sys.executable,str(d/'checks.py'),'song-live-scene-regressions-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--no-fail-fast','--jobs','4']
for target in targets:cmd+=['--test',target]
raise SystemExit(subprocess.call(cmd))
