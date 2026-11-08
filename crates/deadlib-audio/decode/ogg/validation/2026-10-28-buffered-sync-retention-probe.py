from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,re,subprocess
repo=Path.cwd();decode=repo/'crates/deadlib-audio/decode';root=repo/'target/deadlib-audio-reference';run=root/'reference-seek-20261027';gates=root/'buffered-sync-validation-20261028';fuzz=root/'fuzz-buffered-sync-final-20261028'
out=decode/'ogg/validation';out.mkdir(exist_ok=True);prefix='2026-10-28-buffered-sync-';pending={};hashes={};origins={}
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
checks=obj(gates/'checks.json');assert checks['completed_utc'];print('verify compiled sources and corpus',flush=True)
for name,h in checks['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in checks['assets_input_sha256'].items():assert sha(repo/name)==h,name
for name,h in checks['external_inputs'].items():assert sha(root/'corpus'/name)==h,name
for name,h in checks['reference_sources'].items():
 assert subprocess.check_output(['git','-C',str(root/name),'rev-parse','HEAD'],text=True).strip()==h
 parts=['src','include']if name=='ogg'else['lib','include'];assert not subprocess.check_output(['git','-C',str(root/name),'diff','--name-only','--',*parts],text=True).strip()
for name,h in checks['reference_binaries'].items():
 folder='vorbis'if'VORBIS'in name else'ogg';assert sha(root/(folder+'-oracle-build/Release/deadlib_'+folder+'_oracle.exe'))==h
up=root/'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955';assert sha(up/'assets.sha256')==checks['upstream_inputs']['manifest_sha256']
for line in read(up/'assets.sha256').splitlines():
 h,name=line.split('  ',1);assert sha(up/name)==h,name
for c in checks['commands']:
 assert c['exit_code']==0,c;p=gates/(c['name']+'.log');retain(p,c['name']+'.log')
 rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(p))
 if rows:c.update(dict(zip(['passed','failed','ignored'],[sum(int(x[i])for x in rows)for i in range(3)])))
assert next(c for c in checks['commands']if c['name']=='ogg-release')['passed']==42
assert next(c for c in checks['commands']if c['name']=='vorbis-regression')['passed']==108
assert next(c for c in checks['commands']if c['name']=='vorbis-regression')['ignored']==78
assert next(c for c in checks['commands']if c['name']=='independent')['passed']==142
assert next(c for c in checks['commands']if c['name']=='independent')['ignored']==87
assert next(c for c in checks['commands']if c['name']=='fuzz-tests')['passed']==4
reference=obj(run/'sync-reference-checks.json');assert reference['before_commit']=='d98eed6e85574849967051f5932f9bfe46cef52f'
assert reference['new_binary_sha256']==checks['reference_binaries']['DEADLIB_LIBOGG_ORACLE']and reference['old_binary_sha256']==sha(run/'ogg-oracle-before.exe')=='92ebe6d2584dec1337bd1355cbe20c0da63c388a0a18cb72000c6ba7711546d6'
assert len(reference['old_protocols'])==47 and len(reference['new_protocol_receipts'])==879 and reference['counts']['operations']==242352
assert len(reference['fresh_goldens'])==4 and reference['golden_sha256']==sha(decode/'ogg/tests/fixtures/sync-state-v1.libogg')
for name,row in reference['existing_source_proof'].items():assert sha(repo/name)==row['current_sha256']
for row in reference['new_protocol_receipts']:
 p=gates/'sync-inputs'/(row['name']+'.bin');assert sha(p)==row['request_sha256']and sha(p.with_suffix('.out'))==row['response_sha256']
text=read(gates/'ogg-release.log');assert 'SYNC-STATE 528 cases, 95298 operations'in text and'SYNC-READER 91 cases, 8554 operations'in text and'SYNC-RAW 260 batches, 138500 operations, 65536 version/flag pairs, 1024 lacing layouts'in text
for name in ['sync-reference-checks.json','reference-check.log','baseline.json','ogg-oracle-before.c']:retain(run/name,name)
retain(decode/'ogg/tests/oracle/VORBIS-COPYING','transport-driver-COPYING')
for name in ['old-encode-input.bin','old-encoded.ogg']:retain(run/name,name,raw=True)
print('verify sanitizer source identities and all retained seeds',flush=True)
prov=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json');initial=obj(fuzz/'initial-corpus.sha256.json')
assert prov['targets']==['ogg_sync','ogg_container']and prov['seconds_per_target']==300 and prov['sanitizer']=='address'
for name,h in prov['source_sha256'].items():assert sha(decode/name)==h,name
for name,h in prov['external_sha256'].items():assert sha(Path(name))==h
for name,h in initial.items():assert sha(fuzz/name)==h,name
executions=0
for row in results:
 assert row['exit_code']==0 and row['executions']>0 and not list((fuzz/'artifacts'/row['target']).iterdir()),row
 log=read(fuzz/(row['target']+'.log'));assert re.search(r'stat::slowest_unit_time_sec:\s+0',log)
 assert int(re.search(r'stat::new_units_added:\s+(\d+)',log)[1])>0
 assert sha(repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/f"{row['target']}.exe")==row['binary_sha256']
 executions+=row['executions']
 for suffix in ['.log','.stdout.log']:retain(fuzz/(row['target']+suffix),row['target']+suffix)
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log']:retain(fuzz/name,'fuzz-'+name)
seedroot=root/'buffered-sync-seeds-20261028';seeds=obj(seedroot/'checks.json');assert seeds['reference_binary_sha256']==reference['new_binary_sha256']and len(seeds['requests'])==338 and seeds['unique_seeds']=={'ogg_sync':2748,'ogg_container':542}
for row in seeds['requests']:
 p=repo/row['request'];assert sha(p)==row['request_sha256']and sha(p.with_suffix('.out'))==row['response_sha256']
for target in prov['targets']:
 for p in(seedroot/target).iterdir():h=sha(p);assert initial[f'corpus/{target}/resume-{h}']==h
retain(seedroot/'checks.json','targeted-seeds.json')
for src,name in [('validate_buffered_sync.py','validation-probe.py'),('check_buffered_sync_reference.py','reference-probe.py'),('seed_buffered_sync.py','seed-probe.py'),('retain_buffered_sync.py','retention-probe.py')]:retain(root/src,name)
inventory=decode/'vorbis/tests/fixtures/eos-seek-differences-v1.tsv';assert sha(inventory)=='5dd201420dba5cd958be3f0887e069b9ed5918cf196270fc5bd6208eacb7aea6'
assert len(read(inventory).splitlines())==172
audit=obj(run/'next-packet-queue-audit.json');retain(run/'next-packet-queue-audit.json','next-packet-queue-audit.json')
manifest={'completed_utc':datetime.now(timezone.utc).isoformat(),'scope':'Exact libogg buffered transport synchronization and pinned libvorbisfile bounded/cached read/seek behavior, separate from strict grammar. New APIs only; prior Ogg/Vorbis production methods remain identical. Full reference Vorbis seeking remains incomplete with 172 recorded differences. No performance measurement or application integration.','validation':checks,'reference_counts':reference['counts'],'differential_schedules':879,'raw_version_flag_pairs':65536,'raw_lacing_layouts':1024,'fresh_goldens':4,'old_helper_response_checks':47,'existing_methods_unchanged':True,'fuzz':{'targets':2,'seconds_each':300,'executions':executions,'failure_artifacts':0,'slow_units':0,'all_source_inputs_identical':True,'results':results},'remaining_seek_differences':172,'inventory_sha256':sha(inventory),'next_packet_queue':audit,'artifacts_sha256':hashes,'raw_artifact_origins':origins}
for name,data in pending.items():(out/name).write_bytes(data)
(out/(prefix+'checks.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
print('Retained',len(pending),'artifacts plus checks;',executions,'ASAN executions; all compiled sources, binary responses and corpus hashes verified.')
