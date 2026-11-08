from pathlib import Path
from datetime import datetime, timezone
import hashlib, json, re, subprocess

repo=Path.cwd();d=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
out=d/'vorbis/validation';prefix='2026-10-27-concealment-';pending={};artifacts={};originals={}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):
 b=p.read_bytes();return b.decode('utf-16' if b.startswith((b'\xff\xfe',b'\xfe\xff')) else 'utf-8-sig')
def obj(p):return json.loads(read(p))
def retain(p,name,raw=False):
 name=prefix+name;assert not (out/name).exists() and name not in pending,name
 s='\n'.join(x.rstrip() for x in read(p).splitlines()).rstrip() if not raw else ''
 b=p.read_bytes() if raw else (s+'\n' if s else '').encode()
 pending[name]=b;artifacts[name]=hashlib.sha256(b).hexdigest();originals[name]={'path':p.relative_to(repo).as_posix(),'sha256':sha(p)}

gates=root/'concealment-validation-20261027';checks=obj(gates/'checks.json');assert checks['completed_utc']
for name,h in checks['source_sha256'].items():assert sha(d/name)==h,name
for name,h in checks['assets_input_sha256'].items():assert sha(repo/name)==h,name
for name,h in checks['external_inputs'].items():assert sha(root/'corpus'/name)==h,name
for key,h in checks['reference_binaries'].items():
 p=root/('vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe' if key=='DEADLIB_LIBVORBIS_ORACLE' else 'ogg-oracle-build/Release/deadlib_ogg_oracle.exe');assert sha(p)==h,key
for name,h in checks['reference_sources'].items():assert subprocess.check_output(['git','-C',str(root/name),'rev-parse','HEAD'],text=True).strip()==h
up=root/'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955';assert sha(up/'assets.sha256')==checks['upstream_inputs']['manifest_sha256']
for line in (up/'assets.sha256').read_text().splitlines():
 h,name=line.split('  ',1);assert sha(up/name)==h,name
for command in checks['commands']:
 assert command['exit_code']==0,command
 p=gates/(command['name']+'.log');retain(p,command['name']+'.log')
 results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(p))
 if results:command.update(dict(zip(['passed','failed','ignored'],[sum(int(x[i]) for x in results) for i in range(3)])))
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==123
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==94
assert next(c for c in checks['commands'] if c['name']=='independent')['ignored']==30
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
retain(root/'validate_concealment.py','validation-probe.py')

stats=re.findall(r'(?m)^(mono|stereo|eight-channel-silence|eight-channel-tones|floor-zero): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak ([\d.eE+-]+), RMS ([\d.eE+-]+)',read(gates/'release.log'))
assert len(stats)==5
cases=[dict(zip(['label','cases','packets','discarded','truncated','samples','peak_error','rms_error'],[row[0],*[int(v) for v in row[1:6]],*[float(v) for v in row[6:]]])) for row in stats]
assert sum(c['cases'] for c in cases)==7014 and sum(c['samples'] for c in cases)==83072768
assert sum(c['packets'] for c in cases)==29706
run=root/'concealment-parity-20261027';baseline=obj(run/'baseline-sha256.json')
for name,h in baseline.items():assert sha(run/name)==h,name
for name in ['vorbis/src/decoder.rs','vorbis/src/residue.rs']:
 old=subprocess.check_output(['git','show','270dbd6ed:crates/deadlib-audio/decode/'+name])
 assert old.replace(b'\r\n',b'\n')==(run/name).read_bytes().replace(b'\r\n',b'\n'),name
retain(run/'baseline-sha256.json','baseline-source.json')
retain(run/'vorbis/tests/oracle/oracle.c','golden-capture-oracle.c')
for source,name in [('concealment-prefix-reset-20261027.log','baseline-failures.log'),('concealment-prefix-window-20261027.log','window-only.log'),('concealment-prefix-fixed-20261027.log','focused-fixed.log'),('concealment-golden-fixed-20261027.log','golden-fixed.log'),('concealment-oracle-final-build-20261027.log','oracle-build.log')]:retain(root/source,name)
assert read(root/'concealment-prefix-reset-20261027.log').count('FAILED')>=2
assert 'test result: ok. 2 passed' in read(root/'concealment-prefix-fixed-20261027.log')
gold=obj(run/'golden-capture.json');golden=d/'vorbis/tests/fixtures/discarded-window-v1.libvorbis-pcm'
assert sha(golden)==gold['golden_sha256'] and golden.stat().st_size==gold['golden_bytes']
assert sha(repo/gold['fixture'])==gold['fixture_sha256'] and sha(run/'discarded-window-input.bin')==gold['request_sha256']
assert sha(run/'vorbis/tests/oracle/oracle.c')==gold['oracle_source_sha256']
assert gold['final_oracle_verification']['binary_sha256']==checks['reference_binaries']['DEADLIB_LIBVORBIS_ORACLE']
assert gold['final_oracle_verification']['source_sha256']==sha(d/'vorbis/tests/oracle/oracle.c') and gold['final_oracle_verification']['pcm_bytes_identical']
retain(run/'golden-capture.json','golden-capture.json');retain(run/'discarded-window-input.bin','golden-request.bin',raw=True)
oracle=obj(run/'oracle-selfcheck.json');assert oracle['fresh_processes_compared']==21
assert oracle['binary_sha256']==checks['reference_binaries']['DEADLIB_LIBVORBIS_ORACLE']
for record in oracle['files']:assert sha(repo/record['path'])==record['sha256'] and record['fresh_process_pcm_equals_restarted_case_mode_byte_for_byte']
retain(run/'oracle-selfcheck.json','oracle-selfcheck.json');retain(root/'check_concealment_oracle.py','oracle-selfcheck-probe.py')
instrument=root/'concealment-instrumenter-check-20261027';m=obj(instrument/'checks.json');assert m['exit_code']==0 and not m['performance_measurements_run']
for name,h in m['source_sha256'].items():assert sha(repo/name)==h,name
for name,h in m['generated_sha256'].items():assert sha(instrument/name)==h,name
retain(instrument/'checks.json','instrumenter-checks.json');retain(instrument/'check.log','instrumenter-check.log')

fuzz=root/'fuzz-concealment-20261027';p=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json')
assert len(results)==2 and all(r['exit_code']==0 and r['executions']>0 for r in results)
for name,h in p['source_sha256'].items():assert sha(d/name)==h,name
for name,h in p['external_sha256'].items():assert sha(Path(name))==h,name
for name,h in obj(fuzz/'initial-corpus.sha256.json').items():assert sha(fuzz/name)==h,name
slow=[x for x in (fuzz/'artifacts').rglob('*') if x.is_file()]
assert all(x.name.startswith('slow-unit-') for x in slow)
for i,x in enumerate(slow):retain(x,f'fuzz-slow-unit-{i}.bin',raw=True)
for r in results:
 binary=repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/(r['target']+'.exe');assert sha(binary)==r['binary_sha256']
 retain(fuzz/(r['target']+'.log'),'fuzz-'+r['target']+'.log')
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
retain(root/'retain_concealment.py','retention-probe.py')
checks.update({'priority':'User clarified complete behavioral parity rather than performance; original justified PCM tolerances and exact counts/behavior remain required.',
 'scope':'Window overlap uses actual adjacent accepted block sizes; type-zero residue commits complete partitions on nominal truncation. No performance claim.',
 'prefix_cases':cases,'total_cases':7014,'packet_outcomes_checked':29706,'channel_samples_compared':83072768,
 'reference_case_mode_checked_against_fresh_processes':21,'golden':gold,'no_numerical_tolerance_changes':True,
 'fuzz':{'targets':[r['target'] for r in results],'seconds_per_target':300,'executions':sum(r['executions'] for r in results),'failure_artifacts':0,'slow_unit_artifacts':[{'name':x.name,'sha256':sha(x),'bytes':x.stat().st_size} for x in slow],'source_input_binary_hashes_rechecked':True},
 'artifacts':artifacts,'original_artifacts_before_utf8_lf_normalization':originals,'retained_utc':datetime.now(timezone.utc).isoformat(),
 'limitations':['These byte-prefix cases do not prove all malformed-packet behavior or complete Vorbis parity.','Arbitrary bit mutations, unusual legal setup combinations, broader official/real-song corpora and documented strict/legacy/reference differences still require parity work.']})
for name,b in pending.items():(out/name).write_bytes(b)
(out/(prefix+'checks.json')).write_bytes((json.dumps(checks,indent=2)+'\n').encode())
print('Retained',len(pending),'artifacts; 7,014 independent cases; 83,072,768 PCM values;',checks['fuzz']['executions'],'ASan executions.')
