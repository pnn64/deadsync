"""Build unchanged Lua 5.1.5 with a noninteractive internal-assertion diagnostic."""
import hashlib,json,os,subprocess,time
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description='Check the retained lowered-count yield case against Lua 5.1.5 internal assertions without changing the release mismatch audit.')
parser.add_argument('--boundary-proof',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
if os.name!='nt':raise SystemExit('this checked reference requires Windows/MSVC')
root=Path(__file__).resolve().parents[1];proof=json.loads(args.boundary_proof.read_text())
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
assert proof['reference']['archive_sha256']=='2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333'
before={name:sha(root/name)for name in proof['source_hashes']};assert before==proof['source_hashes']
assert proof['reader_call_levels_probe']['completed']and proof['reader_call_levels_probe']['observations_equal']
for path,digest in proof['artifacts'].items():assert sha(path)==digest,path
command=proof['commands'][0]['command'];src=next(Path(x).parent for x in command if x.endswith('lapi.c'))
for name,digest in proof['reference']['source_hashes'].items():assert sha(src/name)==digest,name
out=root/'target'/f'lua-checked-yield-{time.time_ns()}';out.mkdir()
setup=subprocess.check_output([str(Path(os.environ['ProgramFiles(x86)'])/'Microsoft Visual Studio/Installer/vswhere.exe'),'-latest','-products','*','-find','VC/Auxiliary/Build/vcvars64.bat'],text=True).strip().splitlines()[0]
header=out/'checked.h';header.write_text('void deadlua_assertion_failed(const char *,const char *,int);\n#define lua_assert(c) ((c)?(void)0:deadlua_assertion_failed(#c,__FILE__,__LINE__))\n')
helper=out/'checked.c';helper.write_text('#include <stdio.h>\n#include <stdlib.h>\nvoid deadlua_assertion_failed(const char *expr,const char *file,int line){fprintf(stderr,"LUA_ASSERT %s %s:%d\\n",expr,file,line);fflush(stderr);exit(86); }\n')
commands=[]
def build(name,args,dll=True):
 cmd=['cl','/nologo','/O2','/MT',*(['/LD']if dll else []),'/D_CRT_SECURE_NO_WARNINGS',*map(str,args),'/link','/OUT:'+str(out/(name+('.dll'if dll else'.exe'))),*(['/IMPLIB:'+str(out/(name+'.lib'))]if dll else [])]
 rsp=out/(name+'.rsp');rsp.write_text(' '.join('"'+x+'"'for x in cmd[1:])+'\n')
 script=out/(name+'.cmd');script.write_text('@call "'+setup+'" >nul\n@cl @"'+str(rsp)+'"\n@exit /b %errorlevel%\n')
 r=subprocess.run(['cmd','/d','/c',str(script)],cwd=out,capture_output=True);(out/(name+'.log')).write_bytes(r.stdout+r.stderr);assert r.returncode==0,(r.stdout+r.stderr).decode(errors='replace');commands.append(cmd)
files=[p for p in sorted(src.glob('*.c'))if p.name not in ['lua.c','luac.c','print.c']]
build('lua51',['/DLUA_BUILD_AS_DLL','/I'+str(src),'/FI'+str(header),*files,helper]);build('lua',['/DLUA_BUILD_AS_DLL','/I'+str(src),src/'lua.c',out/'lua51.lib'],False)
build('reference_module',['/DREFERENCE','/I'+str(src),root/'crates/deadlib-lua/tests/native-boundary/module.c',out/'lua51.lib'])
previous=proof['native_call_levels_probe']['runs']['reference']['command'];old_module=next(Path(x)for x in proof['artifacts']if x.endswith('reference_module.dll'))
boot=previous[2].replace(json.dumps(str(old_module)),json.dumps(str(out/'reference_module.dll')));assert boot!=previous[2]
control=out/'control.lua';control.write_text((root/'crates/deadlib-lua/tests/native-boundary/lowered-level-yield.lua').read_text().replace('boundary_setlevel(zero,current);',''))
runs={}
for label,path in [('control',control),('lowered',root/'crates/deadlib-lua/tests/native-boundary/lowered-level-yield.lua'),('reader-call-levels',root/'crates/deadlib-lua/tests/native-boundary/reader-call-levels.lua')]:
 cmd=[str(out/'lua.exe'),'-e',boot,str(path)];env=os.environ.copy()
 for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:env.pop(key,None)
 r=subprocess.run(cmd,cwd=out,env=env,capture_output=True,timeout=10);runs[label]=dict(command=cmd,returncode=r.returncode,stdout_hex=r.stdout.hex(),stderr_hex=r.stderr.hex());print(label,'exit',r.returncode,flush=True)
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
result=dict(commands=commands,runs=runs,source_hashes={str(p):sha(p)for p in src.iterdir()if p.is_file()and p.suffix in ['.c','.h']},assertion_header=header.read_text(),assertion_handler=helper.read_text(),artifacts={str(p):sha(p)for p in out.iterdir()if p.suffix in ['.exe','.dll']})
assert {name:sha(root/name)for name in before}==before
assert runs['control']['returncode']==0 and not runs['control']['stderr_hex']
assert runs['lowered']['returncode']==86
assert b'LUA_ASSERT L->nCcalls == L->baseCcalls ' in bytes.fromhex(runs['lowered']['stderr_hex'])
assert runs['reader-call-levels']['returncode']==0 and not runs['reader-call-levels']['stderr_hex']
assert runs['reader-call-levels']['stdout_hex']==proof['reader_call_levels_probe']['runs']['reference']['stdout_hex']
result.update(native_log_hashes={str(out/(name+'.log')):sha(out/(name+'.log'))for name in ['lua51','lua','reference_module']},assertion_source_files={str(header):sha(header),str(helper):sha(helper)},control_source=control.read_text(),boundary_proof_sha256=sha(args.boundary_proof),fixture_hashes={name:before[name]for name in before if name.endswith(('lowered-level-yield.lua','reader-call-levels.lua'))},audit_script_sha256=sha(__file__),instrumentation='Unmodified official source files; force-included lua_assert handler exits 86 on invariant failure.',invariant_rejected=True,control_completed=True,reader_cases_equal=True,release_mismatch_unchanged=True,full_lua_51_parity_proven=False)
args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
