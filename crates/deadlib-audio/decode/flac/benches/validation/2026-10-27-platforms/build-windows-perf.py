from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,time,sys
root=Path(__file__).resolve().parents[2]
source=root/'crates/deadlib-audio/decode/flac'
out=root/'target/deadlib-flac-reference/windows-perf-build'
out.mkdir(exist_ok='--resume' in sys.argv)
attempt=out/('attempt-'+str(time.time_ns()))
attempt.mkdir()
paths=[root/'Cargo.toml',root/'Cargo.lock',root/'.cargo/config.toml',source/'Cargo.toml',source/'README.md',source/'benches/decode.rs',source.parent/'ogg/Cargo.toml']
paths+=list((source/'src').glob('*.rs'))+list((source.parent/'ogg/src').glob('*.rs'))
paths+=list((Path(__file__).parent/'windows-perf').glob('*'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before={str(p.relative_to(root)):sha(p) for p in paths}
(attempt/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
commands=[]
def run(command,name):
 commands.append(command)
 with (attempt/name).open('wb') as log:subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,check=True)
configure=['cmake','-S',str(Path(__file__).parent/'windows-perf'),'-B',str(out/'oracle'),'-A','x64','-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded',
 '-DDEADLIB_ROOT='+root.as_posix(),'-DREFERENCE_ROOT='+(root/'target/deadlib-flac-reference/flac').as_posix(),'-DOGG_REFERENCE_ROOT='+(root/'target/deadlib-audio-reference/ogg').as_posix()]
run(configure,'configure.log')
run(['cmake','--build',str(out/'oracle'),'--config','Release','--target','flac_decode_benchmark','--parallel','4'],'build.log')
run(['cargo','bench','--locked','-p','deadlib-audio-decode-flac','--bench','decode','--no-run','--target-dir',str(root/'target/deadlib-flac')],'native-build.log')
text=(attempt/'native-build.log').read_text(encoding='utf-8')
artifact=Path(re.findall(r'Executable benches[\\/]decode.rs \(([^)]+)\)',text)[-1])
if not artifact.is_absolute():artifact=root/artifact
native=out/'decode.exe'
if native.exists() and not (out/'unverified-first-decode.exe').exists():shutil.copyfile(native,out/'unverified-first-decode.exe')
shutil.copyfile(artifact,native)
after={str(p.relative_to(root)):sha(p) for p in paths}
(attempt/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
assert before==after,[name for name in before if before[name]!=after[name]]
compiler=subprocess.check_output(['rustc','-Vv'],cwd=root,text=True)
report={'built_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'commands':commands,'rustc':compiler,'sources':before,
 'native':str(native),'native_sha256':sha(native),'oracle':str(out/'oracle/Release/flac_decode_benchmark.exe'),
 'oracle_sha256':sha(out/'oracle/Release/flac_decode_benchmark.exe'),
 'method':'MSVC Release, /MT static CRT, IPO for libFLAC/Ogg and C++ driver; separate /GL- PCM address-escape helper. Rust workspace bench profile, static CRT.',
 'cpu':os.environ.get('PROCESSOR_IDENTIFIER'),'attempt':str(attempt)}
(out/'build.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'native':str(native),'oracle':report['oracle'],'native_sha256':sha(native)}),flush=True)
