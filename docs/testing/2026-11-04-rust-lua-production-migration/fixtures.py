"""Pin the fixture submodule, which the parent repository's ls-files omits."""
from pathlib import Path
import hashlib,json,shutil,subprocess,time
root=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent
folder=root/'tests/fixtures';snapshot=root/'target/rust-lua-production-before'
assert not (out/'fixture-sources.json').exists()
def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
status=subprocess.check_output(['git','status','--porcelain=v1','-z'],cwd=folder)
(out/'fixture-git-status.bin').write_bytes(status)
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=folder,text=True).strip()
paths=sorted(p for p in folder.rglob('*') if p.is_file() and '.git' not in p.relative_to(folder).parts)
sources={};bytes_total=0;start=time.monotonic()
for i,path in enumerate(paths):
    name=path.relative_to(root).as_posix();sources[name]=sha(path);size=path.stat().st_size;bytes_total+=size
    if path.suffix in {'.lua','.sm','.ssc','.py','.rs','.md','.json'} and size<=8*1024*1024:
        dest=snapshot/name;dest.parent.mkdir(parents=True,exist_ok=True);assert not dest.exists();shutil.copyfile(path,dest);assert sha(dest)==sources[name]
    if i%500==0:print('Fixtures hashed:',i,'/',len(paths),flush=True)
(out/'fixture-sources.json').write_text(json.dumps(sources,indent=2)+'\n',encoding='utf-8')
report=dict(submodule='tests/fixtures',commit=commit,files=len(sources),bytes=bytes_total,seconds=time.monotonic()-start,manifest_sha256=sha(out/'fixture-sources.json'),git_status_sha256=sha(out/'fixture-git-status.bin'))
(out/'fixtures.json').write_text(json.dumps(report,indent=2)+'\n');print(report,flush=True)
