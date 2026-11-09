from pathlib import Path
d=Path(__file__).parent
s=(d/'audit_song_observations.ps1').read_text(encoding='utf-8-sig')
s=s.replace('song-observations','song-live-frames').replace('review_song_observations','review_live_frames').replace('audit_song_observations','audit_live_frames')
s=s.replace('observation_cases','live_phase_cases')
s=s.replace('no progress: scope acknowledgement only; objective revalidated before edits','progress: compiler observation wiring and checks')
s=s.replace('compiler observation fields wired and checked','live frame host phases implemented and checked')
p=d/'audit_live_frames.ps1';assert not p.exists();p.write_text(s)
print('Prepared independent Windows source/objective audit')
