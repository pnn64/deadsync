"""Run the full actor checks after preserving non-setter fallback behavior."""
from pathlib import Path
p=Path(__file__).with_name('verify_song_actor_setters.py')
text=p.read_text().replace('focus-v4','focus-v5').replace("==45", "==46").replace('-v1','-v2')
exec(compile(text,str(p)+':fallback-correction','exec'))
