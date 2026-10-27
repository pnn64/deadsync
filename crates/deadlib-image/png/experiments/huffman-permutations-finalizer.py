from pathlib import Path
import hashlib, json, re, subprocess

root = Path.cwd(); png = root / 'crates/deadlib-image/png'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
read = lambda name: json.loads((png / name).read_text(encoding='utf-8-sig'))
def write(name, data):
    (png / name).write_bytes((json.dumps(data, indent=2) + '\n').encode())
def totals(text):
    rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
    assert rows
    return dict(zip(['passed','failed','ignored'], [sum(int(row[i]) for row in rows) for i in range(3)]))

meta = read('huffman-permutations-validation.json')
assert len(meta['commands']) == 7 and all(r['exit_code'] == 0 for r in meta['commands'])
assert meta['final_source_hashes_verified'] and 'repository_finalization' not in meta
assert sha(Path(meta['oracle_binary'])) == meta['oracle_sha256']
for row in meta['production_source_sha256'] + meta['test_source_sha256'] + meta['tools']:
    assert sha(png / row['path']) == row['sha256'], row['path']
assert len(meta['production_source_sha256']) == 21
for row in meta['production_source_sha256']:
    original = subprocess.check_output(['git','show',meta['baseline_png_commit'] + ':crates/deadlib-image/png/' + row['path']])
    assert original == (png / row['path']).read_bytes(), row['path']
prior = read(meta['fixture_baseline']['path'])
assert sha(png / meta['fixture_baseline']['path']) == meta['fixture_baseline']['sha256']
for row in prior['test_source_sha256']:
    if row['path'] != 'examples/seed_fuzz.rs':
        assert sha(png / row['path']) == row['sha256']
assert sha(png.parent / 'src/lib.rs') == meta['shared_source_sha256'] == prior['shared_source_sha256']
assert (png / 'experiments/huffman-permutations-fuzz-native-before.py').read_bytes() == subprocess.check_output(
    ['git','show',meta['baseline_png_commit'] + ':crates/deadlib-image/png/scripts/fuzz_native.py'])
isolated = Path(meta['independent_workspace']); copied = 0; documentation_changes = []
for directory in ['src','tests','examples','benches']:
    for source in (png / directory).rglob('*'):
        if source.is_file():
            relative = source.relative_to(png)
            original = isolated / 'png' / relative
            if source.read_bytes() != original.read_bytes():
                assert relative.as_posix() == 'tests/oracle/README.md', relative
                documentation_changes.append({'path':relative.as_posix(),'as_run_sha256':sha(original),'repository_sha256':sha(source)})
            copied += 1
assert copied == 887
assert len(documentation_changes) == 1
assert sha(isolated / 'src/lib.rs') == meta['shared_source_sha256']
assert sha(isolated / 'png/Cargo.toml') == sha(png / 'Cargo.toml')
logs = {name:(png / name).read_bytes() for name in ['huffman-permutations-validation.log','huffman-permutations-independent.log']}
text = logs['huffman-permutations-validation.log'].decode('utf-8-sig').replace('\r\n','\n')
normal = totals(text.split('Normal release suite\n',1)[1].split('C zlib permutation batch\n',1)[0])
offline = totals(logs['huffman-permutations-independent.log'].decode('utf-8-sig'))
assert normal == {'passed':138,'failed':0,'ignored':20}, normal
assert offline == {'passed':4,'failed':0,'ignored':1}, offline
assert 'C zlib checked 10744 symbol-permutation streams with exact outputs' in text

fuzz = read('fuzz/reports/huffman-permutations-source.json')
assert fuzz['isolation_verified'] and fuzz['final_source_hashes_verified']
assert {r['target']:r['requested_runs'] for r in fuzz['campaigns']} == {'png_stream':100000,'inflate_stream':100000}
for row in fuzz['source_sha256']:
    assert sha(png.parent / row['path']) == row['sha256']
for row in fuzz['commands']:
    assert row['exit_code'] == 0 and sha(png / row['repository_log']) == row['log_sha256']
for row in fuzz['artifact_audit']:
    assert (row['counter_bytes'] > 0 and row['pc_table_bytes'] > 0) if row['native_feedback'] else row['counter_bytes'] == row['pc_table_bytes'] == 0
for row in fuzz['harness_object_audit']:
    assert row['counter_bytes'] == row['pc_table_bytes'] == 0
executions = sum(r['runs'] for r in fuzz['campaigns'])
assert all(r['runs'] >= r['requested_runs'] and r['asan_init_defined'] for r in fuzz['campaigns'])
assert executions == fuzz['new_successful_executions']
assert fuzz['historical_successful_executions_all_targets'] == 1908472 + executions
seeds = read('fuzz/reports/huffman-permutations-seeds.json')
assert seeds['archive_sha256'] == fuzz['seed_archive_sha256']
assert {r['target']:r['files'] for r in seeds['initial_corpora']} == {'png_stream':58911,'inflate_stream':17671}
for initial, campaign in zip(seeds['initial_corpora'],fuzz['campaigns']):
    assert initial['target'] == campaign['target']
    for field in ['files','bytes','maximum_file_bytes','sha256']:
        assert initial[field] == campaign['initial_corpus'][field]

recovery = read(meta['initial_lint_recovery']['path'])
assert sha(png / meta['initial_lint_recovery']['path']) == meta['initial_lint_recovery']['sha256']
for row in recovery['initial_evidence']:
    assert sha(png / row['path']) == row['sha256']
    if row['path'].endswith('.log'):
        payload = (png / row['path']).read_bytes()
        (png / row['path']).write_bytes(payload.replace(b'\r\n',b'\n').rstrip(b'\r\n') + b'\n')
        row.update(as_run_sha256=row['sha256'], sha256=sha(png / row['path']), normalization='CRLF to LF and terminal empty separator removal only')
marker = b'#[path = "../tests/support/huffman_permutations.rs"]'
current = (png / 'examples/seed_fuzz.rs').read_bytes()
assert current.count(marker) == 1
assert hashlib.sha256(current.replace(marker,b'#[allow(dead_code)]\n' + marker)).hexdigest() == recovery['initial_seed_source_sha256']
recovery.update(corrected_seed_source_sha256=sha(png / 'examples/seed_fuzz.rs'), only_source_correction='Remove the duplicate outer allow(dead_code); keep helper-level allowance.')
write(meta['initial_lint_recovery']['path'],recovery)
meta['initial_lint_recovery']['as_run_sha256'] = meta['initial_lint_recovery']['sha256']
meta['initial_lint_recovery']['sha256'] = sha(png / meta['initial_lint_recovery']['path'])

archive_scripts = {
    'run_png_huffman_permutations_validation.py':'huffman-permutations-validation-runner.py',
    'prepare_png_permutations_fuzz.py':'huffman-permutations-fuzz-preparation.py',
    'retain_png_huffman_permutations_fuzz.py':'huffman-permutations-fuzz-retainer.py',
    'finalize_png_huffman_permutations.py':'huffman-permutations-finalizer.py',
    'recover_png_permutations_fuzz_launch.py':'huffman-permutations-fuzz-launch-recovery.py',
    'run_png_permutations_fuzz.py':'huffman-permutations-fuzz-launcher.py',
}
archived = []
for source, destination in archive_scripts.items():
    path = png / 'experiments' / destination
    assert not path.exists()
    payload = (root / 'target' / source).read_bytes(); assert b'\r\n' not in payload
    path.write_bytes(payload)
    archived.append({'path':path.relative_to(png).as_posix(),'sha256':sha(path)})
assert archived[0]['sha256'] == meta['runner_sha256']
assert archived[2]['sha256'] == fuzz['retainer_sha256']
launch = read('experiments/huffman-permutations-fuzz-launch.json')
assert launch['exit_code'] == 0 and sha(png / launch['log']) == launch['log_sha256']
assert launch['runner_sha256'] == archived[5]['sha256']
launch_recovery = read('fuzz/reports/huffman-permutations-launch-recovery.json')
assert launch_recovery['recovery_runner_sha256'] == archived[4]['sha256']
assert launch_recovery['all_extracted_seed_bytes_verified'] and launch_recovery['duplicate_seed_extraction_removed_after_verification']
for name in ['stored-growth','stream-copy']:
    record = read(f'fuzz/reports/huffman-permutations-{name}-corpus-storage.json')
    assert record['runner_sha256'] == archived[1]['sha256']
    assert record['all_archive_entries_match_original'] and record['temporary_corpus_removed_after_verification']
    assert sha(png / record['prior_evidence']) == record['prior_evidence_sha256']
    assert sha(root / record['archive']) == record['archive_sha256']

log_records = []
for name, payload in logs.items():
    (png / name).write_bytes(payload.replace(b'\r\n',b'\n').rstrip(b'\r\n') + b'\n')
    log_records.append({'path':name,'as_run_sha256':hashlib.sha256(payload).hexdigest(),
                        'repository_sha256':sha(png / name),'normalization':'CRLF to LF and terminal empty separator removal only'})
meta['repository_finalization'] = dict(normal_release_tests=normal, standalone_offline_permutation_tests=offline,
    copied_files_verified=copied, copied_files_unchanged=copied-len(documentation_changes), all_copied_files_match=False,
    all_copied_compilation_inputs_match=True, post_validation_documentation_changes=documentation_changes,
    production_png_files_changed=[], production_png_files_unchanged=21,
    existing_test_input_changes=['examples/seed_fuzz.rs'], new_test_files=meta['new_test_files'], c_zlib_exact_positive_outputs=10744,
    symbol_depth_cells=8728, selected_complete_tree_shapes=36, new_successful_sanitizer_executions=executions,
    historical_successful_sanitizer_executions=1908472 + executions, logs=log_records, archived_runners=archived,
    previous_goal_turn='Scope clarification without implementation progress; revalidated completed process 73901 as terminal and inspected all retained results before proceeding.',
    performance='No new timings: all production source bytes match the selected e30aac88 baseline; its measured performance limitation remains.')
write('huffman-permutations-validation.json',meta)
seeds['generator_validation_sha256'] = sha(png / 'huffman-permutations-validation.json')
write('fuzz/reports/huffman-permutations-seeds.json',seeds)
write('fuzz/reports/huffman-permutations-seed-build.json', dict(
    validation='huffman-permutations-validation.json', validation_sha256=sha(png / 'huffman-permutations-validation.json'),
    build_command=next(r for r in meta['commands'] if r['label'] == 'Fresh permutation seed export'),
    sources=meta['production_source_sha256'] + meta['test_source_sha256'], shared_source_sha256=meta['shared_source_sha256'],
    binary='target/release/examples/seed_fuzz.exe', binary_sha256=sha(root / 'target/release/examples/seed_fuzz.exe'),
    archive_sha256=seeds['archive_sha256'], added_independent_streams_per_target=10744,
    scope='Fresh narrow export from the same independent fixture helper as the tested target; deterministic union with the recorded earlier immutable seed ZIP. No production encoder is called.'))

families = [dict(name=n,streams=c,alphabet=a,maximum_depth=d,shapes=s,symbol_depth_cells=a*d)
            for n,c,a,d,s in [('literals',4626,257,15,9),('lengths',5148,286,15,9),('distances',780,30,15,13),('code-lengths',190,19,7,5)]]
assert sum(r['streams'] for r in families) == 10744
assert sum(r['symbol_depth_cells'] for r in families) == 8728
evidence = ['huffman-permutations-validation.json','fuzz/reports/huffman-permutations-source.json',
            'fuzz/reports/huffman-permutations-seeds.json','fuzz/reports/huffman-permutations-seed-build.json']
write('huffman-permutations-audit.json',dict(baseline_png_commit=meta['baseline_png_commit'],
    production_changes=[], families=families, independent_construction='Complete comb/split shapes, cyclic assignments, lexicographic canonical wire builder, scalar checksums and PNG framing; expected overlap bytes calculated directly.',
    exercised_symbols='All declared LL symbols, all 30 legal distance symbols, all 19 CL symbols including repeats 16/17/18; all 29 match-length symbols and low/high extra fields, 32 KiB history.',
    boundaries='Buffered and three input/output fragment shapes, strict suffix rejection and prefix consumption, exact/zero/short limits, truncated trailers, PNG buffered/seven-byte reader output, both padding modes.',
    references={'C zlib':10744,'flate2':10744,'Rust png':10744},
    limitation='Selected complete-tree shapes and cyclic assignments cover every legal symbol/depth cell, not all possible trees, assignments, block sequences or malformed inputs. Bounded ASan runs are not a source-coverage or universal conformance proof.',
    new_successful_sanitizer_executions=executions, decoder_completion=False,
    remaining_work='Measured competitive decoder performance and final requirement audit; encoding excluded by user clarification.',
    evidence=[{'path':name,'sha256':sha(png / name)} for name in evidence]))
print(json.dumps(dict(normal_tests=normal,standalone_tests=offline,copied_files=copied,c_zlib_outputs=10744,
                     sanitizer_executions=executions,production_changes=0),indent=2),flush=True)
