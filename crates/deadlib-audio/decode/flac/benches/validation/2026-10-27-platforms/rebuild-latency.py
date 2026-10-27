from pathlib import Path
import hashlib,json,subprocess,sys,time
root=Path(__file__).resolve().parents[2]
windows=sys.platform=='win32'
out=root/'target/deadlib-flac-reference/latency-windows-build' if windows else Path('/home/perfecttaste/.cache/deadlib-flac-performance/latency-oracle')
src=root/'crates/deadlib-audio/decode/flac/benches/libflac'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
inputs={str(p):sha(p) for p in src.glob('*') if p.is_file()}
command=['cmake','--build',str(out),'--config','Release','--target','flac_latency_benchmark','--parallel','4']
with (out/'warm-build.log').open('wb') as log:subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
assert inputs=={str(p):sha(p) for p in src.glob('*') if p.is_file()}
binary=out/('Release/flac_latency_benchmark.exe' if windows else 'flac_latency_benchmark')
(out/'warm-build.json').write_text(json.dumps({'timestamp_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'command':command,'sources':inputs,'binary_sha256':sha(binary)},indent=2)+'\n',encoding='utf-8')
native=root/'target/deadlib-flac-reference/windows-perf-build/decode-latency.exe' if windows else Path('/tmp/deadlib-flac-decode-latency')
target=root/'target/deadlib-flac' if windows else Path('/tmp/deadlib-flac-perf-rust')
subprocess.run([sys.executable,str(root/'crates/deadlib-audio/decode/flac/benches/build_native.py'),'--output',str(native),'--target-dir',str(target)],check=True)
