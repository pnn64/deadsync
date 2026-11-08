from pathlib import Path
from datetime import datetime, timezone
import collections, csv, hashlib, importlib.util, json, re, statistics, struct, subprocess, sys

repo = Path.cwd()
d = repo / 'crates/deadlib-audio/decode'
root = repo / 'target/deadlib-audio-reference'
out = d / 'vorbis/validation'
prefix = '2026-10-27-split-prefix-'
pending, artifacts, originals = {}, {}, {}

def digest(b): return hashlib.sha256(b).hexdigest()
def sha(p): return digest(p.read_bytes())
def read(p):
    b = p.read_bytes()
    return b.decode('utf-16' if b.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig')
def obj(p): return json.loads(read(p))
def rows(p): return list(csv.DictReader(read(p).splitlines(), delimiter='\t'))
def lf(b): return b.replace(b'\r\n', b'\n')
def retain(p, name, raw=False, representation=None):
    name = prefix + name
    assert not (out/name).exists() and name not in pending, name
    s = '\n'.join(line.rstrip() for line in read(p).splitlines()).rstrip() if not raw else ''
    b = (representation or p).read_bytes() if raw else (s+'\n' if s else '').encode()
    pending[name] = b
    artifacts[name] = digest(b)
    originals[name] = {'path': p.relative_to(repo).as_posix(), 'sha256': sha(p)}
    if representation:
        originals[name]['equivalent_zero_context_representation'] = {'path':representation.relative_to(repo).as_posix(),'sha256':sha(representation)}

records = []
variants = [('direct-vq', 'direct-vq-codebook.rs', 'vorbis/src/codebook.rs'),
            ('stereo-pairs', 'stereo-pairs-residue.rs', 'vorbis/src/residue.rs'),
            ('inline-vq', 'inline-vq-codebook.rs', 'vorbis/src/codebook.rs'),
            ('split-prefix', None, None)]
for variant, snapshot, changed_file in variants:
    run = root / (variant+'-pipeline-20261027')
    m = obj(run/'manifest.json'); base = m['baseline']
    assert base['revision'].startswith('995ddca57') and base['adapter_adjustments'] == []
    assert sha(run/'baseline/production-source.zip') == base['source_archive_sha256']
    for group in ['source_sha256', 'adapter_source_sha256']:
        for name, h in base[group].items(): assert sha(run/'baseline'/name) == h, name
    assert base['registry_packages'] == m['registry_packages']
    assert base['cargo_artifact_profile'] == m['cargo_artifact_profile']
    reconstruction = {}
    for name, h in m['source_sha256'].items():
        p = repo/name
        if sha(p) == h:
            reconstruction[name] = 'current exact bytes'; continue
        relative = p.relative_to(d).as_posix() if p.is_relative_to(d) else None
        if snapshot and relative == changed_file:
            assert sha(run/snapshot) == h, name
            reconstruction[name] = snapshot; continue
        original = run/'baseline'/relative if relative else None
        assert original and original.is_file(), name
        candidates = [original.read_bytes(), lf(original.read_bytes())]
        assert h in [digest(b) for b in candidates], name
        reconstruction[name] = 'baseline bytes (raw or LF)'
    for name, h in m['input_sha256'].items(): assert sha(repo/name) == h, name
    for role, entry in obj(run/'saved-binaries.json').items():
        expected = base['binary_sha256'] if role == 'baseline' else m['binary_sha256']['rust' if role == 'current' else 'libvorbis']
        assert sha(repo/entry['path']) == entry['sha256'] == expected, role
    trials = rows(run/'trials.tsv'); assert len(trials) == 420
    counts = {}
    for row in trials:
        if row['phase'] != 'startup':
            n, remainder = divmod(int(row['samples']), int(row['iterations']))
            assert remainder == 0 and counts.setdefault(row['path'], n) == n
    label = 'pipeline' if variant == 'split-prefix' else 'rejected-'+variant
    for source, name in [('manifest.json', '.json'), ('trials.tsv', '-trials.tsv'), ('summary.tsv', '-summary.tsv'), ('saved-binaries.json', '-saved-binaries.json')]:
        retain(run/source, label+name)
    for p in sorted(run.glob('*-build.log')): retain(p, label+'-'+p.name)
    for p in sorted(run.glob('*-configure.log')): retain(p, label+'-'+p.name)
    if snapshot:
        normalized=run/'direct-vq-zero-context.patch' if variant=='direct-vq' else None
        if normalized:
            patch_check=obj(run/'patch-normalization.json')
            assert sha(run/(variant+'.patch'))==patch_check['original_sha256'] and sha(normalized)==patch_check['zero_context_sha256']
            retain(run/'patch-normalization.json',label+'-patch-normalization.json')
        retain(run/(variant+'.patch'), label+'.patch', raw=True, representation=normalized)
        retain(run/snapshot, label+'-source.rs')
    records.append({'variant':variant, 'retained':not bool(snapshot), 'timing_rows':len(trials),
                    'source_input_adapter_archive_saved_binary_hashes_rechecked':True,
                    'profiles_and_registry_artifacts_match':True, 'source_reconstruction':reconstruction})

run = root/'split-prefix-resources-20261027'; m = obj(run/'manifest.json'); base=m['baseline']
assert obj(run/'complete.json')['source_input_binary_hashes_rechecked']
for group in ['source_sha256','input_sha256']:
    for name,h in m[group].items(): assert sha(repo/name)==h,name
assert sha(run/'baseline/production-source.zip')==base['source_archive_sha256'] and base['adapter_adjustments']==[]
for group in ['source_sha256','adapter_source_sha256']:
    for name,h in base[group].items(): assert sha(run/'baseline'/name)==h,name
changed=[name for name in base['source_sha256'] if lf((run/'baseline'/name).read_bytes())!=lf((d/name).read_bytes())]
assert changed==['vorbis/src/bitpack.rs','vorbis/src/codebook.rs','vorbis/src/residue.rs'],changed
assert lf((run/'baseline/vorbis/src/residue.rs').read_bytes()).split(b'#[cfg(test)]')[0]==lf((d/'vorbis/src/residue.rs').read_bytes()).split(b'#[cfg(test)]')[0]
assert base['registry_packages']==m['registry_packages']['rust_timing']
assert base['cargo_artifact_profile']==m['cargo_artifact_profiles']['rust_timing']
for old,new in [('rust_baseline_counted','rust_counted'),('rust_baseline_memory','rust_memory')]:
    assert m['registry_packages'][old]==m['registry_packages'][new]
    assert m['cargo_artifact_profiles'][old]==m['cargo_artifact_profiles'][new]
for role,v in obj(run/'saved-binaries.json').items(): assert sha(repo/v['path'])==v['sha256']==m['workers'][role]['sha256'],role
resources=rows(run/'resources.tsv'); seek=rows(run/'seek-trials.tsv'); assert (len(resources),len(seek))==(480,280)
groups=collections.defaultdict(list)
for row in resources: groups[row['path'],row['engine'],row['phase'],row['instrumented']].append(row)
assert len(groups)==160
fields=['operations','requested_bytes','peak_extra_bytes','retained_delta_bytes','reads','bytes_read','seeks','tells','api_exposed_samples']
for group in groups.values():
    assert len(group)==3
    for k in fields: assert len({r[k] for r in group})==1,k
for row in resources:
    if row['engine']=='native':
        b=groups[row['path'],'native-baseline',row['phase'],row['instrumented']][0]
        assert all(row[k]==b[k] for k in fields),(row['path'],row['phase'])
        if row['phase']=='seek_batch' and row['instrumented']=='True': assert all(int(row[k])==0 for k in fields[:4])
for i,(name,geometry) in enumerate(m['workloads'].items()):
    assert sha(run/f'workloads/{i}-seek.bin')==geometry['sha256']
    assert sha(run/f'workloads/{i}-oracle-requests.bin')==geometry['oracle_requests_sha256']
for row in seek:
    n=m['workloads'][row['path']]['expected_channel_samples'] if row['phase']=='seek_batch' else 1
    assert int(row['units'])==n*int(row['iterations'])
for name in ['manifest.json','resources.tsv','seek-trials.tsv','seek-summary.tsv','complete.json','saved-binaries.json']:
    retain(run/name,'resources.json' if name=='manifest.json' else 'resources-'+name if name in ['complete.json','saved-binaries.json'] else name)
for p in sorted(run.glob('*-build.log')): retain(p,'resources-'+p.name)
for p in sorted(run.glob('*-configure.log')): retain(p,'resources-'+p.name)

sys.path.insert(0,str(d/'scripts'))
spec=importlib.util.spec_from_file_location('profile_codebooks',d/'scripts/profile_codebooks.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
profiles=[]
for label,old in [('main','codebook-profile-checked-20261027'),('floor0','codebook-profile-floor0-checked-20261027')]:
    run=root/f'split-prefix-profile-{label}-20261027';m=obj(run/'manifest.json');before=obj(root/old/'manifest.json')
    assert m['completed_utc'] and m['source_input_binary_hashes_rechecked']
    assert m['files']==before['files'] and m['input_sha256']==before['input_sha256']
    assert m['table_sha256']==before['table_sha256']
    for group in ['source_sha256','input_sha256']:
        for name,h in m[group].items():assert sha(repo/name)==h,name
    for name,h in m['generated_sha256'].items():assert sha(run/name)==h,name
    assert sha(repo/m['binary_path'])==m['binary_sha256']
    for p in (d/'vorbis/src').glob('*.rs'):
        expected=module.instrument(p.read_text()) if p.name=='codebook.rs' else p.read_text()
        assert (run/'profiled/src'/p.name).read_text()==expected,p.name
    for name,h in m['worker_log_sha256'].items():assert sha(run/name)==h,name
    for name,h in m['table_sha256'].items():assert sha(run/name)==h==sha(root/old/name),name
    assert 'Finished' in read(run/'clippy.log') and 'error:' not in read(run/'clippy.log')
    for name in ['manifest.json','build.log','clippy.log']+[p.name for p in sorted(run.glob('worker-*.log'))]:retain(run/name,'profile-'+label+'-'+name)
    profiles.append({'label':label,'cases':len(m['files']),'raw_channel_samples_bit_compared':sum(e['raw_channel_samples'] for e in m['files']),
                     'pcm_digests_geometry_counters_symbol_tables_equal_to_prior_trace':True,'prior_run':old,'unchanged_table_sha256':m['table_sha256']})
assert sum(p['raw_channel_samples_bit_compared'] for p in profiles)==42738816

run=root/'split-prefix-pipeline-20261027'
pe=obj(run/'pe-text-sections.json');bins=obj(run/'saved-binaries.json')
for role,entry in pe.items():
    b=(repo/bins[role]['path']).read_bytes();off=struct.unpack_from('<I',b,0x3c)[0];n=struct.unpack_from('<H',b,off+6)[0];section=off+24+struct.unpack_from('<H',b,off+20)[0]
    matches=[section+i*40 for i in range(n) if b[section+i*40:section+i*40+8].rstrip(b'\0')==b'.text'];assert len(matches)==1
    assert entry=={'virtual_bytes':struct.unpack_from('<I',b,matches[0]+8)[0],'raw_bytes':struct.unpack_from('<I',b,matches[0]+16)[0],'sha256':sha(repo/bins[role]['path'])}
assert pe['baseline']['virtual_bytes']-pe['current']['virtual_bytes']==13120
asm=obj(run/'asm-inspection.json');assert sha(repo/asm['path'])==asm['sha256']
assert asm['packet_decoder_call_counts']=={'refill':21,'decode_from':7}
paired=[]
for label,file in [('pipeline','split-prefix-pipeline-20261027/trials.tsv'),('seeking','split-prefix-resources-20261027/seek-trials.tsv')]:
    groups=collections.defaultdict(dict)
    for row in rows(root/file):
        if row['engine'] in ['native','native-baseline']:groups[row['path'],row['phase']][row['engine'],row['trial']]=float(row['ns_per_run'])
    for (path,phase),group in groups.items():
        ratios=[group['native-baseline',str(i)]/group['native',str(i)] for i in range(7)]
        paired.append({'workload':label,'path':path,'phase':phase,'trial_paired_baseline_over_current':ratios,'median':statistics.median(ratios),'min':min(ratios),'max':max(ratios)})
assert obj(run/'paired-ratios.json')==paired
for name in ['pe-text-sections.json','asm-inspection.json','paired-ratios.json']:retain(run/name,name)
oldasm=root/'inline-vq-pipeline-20261027/asm-comparison.json';comparison=obj(oldasm)
for v in comparison.values():assert sha(repo/v['path'])==v['sha256']
retain(oldasm,'rejected-inline-vq-asm-comparison.json')

gates=root/'split-prefix-validation-20261027';checks=obj(gates/'checks.json');assert checks['completed_utc']
for name,h in checks['source_sha256'].items():assert sha(d/name)==h,name
for name,h in checks['assets_input_sha256'].items():assert sha(repo/name)==h,name
for name,h in checks['external_inputs'].items():assert sha(root/'corpus'/name)==h,name
for key,h in checks['reference_binaries'].items():
    p=root/('vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe' if key=='DEADLIB_LIBVORBIS_ORACLE' else 'ogg-oracle-build/Release/deadlib_ogg_oracle.exe');assert sha(p)==h
for name,h in checks['reference_sources'].items():assert subprocess.check_output(['git','-C',str(root/name),'rev-parse','HEAD'],text=True).strip()==h
up=root/'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955';assert sha(up/'assets.sha256')==checks['upstream_inputs']['manifest_sha256']
for line in (up/'assets.sha256').read_text().splitlines():
    h,name=line.split('  ',1);assert sha(up/name)==h,name
for command in checks['commands']:
    assert command['exit_code']==0,command
    p=gates/(command['name']+'.log');retain(p,command['name']+'.log')
    results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(p))
    if results:command.update(dict(zip(['passed','failed','ignored'],[sum(int(x[i]) for x in results) for i in range(3)])))
assert next(c for c in checks['commands'] if c['name']=='release')['passed']==119
assert next(c for c in checks['commands'] if c['name']=='independent')['passed']==92
for name in ['matrix.tsv','historical.tsv','legacy.tsv','upstream.tsv']:retain(gates/name,name)
retain(root/'validate_split_prefix.py','validation-probe.py')

fuzz=root/'fuzz-split-prefix-20261027';p=obj(fuzz/'provenance.json');results=obj(fuzz/'results.json')
assert len(results)==4
for name,h in p['source_sha256'].items():assert sha(d/name)==h,name
for name,h in p['external_sha256'].items():assert sha(Path(name))==h,name
for name,h in obj(fuzz/'initial-corpus.sha256.json').items():assert sha(fuzz/name)==h,name
for r in results:
    binary=repo/'target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release'/(r['target']+'.exe');assert sha(binary)==r['binary_sha256']
    assert r['exit_code']==(70 if r['target']=='vorbis_stream' else 0) and r['executions']>0
    retain(fuzz/(r['target']+'.log'),'fuzz-'+r['target']+'.log')
for name in ['provenance.json','results.json','results.tsv','initial-corpus.sha256.json','build.log','seeds.log','stream-timeout-replay.json','stream-timeout-replay.log']:retain(fuzz/name,'fuzz-'+name)
timeout_artifacts=[p for p in (fuzz/'artifacts').rglob('*') if p.is_file()];assert len(timeout_artifacts)==1
artifact=timeout_artifacts[0];retain(artifact,'fuzz-stream-timeout.bin',raw=True)
current_replay=obj(fuzz/'stream-timeout-replay.json');assert current_replay['exit_code']==0 and current_replay['artifact_sha256']==sha(artifact)
baseline=root/'split-prefix-timeout-baseline-20261027';build=obj(baseline/'build-command.json');old_replay=obj(baseline/'replay.json')
assert build['exit_code']==0 and old_replay['exit_code']==0 and old_replay['artifact_sha256']==sha(artifact)
for name,h in build['source_sha256'].items():assert sha(baseline/name)==h,name
for name,h in obj(root/'split-prefix-pipeline-20261027/manifest.json')['baseline']['source_sha256'].items():
    assert sha(baseline/name)==h,name
assert sha(baseline/'target/x86_64-pc-windows-msvc/release/vorbis_stream.exe')==old_replay['binary_sha256']
for name in ['build-command.json','build.log','replay.json','replay.log']:retain(baseline/name,'fuzz-timeout-baseline-'+name)
rerun=root/'fuzz-split-prefix-stream-recheck-20261027';r=obj(rerun/'results.json');rp=obj(rerun/'provenance.json')
assert r['exit_code']==0 and r['executions']>0 and rp['timeout_seconds']==120
assert r['binary_sha256']==current_replay['binary_sha256']
slow_units=[p for p in (rerun/'artifacts').rglob('*') if p.is_file()]
assert all(p.name.startswith('slow-unit-') for p in slow_units)
for i,p in enumerate(slow_units):retain(p,f'fuzz-stream-slow-unit-{i}.bin',raw=True)
assert len(obj(rerun/'initial-corpus.sha256.json'))==44
for name,h in obj(rerun/'initial-corpus.sha256.json').items():assert sha(rerun/'corpus'/name)==h,name
for name in ['provenance.json','results.json','initial-corpus.sha256.json','vorbis_stream.log']:retain(rerun/name,'fuzz-stream-recheck-'+name)
retain(root/'retain_split_prefix.py','retention-probe.py')

checks.update({'scope':'Inline the bounded Huffman prefix and VQ wrapper, outline the unchanged tree fallback and byte-cache refill; retain public validation and bit/float operation order.',
 'benchmarks':records,'kept_pipeline_timing_rows':420,'rejected_pipeline_timing_rows':1260,'resource_rows':480,'seek_timing_rows':280,
 'only_changed_production_sources_after_lf_normalization':changed,'residue_runtime_identical_to_baseline':True,
 'native_heap_io_exposure_equal_to_baseline_in_every_group':True,'native_warmed_seek_allocations_all_zero':True,
 'profiles':profiles,'measured_worker_text_virtual_bytes_reduction':13120,
 'fuzz':{'initial_results':results,'initial_stream_timeout_preserved':True,'timeout_artifact_sha256':sha(artifact),
         'current_fixed_input_replay':current_replay,'baseline_fixed_input_replay':old_replay,'stream_recheck_result':r,
         'successful_three_target_executions':sum(v['executions'] for v in results if v['exit_code']==0),
         'stream_successful_campaign_slow_units':[{'name':p.name,'sha256':sha(p),'bytes':p.stat().st_size} for p in slow_units],
         'scope':'ASan smoke campaigns; initial stream campaign timed out on a known initial seed. Both fixed-input replays succeeded; a separate stream campaign retained all 44 seeds and raised the per-input budget to 120 seconds. Slow-unit diagnostics are retained separately from failures. This is not proof of complete coverage.'},
 'artifacts':artifacts,'original_artifacts_before_utf8_lf_normalization':originals,'retained_utc':datetime.now(timezone.utc).isoformat(),
 'limitations':['Shared unlocked machine; native trial ranges overlap and short/startup timings are mixed.',
 'Matching-profile assembly and complete pipeline-worker PE .text size are diagnostic, not isolated library code size.',
 'The slow legacy seed exceeds the default fuzz time budget in the unmodified baseline as well; original timeout retained.',
 'Native throughput still trails Symphonia; broader conformance, sparse codec seeking and encoder quality/rate control remain.']})
for name,b in pending.items():(out/name).write_bytes(b)
(out/(prefix+'checks.json')).write_bytes((json.dumps(checks,indent=2)+'\n').encode())
print('Retained',len(pending),'artifacts; 1,960 timing rows; 480 resource rows; 42,738,816 bit-compared PCM values.')
