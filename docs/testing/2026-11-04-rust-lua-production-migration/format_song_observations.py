import hashlib,json,subprocess
from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync');d=Path(__file__).parent
b=json.loads((d/'song-observations-before-sources.json').read_text(encoding='utf-8-sig'))
paths=[p for p in r.joinpath('crates/deadsync-song-lua/src').rglob('*.rs') if hashlib.sha256(p.read_bytes()).hexdigest()!=b.get(p.relative_to(r).as_posix())]
paths+=[r/'crates/deadsync-song-lua/tests/rust_song_observations.rs']
assert all('rust_engine' in str(p) or p.name=='rust_song_observations.rs' for p in paths), paths
subprocess.run(['rustfmt','--edition','2024','--config','skip_children=true',*map(str,paths)],check=True)
(d/'song-observations-formatted-files.json').write_text(json.dumps([p.relative_to(r).as_posix() for p in paths],indent=2)+'\n')
print('Formatted',len(paths),'Rust files')
