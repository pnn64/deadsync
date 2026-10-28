"""Full historical file PCM/count/seek gate with bounded-memory comparisons."""
import argparse, hashlib, json, math, struct, subprocess
from pathlib import Path
import numpy as np


def pcm(path, start=0, count=None):
    with path.open('rb') as data:
        assert data.read(8) in [b'DLMP3PC1', b'DLMP3FI1']
        while header := data.read(12):
            rate, channels, samples = struct.unpack('<III', header)
            assert rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000]
            assert channels in [1, 2] and 0 < samples <= 2_097_152
            raw = data.read(samples*channels*4); assert len(raw) == samples*channels*4
            if start >= samples:
                start -= samples
                continue
            values = np.frombuffer(raw, dtype='<f4').reshape(samples, channels)[start:]
            start = 0
            if count is not None:
                values = values[:count]
                count -= len(values)
            if len(values):
                yield (rate, channels), values.reshape(-1)
            if count == 0:
                return
        assert start == 0 and (count is None or count == 0), 'insufficient PCM'


def compare(left, right):
    left = iter(left); right = iter(right)
    a = b = None; total = 0; energy = 0.; peak = 0.; exact = True
    while True:
        if a is None or not len(a):
            block = next(left, None); spec_a, a = block if block else (None, None)
        if b is None or not len(b):
            block = next(right, None); spec_b, b = block if block else (None, None)
        if a is None or b is None:
            assert a is None and b is None, 'PCM lengths differ'
            break
        assert spec_a == spec_b
        size = min(len(a), len(b)); x = a[:size]; y = b[:size]
        assert np.all(np.isfinite(x)) and np.all(np.isfinite(y))
        delta = x.astype(np.float64)-y
        energy += float(delta@delta); peak = max(peak, float(np.max(np.abs(delta))))
        exact &= bool(np.array_equal(x.view('<u4'), y.view('<u4')))
        total += size; a = a[size:]; b = b[size:]
    return {'sample_values': total, 'rms': math.sqrt(energy/max(1, total)), 'max': peak, 'bit_identical': exact}


def describe(path):
    blocks = 0; samples = 0; format = None
    for spec, values in pcm(path):
        if format is None:
            format = spec
        assert format == spec
        blocks += 1; samples += len(values)//spec[1]
    return {'rate': format[0], 'channels': format[1], 'samples': samples, 'blocks': blocks}


def main():
    p = argparse.ArgumentParser()
    for name in ['manifest', 'corpus', 'output']:
        p.add_argument('--'+name, type=Path, required=True)
    for name in ['native', 'mpg123', 'symphonia', 'mad']:
        p.add_argument('--'+name, required=True)
    p.add_argument('--wsl-distro')
    args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    prefix = ['wsl', '-d', args.wsl_distro, '--'] if args.wsl_distro else []
    def toolpath(path):
        path = path.resolve()
        return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:] if prefix else str(path)
    commands = []
    def run(binary, *arguments):
        command = prefix+[binary, *arguments]
        result = subprocess.run(command, capture_output=True, text=True)
        record = {'command': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
        commands.append(record); assert not result.returncode, record
        return result.stdout
    def check(delta):
        assert delta['rms'] <= 2e-6 and delta['max'] <= 2e-5, delta
        return delta
    records = []
    for entry in json.loads(args.manifest.read_text())['files']:
        if entry['role'] != 'valid_music':
            continue
        source = args.corpus/entry['file']; assert sha(source) == entry['sha256']
        native = args.output/(source.stem+'-native.bin')
        mpg = args.output/(source.stem+'-mpg123.bin')
        sym = args.output/(source.stem+'-symphonia.bin')
        mad = args.output/(source.stem+'-mad-raw.bin')
        first_command = len(commands)
        stdout = run(args.native, toolpath(source), toolpath(native), 'gapless', 'audit')
        start, end, total, raw, physical = map(int, next(line.split('\t')[1:] for line in stdout.splitlines() if line.startswith('range\t')))
        audit = next(line.split('\t')[1:] for line in stdout.splitlines() if line.startswith('seeks\t'))
        assert audit[-1] == 'bit-identical'
        spec = describe(native); assert spec['samples'] == total and end-start == total
        run(args.mpg123, toolpath(source), toolpath(mpg))
        run(args.symphonia, toolpath(source), toolpath(sym))
        run(args.mad, 'mad', toolpath(source), toolpath(mad))
        raw_spec = describe(mad); assert raw_spec['samples'] == raw and raw_spec['blocks'] == physical
        linear = {name: check(compare(pcm(native), pcm(path))) for name, path in [('mpg123', mpg), ('symphonia', sym)]}
        linear['libmad_cropped_raw'] = check(compare(pcm(native), pcm(mad, start, total)))
        seeks = []
        random = 0x938914
        positions = {0, 1, 529, 575, 576, 1151, 1152, total//2, total-1, total}
        for _ in range(20):
            random = (random*6364136223846793005+1442695040888963407) % 2**64
            positions.add(random % (total+1))
        for position in sorted(positions):
            length = min(577, total-position)
            target = args.output/(source.stem+'-seek-'+str(position)+'.bin')
            run(args.mpg123, toolpath(source), toolpath(target), str(position), str(length))
            delta = check(compare(pcm(target), pcm(mpg, position, length)))
            native_delta = check(compare(pcm(target), pcm(native, position, length)))
            assert delta['sample_values'] == length*spec['channels']
            seeks.append({'position': position, 'samples': length, 'reference_linear': delta, 'native_linear': native_delta})
        record = {'file': source.name, 'input_sha256': sha(source), 'format': spec, 'raw_samples': raw,
                  'physical_frames': physical, 'raw_start': start, 'raw_end': end, 'linear': linear,
                  'native_seek_checks': int(audit[0]), 'native_seek_sample_values': int(audit[1]),
                  'reference_seeks': seeks, 'commands': commands[first_command:],
                  'pcm_sha256': {path.name: sha(path) for path in [native, mpg, sym, mad]}}
        records.append(record)
        (args.output/'results.json').write_text(json.dumps({'method': __doc__, 'limits': {'rms': 2e-6, 'max': 2e-5}, 'records': records}, indent=2)+'\n')
        print(source.name, total, 'samples,', audit[0], 'native seeks pass', flush=True)
    binaries = {binary: (subprocess.check_output(prefix+['sha256sum', binary], text=True).split()[0] if prefix else sha(Path(binary))) for binary in [args.native, args.mpg123, args.symphonia, args.mad]}
    proof = {'passed': bool(records), 'files': len(records), 'binaries': binaries,
             'script_sha256': sha(Path(__file__)), 'manifest_sha256': sha(args.manifest), 'numpy': np.__version__}
    (args.output/'provenance.json').write_text(json.dumps(proof, indent=2)+'\n'); assert proof['passed']


if __name__ == '__main__':
    main()
