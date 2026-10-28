from pathlib import Path
import datetime, hashlib, json, os, subprocess, time

root = Path.cwd(); png = root / 'crates/deadlib-image/png'; base = 'b2b3a74583250f6a60ee220e689570f9521fafaf'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
def save(path, data):
    path.write_bytes((json.dumps(data, indent=2) + '\n').encode())
assert not subprocess.check_output(['git','status','--porcelain','--',str(png)]).strip()
sources = sorted((png / 'src').rglob('*.rs'))
original = {p:p.read_bytes() for p in sources}
assert len(sources) == 21
for p, data in original.items():
    assert data == subprocess.check_output(['git','show',base + ':' + p.relative_to(root).as_posix()])
work = root / 'target/png-literal-refill-20261028-a'; work.mkdir(exist_ok=False)
manifest = png / 'experiments/literal-refill-trial.json'
log = png / 'experiments/literal-refill-trial.log'
assert not manifest.exists() and not log.exists()
archived = png / 'experiments/literal-refill-driver.py'
assert not archived.exists(); archived.write_bytes(Path(__file__).read_bytes())
env = os.environ.copy(); env['CARGO_BUILD_JOBS'] = '4'
meta = dict(baseline_png_commit=base,driver_sha256=sha(Path(__file__)),commands=[],builds=[],
            shared_source_sha256=sha(png.parent / 'src/lib.rs'),
            workspace_manifest_sha256=sha(root / 'Cargo.toml'),workspace_lock_sha256=sha(root / 'Cargo.lock'),
            scope='Buffered decoder literal-run refill experiment. All task builds and correctness checks finish before timed processes. External workload and frequency remain uncontrolled.')
def checkpoint(): save(manifest,meta)
def run(label, command):
    print(label,flush=True)
    with log.open('ab') as stream:
        stream.write((label + '\nCOMMAND: ' + json.dumps(command) + '\n').encode()); stream.flush()
        start = time.monotonic()
        result = subprocess.run(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT)
        stream.write(('\nEXIT: ' + str(result.returncode) + '\n').encode())
    meta['commands'].append(dict(label=label,command=command,exit_code=result.returncode,wall_seconds=round(time.monotonic()-start,3)))
    checkpoint(); assert result.returncode == 0,label
def build(variant):
    print('Build ' + variant,flush=True)
    command = ['cargo','build','--offline','-p','deadlib-image-png','--profile','bench','--example','decode_bench',
               '--target-dir',str(work / 'build'),'--message-format','json']
    result = subprocess.run(command,cwd=root,env=env,capture_output=True)
    build_log = png / f'experiments/literal-refill-{variant}-build.log'
    assert not build_log.exists(); build_log.write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0,result.stderr.decode(errors='replace')
    artifacts = [json.loads(line) for line in result.stdout.splitlines()]
    artifact = next(a for a in reversed(artifacts) if a.get('reason') == 'compiler-artifact' and a['target']['name'] == 'decode_bench' and a.get('executable'))
    binary = Path(artifact['executable']); destination = work / variant; destination.mkdir()
    saved = destination / binary.name; saved.write_bytes(binary.read_bytes())
    pdb = binary.with_suffix('.pdb')
    if pdb.exists(): (destination / pdb.name).write_bytes(pdb.read_bytes())
    record = dict(variant=variant,command=command,exit_code=result.returncode,executable=str(saved),executable_sha256=sha(saved),
                  source_sha256=[dict(path=p.relative_to(png).as_posix(),sha256=sha(p)) for p in sources],
                  build_log=build_log.relative_to(png).as_posix(),build_log_sha256=sha(build_log))
    meta['builds'].append(record); checkpoint(); return record
baseline = build('baseline')
path = png / 'src/zlib.rs'; text = path.read_text()
old = '''                let mut count = bits.count;
                // Append complete primary literals from prefetched
                // bits, without a refill or capacity growth here.
                while room != 0 && count >= 9 {
'''
new = '''                let mut count = bits.count;
                let mut pos = bits.pos;
                let input = bits.input;
                // Keep literal runs in this loop across conservative 32-bit
                // refills. Matches, long codes and the input tail stay on the
                // normal symbol path; no output growth occurs here.
                while room != 0 {
                    if count < 9 {
                        if input.len() - pos < 4 {
                            break;
                        }
                        let bytes = input[pos..pos + 4].try_into().unwrap();
                        word |= u64::from(u32::from_le_bytes(bytes)) << count;
                        pos += 4;
                        count += 32;
                    }
'''
assert text.count(old) == 1
text = text.replace(old,new)
end = '''                bits.word = word;
                bits.count = count;
'''
assert text.count(end) == 1
path.write_text(text.replace(end,end + '                bits.pos = pos;\n'),newline='\n')
patch = png / 'experiments/literal-refill.patch'; assert not patch.exists()
patch.write_bytes(subprocess.check_output(['git','-c','core.autocrlf=false','diff',base,'--',str(path.relative_to(root))]))
meta['patch'] = patch.relative_to(png).as_posix(); meta['patch_sha256'] = sha(patch)
run('Focused raw and PNG correctness', ['cargo','test','--offline','-p','deadlib-image-png','--release','--target-dir',str(work / 'build'),
    '--lib','--test','compression','--test','huffman','--test','huffman_permutations','--test','primary_pairs',
    '--test','deflate_prefix','--test','deflate_stream','--test','zlib_headers','--test','stream_literals','--test','static_png','--','--nocapture'])
run('Strict candidate lint',['cargo','clippy','--offline','-p','deadlib-image-png','--all-targets','--target-dir',str(work / 'build'),'--','-D','warnings'])
run('Scoped formatting',['rustfmt','--edition','2024','--check',str(path)])
candidate = build('candidate')
assert [r['path'] for r in candidate['source_sha256'] if r not in baseline['source_sha256']] == ['src/zlib.rs']
rustc = subprocess.check_output(['rustc','--version'],text=True).strip()
cargo = subprocess.check_output(['cargo','--version'],text=True).strip()
corpora = dict(photos=root / 'target/deadlib-image-reference/kodak',formats=root / 'target/png-decode-corpus-20261027-a')
templates = {}
for name, directory in corpora.items():
    inputs = [dict(file=p.name,bytes=p.stat().st_size,sha256=sha(p).upper()) for p in sorted(directory.glob('*.png'))]
    assert len(inputs) == (24 if name == 'photos' else 60)
    for record in [baseline,candidate]:
        variant = record['variant']
        template = png / f'experiments/literal-refill-{variant}-{name}-source.json'
        assert not template.exists()
        data = dict(target='decode',stage='pipeline',iterations=12,runs=0,warmups_per_operation=3,rustc=rustc,cargo=cargo,commit=base,
                    source_sha256=record['source_sha256'],benchmark_source='examples/decode_bench.rs',benchmark_source_sha256=sha(png / 'examples/decode_bench.rs'),
                    corpus_sha256=inputs,executable=record['executable'],executable_sha256=record['executable_sha256'],cpu=0,affinity=1,measurements=[],
                    source_build_record=manifest.relative_to(png).as_posix(),template_scope='Binary/source/input provenance only; no timed process in this template.')
        save(template,data); templates[variant,name] = template
try:
    for mode,variant in [('before','baseline'),('trial','candidate'),('repeat','candidate'),('control','baseline')]:
        for name,directory in corpora.items():
            record = baseline if variant == 'baseline' else candidate
            prefix = png / f'benchmark-literal-refill-{mode}-{name}-a'
            run('Timed ' + mode + ' ' + name,['powershell','-NoProfile','-File',str(png / 'scripts/benchmark_saved.ps1'),
                '-Manifest',str(templates[variant,name]),'-Executable',record['executable'],'-CorpusDirectory',str(directory),
                '-LogPrefix',str(prefix),'-Iterations','12','-Runs','3'])
finally:
    for p,data in original.items():
        if p == path: p.write_bytes(data)
        else: assert p.read_bytes() == data
    meta['all_production_source_restored'] = all(p.read_bytes() == data for p,data in original.items())
    checkpoint()
for name in corpora:
    run('Summarize ' + name,['python',str(png / 'scripts/summarize_decoder.py'),
        '--before',str(png / f'benchmark-literal-refill-before-{name}-a'),str(png / f'benchmark-literal-refill-control-{name}-a'),
        '--after',str(png / f'benchmark-literal-refill-trial-{name}-a'),str(png / f'benchmark-literal-refill-repeat-{name}-a'),
        '--output',str(png / f'benchmark-literal-refill-{name}-summary.json')])
meta['completed'] = True; checkpoint()
print('Trial complete; production baseline restored for review.',flush=True)
