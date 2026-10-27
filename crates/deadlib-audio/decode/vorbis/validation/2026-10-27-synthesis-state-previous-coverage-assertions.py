from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,re,subprocess,struct
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
run=root/'eos-clamping-20261027';gates=root/'eos-clamping-validation-final-verified-20261027';fuzz=root/'fuzz-eos-clamping-final-20261027'
out=decode/'vorbis/validation';prefix='2026-10-27-eos-clamping-';pending={};hashes={};originals={}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):
 b=p.read_bytes();return b.decode('utf-16' if b.startswith((b'\xff\xfe',b'\xfe\xff')) else 'utf-8-sig')
def obj(p):return json.loads(read(p))
def retain(p,name,raw=False):
 name=prefix+name;assert name not in pending and not (out/name).exists(),name
 if raw:b=p.read_bytes()
 else:
  s='\n'.join(line.rstrip() for line in read(p).splitlines()).rstrip();b=(s+'\n' if s else '').encode()
 pending[name]=b;hashes[name]=hashlib.sha256(b).hexdigest();originals[name]={'path':p.relative_to(repo).as_posix(),'sha256':sha(p)}
checks=obj(gates/'checks.json');assert checks['completed_utc']
for name,h in checks['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in checks['assets_input_sha256'].items():assert sha(repo/name)==h,name
for name,h in checks['external_inputs'].items():assert sha(root/'corpus'/name)==h,name
for name,h in checks['reference_sources'].items():
 assert subprocess.check_output(['git','-C',str(root/name),'rev-parse','HEAD'],text=True).strip()==h
 parts=['src','include'] if name=='ogg' else ['lib','include']
 assert not subprocess.check_output(['git','-C',str(root/name),'diff','--name-only','--',*parts],text=True).strip()
for name,h in checks['reference_binaries'].items():
 folder='vorbis' if 'VORBIS' in name else 'ogg';assert sha(root/(folder+'-oracle-build/Release/deadlib_'+folder+'_oracle.exe'))==h
up=root/'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955';assert sha(up/'assets.sha256')==checks['upstream_inputs']['manifest_sha256']
for line in read(up/'assets.sha256').splitlines():
 h,name=line.split('  ',1);assert sha(up/name)==h,name
for c in checks['commands']:
 assert c['exit_code']==0,c
 p=gates/(c['name']+'.log');retain(p,c['name']+'.log')
 rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(p))
 if rows:c.update(dict(zip(['passed','failed','ignored'],[sum(int(x[i]) for x in rows) for i in range(3)])))
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==200
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==137
assert next(c for c in checks['commands'] if c['name']=='independent')['ignored']==78
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
text=read(gates/'release.log')
text=re.sub(r'test [a-zA-Z_][a-zA-Z_0-9:]* \.\.\. (?:ok|ignored[^\n]*)\n','',text)
assert 'whole setup extent matrix: 475 matching parse/initialization decisions' in text
assert 'codebook extent matrix: 475 declarations' in text
old_bits=re.findall(r'(?m)^[^\r\n]+/packet-\d+/bits-(\d+)-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak',text)
assert len(old_bits)==888 and sum(int(x[6]) for x in old_bits)==663119360 and sum(int(x[2]) for x in old_bits)==55845
old_matrix=re.findall(r'(?m)^((?:modes-|residue-(?!tail-))[^:\r\n]+): (\d+) cases, (\d+) packets, (\d+) discarded\r?$',text)
assert len(old_matrix)==308 and sum(int(x[1]) for x in old_matrix)==31606
prefixes=re.findall(r'(?m)^(mono|stereo|eight-channel-silence|eight-channel-tones|floor-zero): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak',text)
assert len(prefixes)==5 and sum(int(x[1]) for x in prefixes)==7014 and sum(int(x[5]) for x in prefixes)==83072768
geometry=re.findall(r'((?:geometry|unaligned)-ch-[a-z0-9-]+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
geometry_pcm=re.findall(r'((?:geometry|unaligned)-ch-[a-z0-9-]+): (\d+) samples, peak error',text)
assert len(geometry)==len(geometry_pcm)==12109 and sum(int(x[1]) for x in geometry)==332832 and sum(int(x[1]) for x in geometry_pcm)==1836059120
assert len(re.findall(r'(?m)^reference-nonfinite: ([^;\r\n]+); native rejected=(true|false)',text))==11
assert 'nonfinite-reference: 16256 nonfinite samples with matching IEEE classes' in text
empty=re.findall(r'(?m)^(empty-[^:\r\n]+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
assert len(empty)==1767 and sum(int(x[1]) for x in empty)==35080 and sum(int(x[2]) for x in empty)==175400
assert 'floor0 components: 344 setups, 39946 cases, 3091639 finite values, 104041 nonfinite classes, 12818 EOF cases' in text
assert 'reference IEEE MDCT: 24480 isolated nonfinite inputs, 134215680 NaN samples' in text
pcm=re.findall(r'(?m)^floor0-pcm-(\d+)-batch-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
finite=re.findall(r'(?m)^floor0-pcm-\d+-batch-\d+: (\d+) samples, peak error',text)
nonfinite=re.findall(r'(?m)^floor0-pcm-\d+-batch-\d+: (\d+) nonfinite samples',text)
assert len(pcm)==2217 and len(set(x[0] for x in pcm))==1411 and sum(int(x[2]) for x in pcm)==31209 and sum(int(x[3]) for x in pcm)==156045 and sum(int(x[4]) for x in pcm)==1292
assert sum(map(int,finite))==25123840 and sum(map(int,nonfinite))==2274784
assert 'custom floats: 20480 words, all 1024 exponents, both signs and zero/edge mantissas' in text
assert 'reference book bounds: 77048 accepted/initialized, 6920 byte-guard rejections, all 8 bit alignments' in text
assert 'custom float VQ: 2208 books, 6338688 exact vector values, both lookup types, sequence and sparse books' in text
float_pcm=re.findall(r'(?m)^float-pcm-(\d+)-batch-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
float_finite=re.findall(r'(?m)^float-pcm-\d+-batch-\d+: (\d+) samples, peak error',text)
float_ieee=re.findall(r'(?m)^float-pcm-\d+-batch-\d+: (\d+) nonfinite samples',text)
assert len(float_pcm)==920 and len(set(x[0] for x in float_pcm))==420 and sum(int(x[2]) for x in float_pcm)==23460 and sum(int(x[3]) for x in float_pcm)==117300 and sum(int(x[4]) for x in float_pcm)==840
assert sum(map(int,float_finite))==25894160 and sum(map(int,float_ieee))==1328400

assert 'partial VQ additions: 38 books, 119320 cases, 10535488 exact values, 8186 failed words, 1368 sticky EOF cases' in text
pcm=re.findall(r'(?m)^residue-tail-pcm-(\d+)-batch-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
values=re.findall(r'(?m)^residue-tail-pcm-\d+-batch-\d+: (\d+) samples, peak error',text)
assert len(pcm)==707 and len(set(x[0] for x in pcm))==324 and sum(int(x[2]) for x in pcm)==17500 and sum(int(x[3]) for x in pcm)==87500 and sum(int(x[4]) for x in pcm)==648
assert sum(map(int,values))==20010336

assert 'floor1 components: 264 setups, 83876 cases, 6710080 exact values, 66840 present floors, 4752 sticky EOF cases' in text
f1=re.findall(r'(?m)^floor1-pcm-(\d+)-batch-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded',text)
f1_values=re.findall(r'(?m)^floor1-pcm-\d+-batch-\d+: (\d+) samples, peak error',text)
assert len(f1)==660 and len(set(x[0] for x in f1))==336 and sum(int(x[2]) for x in f1)==15243 and sum(int(x[3]) for x in f1)==76215 and sum(int(x[4]) for x in f1)==672
assert sum(map(int,f1_values))==16222224

assert 'EOS stream cases: 1260' in text and 'EOS seek cases: 1260, native seeks 8952' in text
assert 'Unresolved vorbisfile seek-position/count differences: 172' in text
assert 'EOS counter states: 64' in text and 'EOS counter states: 30' in text
reference=obj(run/'independent-reference.json');assert reference['reference_binary_sha256']==sha(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe')
assert reference['old_reference_binary_sha256']==sha(run/'oracle-before.exe')=='eb9af20a42fc5d7cdf83cf2dc5bd6962e795dec20e4597a9b886efc29a8337bf'
assert len(reference['old_pcm_responses'])==40 and len(reference['unchanged_procedural_inputs'])==18737
assert len(reference['packet_goldens'])==2 and len(reference['stream_goldens'])==25 and len(reference['fresh_boundary_records'])==64 and len(reference['raw_prefix_checks'])==1260
for row in reference['unchanged_procedural_inputs']:assert sha(gates/'configuration-inputs'/row['name'])==row['sha256']
for row in reference['old_pcm_responses']:assert sha(repo/row['request'])==row['request_sha256']
for row in reference['timed_batches']:
 p=gates/'eos-inputs'/f"eos-timed-{row['fixture']}.bin";assert sha(p)==row['request_sha256'] and sha(p.with_suffix('.out'))==row['response_sha256']
assert reference['known_seek_differences']['differences']==172
assert sha(decode/'vorbis/tests/fixtures/eos-seek-differences-v1.tsv')==reference['known_seek_differences']['inventory_sha256']
retain(run/'independent-reference.json','independent-reference.json')
commit='f1bc46e6fdadefa07968a0ac9606989ca288eab1'
for name,h in obj(run/'baseline-source.json').items():
 p=run/'baseline-source'/name;assert sha(p)==h
 assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':'+name]).replace(b'\r\n',b'\n')
assert (run/'oracle-before.c').read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':crates/deadlib-audio/decode/vorbis/tests/oracle/oracle.c']).replace(b'\r\n',b'\n')
for name in ['baseline-source.json','baseline.log','oracle-before.c','oracle-build.log']:retain(run/name,name)
probe=root/'eos-clamping-before-after-final-verified-20261027';before_after=obj(probe/'checks.json');assert before_after['before_source_commit']==commit
assert before_after['input_sha256']==sha(probe/'input.ogg') and before_after['reference_response_sha256']==sha(probe/'reference.out')
assert before_after['reference_binary_sha256']==reference['reference_binary_sha256'] and before_after['old_reference_binary_sha256']==reference['old_reference_binary_sha256']
for name,h in before_after['before_sources_sha256'].items():
 p=probe/'before'/name;assert sha(p)==h
 assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':crates/deadlib-audio/decode/'+name]).replace(b'\r\n',b'\n')
for name,h in before_after['after_codec_source_sha256'].items():assert sha(repo/name)==h,name
for row in before_after['results']:
 variant=row['variant'];assert row['frames']==3648 and row['values']==7296 and row['success']==(variant=='after')
 assert sha(probe/variant/'src/main.rs')==row['source_sha256'] and sha(probe/variant/'Cargo.toml')==row['manifest_sha256']
 assert sha(probe/variant/'target/release'/f'deadlib-eos-{variant}.exe')==row['binary_sha256'] and sha(probe/(variant+'.out'))==row['output_sha256']
 assert row['peak_error']<=3e-6*max(1,row['reference_peak']) and row['rms_error']<=5e-7*max(1,row['reference_peak'])
 retain(probe/variant/'src/main.rs',variant+'-probe.rs');retain(probe/(variant+'-build.log'),variant+'-build.log');retain(probe/(variant+'.out'),variant+'.out',raw=True);retain(probe/(variant+'.stderr'),variant+'.stderr')
for name in ['input.ogg','reference.out','reference-old-mode.out']:retain(probe/name,'before-after-'+name,raw=True)
retain(probe/'checks.json','before-after.json')
for name in ['eos-states.bin','eos-states.out']:retain(gates/'eos-inputs'/name,name,raw=True)
provenance=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json');assert provenance['seconds_per_target']==300 and len(results)==3
for name,h in provenance['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in provenance['external_sha256'].items():assert sha(Path(name))==h
initial=obj(fuzz/'initial-corpus.sha256.json')
for name,h in initial.items():assert sha(fuzz/name)==h,name
for row in results:
 assert row['exit_code']==0 and row['executions']>0
 assert sha(repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/(row['target']+'.exe'))==row['binary_sha256']
 assert not list((fuzz/'artifacts'/row['target']).iterdir())
 assert re.search(r'stat::slowest_unit_time_sec:\s+0',read(fuzz/(row['target']+'.log')))
 retain(fuzz/(row['target']+'.log'),row['target']+'.log');retain(fuzz/(row['target']+'.stdout.log'),row['target']+'.stdout.log')
executions=sum(row['executions']for row in results)
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
seeds=root/'eos-clamping-seeds-verified-20261027';targeted=obj(seeds/'targeted.json');assert len(targeted['seeds'])==5040 and len(targeted['probes'])==1260 and targeted['reference_binary_sha256']==reference['reference_binary_sha256']
for row in targeted['seeds']:
 assert sha(seeds/row['path'])==row['sha256']
for row in targeted['probes']:
 p=repo/row['source'];assert sha(p)==row['ogg_sha256'] and sha(p.with_suffix('.out'))==row['reference_response_sha256']
retain(seeds/'targeted.json','targeted-seeds.json')
selected=root/'eos-clamping-selected-seeds-final-20261027';selection=obj(selected/'selection.json');assert len(selection['seeds'])==338 and selection['source_manifest_sha256']==sha(seeds/'targeted.json')
for row in selection['seeds']:
 assert sha(selected/row['path'])==row['sha256'];target=row['path'].split('/')[0];assert initial['corpus/'+target+'/resume-'+row['sha256']]==row['sha256']
assert sha(selected/'counter-wrap.ogg')==selection['counter_wrap']['ogg_sha256'] and sha(selected/'counter-wrap.out')==selection['counter_wrap']['response_sha256'] and selection['counter_wrap']['frames']==1600
retain(selected/'selection.json','selected-seeds.json');retain(selected/'counter-wrap.ogg','counter-wrap.ogg',raw=True);retain(selected/'counter-wrap.out','counter-wrap-reference.out',raw=True)
for name in ['counter-overflow.json','counter-overflow.bin','counter-overflow.out']:retain(run/name,name,raw=name.endswith(('.bin','.out')))
counter=root/'eos-counter-wrap-before-after-verified-20261027';counter_checks=obj(counter/'checks.json')
assert counter_checks['input_sha256']==sha(counter/'input.ogg') and counter_checks['reference_sha256']==sha(counter/'reference.out')
previous=obj(root/'eos-clamping-validation-verified-20261027/checks.json')
for name,h in counter_checks['before_source_snapshot_sha256'].items():
 assert sha(counter/'before'/name)==h and previous['source_sha256'][name]==h,name
for name,h in counter_checks['after_codec_source_sha256'].items():assert sha(repo/name)==h,name
for row in counter_checks['results']:
 variant=row['variant'];assert row['emitted']==row['frames']
 assert row['frames']==(586 if variant=='before'else 1600) and row['declared']==(1<<63)-1
 assert sha(counter/variant/'src/main.rs')==row['source_sha256'] and sha(counter/variant/'Cargo.toml')==row['manifest_sha256']
 assert sha(counter/variant/'target/release'/f'deadlib-counter-{variant}.exe')==row['binary_sha256'] and sha(counter/(variant+'.out'))==row['output_sha256']
 assert row['peak_error']<=3e-6*max(1,row['reference_peak']) and row['rms_error']<=5e-7*max(1,row['reference_peak'])
 retain(counter/variant/'src/main.rs','counter-'+variant+'-probe.rs');retain(counter/(variant+'-build.log'),'counter-'+variant+'-build.log');retain(counter/(variant+'.out'),'counter-'+variant+'.out',raw=True)
retain(counter/'checks.json','counter-wrap-before-after.json')
instrument=root/'eos-clamping-instrumenter-final-verified-20261027';check=obj(instrument/'check.json');assert check['exit_code']==0 and check['source_sha256']==sha(decode/'vorbis/src/decoder.rs') and check['script_sha256']==sha(decode/'scripts/profile_synthesis.py')
for name,h in check['generated_sha256'].items():assert sha(instrument/name)==h
retain(instrument/'check.log','instrumenter-check.log');retain(instrument/'check.json','instrumenter-check.json')
for src,name in [('validate_eos_clamping.py','validation-probe.py'),('check_eos_reference.py','reference-probe.py'),('seed_eos_clamping.py','seed-probe.py'),('select_eos_seeds.py','selected-seed-probe.py'),('check_eos_before_after.py','before-after-probe.py'),('check_eos_instrumenter.py','instrumenter-probe.py'),('retain_eos_clamping.py','retention-probe.py')]:retain(root/src,name)
retain(root/'check_eos_counter_wrap.py','counter-wrap-probe.py')
audit=obj(run/'next-seek-audit.json');assert sha(repo/audit['source'])==audit['source_sha256'];retain(run/'next-seek-audit.json','next-seek-audit.json')
manifest={'completed_utc':datetime.now(timezone.utc).isoformat(),'scope':'Bounded reference EOS clipping with separate declared/emitted frame counts. Original PCM tolerances and exact decisions/counts/clock states; 172 vorbisfile seek differences explicitly remain open. No performance measurement or application integration.','validation':checks,'stream_cases':1260,'timed_cases':1260,'timed_accepted':3330,'timed_discarded':720,'timed_values':7645056,'counter_states':64,'native_seeks':8952,'reference_seek_requests':4476,'remaining_reference_seek_differences':reference['known_seek_differences'],'unchanged_procedural_requests':18737,'reference_helper_pcm_regressions':40,'before_after':before_after,'counter_wrap_before_after':counter_checks,'prepared_eos_seeds':5040,'selected_targeted_seeds':338,'earlier_suites_rechecked':obj(out/'2026-10-27-floor1-guards-checks.json')['earlier_suites_rechecked']|{'floor1_component_cases':83876,'floor1_packet_cases':15243},'fuzz':{'targets':3,'seconds_each':300,'executions':executions,'failure_artifacts':0,'slow_units':0,'all_source_inputs_identical':True,'results':results},'instrumenter':check,'artifacts_sha256':hashes,'raw_artifact_origins':originals}
for name,data in pending.items():(out/name).write_bytes(data)
(out/(prefix+'checks.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
print('Retained',len(pending),'artifacts plus checks;',executions,'ASAN executions; all source/reference/corpus inputs, seeds and binaries verified.')
