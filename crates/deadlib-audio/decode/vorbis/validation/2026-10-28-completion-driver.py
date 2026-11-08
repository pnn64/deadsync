from pathlib import Path
import subprocess,os,json,hashlib,datetime
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';ref=repo/'target/deadlib-audio-reference';run=ref/'nil-eos-final-validation';run.mkdir(exist_ok=False)
env=os.environ.copy()
for package in ['ogg','vorbis']:env['DEADLIB_LIB'+package.upper()+'_ORACLE']=str(ref/(package+'-oracle-build')/'Release'/('deadlib_'+package+'_oracle.exe'))
env.update({'DEADLIB_VORBIS_CORPUS':str(repo/'assets'),'DEADLIB_OGG_CORPUS':str(repo/'assets'),'DEADLIB_VORBIS_EXTERNAL_CORPUS':str(ref/'corpus'),'DEADLIB_OGG_EXTERNAL_CORPUS':str(ref/'corpus'),'DEADLIB_VORBIS_FLOOR0':str(ref/'corpus/floor0/vorbis_floor_type_0.ogg'),'DEADLIB_VORBIS_UPSTREAM_CORPUS':str(ref/'corpus/upstream-completion'),'DEADLIB_VORBIS_SYNTHESIS_TRACES':str(ref/'reference-seeking-captures')})
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=[p for part in ['ogg/src','vorbis/src','fuzz/src','fuzz/fuzz_targets','ogg/tests','vorbis/tests']for p in sorted((decode/part).rglob('*'))if p.is_file()and p.suffix in ['.rs','.c','.h','.ogg','.bin','.out','.libvorbis','.libvorbis-pcm','.tsv']]
sources={p.relative_to(decode).as_posix():sha(p)for p in paths}
receipt={'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'git_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'source_sha256':sources,'environment':{k:v for k,v in env.items()if k.startswith('DEADLIB_')},'commands':[]}
base=['cargo','test','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--release','--all-features','--target-dir','target/vorbis-completion','--','--include-ignored','--skip','regenerate_']
checks=[('release',base),('fmt',['cargo','fmt','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--','--check']),('clippy',['cargo','clippy','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--all-targets','--all-features','--target-dir','target/vorbis-completion','--','-D','warnings']),('clippy-default',['cargo','clippy','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--all-targets','--target-dir','target/vorbis-completion','--','-D','warnings']),('fuzz-tests',['cargo','test','--manifest-path','crates/deadlib-audio/decode/fuzz/Cargo.toml','--lib','--target-dir','target/vorbis-completion-fuzz']),('fuzz-clippy',['cargo','clippy','--manifest-path','crates/deadlib-audio/decode/fuzz/Cargo.toml','--all-targets','--features','seed','--target-dir','target/vorbis-completion-fuzz','--','-D','warnings']),('independent',['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(decode/'scripts/verify_independent.ps1'),'-ScratchRoot',str(ref/'nil-eos-independent')]),('independent-benches',['cargo','check','--offline','--workspace','--all-features','--benches','--manifest-path',str(ref/'nil-eos-independent/Cargo.toml')])]
for name,args in checks:
 print('running',name,flush=True)
 log=run/(name+'.log')
 with log.open('wb')as output:r=subprocess.run(args,cwd=repo,env=env,stdout=output,stderr=subprocess.STDOUT)
 receipt['commands'].append({'name':name,'command':args,'exit_code':r.returncode,'log_sha256':sha(log)})
 (run/'checks.json').write_bytes((json.dumps(receipt,indent=2)+'\n').encode())
 print('finished',name,r.returncode,flush=True)
 if r.returncode:print(log.read_text(errors='replace')[-8000:]);raise SystemExit(r.returncode)
for name,digest in sources.items():assert sha(decode/name)==digest,'source changed during validation: '+name
receipt['completed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();(run/'checks.json').write_bytes((json.dumps(receipt,indent=2)+'\n').encode())
print('Final decoder checks passed',flush=True)
