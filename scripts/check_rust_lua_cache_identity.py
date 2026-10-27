"""Check that persistent-cache build identities change with relevant build inputs."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    crate=root/'crates/deadlib-lua'
    sources=sorted((crate/'src').rglob('*.rs'))+[crate/'build.rs',crate/'Cargo.toml',Path(__file__).resolve()]
    def hashes():
        return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before=hashes();cases=[]
    rustc=shutil.which('rustc')
    assert rustc
    version=subprocess.check_output([rustc,'-vV'],text=True)
    with tempfile.TemporaryDirectory(prefix='lua-cache-identity-',dir=root/'target') as temporary:
        workspace=Path(temporary)
        assert workspace.resolve().is_relative_to((root/'target').resolve())
        shutil.copytree(crate/'src',workspace/'src')
        for name in ['Cargo.toml','build.rs']:shutil.copy2(crate/name,workspace/name)
        binary=workspace/('identity.exe' if os.name=='nt' else 'identity')
        command=[rustc,'--edition=2024','-C','opt-level=2',str(workspace/'build.rs'),'-o',str(binary)]
        if os.name=='nt':command += ['-C','target-feature=+crt-static']
        build=subprocess.run(command,cwd=workspace,capture_output=True,text=True)
        build.check_returncode()
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()
        values=dict(CARGO_CFG_TARGET_ARCH='x86_64',CARGO_CFG_TARGET_OS='windows',
            CARGO_CFG_TARGET_ENV='msvc',CARGO_CFG_TARGET_ENDIAN='little',CARGO_CFG_TARGET_POINTER_WIDTH='64',
            CARGO_CFG_TARGET_FEATURE='crt-static,sse,sse2',CARGO_ENCODED_RUSTFLAGS='-C\x1ftarget-feature=+crt-static',
            DEBUG='false',OPT_LEVEL='3',RUSTC=rustc)
        env=os.environ.copy();env.update(values)
        def run(environment=env):
            output=subprocess.check_output([str(binary)],cwd=workspace,env=environment,text=True)
            ids=re.findall(r'^cargo:rustc-env=DEADLIB_LUA_CACHE_BUILD=([0-9a-f]{32})$',output,re.M)
            assert len(ids)==1,output
            assert 'cargo:rerun-if-changed=src\n' in output
            return ids[0]
        baseline=run()
        def case(name,identity,changed):
            assert (identity!=baseline)==changed,(name,baseline,identity)
            cases.append(dict(name=name,changed_expected=changed,identity=identity,pass_=True))
        case('identical_inputs',run(),False)
        path=workspace/'src/lib.rs';original=path.read_bytes()
        try:
            path.write_bytes(original.replace(b'\r\n',b'\n').replace(b'\n',b'\r\n'))
            case('checkout_crlf',run(),False)
            path.write_bytes(original+b'\n// source identity fixture\n')
            case('source_content',run(),True)
        finally:path.write_bytes(original)
        added=workspace/'src/cache_identity_fixture.rs'
        try:
            added.write_text('// new source identity fixture\n',encoding='utf-8')
            case('new_source_path',run(),True)
        finally:added.unlink()
        for filename in ('Cargo.toml','build.rs'):
            path=workspace/filename;original=path.read_bytes()
            try:
                path.write_bytes(original+(b'\n# identity fixture\n' if filename.endswith('toml') else b'\n// identity fixture\n'))
                case(filename+'_content',run(),True)
            finally:path.write_bytes(original)
        for name,replacement in [('CARGO_CFG_TARGET_ARCH','aarch64'),('CARGO_CFG_TARGET_OS','linux'),
            ('CARGO_CFG_TARGET_ENV','gnu'),('CARGO_CFG_TARGET_ENDIAN','big'),
            ('CARGO_CFG_TARGET_POINTER_WIDTH','32'),('CARGO_CFG_TARGET_FEATURE','sse2'),
            ('CARGO_ENCODED_RUSTFLAGS','-C\x1foverflow-checks=yes'),('DEBUG','true'),('OPT_LEVEL','0')]:
            changed=env.copy();changed[name]=replacement
            case(name,run(changed),True)
        case('restored_inputs',run(),False)
    assert before==hashes(),'sources changed during identity checks'
    result=dict(source_hashes=before,build_command=command,build_binary_sha256=binary_sha256,
        rustc=version,baseline_identity=baseline,fixture_environment=values,cases=cases,
        limitations='Runs the real build script in an isolated source copy. New-file hashing and emitted source-directory watches are checked; this is not a cross-target execution test or a cryptographic-authentication claim.')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(f'{len(cases)}/{len(cases)} identity checks pass')
if __name__=='__main__':
    main()
