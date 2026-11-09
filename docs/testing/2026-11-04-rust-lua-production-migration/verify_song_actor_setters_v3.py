"""Verify final setter ownership, including Sound's resource installation."""
from pathlib import Path
p=Path(__file__).with_name('verify_song_actor_setters.py')
source=p.read_text().replace('focus-v4','focus-v6').replace('==45','==10').replace('-v1','-v3')
exec(compile(source,str(p)+':sound-initialization-correction','exec'))
