import hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
names=['crates/deadsync-song-lua/assets/itgmania/arrow-'+kind+'-native-reference-v1.json' for kind in ['x','y','zoom']]+['tests/fixtures/itgmania-actors/pause-offset.json']
data={n:hashlib.sha256((r/n).read_bytes()).hexdigest() for n in names}
p=d/'song-arrows-reference-inputs.json';assert not p.exists();p.write_text(json.dumps(data,indent=2)+'\n');print(json.dumps(data))
