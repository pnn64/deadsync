from pathlib import Path
import importlib.util,sys,shutil,subprocess,json,hashlib
repo=Path.cwd();scripts=repo/'crates/deadlib-audio/decode/scripts';sys.path.insert(0,str(scripts))
spec=importlib.util.spec_from_file_location('profile_synthesis',scripts/'profile_synthesis.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
root=repo/'target/deadlib-audio-reference';old=root/'residue-geometry-instrumenter-20261027';run=root/'degenerate-books-instrumenter-20261027';(run/'src').mkdir(parents=True,exist_ok=False)
for name in ['main.rs','profile_setup.rs']:shutil.copyfile(old/'src'/name,run/'src'/name)
(run/'Cargo.toml').write_text((old/'Cargo.toml').read_text().replace('deadlib-residue-instrumenter-check','deadlib-book-instrumenter-check'),encoding='utf8',newline='\n')
source=repo/'crates/deadlib-audio/decode/vorbis/src/decoder.rs'
(run/'src/profiled_decoder.rs').write_text(module.instrument(source.read_text()),encoding='utf8',newline='\n')
args=['cargo','clippy','--manifest-path',str(run/'Cargo.toml'),'--','-D','warnings']
with (run/'check.log').open('wb') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
(run/'check.json').write_text(json.dumps({'scope':'Generate and lint the diagnostic decoder copy only; no timing run.','command':args,'exit_code':r.returncode,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'script_sha256':hashlib.sha256((scripts/'profile_synthesis.py').read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf8',newline='\n')
print((run/'check.log').read_text());raise SystemExit(r.returncode)
