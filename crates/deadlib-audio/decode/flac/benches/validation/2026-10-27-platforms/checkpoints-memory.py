from pathlib import Path
import hashlib,json,subprocess
root=Path(__file__).resolve().parents[2]
out=Path('/tmp/deadlib-flac-memory-checkpoints')
out.mkdir(exist_ok=False)
old=json.loads((root/'crates/deadlib-audio/decode/flac/benches/validation/2026-10-27-dense/memory-comparison.json').read_text())
binary=Path('/tmp/deadlib-flac-memory-checkpoints-rust/release/deadlib-flac-allocation-probe')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
records=[]
for before in old['records']:
 if before['mode'].startswith('symphonia'):continue
 command=before['command'].copy();command[0]=str(binary)
 result=subprocess.run(command,capture_output=True,text=True,check=True)
 cells=[list(map(int,line.split('\t')[1:])) for line in result.stderr.splitlines() if line.startswith('ALLOC\t')]
 assert len(cells)==3
 if command[1]=='frames':assert all(p[1:7]==[0,0,0,0,0,0] for p in cells[1:]),cells
 else:assert all(p[6]==0 for p in cells),cells
 records.append(dict(case=before['case'],mode=before['mode'],dense=before['after'],current=cells,command=command,stdout=result.stdout))
 print(before['case'][:2],before['mode'],before['after'][1][5],cells[1][5],flush=True)
sources={p.relative_to(root).as_posix():sha(p) for p in (root/'crates/deadlib-audio/decode/flac/src').glob('*.rs')}
probe={p.relative_to(root).as_posix():sha(p) for p in [root/'target/deadlib-flac-reference/allocator-probe.rs',root/'target/deadlib-flac-reference/decode-shared-probe.rs',root/'target/deadlib-flac-reference/memory-probe/Cargo.toml',root/'target/deadlib-flac-reference/memory-probe/Cargo.lock']}
(out/'results.json').write_text(json.dumps({'records':records,'source_sha256':sources,'probe_sha256':probe,'binary_sha256':sha(binary),'note':'Requested-heap allocation counts only. No throughput, stack or RSS claim. Counting System allocator forwards every request. Fields: iteration, allocations, reallocations, frees, total requested, peak above baseline, final live delta, baseline live bytes.'},indent=2)+'\n')
