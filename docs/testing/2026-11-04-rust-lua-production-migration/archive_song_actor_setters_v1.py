"""Preserve the initial differential failures and source before correcting them."""
import base64, hashlib, json
from pathlib import Path
d=Path(__file__).resolve().parent; r=d.parents[2]
names=list(json.loads((d/'song-actor-setters-edit-baseline.json').read_text()))
names += ['crates/deadsync-song-lua/src/rust_engine/actors/scalar_methods.rs',
          'crates/deadsync-song-lua/tests/rust_actor_setters.rs']
saved={n:{'sha256':hashlib.sha256((r/n).read_bytes()).hexdigest(),
          'base64':base64.b64encode((r/n).read_bytes()).decode()} for n in names}
out=d/'song-actor-setters-v1-source.json'; assert not out.exists()
out.write_text(json.dumps(saved,indent=2)+'\n')
