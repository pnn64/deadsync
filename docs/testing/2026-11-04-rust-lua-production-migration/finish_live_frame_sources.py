from pathlib import Path
import re
r=Path('/mnt/c/GitHub/deadsync/crates/deadsync-song-lua')
p=r/'tests/rust_live_frames.rs';s=p.read_text()
for old,new in [('__songlua_runtime','__songlua_compile_song_runtime'),('__songlua_song_seconds','__songlua_music_seconds'),('__songlua_delta_beat','__songlua_song_delta_beat'),('__songlua_delta_seconds','__songlua_music_delta_seconds')]:s=s.replace(old,new)
s=re.sub(r'(?<![A-Za-z0-9_.])\.(?=\d)','0.',s)
s=s.replace('let windows=Arc::from([','let windows:Arc<[deadsync_gameplay::ChartAttackWindow]>=Arc::from([')
p.write_text(s)
p=r/'src/rust_engine/live_frames.rs';s=p.read_text()
s=s.replace('index(stack, stack.top(), 4)?;','let batch=stack.top();index(stack, batch, 4)?;')
s=s.replace('write(stack, options, "__songlua_player_option_state", stack.value(-1).unwrap(), true)?;','let created=stack.value(-1).unwrap();write(stack, options, "__songlua_player_option_state", created, true)?;')
p.write_text(s)
