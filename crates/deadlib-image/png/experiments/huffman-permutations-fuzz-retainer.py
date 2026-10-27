from pathlib import Path
import hashlib, json, re

root = Path('/mnt/c/GitHub/deadsync'); png = root / 'crates/deadlib-image/png'
output = Path('/tmp/deadlib-image-huffman-permutations-asan-20261028-b')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
manifest = output / 'manifest.json'; data = json.loads(manifest.read_text())
assert data['isolation_verified'] and data['final_source_hashes_verified']
assert data['requested_run_override'] == 100000
assert {r['target']: r['requested_runs'] for r in data['campaigns']} == {'png_stream': 100000, 'inflate_stream': 100000}
for row in data['source_sha256']:
    assert sha(png.parent / row['path']) == row['sha256']
for command, campaign in zip(data['commands'], data['campaigns']):
    log = Path(command['log'])
    assert command['exit_code'] == 0 and sha(log) == command['log_sha256']
    text = log.read_text(errors='replace')
    assert campaign['runs'] >= campaign['requested_runs']
    assert campaign['runs'] == int(re.search(r'stat::number_of_executed_units:\s+(\d+)', text)[1]) == int(re.search(r'Done (\d+) runs in ', text)[1])
    destination = png / ('fuzz/reports/huffman-permutations-' + log.name)
    assert not destination.exists(); destination.write_bytes(log.read_bytes())
    command['repository_log'] = destination.relative_to(png).as_posix()
for row in data['artifact_audit']:
    assert (row['counter_bytes'] > 0 and row['pc_table_bytes'] > 0) if row['native_feedback'] else row['counter_bytes'] == row['pc_table_bytes'] == 0
for row in data['harness_object_audit']:
    assert row['counter_bytes'] == row['pc_table_bytes'] == 0
prior = json.loads((png / 'fuzz/reports/stored-growth-source.json').read_text())
for row in data['campaigns']:
    old = next(r for r in prior['campaigns'] if r['target'] == row['target'])
    row['baseline_binary_sha256'] = old['binary_sha256']
executions = sum(r['runs'] for r in data['campaigns'])
data.update(as_run_manifest_sha256=sha(manifest), change_baseline_png_commit='e30aac88a827ef45eb5928e557fb20ef74c1fadc',
            new_successful_executions=executions, historical_successful_executions_all_targets=1908472 + executions,
            retainer_sha256=sha(Path(__file__)),
            prior_runner_archive={'path':'experiments/huffman-permutations-fuzz-native-before.py',
                                  'sha256':sha(png / 'experiments/huffman-permutations-fuzz-native-before.py')},
            change_scope='New independent decoder fixtures and larger fresh seed corpora; all production PNG/shared source bytes unchanged from the selected baseline. --runs overrides the historical per-target thresholds.')
destination = png / 'fuzz/reports/huffman-permutations-source.json'
assert not destination.exists(); destination.write_text(json.dumps(data, indent=2) + '\n')
archive = Path('/tmp/deadlib-image-huffman-permutations-seeds-20261028-a.zip')
description = json.loads(archive.with_suffix('.json').read_text())
assert description['archive_sha256'] == sha(archive) == data['seed_archive_sha256']
for initial, campaign in zip(description['initial_corpora'], data['campaigns']):
    assert initial['target'] == campaign['target']
    for field in ['files', 'bytes', 'maximum_file_bytes', 'sha256']:
        assert initial[field] == campaign['initial_corpus'][field]
description.update(archive_path=str(archive), generator_validation='huffman-permutations-validation.json',
                   generator_validation_sha256=sha(png / 'huffman-permutations-validation.json'),
                   baseline_seed_description='fuzz/reports/stream-literals-seeds.json',
                   baseline_seed_description_sha256=sha(png / 'fuzz/reports/stream-literals-seeds.json'),
                   added_independent_streams_per_target=10744)
destination = png / 'fuzz/reports/huffman-permutations-seeds.json'
assert not destination.exists(); destination.write_text(json.dumps(description, indent=2) + '\n')
print(json.dumps({'executions':executions, 'historical_executions':1908472 + executions,
                  'archive_audits':len(data['artifact_audit']), 'harness_objects':len(data['harness_object_audit']),
                  'campaigns':[{k:r[k] for k in ['target','runs','peak_rss_mib']} for r in data['campaigns']]}), flush=True)
