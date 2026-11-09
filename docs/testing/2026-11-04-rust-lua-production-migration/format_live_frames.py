import hashlib,json,subprocess
from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync');d=Path(__file__).parent
b=json.loads((d/'song-live-frames-before-sources.json').read_text())
paths=[p for p in (r/'crates/deadsync-song-lua/src').rglob('*.rs') if 'rust_engine' in str(p) and hashlib.sha256(p.read_bytes()).hexdigest()!=b.get(p.relative_to(r).as_posix())]
paths.append(r/'crates/deadsync-song-lua/tests/rust_live_frames.rs')
subprocess.run(['rustfmt','--edition','2024','--config','skip_children=true',*map(str,paths)],check=True)
print('Formatted',len(paths),'Rust adapter/test files; native live.rs untouched by formatter')
