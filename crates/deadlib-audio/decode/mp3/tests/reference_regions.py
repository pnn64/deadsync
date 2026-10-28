"""Distinct-codebook switched regions, with independently known spectra/PCM.

The 1997 Layer3 description counts eight band entries in mixed region zero:
eight long MPEG-1 bands, or six LSF long bands and two short-window bands.
Expected PCM comes from the direct matrix/shift-history oracle, never a decoder.
Reference disagreements are recorded separately from native correctness.
"""
import argparse, csv, hashlib, json, re, subprocess
from pathlib import Path
import numpy as np
from reference_huffman import Bits
from reference_pcm import frames
from reference_stereo import ordering
from reference_stereo_equations import Equations


def frame(rate, mixed, boundary, books, line, negative, codes):
    mpeg1 = rate >= 32000
    version = 3 if mpeg1 else 2 if rate >= 16000 else 0
    rates = [44100, 48000, 32000]
    index = [n // (1 if mpeg1 else 2 if version == 2 else 4) for n in rates].index(rate)
    main = Bits(); big = 0 if line is None else line // 2 + 1
    for pair in range(big):
        x, y = int(pair*2 == line), int(pair*2+1 == line)
        code, width = codes[books[int(pair*2 >= boundary)], x, y]
        main.put(code, width)
        if x or y:
            main.put(negative, 1)
    side = Bits(); side.put(0, 9 if mpeg1 else 8); side.put(0, 5 if mpeg1 else 1)
    if mpeg1:
        side.put(0, 4)
    for gr in range(2 if mpeg1 else 1):
        for value, width in [(len(main.bits) if gr == 0 else 0, 12),
                             (big if gr == 0 else 0, 9), (206, 8),
                             (0, 4 if mpeg1 else 9), (1, 1), (2, 2), (mixed, 1),
                             (books[0], 5), (books[1], 5), (0, 3), (0, 3), (0, 3)]:
            side.put(value, width)
        if mpeg1:
            side.put(0, 1)
        side.put(0, 1); side.put(0, 1)
    assert len(side.bits) == (136 if mpeg1 else 72)
    word = (2047 << 21) | (version << 19) | (1 << 17) | (1 << 16) | (14 << 12) | (index << 10) | (3 << 6)
    size = (144 if mpeg1 else 72) * (320000 if mpeg1 else 160000) // rate
    data = word.to_bytes(4, 'big') + side.data() + main.data()
    assert len(data) <= size
    return data + bytes(size-len(data))


def main():
    p = argparse.ArgumentParser()
    for arg in ['output', 'dewindow']:
        p.add_argument('--'+arg, type=Path, required=True)
    p.add_argument('--native', required=True); p.add_argument('--oracle')
    p.add_argument('--wsl-distro')
    args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
    tests = Path(__file__).resolve().parent
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    bands = json.loads((tests/'data/bands.json').read_text())['rates']
    codes = {(int(r['book']), int(r['x']), int(r['y'])): (int(r['code']), int(r['bits']))
             for r in csv.DictReader((tests/'data/huffman.csv').open())}
    assert sha(args.dewindow) == json.loads((tests/'data/dsp.json').read_text())['dewindow_sha256']
    window = np.array([float(v) for _, v in re.findall(r'D\[\s*(\d+)\s*\]\s*=\s*([-+0-9.]+)', args.dewindow.read_text())])
    prefix = ['wsl', '-d', args.wsl_distro, '--'] if args.wsl_distro else []
    def toolpath(path):
        path = path.resolve()
        return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:] if prefix else str(path)
    limits = {'rms': 2e-6, 'max': 2e-5}; records = []
    for rate_text, tables in bands.items():
        rate = int(rate_text); mpeg1 = rate >= 32000
        for mixed in [False, True]:
            order = ordering(tables, 'mixed' if mixed else 'short', mpeg1)
            band_entries = list(dict.fromkeys((band, win) for band, win, _ in order))
            region_zero = set(band_entries[:8 if mixed else 9])
            boundary = sum((band, win) in region_zero for band, win, _ in order)
            assert boundary == (36 if not mixed and rate != 8000 or mixed and mpeg1 else
                                72 if not mixed else 96 if rate == 8000 else 48)
            previous = 3*tables[1][3]
            lines = sorted({0, 575, *range(previous-2, previous+3), *range(boundary-2, boundary+3)})
            cases = [(books, line, sign) for books in [(1, 2), (2, 1)] for line in lines for sign in [0, 1]]
            dsp = Equations(window); expected = []; data = bytearray()
            subbands = (4 if rate == 8000 else 2) if mixed else 0
            for books, line, sign in cases:
                for value in [line, None, None]:
                    data += frame(rate, mixed, boundary, books, value, sign, codes)
                    spectrum = np.zeros((2, 576))
                    if value is not None:
                        spectrum[0, order[value][2]] = -.5 if sign else .5
                    expected.append(dsp.granule(spectrum, subbands)[::2])
                    if mpeg1:
                        expected.append(dsp.granule(np.zeros((2, 576)), subbands)[::2])
            expected = np.concatenate(expected)
            source = args.output/f'{rate}-{"mixed" if mixed else "short"}.mp3'; source.write_bytes(data)
            record = {'file': source.name, 'input_sha256': sha(source), 'boundary': boundary,
                      'cases': cases, 'sample_values': len(expected), 'results': {}}
            for mode in ['native', 'symphonia'] + (['mad', 'mpg123'] if args.oracle else []):
                binary = args.native if mode in ['native', 'symphonia'] else args.oracle
                target = source.with_name(source.stem+'-'+mode+'.bin')
                command = prefix+[binary, mode, toolpath(source), toolpath(target)]
                run = subprocess.run(command, capture_output=True, text=True)
                result = {'command': command, 'exit_code': run.returncode, 'stderr': run.stderr, 'passed': False}
                if not run.returncode:
                    decoded = frames(target)
                    assert all(h == (rate, 1, 1152 if mpeg1 else 576) for h, _ in decoded)
                    actual = np.concatenate([pcm for _, pcm in decoded]); assert len(actual) == len(expected)
                    delta = actual-expected
                    result.update(rms=float(np.sqrt(np.mean(delta*delta))), max=float(np.max(np.abs(delta))), pcm_sha256=sha(target))
                    result['passed'] = result['rms'] <= limits['rms'] and result['max'] <= limits['max']
                record['results'][mode] = result
            records.append(record)
            (args.output/'results.json').write_text(json.dumps({'method': __doc__, 'limits': limits, 'records': records}, indent=2)+'\n')
            print(source.name, {mode: (r['passed'], r.get('max')) for mode, r in record['results'].items()}, flush=True)
    binaries = {name: subprocess.check_output(prefix+['sha256sum', binary], text=True).split()[0]
                if prefix else sha(Path(binary)) for name, binary in [('native', args.native), ('oracle', args.oracle)] if binary}
    proof = {'passed': all(r['results']['native']['passed'] for r in records), 'files': len(records),
             'sample_values': sum(r['sample_values'] for r in records), 'binaries': binaries,
             'script_sha256': sha(Path(__file__)), 'numpy': np.__version__}
    (args.output/'provenance.json').write_text(json.dumps(proof, indent=2)+'\n')
    assert proof['passed'], 'native decoding differs from independently encoded coefficients'


if __name__ == '__main__':
    main()
