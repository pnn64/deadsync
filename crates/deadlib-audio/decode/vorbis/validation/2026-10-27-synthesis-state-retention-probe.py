from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,re,subprocess
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
run=root/'seek-reference-20261027';gates=root/'synthesis-state-validation-final-20261027';fuzz=root/'fuzz-synthesis-state-final-20261027'
out=decode/'vorbis/validation';prefix='2026-10-27-synthesis-state-';pending={};hashes={};origins={}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):
 b=p.read_bytes();return b.decode('utf-16'if b.startswith((b'\xff\xfe',b'\xfe\xff'))else'utf-8-sig')
def obj(p):return json.loads(read(p))
def retain(p,name,raw=False):
 name=prefix+name;assert name not in pending and not(out/name).exists(),name
 if raw:b=p.read_bytes()
 else:
  s='\n'.join(line.rstrip()for line in read(p).splitlines()).rstrip();b=(s+'\n'if s else'').encode()
 pending[name]=b;hashes[name]=hashlib.sha256(b).hexdigest();origins[name]={'path':p.relative_to(repo).as_posix(),'sha256':sha(p)}
checks=obj(gates/'checks.json');assert checks['completed_utc']
print('verify sources, reference and corpus',flush=True)
for name,h in checks['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in checks['assets_input_sha256'].items():assert sha(repo/name)==h,name
for name,h in checks['external_inputs'].items():assert sha(root/'corpus'/name)==h,name
for name,h in checks['reference_sources'].items():
 assert subprocess.check_output(['git','-C',str(root/name),'rev-parse','HEAD'],text=True).strip()==h
 parts=['src','include']if name=='ogg'else['lib','include']
 assert not subprocess.check_output(['git','-C',str(root/name),'diff','--name-only','--',*parts],text=True).strip()
for name,h in checks['reference_binaries'].items():
 folder='vorbis'if'VORBIS'in name else'ogg';assert sha(root/(folder+'-oracle-build/Release/deadlib_'+folder+'_oracle.exe'))==h
up=root/'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955';assert sha(up/'assets.sha256')==checks['upstream_inputs']['manifest_sha256']
for line in read(up/'assets.sha256').splitlines():
 h,name=line.split('  ',1);assert sha(up/name)==h,name
for c in checks['commands']:
 assert c['exit_code']==0,c
 p=gates/(c['name']+'.log');retain(p,c['name']+'.log')
 rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(p))
 if rows:c.update(dict(zip(['passed','failed','ignored'],[sum(int(x[i])for x in rows)for i in range(3)])))
assert next(c for c in checks['commands']if c['name']=='release')['passed']==206
assert next(c for c in checks['commands']if c['name']=='independent')['passed']==139
assert next(c for c in checks['commands']if c['name']=='independent')['ignored']==83
assert next(c for c in checks['commands']if c['name']=='fuzz-tests')['passed']==3
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
print('verify all comparison coverage',flush=True)
text=re.sub(r'test [a-zA-Z_][a-zA-Z_0-9:]* \.\.\. ok','',read(gates/'release.log'))
text=re.sub(r':\s*\n(?=\d+ (?:cases|samples|ops|nonfinite))',': ',text)
# Preserve all exact earlier matrix coverage assertions, independently of the
# newly added state tests. The extracted source is retained with this script.
old=read(root/'retain_eos_clamping.py');start=old.index("assert 'whole setup extent matrix:");end=old.index("reference=obj(run/",start)
exec(old[start:end],globals())
labels=re.findall(r'(synthesis-[^:\r\n]+):\s+(\d+) ops, (\d+) accepted, (\d+) discarded, (\d+) busy, (\d+) reads, (\d+) read errors, (\d+) finite values, (\d+) nonfinite classes, peak ([^,]+), RMS ([^,]+), signal ([^\r\n]+)',text)
assert len(labels)==1342 and len({r[0]for r in labels})==len(labels)
synthesis=dict(zip(['operations','accepted_inputs','discarded_prefixes','busy_insertions','read_attempts','read_errors','finite_values','nonfinite_classes'],[sum(int(r[i])for r in labels)for i in range(1,9)]))
assert synthesis==dict(zip(synthesis,[250635,88711,16482,22635,133738,22918,355093102,19415954]))
assert 'Captured synthesis seek schedules: 88'in text
receipts=obj(run/'synthesis-receipts.json');assert receipts['comparisons']==len(labels)==1342 and receipts['totals']==list(synthesis.values()) and receipts['release_log_sha256']==sha(gates/'release.log')
assert receipts['procedural_requests']==1248 and receipts['captured_requests']==88 and receipts['golden_requests']==6
for row in receipts['receipts']:
 assert sha(repo/row['request'])==row['request_sha256'] and sha(repo/row['response'])==row['response_sha256']
retain(run/'synthesis-receipts.json','synthesis-receipts.json');retain(root/'check_synthesis_receipts.py','synthesis-receipt-probe.py');retain(run/'receipt-check.log','synthesis-receipt-check.log')
print('verify previous input identities and helper responses',flush=True)
compat=obj(run/'oracle-compatibility.json');assert compat['new_binary_sha256']==sha(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe')
assert compat['old_binary_sha256']==sha(run/'oracle-before.exe')=='3f43c80a840124489521db2f32a0df3a9b4b416d7aa9acd94e6bf4a62231359b'
assert len(compat['old_protocol_checks'])==75 and len(compat['fresh_synthesis_goldens'])==6
assert compat['synthesis_golden_sha256']==sha(decode/'vorbis/tests/fixtures/synthesis-state-v1.libvorbis')
for row in compat['old_protocol_checks']:
 assert sha(root/row['path'])==row['input_sha256']
retain(run/'oracle-compatibility.json','oracle-compatibility.json')
old_inputs=root/'eos-clamping-validation-final-verified-20261027/configuration-inputs';unchanged={}
for p in sorted(old_inputs.glob('*.bin')):
 q=gates/'configuration-inputs'/p.name;assert p.read_bytes()==q.read_bytes(),p.name;unchanged[p.name]=sha(p)
assert len(unchanged)==18737
retain(root/'retain_eos_clamping.py','previous-coverage-assertions.py')
commit='921cf04428c6084deea0ab97c40560d0f75008ee';baseline=obj(run/'baseline-source.json')
for name,h in baseline.items():
 p=run/'baseline-source'/name;assert sha(p)==h
 assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':'+name]).replace(b'\r\n',b'\n')
assert(run/'oracle-before.c').read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':crates/deadlib-audio/decode/vorbis/tests/oracle/oracle.c']).replace(b'\r\n',b'\n')
for name in ['baseline-source.json','baseline.json','oracle-before.c','oracle-build.log']:retain(run/name,name)
print('verify compiled before/after, traces and controller',flush=True)
raw=obj(run/'raw-refactor/checks.json');assert raw['before_commit']==commit and len(raw['requests'])==172
for name,h in raw['after_source_sha256'].items():assert sha(decode/name)==h,name
for row in raw['requests']:assert sha(repo/row['request'])==row['request_sha256']
for row in raw['builds']:
 v=row['variant'];folder=run/'raw-refactor'/v
 assert sha(folder/'src/main.rs')==row['driver_sha256'] and sha(folder/'Cargo.toml')==row['manifest_sha256']
 assert sha(folder/f'target/release/deadlib-raw-refactor-{v}.exe')==row['binary_sha256']
 retain(folder/'Cargo.toml',f'raw-{v}-Cargo.toml');retain(run/'raw-refactor'/f'{v}-build.log',f'raw-{v}-build.log')
retain(run/'raw-refactor/before/src/main.rs','raw-driver.rs');retain(run/'raw-refactor/checks.json','raw-refactor.json')
captured=obj(run/'captured-final/checks.json');assert captured==checks['captured_schedules']
assert len(captured['captures'])==88 and captured['known_difference_streams']==70 and captured['operations']==2515 and captured['requests']==450 and captured['chunks']==434
for row in captured['captures']:
 folder=run/'captured-final'/row['label'];assert obj(folder/'checks.json')=={k:v for k,v in row.items()if k not in ['known_difference','label']}
 for name,key in [('input.ogg','input_sha256'),('requests.bin','requests_sha256'),('file.out','file_response_sha256'),('trace.tsv','trace_sha256'),('schedule.bin','synthesis_request_sha256'),('schedule.out','synthesis_response_sha256')]:assert sha(folder/name)==row[key]
 assert row['reference_binary_sha256']==compat['new_binary_sha256'] and row['trace_binary_sha256']==sha(run/'trace-final/build/Release/deadlib_vorbis_seek_trace.exe')
for label in ['eos-115','origin-900']:
 for name in ['input.ogg','requests.bin','file.out','trace.tsv','schedule.bin','schedule.out','bounds.txt','checks.json']:
  retain(run/'captured-final'/label/name,f'capture-{label}-{name}',raw=name.endswith(('.ogg','.bin','.out')))
for i in [-576,-37,0,900]:
 for ext in ['ogg','bin']:retain(run/f'origin-{i}.{ext}',f'origin-{i}.{ext}',raw=True)
retain(run/'captured-final/checks.json','captured-schedules.json')
trace=obj(run/'trace-final/provenance.json');assert trace['original_source_sha256']==sha(root/'vorbis/lib/vorbisfile.c') and trace['oracle_source_sha256']==sha(decode/'vorbis/tests/oracle/oracle.c') and trace['script_sha256']==sha(decode/'vorbis/tests/oracle/trace_seek.py')
assert trace['patched_source_sha256']==sha(run/'trace-final/vorbisfile-traced.c') and trace['binary_sha256']==sha(run/'trace-final/build/Release/deadlib_vorbis_seek_trace.exe')
for name in ['provenance.json','vorbisfile-traced.c','CMakeLists.txt','build-0.log','build-1.log']:retain(run/'trace-final'/name,'trace-'+name)
retain(root/'vorbis/COPYING','trace-COPYING')
model=root/'seek-reference-model-20261027';control=obj(model/'checks.json');assert len(control['results'])==88 and sum(r['requests']for r in control['results'])==450 and sum(r['chunks']for r in control['results'])==434
assert control['controller_sha256']==sha(model/'model.py') and control['native_service_source_sha256']==sha(model/'src/main.rs') and control['native_service_binary_sha256']==sha(model/'target/release/deadlib-seek-reference-model.exe')
for row in control['results']:
 folder=run/'captured-final'/row['label'];assert sha(folder/'input.ogg')==row['input_sha256'] and sha(folder/'schedule.bin')==row['derived_schedule_sha256'] and sha(folder/'file.out')==row['file_response_sha256']
 assert row['peak_error']<=3e-6*max(1,row['reference_peak']) and row['rms_error']<=5e-7*max(1,row['reference_peak'])
for name in ['checks.json','model.py','src/main.rs','Cargo.toml','build.log','run.log']:retain(model/name,'controller-'+name.replace('/','-'))
instrument=root/'synthesis-state-instrumenter-final-20261027';check=obj(instrument/'check.json');assert check['exit_code']==0 and check['source_sha256']==sha(decode/'vorbis/src/decoder.rs') and check['script_sha256']==sha(decode/'scripts/profile_synthesis.py')
for name,h in check['generated_sha256'].items():assert sha(instrument/name)==h
retain(instrument/'check.log','instrumenter-check.log');retain(instrument/'check.json','instrumenter-check.json')
print('verify sanitizer source identities and seeds',flush=True)
provenance=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json');initial=obj(fuzz/'initial-corpus.sha256.json')
assert provenance['targets']==['vorbis_synthesis','vorbis_packets','vorbis_stream'] and provenance['seconds_per_target']==300 and provenance['sanitizer']=='address'
for name,h in provenance['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in initial.items():assert sha(fuzz/name)==h,name
for name,h in provenance['external_sha256'].items():assert sha(Path(name))==h
executions=0
for row in results:
 assert row['exit_code']==0 and row['executions']>0,row
 assert not list((fuzz/'artifacts'/row['target']).iterdir())
 assert re.search(r'stat::slowest_unit_time_sec:\s+0',read(fuzz/(row['target']+'.log')))
 executions+=row['executions']
 assert sha(repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/f"{row['target']}.exe")==row['binary_sha256']
 for suffix in ['.log','.stdout.log']:retain(fuzz/(row['target']+suffix),'fuzz-'+row['target']+suffix)
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
seeds=root/'synthesis-state-seeds-20261027';selected=obj(seeds/'checks.json');assert selected['reference_binary_sha256']==compat['new_binary_sha256'] and selected['unique_synthesis_seeds']==960 and len(selected['requests'])==299
for row in selected['requests']:
 p=repo/row['request'];assert sha(p)==row['request_sha256'] and sha(p.with_suffix('.out'))==row['response_sha256']
for name,p in [(p.name,p)for target in provenance['targets']for p in(seeds/target).iterdir()]:
 h=sha(p);target=p.parent.name;assert initial[f'corpus/{target}/resume-{h}']==h
retain(seeds/'checks.json','targeted-seeds.json')
for src,name in [('validate_synthesis_state.py','validation-probe.py'),('capture_synthesis.py','capture-probe.py'),('check_synthesis_raw_refactor.py','raw-refactor-probe.py'),('prepare_synthesis_seeds.py','seed-probe.py'),('check_synthesis_instrumenter.py','instrumenter-probe.py'),('retain_synthesis_state.py','retention-probe.py')]:retain(root/src,name)
audit=obj(run/'next-seek-audit.json');assert audit['reference_source_sha256']==sha(repo/audit['reference_source']) and audit['inventory_sha256']==sha(repo/audit['known_difference_inventory']);retain(run/'next-seek-audit.json','next-seek-audit.json')
extra=root/'fuzz-synthesis-state-stream-final-20261027';extra_prov=obj(extra/'provenance.json');extra_results=obj(extra/'results.json');extra_initial=obj(extra/'initial-corpus.sha256.json')
assert extra_prov['targets']==['vorbis_stream'] and extra_prov['seconds_per_target']==300 and extra_prov['source_sha256']==provenance['source_sha256']
for name,h in extra_initial.items():assert sha(extra/name)==h,name
assert len(extra_results)==1
for row in extra_results:
 assert row['exit_code']==0 and row['executions']>0 and not list((extra/'artifacts'/row['target']).iterdir())
 log=read(extra/(row['target']+'.log'));assert re.search(r'stat::slowest_unit_time_sec:\s+0',log)
 assert int(re.search(r'stat::new_units_added:\s+(\d+)',log)[1])>0
 assert sha(repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/f"{row['target']}.exe")==row['binary_sha256']
 executions+=row['executions']
stream_seeds=root/'synthesis-state-stream-seeds-20261027';stream_selected=obj(stream_seeds/'checks.json');assert stream_selected['unique_seeds']==176
for row in stream_selected['seeds']:
 assert sha(repo/row['source'])==row['source_sha256'] and sha(stream_seeds/'vorbis_stream'/row['seed_sha256'])==row['seed_sha256']
 assert extra_initial['corpus/vorbis_stream/resume-'+row['seed_sha256']]==row['seed_sha256']
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log','vorbis_stream.log','vorbis_stream.stdout.log']:retain(extra/name,'stream-extra-'+name)
retain(stream_seeds/'checks.json','stream-extra-targeted-seeds.json')
manifest={'completed_utc':datetime.now(timezone.utc).isoformat(),'scope':'Raw reference synthesis clocks, track-only insertion, partial reads, retained-buffer restarts and exact queue/count behavior. Original PCM tolerances. The 172 indexed libvorbisfile seek differences remain open. No performance measurements or application integration.','validation':checks,'synthesis_comparisons':len(labels),'independently_parsed_c_responses':receipts['comparisons'],'synthesis':synthesis,'captured':captured,'raw_refactor':raw,'experimental_controller':control,'unchanged_procedural_inputs':unchanged,'unchanged_helper_responses':75,'offline_golden_operations':392,'remaining_seek_differences':172,'earlier_suites_rechecked':obj(out/'2026-10-27-eos-clamping-checks.json')['earlier_suites_rechecked']|{'eos_stream_cases':1260,'eos_native_seeks':8952,'eos_counter_states':64},'fuzz':{'targets':3,'campaigns':4,'seconds_each':300,'first_stream_run':'Complete retained corpus replay; no new mutation units. Additional targeted streaming campaign requires new units.','extra_stream_results':extra_results,'executions':executions,'failure_artifacts':0,'slow_units':0,'all_source_inputs_identical':True,'results':results},'instrumenter':check,'artifacts_sha256':hashes,'raw_artifact_origins':origins}
for name,data in pending.items():(out/name).write_bytes(data)
(out/(prefix+'checks.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
print('Retained',len(pending),'artifacts plus checks;',executions,'ASAN executions; source/reference/corpus inputs, seeds and binaries verified.')
