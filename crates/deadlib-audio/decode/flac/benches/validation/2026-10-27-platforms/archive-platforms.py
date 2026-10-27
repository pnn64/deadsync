from pathlib import Path
import hashlib,json,shutil,subprocess
root=Path(__file__).resolve().parents[2]
crate=root/'crates/deadlib-audio/decode/flac'
target=root/'target/deadlib-flac-reference'
win=target/'windows-perf-build'
linux=Path(r'\\wsl.localhost\Debian')
out=crate/'benches/validation/2026-10-27-platforms'
out.mkdir(exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
copy=lambda a,b:shutil.copyfile(a,out/b)
base=json.loads((crate/'benches/validation/2026-10-27-checkpoints/final-validation.json').read_text())
codec={n:d for n,d in base['source_sha256'].items() if n.startswith('src/')}
assert all(sha(crate/n)==d for n,d in codec.items())
manifests=[]
for label in ['raw','pipelines','broad','startup','seek','seek-warm']:
 folder=win/('measure-'+label)
 for name in ['raw.tsv','summary.json','manifest.json','errors.json','windows-adapter.json','executed-runner.py','commands.json']:
  if (folder/name).is_file():copy(folder/name,'windows-'+label+'-'+name)
 manifests.append(json.loads((folder/'manifest.json').read_text()))
for label in ['startup','seek','seek-warm']:
 folder=linux/'tmp'/('deadlib-flac-final-'+label)
 for name in ['raw.tsv','summary.json','manifest.json','errors.json','commands.json']:
  if (folder/name).is_file():copy(folder/name,'linux-'+label+'-'+name)
 manifests.append(json.loads((folder/'manifest.json').read_text()))
for m in manifests:
 assert {n.replace('\\','/'):d for n,d in m['native_sources'].items()}==codec
 for error in m.get('unsupported',[]):
  assert error['mode']=='symphonia-crc' and '05 - 32bps audio.flac' in error['file'] and 'PCM endpoint check' in error['stderr'],error
for label,folder in [('windows-original',win/'oracle-audit'),('windows-latency',win/'latency-oracle-audit'),('linux-latency',linux/'tmp/deadlib-flac-final-oracle-audit')]:
 copy(folder/'audit.json',label+'-oracle-audit.json')
for source,name in [(win/'build.json','windows-original-build.json'),(win/'decode-latency.exe.json','windows-native-latency-build.json'),(win/'decode-latency.exe.build.log','windows-native-latency-build.log'),(linux/'tmp/deadlib-flac-decode-latency.json','linux-native-latency-build.json'),(linux/'tmp/deadlib-flac-decode-latency.build.log','linux-native-latency-build.log'),(linux/'tmp/deadlib-flac-memory-checkpoints/results.json','memory-checkpoints.json')]:copy(source,name)
for label,folder in [('windows-c',target/'latency-windows-build'),('linux-c',linux/'home/perfecttaste/.cache/deadlib-flac-performance/latency-oracle')]:
 for name in ['build.json','warm-build.json','configure.log','build.log','warm-build.log','CMakeCache.txt']:
  copy(folder/name,label+'-'+name)
attempt=Path(json.loads((win/'build.json').read_text())['attempt'])
for name in ['source-before.json','source-after.json','configure.log','build.log','native-build.log']:copy(attempt/name,'windows-original-'+name)
# Retain the private Windows adapter and C driver used for the original trials.
for name in ['prepare-windows-perf.py','build-windows-perf.py','run-windows-perf.py','build-latency.py','rebuild-latency.py','final-measurements.py','final-measurements.json','checkpoints-memory.py','archive-platforms.py']:copy(target/name,name)
for name in ['main.cpp','opaque.c','CMakeLists.txt']:copy(target/'windows-perf'/name,'windows-original-'+name)
copy(target/'latency-perf/main.cpp','latency-initial-benchmark.cpp')
initial=json.loads((target/'latency-windows-build/build.json').read_text())
assert sha(out/'latency-initial-benchmark.cpp')==next(d for n,d in initial['sources'].items() if n.endswith('benchmark.cpp'))
# Restore the exact pre-warm runner bytes from its recorded hash.
text=(crate/'benches/run_seek.py').read_text()
text=text.replace("    parser.add_argument('--warm-index', action='store_true', help='prime all 97 targets before timing in both APIs')\n",'')
text=text.replace("    native_mode = 'seek-warm' if args.warm_index else 'seek'\n    oracle_mode = 'libflac-' + native_mode\n",'')
text=text.replace("modes = [native_mode, oracle_mode]","modes = ['seek', 'libflac-seek']")
text=text.replace("if mode == native_mode else","if mode == 'seek' else")
text=text.replace("for m in [native_mode, oracle_mode]","for m in ['seek', 'libflac-seek']")
text=text.replace("'warmup': ('all 97 targets, then zero' if args.warm_index else 'target zero') + '; initialization outside clock'","'warmup': 'target zero; initialization outside clock'")
digest=next(d for n,d in json.loads((out/'windows-seek-manifest.json').read_text())['driver_sources'].items() if n.replace('\\','/')=='benches/run_seek.py')
versions=[text.encode(),text.replace('\n','\r\n').encode()]
data=next(v for v in versions if hashlib.sha256(v).hexdigest()==digest)
(out/'seek-initial-runner.py').write_bytes(data)
copy(crate/'benches/bench_host.py','bench_host.py')
for name in ['run.py','run_seek.py','build_native.py','decode.rs']:copy(crate/'benches'/name,'final-'+name)
for name in ['benchmark.cpp','opaque.c','CMakeLists.txt']:copy(crate/'benches/libflac'/name,'final-libflac-'+name)
# Freeze the actual optimizer/ISA/CRT project settings, rather than assuming LTO.
projects={}
for label,folder in [('original',win/'oracle'),('latency',target/'latency-windows-build')]:
 for p in folder.rglob('*.vcxproj'):
  if p.stem not in ['FLAC','ogg','flac_decode_benchmark','flac_latency_benchmark']:continue
  projects[label+'/'+str(p.relative_to(folder))]=[line.strip() for line in p.read_text().splitlines() if any(k in line for k in ['WholeProgramOptimization','RuntimeLibrary','EnableEnhancedInstructionSet','AdditionalOptions','Optimization>'])]
(out/'windows-project-settings.json').write_text(json.dumps(projects,indent=2)+'\n',encoding='utf-8')
refs={}
for name,p in [('libFLAC',target/'flac'),('libogg',root/'target/deadlib-audio-reference/ogg')]:
 refs[name]={'commit':subprocess.check_output(['git','-C',str(p),'rev-parse','HEAD'],text=True).strip(),'tracked_changes':subprocess.check_output(['git','-C',str(p),'status','--porcelain','--untracked-files=no'],text=True)}
 assert not refs[name]['tracked_changes'],refs[name]
# Aggregate endpoint checksums also agree between the lazy and primed policies.
for osname in ['windows','linux']:
 a=json.loads((out/(osname+'-seek-commands.json')).read_text())
 b=json.loads((out/(osname+'-seek-warm-commands.json')).read_text())
 def signatures(records):
  return [tuple(map(int,r['stdout'].strip().split('\t')[3:5])) for r in records if 'seek' in r['arguments'][1]]
 assert signatures(a)==signatures(b),osname
record={'passed':True,'scope':'Windows native MSVC/libFLAC/Symphonia throughput and startup; Linux/WSL latency follow-up; lazy and explicitly primed exact seeking; requested heap only.',
 'codec_source_sha256':codec,'codec_validation':'../2026-10-27-checkpoints/final-validation.json',
 'unit_tests':base['unit_tests'],'integration_and_external_gates':base['integration_and_external_gates'],'doctests':base['doctests'],'ASan_executions':base['ASan_executions'],
 'production_source_changed':False,'references':refs,
 'limitations':['Shared Broadwell host, fixed virtual CPU affinity, no frequency lock. Windows and WSL timings are separate populations.','FFmpeg optimized throughput comparisons remain the Linux/WSL 27-case milestone. Windows uses optimized default MSVC libFLAC with AVX2 and LTO for libFLAC/driver; libogg uses normal Release optimization (its older CMake policy ignores IPO).','Symphonia startup omits footer CRC; throughput comparisons add CRC. Its known 32-bit stereo endpoint mismatch is recorded as unsupported, not timed.','Lazy seek includes first-pass index discovery; primed mode warms the complete 97-target request set in both APIs. Native does not perform libFLAC-style byte interpolation.','Heap measurements cover two representative files and exclude stack, RSS and allocator metadata.']}
record['report_sha256']={p.name:sha(p) for p in out.iterdir() if p.is_file()}
(out/'final-validation.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print('Archived',len(record['report_sha256']),'platform evidence files')
