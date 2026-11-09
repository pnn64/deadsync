"""Stop development verification to fix Sound's bound-table replacement."""
from pathlib import Path
p=Path(__file__).with_name('stop_song_actor_setter_checks.py')
source=p.read_text()
source=source.replace("b'/verify_song_actor_setters.py'", "b'/verify_song_actor_setters_v2.py'")
source=source.replace('song-actor-setters-development-stop.json','song-actor-setters-development-stop-v2.json')
source=source.replace('Review found non-setter class fallback regression; stop before more checks, then fix and rerun.',
                      'Sound initialization replaces the bound-method table and discards migrated scalar setters; fix before broad verification.')
exec(compile(source,str(p)+':sound-initialization-review','exec'))
