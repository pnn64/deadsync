from pathlib import Path
import csv, hashlib, json, os, subprocess
root = Path.cwd(); png = root / 'crates/deadlib-image/png'; base = '8ce596494afb8cdfa09a273f6f8091cb02a79106'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
work = root / 'target/png-stored-growth-allocations'; work.mkdir(exist_ok=False)
baseline = work / 'baseline-image'; baseline.mkdir()
paths = ['Cargo.toml', 'src/lib.rs', 'png/Cargo.toml', 'png/benches/pipeline.rs'] + ['png/' + p.relative_to(png).as_posix() for p in sorted((png / 'src').rglob('*.rs'))]
original = {}
for name in paths:
    data = subprocess.check_output(['git', 'show', base + ':crates/deadlib-image/' + name]); original[name] = hashlib.sha256(data).hexdigest()
    path = baseline / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
with (baseline / 'Cargo.toml').open('ab') as output: output.write(b'\n[workspace]\nmembers = [".", "png"]\nresolver = "3"\n')
probe = png / 'experiments/stored-growth-allocations.rs'
meta = {'baseline_png_commit': base, 'probe': probe.relative_to(png).as_posix(), 'probe_sha256': sha(probe), 'baseline_source_sha256': original, 'candidate_source_sha256': [{'path': p.relative_to(png).as_posix(), 'sha256': sha(p)} for p in sorted((png / 'src').rglob('*.rs'))], 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(), 'cargo': subprocess.check_output(['cargo', '--version'], text=True).strip(), 'commands': [], 'measurements': [], 'samples': [], 'scope': 'Untimed single-threaded System global-allocator wrapper. Counts successful calls and requested layout bytes for one native API call, after warmup. Logical live-byte peaks exclude allocator overhead, usable-size rounding, internal realloc overlap and process RSS. Inputs and caller-retained prior outputs are outside each measurement. Instrumented binaries are never used for throughput timing; compiler allocation elision can affect observed calls.'}
env = os.environ.copy(); env['CARGO_BUILD_JOBS'] = '2'
log = png / 'stored-growth-allocations.log'; assert not log.exists()
with log.open('wb') as output:
    for side in ['baseline', 'candidate']:
        project = work / side; (project / 'src').mkdir(parents=True)
        (project / 'src/main.rs').write_bytes(probe.read_bytes())
        native = baseline / 'png' if side == 'baseline' else png
        text = '[package]\nname = "png-allocation-probe"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n[dependencies]\ndeadlib-image-png = { path = ' + json.dumps(native.as_posix()) + ' }\nflate2 = "=1.1.10"\n[profile.release]\nopt-level = 3\nlto = true\ndebug = 1\npanic = "abort"\n'
        manifest = project / 'Cargo.toml'; manifest.write_text(text, encoding='utf-8', newline='\n')
        target = work / ('target-' + side)
        commands = [['cargo', 'build', '--release', '--offline', '--manifest-path', str(manifest), '--target-dir', str(target)]]
        if side == 'candidate': commands.append(['cargo', 'clippy', '--release', '--offline', '--manifest-path', str(manifest), '--target-dir', str(target), '--all-targets', '--', '-D', 'warnings'])
        for command in commands:
            output.write(('COMMAND: ' + json.dumps(command) + '\n').encode()); output.flush()
            result = subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
            meta['commands'].append({'command': command, 'exit_code': result.returncode})
            if result.returncode: raise SystemExit(result.returncode)
        binary = target / 'release/png-allocation-probe.exe'
        for corpus in ['photos', 'formats']:
            directory = root / ('target/deadlib-image-reference/kodak' if corpus == 'photos' else 'target/png-decode-corpus-20261027-a')
            samples = work / (side + '-' + corpus + '-samples')
            command = [str(binary), str(directory), str(samples)]
            output.write(('COMMAND: ' + json.dumps(command) + '\n').encode()); output.flush()
            result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=output, check=True)
            path = png / f'stored-growth-allocations-{side}-{corpus}.tsv'; assert not path.exists(); path.write_bytes(result.stdout.replace(b'\r\n', b'\n'))
            meta['measurements'].append({'side': side, 'corpus': corpus, 'path': path.relative_to(png).as_posix(), 'sha256': sha(path), 'binary': str(binary), 'binary_sha256': sha(binary), 'command': command, 'exit_code': result.returncode})
    summary = {}
    for corpus in ['photos', 'formats']:
        tables = {}
        for side in ['baseline', 'candidate']:
            rows = list(csv.DictReader((png / f'stored-growth-allocations-{side}-{corpus}.tsv').open(), delimiter='\t'))
            assert len(rows) == (48 if corpus == 'photos' else 120)
            tables[side] = {(row['case'], row['operation']): row for row in rows}
            assert all(int(r['failed_calls']) == 0 for r in rows)
        assert tables['baseline'].keys() == tables['candidate'].keys()
        changes = []
        for key, before in tables['baseline'].items():
            after = tables['candidate'][key]
            assert before['sample_bytes'] == after['sample_bytes']
            assert before['peak_requested_live_bytes'] == after['peak_requested_live_bytes']
            assert before['retained_requested_live_bytes'] == after['retained_requested_live_bytes']
            if before['realloc_calls'] != after['realloc_calls']:
                changes.append({'case': key[0], 'operation': key[1], 'before_realloc_calls': int(before['realloc_calls']), 'after_realloc_calls': int(after['realloc_calls'])})
        summary[corpus] = {'cases': len(tables['baseline']) // 2, 'peak_and_retained_requested_bytes_identical': True, 'reallocation_changes': changes}
        for before in sorted((work / ('baseline-' + corpus + '-samples')).glob('*.samples')):
            after = work / ('candidate-' + corpus + '-samples') / before.name; assert before.read_bytes() == after.read_bytes()
            meta['samples'].append({'corpus': corpus, 'file': before.name, 'bytes': before.stat().st_size, 'sha256': sha(before), 'baseline': str(before), 'candidate': str(after)})
    for row in meta['candidate_source_sha256']: assert sha(png / row['path']) == row['sha256']
meta.update(summary=summary, log=log.relative_to(png).as_posix(), log_sha256=sha(log))
path = png / 'experiments/stored-growth-allocations.json'; assert not path.exists(); path.write_bytes((json.dumps(meta, indent=2) + '\n').encode())
print(json.dumps(summary, indent=2), flush=True)
