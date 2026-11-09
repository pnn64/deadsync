from pathlib import Path
import hashlib,json,os,subprocess,time
base=Path(__file__).resolve().parent
root=base.parents[2]
out=base/'song-perframes-typecheck-v1'
out.mkdir(exist_ok=False)
env=os.environ.copy()
env.update(CARGO_TARGET_DIR='/mnt/deadsync-build-d/CodexBuildCache/deadsync-ease-probes-20261107',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='4')
native=Path('/var/tmp/deadsync-rust-adapters-native-deps/root/usr')
env.update(PKG_CONFIG_PATH=str(native/'lib/x86_64-linux-gnu/pkgconfig')+':'+str(native/'share/pkgconfig'),CPATH=str(native/'include'))
env['PATH']=str(native/'bin')+':'+env['PATH']
env['LD_LIBRARY_PATH']=str(native/'lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH','')
command=['cargo','check','--locked','-p','deadsync-song-lua','--features','rust-engine,test-support','--lib']
start=time.monotonic()
with (out/'run.log').open('wb') as log: result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
report=dict(command=command,exit_code=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256((out/'run.log').read_bytes()).hexdigest(),scope='Early library-only diagnostic while integration tests are authored; not final-source verification.')
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
if result.returncode: print((out/'run.log').read_text()[-10000:])
raise SystemExit(result.returncode)