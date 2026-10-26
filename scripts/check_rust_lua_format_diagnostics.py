"""Compare format diagnostics with a committed parent and an audited Lua oracle."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent',required=True)
    parser.add_argument('--audit',required=True,type=Path,help='Completed ordinary CLI audit; its pinned oracle is reused without rebuilding')
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    audit=args.audit.resolve();output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    report=json.loads(audit.read_text());reference=report['reference']
    oracle=root/'target/lua-cli-reference'/('lua.exe'if os.name=='nt'else'lua')
    sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    assert reference['archive_sha256']=='2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333'
    assert sha(oracle)==reference['binary_sha256']
    assert all(sha(oracle.parent/'lua-5.1.5/src'/name)==value for name,value in reference['source_hashes'].items())
    revision=subprocess.check_output(['git','rev-parse',args.parent+'^{commit}'],cwd=root,text=True).strip()
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[Path(__file__).resolve()]
    sources += [root/n for n in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]
    def hashes():return {p.relative_to(root).as_posix():sha(p)for p in sorted(sources)}
    before=hashes();commands=[];logs=[]
    formats=[('trailing_percent',"'%'",True),('trailing_flag',"'%0'",True),
             ('nul_conversion',"'%\\000tail'",True),('precision_nul',"'prefix%.\\000tail'",True),
             ('invalid_nonzero',"'%Z'",False),('literal_nul',"'before\\000after'",False)]
    cases=[]
    for name,form,changed in formats:
        source=(f"local ok,e=pcall(string.format,{form},17);local t={{}};"
                "for i=1,#e do t[i]=string.format('%02x',e:byte(i))end;print(ok,#e,table.concat(t))")
        env=os.environ.copy()
        for variable in ('LUA_INIT','LUA_PATH','LUA_CPATH'):env.pop(variable,None)
        run=subprocess.run(['lua','-e',source],executable=oracle,cwd=root,env=env,capture_output=True,timeout=10,check=True)
        assert not run.stderr
        cases.append(dict(name=name,source=source,expected_change=changed,reference_stdout_hex=run.stdout.hex()))
    rows=',\n'.join('('+json.dumps(c['name'])+','+json.dumps(c['source'])+')'for c in cases)
    harness='''
use std::fmt::Write;
macro_rules! probe {($lua:ident,$engine:expr,$name:expr,$source:expr)=>{{
    let p=$lua::compile_named($source.as_bytes(),b"=format-probe").unwrap();
    let mut vm=$lua::Vm::new();vm.set_output_capture(true);vm.execute(&p).unwrap();
    let mut hex=String::new();for byte in vm.output().unwrap(){write!(&mut hex,"{byte:02x}").unwrap();}
    println!("result\\t{}\\t{}\\t{}\\t{}\\t{}",$engine,$name,p.instruction_count(),p.register_count(),hex);
}}}
fn main(){
    println!("size\\tparent\\t{}\\t{}",std::mem::size_of::<lua_parent::Vm>(),std::mem::size_of::<lua_parent::Value>());
    println!("size\\tcurrent\\t{}\\t{}",std::mem::size_of::<lua_current::Vm>(),std::mem::size_of::<lua_current::Value>());
    for(name,source)in [FIXTURES]{probe!(lua_parent,"parent",name,source);probe!(lua_current,"current",name,source);}
}
'''.replace('FIXTURES',rows)
    with tempfile.TemporaryDirectory(prefix='lua-format-diagnostics-',dir=root/'target')as temporary:
        directory=Path(temporary);assert directory.resolve().is_relative_to((root/'target').resolve())
        archive=subprocess.check_output(['git','archive',revision,'crates/deadlib-lua/src'],cwd=root)
        with tarfile.open(fileobj=io.BytesIO(archive))as tar:tar.extractall(directory/'parent',filter='data')
        parent=directory/'parent/crates/deadlib-lua'
        parent_hashes={p.relative_to(directory/'parent').as_posix():sha(p)for p in sorted((parent/'src').rglob('*.rs'))}
        current=directory/'current';shutil.copytree(root/'crates/deadlib-lua/src',current/'src')
        flags=['--edition=2024','-C','opt-level=0','-C','debuginfo=1']
        if os.name=='nt':flags+=['-C','target-feature=+crt-static']
        libraries={}
        def build(command):
            commands.append(command);run=subprocess.run(command,cwd=root,capture_output=True,text=True)
            logs.append(run.stdout+run.stderr);output.with_suffix('.build.log').write_text(''.join(logs),encoding='utf-8');run.check_returncode()
        for engine,copy in [('parent',parent),('current',current)]:
            libraries[engine]=directory/f'liblua_{engine}.rlib'
            build(['rustc',*flags,'--crate-type=rlib','--crate-name',f'lua_{engine}',str(copy/'src/lib.rs'),'-o',str(libraries[engine])])
        code=directory/'probe.rs';code.write_text(harness,encoding='utf-8');output.with_suffix('.rs').write_text(harness,encoding='utf-8')
        binary=directory/('probe.exe'if os.name=='nt'else'probe')
        build(['rustc',*flags,str(code),'--extern',f"lua_parent={libraries['parent']}",'--extern',f"lua_current={libraries['current']}",'-o',str(binary)])
        binary_hash=sha(binary)
        run=subprocess.run([str(binary)],cwd=root,capture_output=True,text=True,check=True,timeout=30)
        assert not run.stderr
    assert hashes()==before and sha(oracle)==reference['binary_sha256']
    output.with_suffix('.tsv').write_text(run.stdout,encoding='utf-8')
    sizes={};observed={}
    for line in run.stdout.splitlines():
        fields=line.split('\t')
        if fields[0]=='size':
            _,engine,vm,value=fields;sizes[engine]=dict(vm_bytes=int(vm),value_bytes=int(value))
        else:
            _,engine,name,instructions,registers,data=fields
            observed.setdefault(name,{})[engine]=dict(instructions=int(instructions),entry_registers=int(registers),stdout_hex=data)
    def observation(data,newline):
        assert data.endswith(newline)
        status,length,hex_bytes=data[:-len(newline)].split(b'\t')
        value=bytes.fromhex(hex_bytes.decode('ascii'))
        assert int(length)==len(value)and status in (b'true',b'false')
        return dict(ok=status==b'true',length=len(value),bytes_hex=value.hex())
    for case in cases:
        case.update(observed[case['name']])
        assert case['parent']['instructions']==case['current']['instructions']
        assert case['parent']['entry_registers']==case['current']['entry_registers']
        case['reference_value']=observation(bytes.fromhex(case['reference_stdout_hex']),b'\r\n'if os.name=='nt'else b'\n')
        for engine in ('parent','current'):
            case[engine]['value']=observation(bytes.fromhex(case[engine]['stdout_hex']),b'\n')
        case['parent_matches_reference']=case['parent']['value']==case['reference_value']
        case['current_matches_reference']=case['current']['value']==case['reference_value']
        assert case['current_matches_reference']and case['parent_matches_reference']!=case['expected_change'],case
    result=dict(parent_revision=revision,parent_source_hashes=parent_hashes,source_hashes=before,
                cli_audit=str(audit),cli_audit_sha256=sha(audit),reference=reference,cases=cases,sizes=sizes,
                build_commands=commands,binary_sha256=binary_hash,harness_stdout=run.stdout,
                protocol='Correctness-only comparison. Identical unoptimized rustc flags build separate parent/current rlibs and one harness in isolated source copies. No compiler wrappers, performance timings or allocation claims. Oracle binary/source identity is verified against a completed pinned CLI audit. Lua encodes the full result/error bytes and its length before printing. The observer parses those fields after removing the exact transport newline: platform stdout newline for the C executable, logical LF for VM output capture. All raw streams are retained; no result/error bytes are normalized.')
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('Six current/reference matches; four parent mismatches reproduced; two unchanged controls.')


if __name__=='__main__':main()
