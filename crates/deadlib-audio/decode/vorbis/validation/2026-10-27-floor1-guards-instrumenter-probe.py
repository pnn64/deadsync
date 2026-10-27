from pathlib import Path
import importlib.util,sys,shutil,subprocess,json,hashlib
repo=Path.cwd();scripts=repo/'crates/deadlib-audio/decode/scripts';sys.path.insert(0,str(scripts))
spec=importlib.util.spec_from_file_location('profile_synthesis',scripts/'profile_synthesis.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
root=repo/'target/deadlib-audio-reference';run=root/'floor1-guards-instrumenter-verified-20261027';run.mkdir(exist_ok=False)
generated=module.generate(run,scripts.parent/'vorbis/benches/support/synthesis_profile.rs')
source=repo/'crates/deadlib-audio/decode/vorbis/src/decoder.rs'
args=['cargo','clippy','--manifest-path',str(run/'Cargo.toml'),'--','-D','warnings']
with (run/'check.log').open('wb') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
(run/'check.json').write_text(json.dumps({'scope':'Generate and lint the diagnostic decoder copy only; no timing run. Copied modules share a complete isolated crate privacy boundary.','command':args,'exit_code':r.returncode,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'script_sha256':hashlib.sha256((scripts/'profile_synthesis.py').read_bytes()).hexdigest(),'generated_sha256':{p.relative_to(run).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in generated}},indent=2)+'\n',encoding='utf8',newline='\n')
print((run/'check.log').read_text(encoding='utf8'));raise SystemExit(r.returncode)
