from pathlib import Path
from datetime import datetime, timezone
import hashlib, json, re, subprocess

repo=Path.cwd();d=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
out=d/'vorbis/validation';prefix='2026-10-27-bit-mutations-';pending={};artifacts={};originals={}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):
 b=p.read_bytes();return b.decode('utf-16' if b.startswith((b'\xff\xfe',b'\xfe\xff')) else 'utf-8-sig')
def obj(p):return json.loads(read(p))
def retain(p,name,raw=False):
 name=prefix+name;assert not (out/name).exists() and name not in pending,name
 s='\n'.join(x.rstrip() for x in read(p).splitlines()).rstrip() if not raw else ''
 b=p.read_bytes() if raw else (s+'\n' if s else '').encode()
 pending[name]=b;artifacts[name]=hashlib.sha256(b).hexdigest();originals[name]={'path':p.relative_to(repo).as_posix(),'sha256':sha(p)}

gates=root/'bit-mutations-validation-final-20261027';checks=obj(gates/'checks.json');assert checks['completed_utc']
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
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==128
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==97
assert next(c for c in checks['commands'] if c['name']=='independent')['ignored']==32
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
retain(root/'validate_bit_mutations_final.py','validation-probe.py')

import collections
text=read(gates/'release.log')
pattern=r'(?m)^([^\r\n]+/packet-\d+/bits-(\d+)-(\d+)): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak ([\d.eE+-]+), RMS ([\d.eE+-]+)'
groups=collections.defaultdict(lambda:{'input_cases':0,'finite_reference_cases':0,'packets':0,'discarded':0,'truncated':0,'samples':0,'batches':0,'peak_error':0.0,'maximum_batch_rms':0.0,'reference_nonfinite_cases':0,'native_nonfinite_rejections':0})
rows=re.findall(pattern,text);assert len(rows)==888
for label,start,end,cases,packets,discarded,truncated,samples,peak,rms in rows:
 g=groups[label.split('/packet-')[0]]
 for k,v in zip(['input_cases','finite_reference_cases','packets','discarded','truncated','samples','batches'],[int(end)-int(start),int(cases),int(packets),int(discarded),int(truncated),int(samples),1]):g[k]+=v
 g['peak_error']=max(g['peak_error'],float(peak));g['maximum_batch_rms']=max(g['maximum_batch_rms'],float(rms))
for label,count,rejected in re.findall(r'(?m)^([^\r\n]+/packet-\d+/bits-\d+-\d+): reference-nonfinite (\d+), native-nonfinite-rejections (\d+)',text):
 g=groups[label.split('/packet-')[0]];g['reference_nonfinite_cases']+=int(count);g['native_nonfinite_rejections']+=int(rejected)
assert sum(g['input_cases'] for g in groups.values())==55856
assert sum(g['finite_reference_cases'] for g in groups.values())==55845
assert sum(g['samples'] for g in groups.values())==663119360
assert sum(g['packets'] for g in groups.values())==236529
assert sum(g['reference_nonfinite_cases'] for g in groups.values())==11
for g in groups.values():assert g['finite_reference_cases']+g['reference_nonfinite_cases']==g['input_cases']
nonfinite=re.findall(r'(?m)^reference-nonfinite: ([^;\r\n]+); native rejected=(true|false)',text);assert len(nonfinite)==11
prefixes=re.findall(r'(?m)^(mono|stereo|eight-channel-silence|eight-channel-tones|floor-zero): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak ([\d.eE+-]+), RMS ([\d.eE+-]+)',text)
assert len(prefixes)==5 and sum(int(r[1]) for r in prefixes)==7014 and sum(int(r[5]) for r in prefixes)==83072768
assert set(re.findall(r'historical independently selected packets: (\[[^\]]+\])',text))=={'[0, 1, 2, 3, 4, 5, 25, 26, 27, 28, 29, 30, 31, 32]'}

run=root/'bit-mutations-parity-20261027';baseline=obj(run/'baseline-sha256.json')
for name,h in baseline.items():assert sha(run/name)==h,name
for name in ['vorbis/src/floor.rs','vorbis/src/codebook.rs']:
 old=subprocess.check_output(['git','show','9c82f20f0:crates/deadlib-audio/decode/'+name]);assert old.replace(b'\r\n',b'\n')==(run/name).read_bytes().replace(b'\r\n',b'\n'),name
retain(run/'baseline-sha256.json','baseline-source.json')
for source,name in [('bit-mutations-floor0-initial-20261027.log','baseline-rounding-failure.log'),('bit-mutations-floor0-rounding-20261027.log','selector-failure.log'),('bit-mutations-floor0-selectors-20261027.log','sequence-failure.log'),('bit-mutations-floor0-sequence-20261027.log','reference-nonfinite-discovery.log'),('bit-mutations-focused-final-preflight-20261027.log','focused.log')]:retain(root/source,name)
assert 'PCM peak tolerance' in read(root/'bit-mutations-floor0-initial-20261027.log')
assert 'floor0 book number' in read(root/'bit-mutations-floor0-rounding-20261027.log')
assert 'PCM RMS tolerance' in read(root/'bit-mutations-floor0-selectors-20261027.log')
ref=obj(run/'sequence-books-reference.json');assert ref['binary_sha256']==checks['reference_binaries']['DEADLIB_LIBVORBIS_ORACLE']
assert [r['lookup_type'] for r in ref['books']]==[1,2] and all(r['reference_vector']==[1,2,3] for r in ref['books'])
for record in ref['books']:assert sha(run/f"sequence-book-{record['lookup_type']}.bin")==record['input_sha256']
nf=obj(run/'nonfinite-case-reference.json');assert nf['reference_binary_sha256']==ref['binary_sha256'] and sha(root/'corpus/floor0/vorbis_floor_type_0.ogg')==nf['source_sha256']
assert sha(run/'nonfinite-case.bin')==nf['input_sha256'] and sum(r.get('nonfinite',0) for r in nf['records'])==1600
for name in ['sequence-books-reference.json','nonfinite-case-reference.json']:retain(run/name,name)
for lookup in [1,2]:retain(run/f'sequence-book-{lookup}.bin',f'sequence-book-{lookup}.bin',raw=True)
retain(root/'check_bit_mutations_reference.py','reference-probe.py')

seed=root/'bit-mutations-fuzz-seeds-20261027';sm=obj(seed/'manifest.json');assert sha(repo/sm['source'])==sm['source_sha256']
for case in sm['seeds']:
 for kind in ['raw','stream']:
  entry=case[kind];assert sha(repo/entry['path'])==entry['sha256'] and (repo/entry['path']).stat().st_size==entry['bytes']
retain(seed/'manifest.json','targeted-seeds.json');retain(root/'seed_bit_mutations_fuzz.py','targeted-seed-probe.py')
instrument=root/'bit-mutations-instrumenter-check-20261027';im=obj(instrument/'checks.json');assert im['exit_code']==0 and not im['performance_measurements_run']
for name,h in im['source_sha256'].items():assert sha(repo/name)==h,name
for name,h in im['generated_sha256'].items():assert sha(instrument/name)==h,name
retain(instrument/'checks.json','instrumenter-checks.json');retain(instrument/'check.log','instrumenter-check.log')

fuzz=root/'fuzz-bit-mutations-final-20261027';fp=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json');assert len(results)==3 and all(r['exit_code']==0 and r['executions']>0 for r in results)
for name,h in fp['source_sha256'].items():assert sha(d/name)==h,name
for name,h in fp['external_sha256'].items():assert sha(Path(name))==h,name
initial=obj(fuzz/'initial-corpus.sha256.json')
for name,h in initial.items():assert sha(fuzz/name)==h,name
for case in sm['seeds']:
 for kind,target in [('raw','vorbis_packets'),('stream','vorbis_stream')]:assert initial[f"corpus/{target}/resume-{case[kind]['sha256']}"]==case[kind]['sha256']
slow=[p for p in (fuzz/'artifacts').rglob('*') if p.is_file()];assert all(p.name.startswith('slow-unit-') for p in slow)
for i,p in enumerate(slow):retain(p,f'fuzz-slow-unit-{i}.bin',raw=True)
for r in results:
 binary=repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/(r['target']+'.exe');assert sha(binary)==r['binary_sha256'];retain(fuzz/(r['target']+'.log'),'fuzz-'+r['target']+'.log')
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
retain(root/'retain_bit_mutations.py','retention-probe.py')
checks.update({'priority':'Complete behavioral parity; original PCM tolerances and exact finite-reference behavior/counts. No performance comparison.',
 'scope':'Every bit of generated fixture audio packets and fourteen independently selected legacy floor-zero packets, with bounded reference batches, pre-roll and recovery.',
 'bit_mutations':dict(groups),'input_mutations':55856,'finite_reference_cases':55845,'finite_reference_channel_samples':663119360,'finite_reference_packet_decisions':236529,
 'reference_nonfinite_cases':[{'case':name,'native_rejected_nonfinite':rejected=='true'} for name,rejected in nonfinite],
 'nonfinite_cases_excluded_from_pcm_parity_totals':True,'original_prefix_cases_rechecked':7014,'original_prefix_channel_samples':83072768,
 'legacy_nonzero_case_selection_uses_independent_libvorbis':True,'numerical_tolerances_unchanged':True,'sequence_books_reference':ref,
 'fuzz':{'targets':[r['target'] for r in results],'seconds_per_target':300,'executions':sum(r['executions'] for r in results),'failure_artifacts':0,'slow_unit_artifacts':[{'name':p.name,'sha256':sha(p),'bytes':p.stat().st_size} for p in slow],'targeted_seed_count':10,'source_input_binary_hashes_rechecked':True},
 'artifacts':artifacts,'original_artifacts_before_utf8_lf_normalization':originals,'retained_utc':datetime.now(timezone.utc).isoformat(),
 'limitations':['Reference nonfinite output is an explicit behavior divergence; native finite-output rejection/reset is checked separately, not counted as PCM parity.',
 'This does not establish complete Vorbis parity. Broader legal setups, malformed inputs, reference acceptance policies and external conformance/real-song coverage remain.']})
for name,b in pending.items():(out/name).write_bytes(b)
(out/(prefix+'checks.json')).write_bytes((json.dumps(checks,indent=2)+'\n').encode())
print('Retained',len(pending),'artifacts; 55,845 finite-reference mutations;',checks['fuzz']['executions'],'final-source ASan executions.')
