from pathlib import Path
import sys
if '-v1' in sys.argv:
    print('Development verification stopped before shipping/broad checks: preserve non-setter class fallback first.',flush=True)
    raise SystemExit(2)
p=Path(__file__).with_name('check_song_without_test_support.py')
exec(compile(p.read_text().replace('song-observations','song-actor-setters'),str(p)+':song-actor-setters-phase','exec'))
