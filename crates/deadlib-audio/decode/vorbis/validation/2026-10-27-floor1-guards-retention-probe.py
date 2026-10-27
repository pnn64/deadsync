from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,re,subprocess,struct
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
run=root/'floor1-guards-20261027';gates=root/'floor1-guards-validation-verified-20261027';fuzz=root/'fuzz-floor1-guards-20261027'
out=decode/'vorbis/validation';prefix='2026-10-27-floor1-guards-';pending={};hashes={};originals={}
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
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==190
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==132
assert next(c for c in checks['commands'] if c['name']=='independent')['ignored']==71
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
reference=obj(run/'independent-reference.json');assert reference['component_cases']==83876 and reference['component_values']==6710080
assert len(reference['component_goldens'])==8 and len(reference['pcm_goldens'])==6 and len(reference['container_reference_checks'])==6
assert sum(len(x['seeks']) for x in reference['container_reference_checks'])==84
assert reference['reference_binary_sha256']==sha(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe')
assert reference['previous_binary_sha256']==sha(run/'oracle-before.exe')=='0508d936741fb2fe5ab84ca261cdfffbdec359abad3091d86cd28f9109c09bb2'
assert len(reference['old_pcm_regressions'])==34 and len(reference['unchanged_procedural_inputs'])==18077
assert len(reference['component_fresh_records'])>500
for row in reference['component_matrix']:
 p=repo/row['request'];assert sha(p)==row['request_sha256'] and sha(p.with_suffix('.out'))==row['response_sha256']
 q=gates/'floor1-inputs'/p.name;assert q.read_bytes()==p.read_bytes() and q.with_suffix('.out').read_bytes()==p.with_suffix('.out').read_bytes()
for row in reference['unchanged_procedural_inputs']:assert sha(gates/'configuration-inputs'/row['name'])==row['sha256']
for row in reference['pcm_goldens']:
 p=run/'golden-pcm-inputs'/f"floor1-golden-{row['case']}.bin";assert sha(p)==row['request_sha256'] and row['fresh_pcm_identical']
for row in reference['old_pcm_regressions']:assert sha(repo/row['request'])==row['request_sha256']
for row in reference['container_reference_checks']:
 p=gates/'floor1-inputs'/f"floor1-stream-{row['stream']}.ogg";assert sha(p)==row['sha256'] and row['raw_canonical_pcm_identical']
 for item in row['seeks']:assert item['position']==item['target'] and item['frames']==row['frames']-item['target'] and item['canonical_raw_suffix_identical']
for name,row in reference['goldens'].items():
 p=decode/'vorbis/tests/fixtures'/name;assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256']
retain(run/'independent-reference.json','independent-reference.json')
commit='1f8f73b5d6f0124e5b5060c67be34d24a0d10b2b'
for name,h in obj(run/'baseline-source.json').items():
 p=run/'baseline-source'/name;assert sha(p)==h
 assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':'+name]).replace(b'\r\n',b'\n')
assert (run/'oracle-before.c').read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':crates/deadlib-audio/decode/vorbis/tests/oracle/oracle.c']).replace(b'\r\n',b'\n')
for name in ['baseline-source.json','baseline.log','oracle-before.c','oracle-build.log']:retain(run/name,name)
for p in sorted((run/'baseline-inputs').glob('*')):retain(p,p.name,raw=True)
probe=root/'floor1-guards-before-after-final-verified-20261027';before_after=obj(probe/'checks.json');assert before_after['before_source_commit']=='1f8f73b5d6f0124e5b5060c67be34d24a0d10b2b' and before_after['reference_helpers_byte_identical']
assert before_after['input_sha256']==sha(probe/'input.bin') and before_after['reference_response_sha256']==sha(probe/'reference.out')
assert before_after['reference_binary_sha256']==reference['reference_binary_sha256'] and before_after['old_reference_binary_sha256']==reference['previous_binary_sha256']
for name,h in before_after['before_sources_sha256'].items():
 p=probe/'before'/name;assert sha(p)==h
 if name.startswith(('ogg/src/','vorbis/src/')):
  full='crates/deadlib-audio/decode/'+name;assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show','1f8f73b5:'+full]).replace(b'\r\n',b'\n')
for name,h in before_after['after_codec_source_sha256'].items():assert sha(repo/name)==h,name
for row in before_after['results']:
 variant=row['variant'];assert row['frames']==368 and row['finite_values']==1104
 assert sha(probe/variant/'src/main.rs')==row['source_sha256'] and sha(probe/variant/'Cargo.toml')==row['manifest_sha256']
 assert sha(probe/variant/'target/release'/f'deadlib-floor1-{variant}.exe')==row['binary_sha256'] and sha(probe/(variant+'.out'))==row['pcm_sha256']
 assert row['matches_original_pcm_bounds']==(variant=='after')
 retain(probe/variant/'src/main.rs',variant+'-probe.rs');retain(probe/(variant+'-build.log'),variant+'-build.log');retain(probe/(variant+'.out'),variant+'.out',raw=True)
assert before_after['results'][0]['peak_error']>3e-6*max(1,before_after['results'][0]['reference_peak'])
assert before_after['results'][1]['peak_error']<=3e-6*before_after['results'][1]['reference_peak'] and before_after['results'][1]['rms_error']<=5e-7*before_after['results'][1]['reference_peak']
retain(probe/'checks.json','before-after.json');retain(probe/'input.bin','before-after.bin',raw=True);retain(probe/'reference.out','before-after-reference.out',raw=True)
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
executions=sum(row['executions'] for row in results)
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
seeds=root/'floor1-guards-seeds-verified-20261027';targeted=obj(seeds/'targeted.json');assert len(targeted['seeds'])==540 and len(targeted['probes'])==54
assert targeted['reference_binary_sha256']==reference['reference_binary_sha256']
for seed in targeted['seeds']:
 assert sha(seeds/seed['path'])==seed['sha256'];target=seed['path'].split('/')[0]
 assert initial['corpus/'+target+'/resume-'+seed['sha256']]==seed['sha256']
for row in targeted['probes']:
 assert row['frames']==368 and sha(seeds/f"probe-{row['case']}.bin")==row['request_sha256'] and sha(seeds/f"probe-{row['case']}.out")==row['response_sha256']
 assert sha(repo/row['source_request'])==row['source_request_sha256']
retain(seeds/'targeted.json','targeted-seeds.json')
instrument=root/'floor1-guards-instrumenter-verified-20261027';check=obj(instrument/'check.json');assert check['exit_code']==0 and check['source_sha256']==sha(decode/'vorbis/src/decoder.rs') and check['script_sha256']==sha(decode/'scripts/profile_synthesis.py')
for name,h in check['generated_sha256'].items():assert sha(instrument/name)==h
retain(instrument/'check.log','instrumenter-check.log');retain(instrument/'check.json','instrumenter-check.json')
for src,name in [('validate_floor1_guards.py','validation-probe.py'),('check_floor1_reference.py','reference-probe.py'),('seed_floor1_guards.py','seed-probe.py'),('check_floor1_before_after.py','before-after-probe.py'),('check_floor1_instrumenter.py','instrumenter-probe.py'),('retain_floor1_guards.py','retention-probe.py')]:retain(root/src,name)

manifest={'completed_utc':datetime.now(timezone.utc).isoformat(),'scope':'Explicit floor-one reference 15-bit prediction/reconstruction masks, multiplied ordinate guards and nominal EOF decisions. Published range/EOF policy remains default. Original PCM tolerances and exact decisions/counts/positions; no application integration or performance measurement.','validation':checks,'component_setups':264,'component_cases':83876,'component_values':6710080,'component_present_floors':66840,'component_sticky_eof_cases':4752,'component_fresh_records':len(reference['component_fresh_records']),'component_golden_cases':146,'packet_configurations':336,'packet_cases':15243,'packet_decisions':76215,'packet_discards':672,'packet_finite_values':16222224,'native_seek_checks':336,'reference_ogg_streams':6,'reference_ogg_seeks':84,'unchanged_procedural_requests':18077,'reference_helper_pcm_regressions':34,'before_after':{'source_commit':commit,'frames':368,'values':1104,'before_scaled_peak_error':before_after['results'][0]['peak_error']/max(1,before_after['results'][0]['reference_peak']),'before_scaled_rms_error':before_after['results'][0]['rms_error']/max(1,before_after['results'][0]['reference_peak']),'after_scaled_peak_error':before_after['results'][1]['peak_error']/max(1,before_after['results'][1]['reference_peak']),'after_scaled_rms_error':before_after['results'][1]['rms_error']/max(1,before_after['results'][1]['reference_peak'])},'earlier_suites_rechecked':{'configuration_cases':31606,'prefix_cases':7014,'finite_bit_cases':55845,'explicit_nonfinite_cases':11,'empty_book_cases':35080,'residue_geometry_cases':332832,'residue_geometry_pcm_values':1836059120,'codebook_extent_declarations_and_setups':475,'floor0_component_cases':39946,'floor0_packet_cases':31209,'floor0_bark_indices':1436160,'isolated_nonfinite_spectra':24480,'custom_float_words':20480,'float_vq_books':2208,'float_bound_cases':83968,'float_packet_cases':23460,'residue_tail_callback_cases':119320,'residue_tail_packet_cases':17500},'fuzz':{'targets':3,'seconds_each':300,'executions':executions,'failure_artifacts':0,'slow_units':0,'all_source_inputs_identical':True,'results':results},'instrumenter':check,'artifacts_sha256':hashes,'raw_artifact_origins':originals}
for name,data in pending.items():(out/name).write_bytes(data)
(out/(prefix+'checks.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
print('Retained',len(pending),'artifacts plus checks;',executions,'ASAN executions; all source/reference/corpus inputs, seeds and binaries verified.')
