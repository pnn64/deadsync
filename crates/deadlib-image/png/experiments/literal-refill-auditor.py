from pathlib import Path
import hashlib, importlib.util, json, re, subprocess, tempfile

root = Path.cwd(); png = root / 'crates/deadlib-image/png'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
read = lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
meta = read(png / 'experiments/literal-refill-trial.json')
assert meta['completed'] and meta['all_production_source_restored']
assert len(meta['commands']) == 13 and all(r['exit_code'] == 0 for r in meta['commands'])
baseline, candidate = meta['builds']; assert baseline['variant'] == 'baseline' and candidate['variant'] == 'candidate'
assert len(baseline['source_sha256']) == len(candidate['source_sha256']) == 21
assert [r['path'] for r in candidate['source_sha256'] if r not in baseline['source_sha256']] == ['src/zlib.rs']
for row in baseline['source_sha256']:
    data = subprocess.check_output(['git','show',meta['baseline_png_commit'] + ':crates/deadlib-image/png/' + row['path']])
    assert hashlib.sha256(data).hexdigest() == sha(png / row['path']) == row['sha256']
assert sha(png.parent / 'src/lib.rs') == meta['shared_source_sha256']
assert sha(root / 'Cargo.toml') == meta['workspace_manifest_sha256']
assert sha(root / 'Cargo.lock') == meta['workspace_lock_sha256']
assert sha(png / 'experiments/literal-refill-driver.py') == meta['driver_sha256']
assert sha(png / meta['patch']) == meta['patch_sha256']
directory = Path(tempfile.mkdtemp(prefix='png-literal-refill-reconstruct-'))
for row in baseline['source_sha256']:
    destination = directory / 'crates/deadlib-image/png' / row['path']
    destination.parent.mkdir(parents=True,exist_ok=True); destination.write_bytes((png / row['path']).read_bytes())
subprocess.run(['git','-c','core.autocrlf=false','apply','--no-index',str(png / meta['patch'])],cwd=directory,check=True,capture_output=True)
for row in candidate['source_sha256']:
    assert sha(directory / 'crates/deadlib-image/png' / row['path']) == row['sha256'], row['path']
prior = read(png / 'huffman-permutations-validation.json')
for row in prior['production_source_sha256'] + prior['test_source_sha256'] + prior['tools']:
    assert sha(png / row['path']) == row['sha256']
for record in meta['builds']:
    assert record['exit_code'] == 0 and sha(Path(record['executable'])) == record['executable_sha256']
    assert sha(png / record['build_log']) == record['build_log_sha256']
text = (png / 'experiments/literal-refill-trial.log').read_text()
focused = text.split('Strict candidate lint\n',1)[0]
rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',focused)
totals = [sum(int(row[n]) for row in rows) for n in range(3)]
assert totals == [41,0,5],totals
spec = importlib.util.spec_from_file_location('summary',png / 'scripts/summarize_decoder.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
groups = []; summaries = []
for corpus, count in [('photos',24),('formats',60)]:
    corpus_dir = root / ('target/deadlib-image-reference/kodak' if corpus == 'photos' else 'target/png-decode-corpus-20261027-a')
    for mode, record in [('before',baseline),('trial',candidate),('repeat',candidate),('control',baseline)]:
        path = png / f'benchmark-literal-refill-{mode}-{corpus}-a-manifest.json'; data = read(path)
        assert data['source_sha256'] == record['source_sha256']
        assert data['executable_sha256'] == record['executable_sha256']
        assert (data['iterations'],data['runs'],data['warmups_per_operation'],data['cpu'],data['affinity']) == (12,3,3,0,1)
        assert len(data['measurements']) == 3 and all(not row['stderr'] for row in data['measurements'])
        assert len(data['corpus_sha256']) == count
        for row in data['corpus_sha256']:
            assert (corpus_dir / row['file']).stat().st_size == row['bytes']
            assert sha(corpus_dir / row['file']).upper() == row['sha256'].upper()
        assert sha(png / data['benchmark_source']) == data['benchmark_source_sha256']
        assert sha(Path(data['control_of'])) == data['control_manifest_sha256'].lower()
        assert sha(png / 'scripts/benchmark_saved.ps1') == data['control_runner_sha256'].lower()
        groups.append(dict(path=path.relative_to(png).as_posix(),sha256=sha(path),variant=record['variant']))
    path = png / f'benchmark-literal-refill-{corpus}-summary.json'; data = read(path)
    loaded = {}
    for side, modes in [('before',['before','control']),('after',['trial','repeat'])]:
        loaded[side] = module.load([png / f'benchmark-literal-refill-{mode}-{corpus}-a' for mode in modes])
        for field in ['rates_mib_s','process_runs','artifacts']:
            assert loaded[side][field] == data[side][field]
    assert loaded['before']['rows'].keys() == loaded['after']['rows'].keys()
    for key,row in loaded['before']['rows'].items():
        for field in ['encoded_bytes','sample_bytes','image_pipeline_bytes']:
            assert row[field] == loaded['after']['rows'][key][field]
    for operation,row in data['native_changes'].items():
        native = 'deadlib-image-png/' + operation; reference = row['reference']
        b = loaded['before']['rates_mib_s']; a = loaded['after']['rates_mib_s']
        assert row['before_mib_s'] == b[native] and row['after_mib_s'] == a[native]
        assert row['reference_relative_change_percent'] == (a[native]/a[reference]/(b[native]/b[reference])-1)*100
    assert data['runner_sha256'] == sha(png / 'scripts/summarize_decoder.py')
    summaries.append(dict(path=path.relative_to(png).as_posix(),sha256=sha(path),native_changes=data['native_changes']))
audit = dict(decision='reject',reason='No photo decoding/inflation benefit after reference normalization; a local format-inflate gain does not establish an overall PNG benefit. Preserve the previously selected decoder.',
    baseline_png_commit=meta['baseline_png_commit'],production_changes=[],all_21_production_sources_restored=True,
    candidate_reconstruction_verified=True,reconstructed_source_directory=str(directory),focused_tests=dict(passed=41,failed=0,ignored=5),
    strict_all_target_clippy=True,scoped_rustfmt=True,benchmark_groups=groups,timed_processes=24,summaries=summaries,
    unchanged_previous_validation=dict(path='huffman-permutations-validation.json',sha256=sha(png / 'huffman-permutations-validation.json'),all_recorded_compiled_inputs_match=True),
    scope='Each timed process validates raw/native output on all 84 corpus files before measurement. Three warmups, twelve iterations, CPU 0. No task builds/tests/fuzzing/profiling overlap timing. External host work and frequency are uncontrolled; zero pipeline-byte columns do not measure allocation or peak memory.',
    no_new_sanitizer_executions=True,no_full_candidate_suite_claimed=True,
    trial_manifest_sha256=sha(png / 'experiments/literal-refill-trial.json'),trial_log_sha256=sha(png / 'experiments/literal-refill-trial.log'),
    previous_goal_turn='Progress: verified b2b3a745 conformance milestone. This turn tests a distinct decoder performance hypothesis.',
    audit_runner_sha256=sha(Path(__file__)))
destination = png / 'experiments/literal-refill-selection.json'; assert not destination.exists()
destination.write_bytes((json.dumps(audit,indent=2) + '\n').encode())
archived = png / 'experiments/literal-refill-auditor.py'; assert not archived.exists(); archived.write_bytes(Path(__file__).read_bytes())
print(json.dumps(dict(decision='reject',focused_tests=totals,timed_processes=24,production_sources_restored=21),indent=2),flush=True)
