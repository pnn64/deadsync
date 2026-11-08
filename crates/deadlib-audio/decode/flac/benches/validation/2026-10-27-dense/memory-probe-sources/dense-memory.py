from pathlib import Path
import hashlib,json,subprocess
root=Path('/mnt/c/GitHub/deadsync')
out=Path('/tmp/deadlib-flac-memory-dense')
out.mkdir(exist_ok=False)
old=json.loads(Path('/tmp/deadlib-flac-memory-pilot/results.json').read_text())
binary=Path('/tmp/deadlib-flac-memory-rust/release/deadlib-flac-allocation-probe')
records=[]
for before in old:
 if not before['command'][0].endswith('deadlib-flac-allocation-probe'):continue
 command=before['command']
 result=subprocess.run(command,capture_output=True,text=True,check=True)
 cells=[list(map(int,line.split('\t')[1:])) for line in result.stderr.splitlines() if line.startswith('ALLOC\t')]
 assert len(cells)==3
 record=dict(case=before['case'],mode=before['mode'],before=before['phases'],after=cells,command=command,
  before_binary_sha256=before['binary_sha256'],after_binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
 records.append(record)
 print(before['case'][:2],before['mode'],before['phases'][1][5],cells[1][5],flush=True)
sources={p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'crates/deadlib-audio/decode/flac/src').glob('*.rs')}
(out/'results.json').write_text(json.dumps({'records':records,'after_sources':sources},indent=2)+'\n')
