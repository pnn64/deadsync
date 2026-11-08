"""Non-seekable gapless output against pinned independently verified file PCM."""
import argparse, hashlib, json, subprocess
from pathlib import Path
from reference_historical import compare, pcm

p = argparse.ArgumentParser()
for name in ['corpus', 'reference', 'output']:
    p.add_argument('--'+name, type=Path, required=True)
p.add_argument('--native', required=True)
p.add_argument('--wsl-distro')
p.add_argument('--chunk', type=int, action='append')
p.add_argument('--case', action='append')
args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
prefix = ['wsl', '-d', args.wsl_distro, '--'] if args.wsl_distro else []
def toolpath(path):
    path = path.resolve()
    return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:] if prefix else str(path)
proof = json.loads((args.reference/'results.json').read_text())
baseline = {r['file']: r for r in proof['records']}
records = []
for source in sorted(args.corpus.glob('*.mp3')):
    if source.name not in baseline or args.case and not any(source.name.startswith(s) for s in args.case):
        continue
    prior = baseline[source.name]; assert sha(source) == prior['input_sha256']
    reference = args.reference/(source.stem+'-native.bin')
    assert sha(reference) == prior['pcm_sha256'][reference.name]
    schedules = []
    for size in args.chunk or [0, 1, 7, 127, 4096]:
        target = args.output/(source.stem+'-'+str(size)+'.bin')
        command = prefix+[args.native, toolpath(source), toolpath(target), str(size)]
        run = subprocess.run(command, capture_output=True, text=True)
        assert not run.returncode, (command, run.stderr)
        stats = list(map(int, next(line.split('\t')[1:] for line in run.stdout.splitlines() if line.startswith('samples\t'))))
        delta = compare(pcm(target), pcm(reference))
        assert delta['bit_identical'], (source.name, size, delta)
        expected = prior.get('samples', prior.get('format', {}).get('samples'))
        assert stats[0] == expected
        schedules.append({'chunk': size, 'command': command, 'exit_code': run.returncode,
                          'statistics': stats, 'comparison': delta, 'output_sha256': sha(target)})
    records.append({'file': source.name, 'input_sha256': sha(source), 'reference_sha256': sha(reference), 'schedules': schedules})
    (args.output/'results.json').write_text(json.dumps({'method': __doc__, 'records': records}, indent=2)+'\n')
    print(source.name, len(schedules), 'gapless read schedules pass', flush=True)
digest = subprocess.check_output(prefix+['sha256sum', args.native], text=True).split()[0] if prefix else sha(Path(args.native))
report = {'passed': bool(records), 'files': len(records), 'read_schedules': sum(len(r['schedules']) for r in records),
          'binary_sha256': digest, 'script_sha256': sha(Path(__file__)), 'reference_results_sha256': sha(args.reference/'results.json')}
(args.output/'provenance.json').write_text(json.dumps(report, indent=2)+'\n'); assert report['passed']
