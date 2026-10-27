"""Paired resident-input decode timings on Windows or Linux/WSL."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import time
from bench_host import cpu_info, set_affinity

DEFAULT_SUBSET = [1, 3, 6, 11, 12, 14, 15, 16, 17, 22, 23, 24, 28, 31, 38, 41, 43, 44, 60, 61, 62, 63, 64]
DEFAULT_UNCOMMON = [5, 7, 8, 9]
MODES = ['frames', 'symphonia', 'symphonia-crc', 'libflac-frames', 'ffmpeg',
         'borrowed', 'stream', 'stream-md5', 'libflac', 'libflac-md5', 'startup', 'libflac-startup', 'crc',
         'libflac-new', 'libflac-new-md5', 'libflac-new-startup',
         'symphonia-new', 'symphonia-new-crc', 'symphonia-new-startup',
         'ogg-stream', 'ogg-stream-md5', 'libflac-new-ogg', 'libflac-new-ogg-md5',
         'symphonia-new-ogg', 'symphonia-new-ogg-crc']


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--oracle', type=Path, required=True)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--milliseconds', type=int, default=100)
    parser.add_argument('--cpu', type=int, default=2)
    baseline_modes = ['baseline-' + mode for mode in MODES if not mode.startswith(('libflac', 'ffmpeg'))]
    parser.add_argument('--modes', nargs='+', choices=MODES+baseline_modes, default=MODES)
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--all', action='store_true')
    parser.add_argument('--case', action='append', help='case prefix, e.g. subset/44; repeat to select several')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    args.native = args.native.resolve()
    args.oracle = args.oracle.resolve()
    root = Path(__file__).resolve().parents[1]
    # Capture the actual candidate source before any long-running measurements.
    # Baseline binaries are identified separately by their preserved binary hash.
    native_sources = {str(p.relative_to(root)): sha(p) for p in sorted((root/'src').rglob('*.rs'))}
    driver_files = ['benches/decode.rs', 'benches/run.py', 'benches/build.sh',
                    'benches/oracle/CMakeLists.txt', 'benches/oracle/benchmark.cpp', 'benches/build_native.py',
                    'benches/bench_host.py', 'benches/libflac/CMakeLists.txt',
                    'benches/libflac/benchmark.cpp', 'benches/libflac/opaque.c']
    driver_sources = {name: sha(root/name) for name in driver_files}
    set_affinity(args.cpu)
    paths = []
    for group, selected in [('subset', DEFAULT_SUBSET), ('uncommon', DEFAULT_UNCOMMON)]:
        for path in sorted((args.corpus / group).glob('*.flac')):
            if args.case and not any(f'{group}/{path.name}'.startswith(prefix) for prefix in args.case):
                continue
            if args.all or args.case or int(path.name[:2]) in selected:
                if path.read_bytes()[:4] == b'fLaC':
                    paths.append(path.resolve())
    if args.quick:
        paths = [p for p in paths if p.parent.name == 'subset' and int(p.name[:2]) in [1, 3, 15, 17, 28, 31, 44]]
    rng = random.Random(9639)
    rows, errors = [], []
    case_info = []
    needs_ogg = any('ogg' in mode for mode in args.modes)

    def is_crc(mode):
        return mode in ['crc', 'baseline-crc']

    def execute(mode, path, iterations, index, create_index=False):
        if mode.startswith(('libflac', 'ffmpeg')):
            input_path = index.with_suffix('.oga') if 'ogg' in mode else path
            command = [str(args.oracle), mode, str(input_path), str(index), str(iterations)]
        else:
            executable = args.baseline if mode.startswith('baseline-') else args.native
            assert executable is not None, 'baseline mode needs --baseline'
            command = [str(executable), mode.removeprefix('baseline-'), str(path), str(iterations)]
            if create_index:
                command.append(str(index))
                if needs_ogg:
                    command.append(str(index.with_suffix('.oga')))
            elif 'ogg' in mode:
                command += [str(index), str(index.with_suffix('.oga'))]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            return None, {'mode': mode, 'file': str(path), 'exit_code': result.returncode,
                          'stderr': result.stderr, 'arguments': command}
        fields = result.stdout.strip().split('\t')
        assert len(fields) == 9, result.stdout
        return dict(zip(['mode', 'iterations', 'ns', 'units', 'checksum', 'bytes', 'frames', 'bits', 'channels'],
                        [mode] + list(map(int, fields[1:])))), None

    for path in paths:
        case = f'{path.parent.name}/{path.name}'
        index = args.output / (sha(path) + '.index')
        base, failure = execute('frames', path, 1, index, True)
        assert not failure, failure
        details = {'case': case, 'sha256': sha(path), 'bytes': path.stat().st_size,
                   'frames': base['frames'], 'bits': base['bits'], 'channels': base['channels']}
        if needs_ogg:
            ogg = index.with_suffix('.oga')
            details.update(ogg_sha256=sha(ogg), ogg_bytes=ogg.stat().st_size)
        case_info.append(details)
        startup = None
        if any('startup' in mode for mode in args.modes):
            startup, error = execute('startup', path, 1, index)
            assert not error, error
        cells = []
        for mode in args.modes:
            pilot, error = execute(mode, path, 1, index)
            if error:
                errors.append(error)
                print(f'UNSUPPORTED {case}: {mode}', flush=True)
                continue
            if not is_crc(mode):
                expected = startup if 'startup' in mode else base
                assert pilot['units'] == expected['units'], (case, mode, 'sample count')
                assert pilot['checksum'] == expected['checksum'], (case, mode, 'PCM endpoint checksum')
            iterations = max(1, min(100000, round(args.milliseconds * 1e6 / max(1, pilot['ns']))))
            cells.append((mode, iterations))
        for trial in range(args.rounds):
            rng.shuffle(cells)
            for mode, iterations in cells:
                value, error = execute(mode, path, iterations, index)
                assert error is None, error
                if not is_crc(mode):
                    expected = startup if 'startup' in mode else base
                    assert value['units'] == expected['units'] * iterations, (case, mode)
                    assert value['checksum'] == expected['checksum'] * iterations % (1 << 64), (case, mode)
                value.update(case=case, trial=trial, ns_per_unit=value['ns']/(value['units'] or iterations),
                             ns_per_iteration=value['ns']/iterations,
                             unit=('iteration' if not value['units'] else
                                   'encoded byte' if is_crc(mode) else 'channel sample'))
                rows.append(value)
        print(f'{case}: {len(cells)} modes x {args.rounds} rounds', flush=True)
        with (args.output / 'raw.tsv').open('w', newline='') as output:
            writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter='\t')
            writer.writeheader()
            writer.writerows(rows)
    summary = []
    for case in case_info:
        for mode in args.modes:
            group = [r for r in rows if r['case'] == case['case'] and r['mode'] == mode]
            if group:
                values = [r['ns_per_unit'] for r in group]
                summary.append({'case': case['case'], 'mode': mode, 'median_ns_per_unit': statistics.median(values),
                                'min_ns_per_unit': min(values), 'max_ns_per_unit': max(values),
                                'median_ns_per_iteration': statistics.median(r['ns_per_iteration'] for r in group)})
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    (args.output / 'errors.json').write_text(json.dumps(errors, indent=2) + '\n')
    manifest = {'timestamp_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                'native': str(args.native), 'native_sha256': sha(args.native),
                'oracle': str(args.oracle), 'oracle_sha256': sha(args.oracle),
                'baseline': str(args.baseline), 'baseline_sha256': sha(args.baseline) if args.baseline else None,
                'cpu': args.cpu, 'rounds': args.rounds, 'target_milliseconds': args.milliseconds,
                'cpuinfo': cpu_info(),
                'native_sources': native_sources, 'driver_sources': driver_sources,
                'cases': case_info, 'unsupported': errors}
    for label, binary in [('native_build', args.native), ('baseline_build', args.baseline)]:
        sidecar = Path(str(binary)+'.json') if binary else None
        manifest[label] = json.loads(sidecar.read_text()) if sidecar and sidecar.is_file() else None
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
