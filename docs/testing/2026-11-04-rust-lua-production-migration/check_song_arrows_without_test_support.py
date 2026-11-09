from pathlib import Path
p=Path(__file__).with_name('check_song_without_test_support.py')
exec(compile(p.read_text().replace('song-observations','song-arrows'),str(p)+':song-arrows-phase','exec'))
