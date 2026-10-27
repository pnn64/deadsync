"""Compare resident-PCM encoding, source error and independent decoder parity."""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tomllib
import uuid
from benchmark import DECODE, REPO, PINS, label, registry_packages, rust_worker, sha, baseline_worker


def invoke(command, log=None):
    result = subprocess.run([str(a) for a in command], cwd=REPO, capture_output=True)
    if log:
        log.write_bytes(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {command}\n{result.stderr.decode(errors='replace')[-6000:]}")
    return result.stdout.decode('utf-8-sig')


def write_json(path, value):
    path.write_bytes((json.dumps(value, indent=2, allow_nan=False) + '\n').encode())


def write_tsv(path, rows):
    with path.open('w', encoding='utf-8', newline='') as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(rows)


def packet_geometry(path):
    data = path.read_bytes()
    offset, current, sizes, pages = 0, 0, [], 0
    while offset < len(data):
        if data[offset:offset + 5] != b'OggS\0':
            raise ValueError('invalid output framing')
        laces = data[offset + 27:offset + 27 + data[offset + 26]]
        for size in laces:
            current += size
            if size < 255:
                sizes.append(current)
                current = 0
        offset += 27 + len(laces) + sum(laces)
        pages += 1
    if offset != len(data) or current or len(sizes) < 4:
        raise ValueError('incomplete output packets')
    return {'pages': pages, 'audio_packets': len(sizes) - 3,
            'header_packet_bytes': sum(sizes[:3]), 'audio_packet_bytes': sum(sizes[3:])}


def source_snapshot():
    files = [p for d in ['ogg/src', 'vorbis/src', 'vorbis/benches'] for p in (DECODE / d).rglob('*') if p.is_file()]
    files += [DECODE / p for p in ['ogg/Cargo.toml', 'vorbis/Cargo.toml', 'scripts/benchmark.py', 'scripts/benchmark_encoding.py']]
    files += [REPO / 'Cargo.toml', REPO / 'Cargo.lock']
    if (REPO / '.cargo/config.toml').exists():
        files.append(REPO / '.cargo/config.toml')
    return {label(p): sha(p) for p in sorted(files)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='*')
    parser.add_argument('--run-root', type=Path)
    parser.add_argument('--reference-root', type=Path, default=REPO / 'target/deadlib-audio-reference')
    parser.add_argument('--seconds', type=int, default=5)
    parser.add_argument('--skip-seconds', type=int, default=15)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--min-seconds', type=float, default=0.15)
    parser.add_argument('--seed', type=int, default=4187)
    parser.add_argument('--baseline-revision', help='interleave prior production sources using the exact same encoding adapter')
    args = parser.parse_args()
    if not (args.seconds > 0 and args.skip_seconds >= 0 and args.repeats > 0 and math.isfinite(args.min_seconds) and args.min_seconds > 0):
        parser.error('invalid duration/repeat limits')
    run = (args.run_root or REPO / 'target/deadlib-audio-reference' / ('encoding-' + uuid.uuid4().hex)).resolve()
    run.mkdir(parents=True, exist_ok=False)
    reference = args.reference_root.resolve()
    for name, pin in PINS.items():
        actual = invoke(['git', '-C', reference / name, 'rev-parse', 'HEAD']).strip()
        if actual != pin or invoke(['git', '-C', reference / name, 'status', '--porcelain', '--untracked-files=no']).strip():
            raise RuntimeError(f'reference {name} must be the clean pinned source')
    build = invoke(['cargo', 'bench', '--locked', '-p', 'deadlib-audio-decode-vorbis', '--bench', 'encoding', '--no-run', '--message-format=json'], run / 'rust-build.log')
    rust, profile = rust_worker(build, 'encoding')
    baseline, baseline_metadata = None, None
    if args.baseline_revision:
        baseline, baseline_metadata = baseline_worker(args.baseline_revision, run, 'encoding')
        if baseline_metadata['cargo_artifact_profile'] != profile or baseline_metadata['registry_packages'] != registry_packages(build):
            raise RuntimeError('baseline/current Cargo profiles or registry artifacts differ')
        if baseline_metadata['adapter_adjustments']:
            raise RuntimeError('encoding comparison requires identical adapters')
    cbuild = reference / 'vorbis-encoding-build'
    configure = ['cmake', '-S', DECODE / 'vorbis/benches/oracle', '-B', cbuild,
                 f'-DREFERENCE_ROOT={reference}', '-DCMAKE_BUILD_TYPE=Release', '-DDEADLIB_TRACK_ALLOCATIONS=OFF']
    if os.name == 'nt':
        configure += ['-A', 'x64']
    invoke(configure, run / 'c-configure.log')
    invoke(['cmake', '--build', cbuild, '--config', 'Release', '--target', 'deadlib_vorbis_encoding', '--parallel', '4'], run / 'c-build.log')
    c = cbuild / ('Release/deadlib_vorbis_encoding.exe' if os.name == 'nt' else 'deadlib_vorbis_encoding')
    files = args.files or [DECODE / 'vorbis/tests/fixtures/mono.ogg', DECODE / 'vorbis/tests/fixtures/stereo.ogg',
                          DECODE / 'vorbis/tests/fixtures/eight-channel.ogg',
                          REPO / 'assets/sounds/boom.ogg', REPO / 'assets/music/in_two (loop).ogg']
    song = reference / 'corpus/itgmania/goin-under.ogg'
    if not args.files and song.exists():
        files.append(song)
    files = [p.resolve() for p in files]
    configs = [('native', str(p)) for p in range(1, 5)] + [('libvorbis', q) for q in ['0', '0.4', '0.9']]
    if baseline:
        configs += [('native-baseline', str(p)) for p in range(1, 5)]
    manifest = {
        'started_utc': datetime.now(timezone.utc).isoformat(), 'git_head': invoke(['git', 'rev-parse', 'HEAD']).strip(),
        'rustc': invoke(['rustc', '-Vv']), 'cmake': invoke(['cmake', '--version']).splitlines()[0],
        'platform': platform.platform(), 'processor': platform.processor(), 'logical_cpus': os.cpu_count(),
        'reference_pins': PINS, 'repeats': args.repeats, 'min_seconds': args.min_seconds, 'seed': args.seed,
        'excerpt_seconds': args.seconds, 'preferred_skip_seconds': args.skip_seconds,
        'binary_paths': {'rust': str(rust), 'libvorbis': str(c)}, 'binary_sha256': {'rust': sha(rust), 'libvorbis': sha(c)},
        'baseline': baseline_metadata,
        'cargo_artifact_profile': profile, 'registry_packages': registry_packages(build),
        'cargo_profiles': tomllib.loads((REPO / 'Cargo.toml').read_text(encoding='utf-8'))['profile'],
        'rustflags_environment': {k: os.environ.get(k) for k in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']},
        'cmake_release_flags': [line for line in (cbuild / 'CMakeCache.txt').read_text().splitlines() if line.startswith(('CMAKE_C_FLAGS_RELEASE:', 'CMAKE_CXX_FLAGS_RELEASE:', 'CMAKE_EXE_LINKER_FLAGS_RELEASE:'))],
        'source_sha256': source_snapshot(), 'input_sha256': {label(p): sha(p) for p in files}, 'prepared_inputs': {},
        'configurations': configs,
        'timing_policy': 'resident interleaved f32 input, 4096-frame pushes, fresh encoder per iteration, full native/Xiph Ogg mux into discard sink; one warm run per worker; input/output file IO excluded; uninstrumented',
        'open_policy': 'through setup/metadata/header page generation and flushing; before audio input',
        'first_packet_policy': 'through encoding and mux submission of first audio packet with granule >0; page need not yet be flushed; includes open and configured input batching; supplied frames are the caller chunk boundary, not a minimum algorithmic lookahead claim; report granule separately',
        'quality_policy': 'exact retained frames/channels/rate; source PCM error without alignment or gain fitting; libvorbisfile raw-start and native/Symphonia decoder parity, peak 3e-6 and RMS 5e-7 times max(1, reference_peak)',
        'scope_limit': 'Precision stages and Xiph VBR quality are distinct configurations, not matched perceptual quality or bitrate. Source excerpts are decoded lossy files, not original studio PCM.',
    }
    write_json(run / 'manifest.json', manifest)
    rows, qualities = [], []
    rng = random.Random(args.seed)
    for number, path in enumerate(files):
        pcm = run / f'input-{number}.pcm'
        geometry = json.loads(invoke([rust, 'prepare', path, pcm, args.seconds, args.skip_seconds]))
        c_pcm = run / f'input-{number}-reference.pcm'
        invoke([c, 'extract', path, c_pcm, geometry['start_frame'], geometry['frames']])
        input_parity = json.loads(invoke([rust, 'compare', pcm, c_pcm]))
        geometry.update({'pcm_sha256': sha(pcm), 'reference_pcm_sha256': sha(c_pcm), 'input_parity': input_parity})
        manifest['prepared_inputs'][label(path)] = geometry
        write_json(run / 'manifest.json', manifest)
        def worker(engine, setting, iterations, output='-'):
            executable = baseline if engine == 'native-baseline' else rust if engine == 'native' else c
            actual_engine = 'native' if engine == 'native-baseline' else engine
            command = [executable, 'encode', setting, iterations, pcm, output]
            values = invoke(command).strip().split('\t')
            if len(values) != 12 or values[:2] != [actual_engine, setting]:
                raise RuntimeError(f'invalid worker result: {values}')
            names = ['iterations', 'full_ns', 'open_ns', 'first_ns', 'frames', 'encoded_bytes', 'audio_packets', 'first_input_frames', 'first_granule', 'first_packet_bytes']
            row = {'path': label(path), 'engine': engine, 'setting': setting, **dict(zip(names, map(int, values[2:])))}
            if row['iterations'] != iterations or row['frames'] != geometry['frames'] or not (0 < row['open_ns'] <= row['first_ns'] <= row['full_ns']):
                raise RuntimeError(f'inconsistent worker result: {row}')
            return row
        pilots = {}
        for engine, setting in configs:
            output = run / f'output-{number}-{engine}-{setting}.ogg'
            pilot = worker(engine, setting, 1, output)
            pilots[engine, setting] = min(10000, max(1, math.ceil(args.min_seconds * 1e9 / pilot['full_ns'])))
            reference_pcm = run / f'output-{number}-{engine}-{setting}.pcm'
            invoke([c, 'decode', output, reference_pcm])
            quality = json.loads(invoke([rust, 'verify', pcm, output, reference_pcm]))
            packets = packet_geometry(output)
            if pilot['encoded_bytes'] != output.stat().st_size or pilot['audio_packets'] != packets['audio_packets']:
                raise RuntimeError('discard/output geometry disagreement')
            qualities.append({'path': label(path), 'engine': engine, 'setting': setting,
                              'encoded_bytes': pilot['encoded_bytes'], **packets,
                              'encoded_sha256': sha(output), 'decoded_reference_sha256': sha(reference_pcm),
                              'total_bitrate_bps': pilot['encoded_bytes'] * 8 * geometry['rate'] / geometry['frames'],
                              'audio_bitrate_bps': packets['audio_packet_bytes'] * 8 * geometry['rate'] / geometry['frames'], **quality})
            write_tsv(run / 'quality.tsv', qualities)
        if baseline:
            exact_rows = []
            for setting in map(str, range(1, 5)):
                before = run / f'output-{number}-native-baseline-{setting}.ogg'
                after = run / f'output-{number}-native-{setting}.ogg'
                exact = json.loads(invoke([rust, 'exact', pcm, before, after]))
                before_pcm = run / f'output-{number}-native-baseline-{setting}.pcm'
                after_pcm = run / f'output-{number}-native-{setting}.pcm'
                if before_pcm.read_bytes() != after_pcm.read_bytes():
                    raise RuntimeError('baseline/current libvorbis decoded PCM changed')
                exact_rows.append({'path': label(path), 'setting': setting, 'libvorbis_bit_exact': True, **exact})
            previous = json.loads((run / 'exact.json').read_text()) if (run / 'exact.json').exists() else []
            write_json(run / 'exact.json', previous + exact_rows)
        for trial in range(args.repeats):
            order = list(configs)
            rng.shuffle(order)
            for engine, setting in order:
                row = worker(engine, setting, pilots[engine, setting])
                row['trial'] = trial
                rows.append(row)
            write_tsv(run / 'trials.tsv', rows)
        print(f'{label(path)}: {len(configs)} configurations verified and timed', flush=True)
    summary = []
    for path in files:
        for engine, setting in configs:
            group = [r for r in rows if (r['path'], r['engine'], r['setting']) == (label(path), engine, setting)]
            entry = {k: group[0][k] for k in ['path', 'engine', 'setting', 'frames', 'encoded_bytes', 'audio_packets', 'first_input_frames', 'first_granule', 'first_packet_bytes']}
            if any(any(r[k] != group[0][k] for k in entry) for r in group):
                raise RuntimeError('nondeterministic trial geometry')
            for phase in ['full', 'open', 'first']:
                times = [r[phase + '_ns'] / r['iterations'] for r in group]
                median = statistics.median(times)
                entry.update({phase + '_median_ns': median, phase + '_min_ns': min(times), phase + '_max_ns': max(times),
                              phase + '_mad_ns': statistics.median(abs(t - median) for t in times)})
            summary.append(entry)
    write_tsv(run / 'summary.tsv', summary)
    if source_snapshot() != manifest['source_sha256']:
        raise RuntimeError('benchmark sources/build configuration changed during run')
    for path in files:
        if sha(path) != manifest['input_sha256'][label(path)]:
            raise RuntimeError('source input changed during run')
    if {'rust': sha(rust), 'libvorbis': sha(c)} != manifest['binary_sha256']:
        raise RuntimeError('worker binary changed during run')
    if baseline and sha(baseline) != baseline_metadata['binary_sha256']:
        raise RuntimeError('baseline worker changed during run')
    manifest['completed_utc'] = datetime.now(timezone.utc).isoformat()
    manifest['quality_cases'] = len(qualities)
    manifest['timing_rows'] = len(rows)
    manifest['final_hashes_verified'] = True
    write_json(run / 'manifest.json', manifest)
    print(f'Retained encoding evidence: {run}', flush=True)


if __name__ == '__main__':
    main()
