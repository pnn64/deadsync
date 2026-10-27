"""Resident-input exact seeks. Both APIs return PCM beginning at the target.

Construct metadata-only readers before timing; retain their seek state across
requests. Warm up at zero, then visit two full cycles of 97 deterministic targets.
Native uses its lazy frame index, libFLAC its own seek table/search algorithm.
"""
import argparse
import csv
import json
from pathlib import Path
import random
import statistics
import subprocess
import time
from bench_host import cpu_info, set_affinity
from run import sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['native', 'oracle', 'corpus', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--case', action='append', required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--iterations', type=int, default=194)
    parser.add_argument('--cpu', type=int, default=2)
    args = parser.parse_args()
    assert args.iterations >= 194 and args.iterations % 97 == 0
    assert args.rounds > 0
    args.output.mkdir(parents=True, exist_ok=False)
    args.native, args.oracle = args.native.resolve(), args.oracle.resolve()
    set_affinity(args.cpu)
    root = Path(__file__).resolve().parents[1]
    source = {str(p.relative_to(root)): sha(p) for p in sorted((root/'src').glob('*.rs'))}
    drivers = {str(p.relative_to(root)): sha(p) for p in [Path(__file__), root/'benches/bench_host.py',
               root/'benches/decode.rs', *sorted((root/'benches/libflac').glob('*'))] if p.is_file()}
    commands, cases, rows = [], [], []
    rng = random.Random(9639)

    def execute(command):
        result = subprocess.run(list(map(str, command)), capture_output=True, text=True)
        commands.append({'arguments': list(map(str, command)), 'exit_code': result.returncode,
                         'stdout': result.stdout, 'stderr': result.stderr})
        assert not result.returncode, commands[-1]
        fields = result.stdout.strip().split('\t')
        assert len(fields) == 9, result.stdout
        return dict(zip(['mode', 'iterations', 'ns', 'units', 'checksum', 'bytes', 'frames', 'bits', 'channels'],
                        [fields[0]] + list(map(int, fields[1:]))))

    paths = sorted(p for p in args.corpus.rglob('*.flac')
                   if any(p.relative_to(args.corpus).as_posix().startswith(c) for c in args.case))
    assert paths, 'no matching cases'
    for path in paths:
        path = path.resolve()
        case = path.relative_to(args.corpus.resolve()).as_posix()
        index = (args.output/(sha(path)+'.index')).resolve()
        execute([args.native, 'frames', path, 1, index])
        cases.append({'case': case, 'sha256': sha(path), 'bytes': path.stat().st_size})
        reference = None
        for trial in range(args.rounds):
            modes = ['seek', 'libflac-seek']
            rng.shuffle(modes)
            for mode in modes:
                command = ([args.native, mode, path, args.iterations] if mode == 'seek' else
                           [args.oracle, mode, path, index, args.iterations])
                row = execute(command)
                assert row['units'] == args.iterations, row
                signature = row['checksum']
                if reference is None:
                    reference = signature
                assert signature == reference, (case, mode, 'trimmed PCM endpoint checksum', row)
                row.update(case=case, trial=trial, ns_per_seek=row['ns']/args.iterations)
                rows.append(row)
        print(case, {m: round(statistics.median(r['ns_per_seek'] for r in rows
              if r['case'] == case and r['mode'] == m)/1000, 3) for m in ['seek', 'libflac-seek']}, flush=True)
        with (args.output/'raw.tsv').open('w', newline='') as output:
            writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter='\t')
            writer.writeheader()
            writer.writerows(rows)
        (args.output/'commands.json').write_text(json.dumps(commands, indent=2)+'\n', encoding='utf-8')
    summary = [{'case': c['case'], 'mode': m,
                'median_ns_per_seek': statistics.median(r['ns_per_seek'] for r in rows if r['case'] == c['case'] and r['mode'] == m),
                'min_ns_per_seek': min(r['ns_per_seek'] for r in rows if r['case'] == c['case'] and r['mode'] == m),
                'max_ns_per_seek': max(r['ns_per_seek'] for r in rows if r['case'] == c['case'] and r['mode'] == m)}
               for c in cases for m in ['seek', 'libflac-seek']]
    (args.output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n', encoding='utf-8')
    manifest = {'timestamp_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                'native': str(args.native), 'native_sha256': sha(args.native),
                'oracle': str(args.oracle), 'oracle_sha256': sha(args.oracle),
                'cpu': args.cpu, 'cpuinfo': cpu_info(), 'rounds': args.rounds, 'iterations': args.iterations,
                'cases': cases, 'native_sources': source, 'driver_sources': drivers,
                'policy': __doc__, 'warmup': 'target zero; initialization outside clock',
                'target': '(total_samples-1)*(iteration%97)/97; iteration 1..iterations',
                'consumption': 'returned trimmed frame endpoints of every channel; exact equal checksums',
                'integrity': 'frame CRC enabled; whole-stream MD5 unavailable after arbitrary seeks',
                'commands_sha256': sha(args.output/'commands.json')}
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n', encoding='utf-8')


if __name__ == '__main__':
    main()
