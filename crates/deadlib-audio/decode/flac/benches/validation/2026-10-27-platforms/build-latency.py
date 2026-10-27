from pathlib import Path
import hashlib,json,subprocess,sys,time
root=Path(__file__).resolve().parents[2]
windows=sys.platform=='win32'
out=root/'target/deadlib-flac-reference/latency-windows-build' if windows else Path('/home/perfecttaste/.cache/deadlib-flac-performance/latency-oracle')
out.mkdir(parents=True,exist_ok=False)
src=root/'crates/deadlib-audio/decode/flac/benches/libflac'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
inputs={str(p):sha(p) for p in src.glob('*') if p.is_file()}
command=['cmake','-S',str(src),'-B',str(out),'-DREFERENCE_ROOT='+str(root/'target/deadlib-flac-reference/flac'),'-DOGG_REFERENCE_ROOT='+str(root/'target/deadlib-audio-reference/ogg')]
command+=['-A','x64','-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded'] if windows else ['-DCMAKE_BUILD_TYPE=Release']
commands=[command,['cmake','--build',str(out),'--config','Release','--target','flac_latency_benchmark','--parallel','4']]
for c,name in zip(commands,['configure.log','build.log']):
 with (out/name).open('wb') as log:subprocess.run(c,stdout=log,stderr=subprocess.STDOUT,check=True)
assert inputs=={str(p):sha(p) for p in src.glob('*') if p.is_file()}
binary=out/('Release/flac_latency_benchmark.exe' if windows else 'flac_latency_benchmark')
(out/'build.json').write_text(json.dumps({'timestamp_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'commands':commands,'sources':inputs,'binary_sha256':sha(binary)},indent=2)+'\n',encoding='utf-8')
print(binary,flush=True)
