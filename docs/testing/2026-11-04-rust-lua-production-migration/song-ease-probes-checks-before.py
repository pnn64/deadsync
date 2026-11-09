"""Record unmodified production regression baselines and later migration runs."""
from pathlib import Path
import argparse,hashlib,json,os,re,subprocess,time
p=argparse.ArgumentParser(description=__doc__);p.add_argument('stage');p.add_argument('suite',choices=['core','song','noteskin','gifs','application']);p.add_argument('--target-dir',required=True);p.add_argument('--release',action='store_true');p.add_argument('--test',action='append');p.add_argument('--features');p.add_argument('--clippy',action='store_true');p.add_argument('--lib',action='store_true');p.add_argument('--filter');p.add_argument('--no-fail-fast',action='store_true');args=p.parse_args()
root=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent/(args.stage+'-'+args.suite+('-windows' if os.name=='nt' else '-linux')+('-release' if args.release else '-debug'));out.mkdir(exist_ok=False)
env=os.environ.copy();env.update(CARGO_TARGET_DIR=args.target_dir,CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_RELEASE_DEBUG='0',CARGO_BUILD_JOBS='4')
if os.name!='nt':
    native=Path('/var/tmp/deadsync-rust-adapters-native-deps/root/usr')
    if native.is_dir():
        env.update(PKG_CONFIG_PATH=str(native/'lib/x86_64-linux-gnu/pkgconfig')+':'+str(native/'share/pkgconfig'),CPATH=str(native/'include'))
        env['PATH']=str(native/'bin')+':'+env.get('PATH','');env['LD_LIBRARY_PATH']=str(native/'lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH','')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def sources():
    # Git's ignore rules exclude nested Cargo targets while retaining new source
    # files. Enumerate again after the check so added/removed files are detected.
    names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{name}' for name in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=root).decode().split('\0')
    paths={root/name for name in names if name and Path(name).suffix in {'.rs','.lua','.toml'}}
    paths.update(root/name for name in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
    return {p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
before=sources();(out/'sources.json').write_text(json.dumps(before,indent=2)+'\n')
options=['--release'] if args.release else []
package={'core':'deadlib-lua','song':'deadsync-song-lua','noteskin':'deadsync-noteskin','gifs':'deadsync-theme-simply-love','application':'deadsync'}[args.suite]
if args.suite=='application':command=['cargo','build','--locked','-p',package,'--bin','deadsync',*options]
else:
    command=['cargo','clippy' if args.clippy else 'test','--locked','-p',package,*options]
    features=args.features or ('test-support' if args.suite=='song' else None)
    if features:command+=['--features',features]
    for test in args.test or []:command+=['--test',test]
    if args.no_fail_fast and not args.clippy:command+=['--no-fail-fast']
    if args.lib:command+=['--lib']
    if args.filter:command+=[args.filter]
    if args.suite=='gifs':command+=['--lib','step_stats_gifs']
    command+=['--no-deps','--lib'] if args.clippy else ['--','--nocapture','--test-threads=1']
print(args.stage,args.suite,'started',flush=True);start=time.monotonic();log=out/'run.log'
with log.open('wb') as f:result=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
txt=log.read_text(errors='replace');report=dict(stage=args.stage,suite=args.suite,command=command,exit_code=result.returncode,seconds=time.monotonic()-start,log_sha256=sha(log),tests_passed=sum(map(int,re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed;',txt))),tests_failed=sum(map(int,re.findall(r'test result: (?:ok|FAILED)\. \d+ passed; (\d+) failed;',txt))),test_results=re.findall(r'test result: .*',txt),sources_unchanged=before==sources(),environment={k:v for k,v in env.items() if k.startswith(('CARGO_','CFLAGS_')) or k in ['CPATH','PKG_CONFIG_PATH','LD_LIBRARY_PATH']})
if args.suite=='application' and result.returncode==0:
    artifact=Path(args.target_dir)/('release' if args.release else 'debug')/('deadsync.exe' if os.name=='nt' else 'deadsync')
    destination=root/'target/rust-lua-production-before/artifacts'/artifact.name;destination.parent.mkdir(parents=True,exist_ok=True)
    if args.stage=='baseline':
        import shutil
        assert not destination.exists();shutil.copyfile(artifact,destination)
    report['artifact']=dict(path=str(artifact),sha256=sha(artifact),preserved_baseline=str(destination) if args.stage=='baseline' else None)
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k not in ['environment','test_results']}),flush=True)
if result.returncode:print(txt[-6000:]);raise SystemExit(result.returncode)
assert report['sources_unchanged']
