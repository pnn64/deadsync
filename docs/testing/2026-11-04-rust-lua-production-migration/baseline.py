"""Freeze the actual dirty worktree before the production Lua migration."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,time
root=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent
snapshot=root/'target/rust-lua-production-before'
assert not snapshot.exists(),'Never overwrite the migration baseline'
snapshot.mkdir(parents=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
status=subprocess.check_output(['git','status','--porcelain=v1','-z'],cwd=root)
(out/'baseline-git-status.bin').write_bytes(status)
names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z'],cwd=root).decode().split('\0'))-{''})
prefixes=['crates/deadlib-lua/','crates/deadsync-song-lua/','crates/deadsync-noteskin/','crates/deadsync-theme-simply-love/','crates/deadsync-assets/','crates/deadsync-notefield/','crates/deadsync-shell/','src/','tests/','examples/','scripts/']
exact=['Cargo.toml','Cargo.lock','.cargo/config.toml','build.rs','docs/lua-architecture.md','docs/lua-engine.md','docs/lua-parity-matrix.md','docs/lua-cache.md']
sources={};copies={}
for name in names:
    p=root/name
    if not p.is_file() or name.startswith(out.relative_to(root).as_posix()+'/'):continue
    digest=sha(p);sources[name]=digest
    if name in exact or (any(name.startswith(prefix) for prefix in prefixes) and p.suffix in {'.rs','.toml','.lua','.py','.md','.json','.sm','.ssc'}):
        destination=snapshot/name;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,destination)
        assert sha(destination)==digest,name
        copies[name]=digest
for filename,data in [('baseline-sources.json',sources),('baseline-copies.json',copies)]:
    (out/filename).write_text(json.dumps(data,indent=2)+'\n',encoding='utf-8')
commands=[('metadata',['cargo','metadata','--locked','--no-deps','--format-version','1']),('production-tree',['cargo','tree','--locked','-p','deadsync','-e','normal,build']),('toolchain',['rustc','-vV']),('installed-targets',['rustup','target','list','--installed']),('prior-removal-verifier',['python','docs/testing/2026-11-04-rust-lua-capi-removal/verify.py'])]
rows=[]
for label,command in commands:
    start=time.monotonic();r=subprocess.run(command,cwd=root,capture_output=True)
    (out/(label+'.stdout')).write_bytes(r.stdout);(out/(label+'.stderr')).write_bytes(r.stderr)
    rows.append(dict(name=label,command=command,exit_code=r.returncode,seconds=time.monotonic()-start,stdout_sha256=sha(out/(label+'.stdout')),stderr_sha256=sha(out/(label+'.stderr'))))
    assert r.returncode==0,(label,r.stderr.decode(errors='replace'))
objective=Path('C:/Users/PerfectTaste/.codex/attachments/bcf567e6-d9b3-4fcf-b693-47949c76632c/pasted-text-1.txt')
(out/'objective.txt').write_bytes(objective.read_bytes())
report=dict(source_files=len(sources),copied_files=len(copies),snapshot=str(snapshot),baseline_manifest_sha256=sha(out/'baseline-sources.json'),commands=rows)
(out/'baseline.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(source_files=len(sources),copied_files=len(copies),commands=[(r['name'],r['exit_code']) for r in rows])),flush=True)
