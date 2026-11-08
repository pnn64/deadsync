"""Interleave resident codec binaries; report CPU cycles and PCM throughput."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
from bench_host import cpu_info

CASES = ['smooth16-l0', 'smooth16-l2', 'smooth16-l5', 'smooth16-l8',
         'noise16-l2', 'noise24-l2', 'smooth32-l5', 'short16-l2', 'noise32-l0']


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def execute(binary, case, iterations):
    fields = subprocess.check_output([str(binary), case, str(iterations)], text=True).strip().split('\t')
    names = ['iterations', 'cycles', 'ns', 'samples', 'bytes', 'cold_allocs', 'cold_reallocs',
             'cold_requested_bytes', 'warm_allocs', 'warm_reallocs']
    return dict(zip(names, map(int, fields[1:-1])), signature=fields[-1])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=11)
    parser.add_argument('--milliseconds', type=int, default=60)
    parser.add_argument('--case', action='append', help='case supported by both benchmark binaries')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('CPU-cycle comparisons require Windows QueryThreadCycleTime')
    assert args.rounds > 0 and args.milliseconds > 0
    assert not args.output.exists(), 'preserve previous measurements'
    binaries = {'before': args.before.resolve(), 'after': args.after.resolve()}
    report = {'cpu': cpu_info(), 'rounds': args.rounds, 'milliseconds': args.milliseconds,
              'driver_sha256': sha(Path(__file__)),
              'binaries': {k: {'path': str(p), 'sha256': sha(p)} for k, p in binaries.items()},
              'cases': {}}
    rng = random.Random(9639)
    for case in args.case or CASES:
        pilots = {k: execute(p, case, 1) for k, p in binaries.items()}
        for field in ['samples', 'bytes', 'signature']:
            assert pilots['before'][field] == pilots['after'][field], (case, field)
        # Equal work counts avoid a calibration difference biasing power state.
        iterations = max(1, round(args.milliseconds * 1e6 / max(v['ns'] for v in pilots.values())))
        rows = []
        for trial in range(args.rounds):
            order = list(binaries)
            rng.shuffle(order)
            for variant in order:
                row = execute(binaries[variant], case, iterations)
                assert row['signature'] == pilots['before']['signature']
                rows.append(dict(variant=variant, trial=trial, **row))
        summary = {}
        for variant in binaries:
            selected = [r for r in rows if r['variant'] == variant]
            cycles = statistics.median(r['cycles'] / r['iterations'] for r in selected)
            ns = statistics.median(r['ns'] / r['iterations'] for r in selected)
            summary[variant] = dict(cycles_per_frame=cycles, ns_per_frame=ns,
                                    msamples_per_second=pilots[variant]['samples'] * 1000 / ns,
                                    allocations=pilots[variant])
        speedup = summary['before']['cycles_per_frame'] / summary['after']['cycles_per_frame']
        report['cases'][case] = dict(summary=summary, cpu_speedup=speedup, rows=rows)
        print(f'{case}: {speedup:.3f}x CPU throughput', flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
