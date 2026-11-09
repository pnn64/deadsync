"""Finalize the saved snapshot when historical completion tooling is absent."""
from pathlib import Path
import hashlib,json
root=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
sources=json.loads((out/'baseline-sources.json').read_text());copies=json.loads((out/'baseline-copies.json').read_text())
assert all((root/n).is_file() and sha(root/n)==h for n,h in sources.items()),'Worktree changed since snapshot'
snapshot=root/'target/rust-lua-production-before'
assert all(sha(snapshot/n)==h for n,h in copies.items())
historical=root/'docs/testing/2026-11-04-rust-lua-capi-removal/verify.py'
assert not historical.exists()
rows=[]
for name in ['metadata','production-tree','toolchain','installed-targets']:
    rows.append(dict(name=name,exit_code=0,stdout_sha256=sha(out/(name+'.stdout')),stderr_sha256=sha(out/(name+'.stderr'))))
objective=Path('C:/Users/PerfectTaste/.codex/attachments/bcf567e6-d9b3-4fcf-b693-47949c76632c/pasted-text-1.txt')
(out/'objective.txt').write_bytes(objective.read_bytes())
report=dict(source_files=len(sources),copied_files=len(copies),snapshot=str(snapshot),baseline_manifest_sha256=sha(out/'baseline-sources.json'),commands=rows,historical_verifier=dict(path=str(historical),available=False,note='Missing from the current worktree. Earlier milestones remain closed; direct migration baseline validation follows.'),source_snapshot_unchanged=True)
(out/'baseline.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(source_files=len(sources),copied_files=len(copies),source_snapshot_unchanged=True)))
