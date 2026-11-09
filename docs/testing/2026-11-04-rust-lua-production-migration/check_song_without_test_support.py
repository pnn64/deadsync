"""Compile the song adapter with test-only observations absent."""
import hashlib,json,os,subprocess,sys,time
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
out=d/('song-observations-shipping-cfg'+''.join(sys.argv[1:]));out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def sources():
 names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
 paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
 paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
 return {p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
before=sources();env=os.environ.copy()
env.update(CARGO_TARGET_DIR='/var/tmp/deadsync-song-rust-production-20261107',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='4')
native=Path('/var/tmp/deadsync-rust-adapters-native-deps/root/usr')
env.update(PKG_CONFIG_PATH=str(native/'lib/x86_64-linux-gnu/pkgconfig')+':'+str(native/'share/pkgconfig'),CPATH=str(native/'include'))
env['PATH']=str(native/'bin')+':'+env.get('PATH','');env['LD_LIBRARY_PATH']=str(native/'lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH','')
cmd=['cargo','check','--locked','-p','deadsync-song-lua','--features','rust-engine','--lib']
print('song-observations-shipping-cfg started',flush=True);start=time.monotonic()
with (out/'run.log').open('wb') as f:result=subprocess.run(cmd,cwd=r,env=env,stdout=f,stderr=subprocess.STDOUT)
report=dict(command=cmd,exit_code=result.returncode,seconds=time.monotonic()-start,sources=before,sources_unchanged=before==sources(),log_sha256=sha(out/'run.log'))
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='sources'}),flush=True)
assert result.returncode==0 and report['sources_unchanged']
