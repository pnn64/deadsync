from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,re,subprocess,struct
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference'
run=root/'floor0-parameters-20261027';gates=root/'floor0-parameters-validation-verified-20261027';fuzz=root/'fuzz-floor0-parameters-20261027'
out=decode/'vorbis/validation';prefix='2026-10-27-floor0-parameters-';pending={};hashes={};originals={}
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
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==165
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==118
assert next(c for c in checks['commands'] if c['name']=='independent')['ignored']==54
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
text=read(gates/'release.log')
text=re.sub(r'test [a-zA-Z_][a-zA-Z_0-9:]* \.\.\. (?:ok|ignored[^\n]*)\n','',text)
assert 'whole setup extent matrix: 475 matching parse/initialization decisions' in text
assert 'codebook extent matrix: 475 declarations' in text
old_bits=re.findall(r'(?m)^[^\r\n]+/packet-\d+/bits-(\d+)-(\d+): (\d+) cases, (\d+) packets, (\d+) discarded, (\d+) truncated, (\d+) samples, peak',text)
assert len(old_bits)==888 and sum(int(x[6]) for x in old_bits)==663119360 and sum(int(x[2]) for x in old_bits)==55845
old_matrix=re.findall(r'(?m)^((?:modes|residue)-[^:\r\n]+): (\d+) cases, (\d+) packets, (\d+) discarded\r?$',text)
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
reference=obj(run/'independent-reference.json');assert reference['bark']['maps']==1088 and reference['bark']['indices']==1436160 and reference['bark']['published_reference_integer_differences']==312
assert len(reference['component_goldens'])==69 and reference['fresh_single_case_commands']==207
assert len(reference['component_matrix'])==344 and sum(x['cases'] for x in reference['component_matrix'])==39946
assert len(reference['pcm_goldens'])==7 and len(reference['old_pcm_regressions'])==15 and len(reference['container_reference_checks'])==7
assert sum(len(x['seeks']) for x in reference['container_reference_checks'])==98
assert reference['reference_binary_sha256']==sha(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe')
assert reference['previous_binary_sha256']==sha(run/'oracle-before.exe')=='ce9e8c877a0a1e73445f11657eb9ebed9bc047b8f1bc4a4aa3fa385e41aed102'
for row in reference['component_matrix']:
 p=gates/'floor0-inputs'/f"components-{row['case']}.bin";assert sha(p)==row['request_sha256'] and sha(p.with_suffix('.out'))==row['response_sha256']
for row in reference['pcm_goldens']:
 assert sha(run/'golden-pcm-inputs'/f"floor0-golden-{row['case']}.bin")==row['request_sha256'] and row['fresh_pcm_identical']
for row in reference['old_pcm_regressions']:assert sha(repo/row['request'])==row['request_sha256']
for row in reference['container_reference_checks']:
 p=gates/'floor0-inputs'/f"floor0-stream-{row['stream']}.ogg";assert sha(p)==row['sha256'] and row['raw_canonical_pcm_identical']
 for seek in row['seeks']:assert seek['position']==seek['target'] and seek['frames']==row['frames']-seek['target'] and seek['canonical_raw_suffix_identical']
for name,row in reference['goldens'].items():
 p=decode/'vorbis/tests/fixtures'/name;assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256']
retain(run/'independent-reference.json','independent-reference.json')
for name,h in obj(run/'baseline-source.json').items():
 p=run/'baseline-source'/name;assert sha(p)==h
 committed=subprocess.check_output(['git','show','1a91a2ab0:'+name]);assert committed.replace(b'\r\n',b'\n')==p.read_bytes().replace(b'\r\n',b'\n')
assert (run/'oracle-before.c').read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show','1a91a2ab0:crates/deadlib-audio/decode/vorbis/tests/oracle/oracle.c']).replace(b'\r\n',b'\n')
for name in ['baseline-source.json','baseline.log','sticky-eof-before.log','sticky-eof-cases-before.rs','sticky-eof-floor-before.rs','pcm-preflight.log','decoder-before-trace.rs','oracle-before.c','oracle-build.log']:retain(run/name,name)
# Retain small procedural witnesses; no downloaded media is redistributed.
for src,name in [('baseline-inputs/floor0-width-32.bin','width32-baseline.bin'),('mdct-trace/288-0-256.bin','isolated-infinity-mdct.bin'),('mdct-trace/288-0-256.out','isolated-infinity-mdct.out'),('golden-inputs/bark-1-32-64.bin','bark-witness.bin'),('golden-inputs/bark-1-32-64.out','bark-witness.out')]:retain(run/src,name,raw=True)
raw=(run/'golden-inputs/component-golden-3.bin').read_bytes();at=0
headers=[]
for _ in range(3):
 n=int.from_bytes(raw[at:at+8],'little');headers.append(raw[at:at+8+n]);at+=8+n
count=int.from_bytes(raw[at:at+8],'little');at+=8;found=False
for _ in range(count):
 start=at;w,prefix_bits,n=struct.unpack_from('<QQQ',raw,at);at+=24;data=raw[at:at+n];at+=n
 if prefix_bits==3 and data==b'\x0d':
  request=b''.join(headers)+struct.pack('<Q',1)+raw[start:at];found=True;break
assert found
p=run/'incomplete-lsp-witness.bin';p.write_bytes(request);r=subprocess.run([str(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'),'floor0cases',str(p)],capture_output=True);assert r.returncode==0
assert r.stdout[8]==0 and int.from_bytes(r.stdout[9:17],'little')==8 and r.stdout[17]==0
(run/'incomplete-lsp-witness.out').write_bytes(r.stdout)
retain(p,'incomplete-lsp-witness.bin',raw=True);retain(run/'incomplete-lsp-witness.out','incomplete-lsp-witness.out',raw=True)
provenance=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json');assert provenance['seconds_per_target']==300 and len(results)==3
for name,h in provenance['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in provenance['external_sha256'].items():assert sha(Path(name))==h
initial=obj(fuzz/'initial-corpus.sha256.json')
for name,h in initial.items():assert sha(fuzz/name)==h,name
for r in results:
 assert r['exit_code']==0 and r['executions']>0
 assert sha(repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/(r['target']+'.exe'))==r['binary_sha256']
 assert not list((fuzz/'artifacts'/r['target']).iterdir())
 retain(fuzz/(r['target']+'.log'),r['target']+'.log');retain(fuzz/(r['target']+'.stdout.log'),r['target']+'.stdout.log')
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
seeds=root/'floor0-parameters-seeds-20261027';targeted=obj(seeds/'targeted.json');assert len(targeted['seeds'])==105 and len(targeted['probes'])==7
for seed in targeted['seeds']:
 assert sha(seeds/seed['path'])==seed['sha256'];target=seed['path'].split('/')[0]
 assert initial['corpus/'+target+'/resume-'+seed['sha256']]==seed['sha256']
retain(seeds/'targeted.json','targeted-seeds.json')
instrument=root/'floor0-parameters-instrumenter-verified-20261027';check=obj(instrument/'check.json');assert check['exit_code']==0 and check['source_sha256']==sha(decode/'vorbis/src/decoder.rs') and check['script_sha256']==sha(decode/'scripts/profile_synthesis.py')
for name,h in check['generated_sha256'].items():assert sha(instrument/name)==h
retain(instrument/'check.log','instrumenter-check.log');retain(instrument/'check.json','instrumenter-check.json')
for src,name in [('validate_floor0_parameters.py','validation-probe.py'),('check_floor0_parameters_reference.py','reference-probe.py'),('seed_floor0_parameters.py','seed-probe.py'),('check_floor0_instrumenter.py','instrumenter-probe.py'),('retain_floor0_parameters.py','retention-probe.py')]:retain(root/src,name)
manifest={'completed_utc':datetime.now(timezone.utc).isoformat(),'scope':'Floor-zero published/reference amplitude, Bark and partial-LSP policies; explicit IEEE whole-block NaN propagation. Exact behavior/counts/positions with original PCM tolerances. No application integration or performance measurement.','validation':checks,'component_cases':39946,'component_finite_values':3091639,'component_nonfinite_classes':104041,'component_eof_cases':12818,'bark_maps':1088,'bark_indices':1436160,'published_reference_map_differences':312,'packet_configurations':1411,'packet_cases':31209,'packet_decisions':156045,'packet_discards':1292,'packet_finite_values':25123840,'packet_nonfinite_classes':2274784,'isolated_nonfinite_spectra':24480,'reference_nan_transform_values':134215680,'native_seek_checks':196,'reference_ogg_streams':7,'reference_ogg_seeks':98,'reference_helper_pcm_regressions':15,'earlier_suites_rechecked':{'configuration_cases':31606,'prefix_cases':7014,'finite_bit_cases':55845,'explicit_nonfinite_cases':11,'empty_book_cases':35080,'residue_geometry_cases':332832,'residue_geometry_pcm_values':1836059120,'codebook_extent_declarations_and_setups':475},'fuzz':{'targets':3,'seconds_each':300,'executions':sum(r['executions'] for r in results),'failure_artifacts':0,'all_source_inputs_identical':True,'results':results},'instrumenter':check,'artifacts_sha256':hashes,'raw_artifact_origins':originals}
for name,data in pending.items():(out/name).write_bytes(data)
(out/(prefix+'checks.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
print('Retained',len(pending),'artifacts plus checks; final sources, corpus/reference inputs, seeds and binaries verified.')
