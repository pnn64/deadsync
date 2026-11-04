"""Check indexed folder suffix edge cases against the real Cargo library."""
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('driver',ROOT/'scripts/benchmark_profile_folders.py')
driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)

def run():
    archive=Path(os.environ.get('DEADSYNC_PROFILE_FOLDERS_ARCHIVE',ROOT/'target/profile-folders'))
    out=archive/'index-edges';out.mkdir(parents=True,exist_ok=True)
    final=json.loads((ROOT/'docs/performance/2026-11-04-profile-folders-final.manifest.json').read_text(encoding='utf-8'))
    work=Path(final['source_archive']).parent.parent/'full-workspace'
    target=work/'crates/deadsync-profile/tests/folder_index_edgecases.rs'
    assert target.resolve().parent==(work/'crates/deadsync-profile/tests').resolve()
    wrapper='include!('+json.dumps((ROOT/'tests/perf/profile_folder_index_edges.rs').as_posix())+');\n'
    assert not target.exists(),'temporary edge target already exists'
    manifest=work/'Cargo.toml';base=manifest.read_text(encoding='utf-8');assert base==final['workspace_manifest']
    paths=['crates/deadsync-profile/src/lib.rs','crates/deadsync-profile/src/folder_paths_original.rs','tests/perf/profile_folder_index_edges.rs',Path(__file__).resolve().relative_to(ROOT).as_posix()]
    sources={}
    for p in paths:
        snapshot=out/'source'/p;snapshot.parent.mkdir(parents=True,exist_ok=True);snapshot.write_bytes((ROOT/p).read_bytes());sources[p]=driver.sha(snapshot)
    metadata=dict(sources=sources,profiles={},rustc=subprocess.check_output(['rustc','-Vv'],text=True,encoding='utf-8'))
    try:
        target.write_text(wrapper,encoding='utf-8')
        for profile in ['release','debug']:
            stage=out/profile;stage.mkdir(exist_ok=True)
            contents=base+('\n[profile.release.package.deadsync-profile]\nopt-level = 0\ndebug-assertions = true\noverflow-checks = true\n' if profile=='debug' else '')
            manifest.write_text(contents,encoding='utf-8')
            cmd=['cargo','test','--offline','--locked','--jobs','2','--manifest-path',str(manifest),'-p','deadsync-profile','--test','folder_index_edgecases','--release','--no-run','--message-format=json']
            with (stage/'build.log').open('w',encoding='utf-8') as stream:result=subprocess.run(cmd,cwd=ROOT,env=dict(os.environ,RUSTFLAGS='-C target-feature=+crt-static',TEMP=str(archive/'compile-temp'),TMP=str(archive/'compile-temp')),stdout=stream,stderr=subprocess.STDOUT)
            assert not result.returncode,(stage/'build.log').read_text(encoding='utf-8')[-4000:]
            events=[json.loads(s)for s in (stage/'build.log').read_text(encoding='utf-8').splitlines()if s.startswith('{')]
            event=[e for e in events if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='folder_index_edgecases' and e.get('executable')][0]
            binary=stage/Path(event['executable']).name;shutil.copyfile(event['executable'],binary)
            command=[str(binary),'--test-threads=1']
            with (stage/'tests.log').open('w',encoding='utf-8') as stream:result=subprocess.run(command,cwd=work,stdout=stream,stderr=subprocess.STDOUT)
            text=(stage/'tests.log').read_text(encoding='utf-8');assert not result.returncode,text
            metadata['profiles'][profile]=dict(build_command=cmd,command=command,profile=event['profile'],passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',text))),exit_code=result.returncode,binary_sha256=driver.sha(binary),build_sha256=driver.sha(stage/'build.log'),log_sha256=driver.sha(stage/'tests.log'))
            print('indexed suffix edge cases:',profile,'passed',flush=True)
    finally:
        manifest.write_text(base,encoding='utf-8')
        assert target.resolve().parent==(work/'crates/deadsync-profile/tests').resolve() and target.read_text(encoding='utf-8')==wrapper
        target.unlink()
    assert sources=={p:driver.sha(ROOT/p)for p in paths}
    assert not target.exists() and manifest.read_text(encoding='utf-8')==base
    metadata['temporary_target_removed']=True
    metadata['workspace_manifest_restored']=True
    (ROOT/'docs/performance/2026-11-04-profile-folders-index-edges.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')

if __name__=='__main__':run()
