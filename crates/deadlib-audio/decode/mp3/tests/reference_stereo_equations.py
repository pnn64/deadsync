"""Direct-equation oracle for the independently bit-written stereo survey.

Expected spectra derive from each input's isolated coefficient and transmitted
pan, not native output. Complete stateful PCM follows the MPEG matrix equations.
"""
import argparse, hashlib, json, math, re, subprocess
from pathlib import Path
import numpy as np
from reference_pcm import frames
from reference_stereo import ordering


class Equations:
    def __init__(self, window):
        self.window = window
        self.long = np.cos(np.pi/72*(2*np.arange(36)[:, None]+19)*(2*np.arange(18)[None, :]+1))
        self.short = np.cos(np.pi/24*(2*np.arange(12)[:, None]+7)*(2*np.arange(6)[None, :]+1))
        self.lw = np.sin(np.pi/36*(np.arange(36)+.5))
        self.sw = np.sin(np.pi/12*(np.arange(12)+.5))
        self.matrix = np.cos(np.pi/64*(16+np.arange(64)[:, None])*(2*np.arange(32)[None, :]+1))
        self.indices = np.array([128*(n//64)+(n%64 if n%64<32 else n%64+64) for n in range(512)])
        ci = np.array([-.6, -.535, -.33, -.185, -.095, -.041, -.0142, -.0037])
        self.cs = 1/np.sqrt(1+ci*ci)
        self.ca = ci*self.cs
        self.overlap = np.zeros((2, 32, 18))
        self.history = np.zeros((2, 1024))

    def granule(self, spectra, long_subbands):
        for boundary in range(18, long_subbands*18, 18):
            a = boundary-1-np.arange(8); b = boundary+np.arange(8)
            up = spectra[:, a].copy(); down = spectra[:, b].copy()
            spectra[:, a] = up*self.cs-down*self.ca
            spectra[:, b] = down*self.cs+up*self.ca
        spectra = spectra.reshape(2, 32, 18)
        transformed = np.zeros((2, 32, 36))
        transformed[:, :long_subbands] = (spectra[:, :long_subbands]@self.long.T)*self.lw
        for w in range(3):
            transformed[:, long_subbands:, 6+6*w:18+6*w] += (spectra[:, long_subbands:, w::3]@self.short.T)*self.sw
        hybrid = transformed[:, :, :18]+self.overlap
        self.overlap = transformed[:, :, 18:].copy()
        hybrid[:, 1::2, 1::2] *= -1
        pcm = np.empty((576, 2))
        for t in range(18):
            self.history[:, 64:] = self.history[:, :-64].copy()
            self.history[:, :64] = hybrid[:, :, t]@self.matrix.T
            pcm[32*t:32*t+32] = ((self.history[:, self.indices]*self.window).reshape(2, 16, 32).sum(axis=1)).T
        return pcm.reshape(-1)


def spectrum(case, order, shape, mpeg1, mode, scale):
    """The survey has at most one known coefficient per encoded channel.

    Therefore eligibility is an inequality on the two known band/window labels;
    no decoder search for zero bounds or reconstructed spectra is reused.
    """
    pan, left, right = case
    result = np.zeros((2, 576))
    if left is None and right is None:
        return result
    r = order[right] if right is not None else None
    if left is not None:
        result[0, order[left][2]] = .5
    if right is not None:
        band, win, freq = r
        result[1, freq] = .5 if band == (21 if win < 0 else 12) else .5*2**(-pan/2)
    for coded in {n for n in [left, right] if n is not None}:
        band, win, freq = order[coded]
        if win < 0:
            bound = r[0] if r and r[1] < 0 else 22 if r else -1
        else:
            bound = r[0] if r and r[1] == win else (2 if shape == 'mixed' else -1)
        eligible = band > bound
        top = band == (21 if win < 0 else 12)
        top_only = top and bound == band-1
        position = (3 if mpeg1 else 0) if top_only else pan
        if eligible and position != 7:
            if mpeg1:
                angle = position*math.pi/12
                gains = np.array([math.sin(angle), math.cos(angle)])/(math.sin(angle)+math.cos(angle))
            else:
                gains = np.ones(2)
                if position:
                    gains[0 if position % 2 else 1] = (2**(-.5 if scale else -.25))**((position+1)//2)
            result[:, freq] = result[0, freq]*gains
        elif mode == 3:
            mid, side = result[:, freq].copy()
            result[:, freq] = [(mid+side)/math.sqrt(2), (mid-side)/math.sqrt(2)]
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--native', required=True)
    for name in ['corpus', 'dewindow', 'output']:
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--wsl-distro')
    p.add_argument('--case', action='append')
    args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    tests = Path(__file__).resolve().parent
    bands = json.loads((tests/'data/bands.json').read_text())['rates']
    assert sha(args.dewindow) == json.loads((tests/'data/dsp.json').read_text())['dewindow_sha256']
    window = np.array([float(v) for _, v in re.findall(r'D\[\s*(\d+)\s*\]\s*=\s*([-+0-9.]+)', args.dewindow.read_text())])
    generation = json.loads((args.corpus/'generation.json').read_text())
    assert generation['bands_sha256'] == sha(tests/'data/bands.json')
    def toolpath(path):
        path = path.resolve()
        return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:] if args.wsl_distro else str(path)
    prefix = ['wsl', '-d', args.wsl_distro, '--'] if args.wsl_distro else []
    records = []; limits = {'rms': 2e-6, 'max': 2e-5}
    for entry in generation['records']:
        if args.case and not any(entry['file'].startswith(case) for case in args.case):
            continue
        source = args.corpus/entry['file']; assert sha(source) == entry['sha256']
        rate, shape, mode, scale = source.stem.split('-')
        rate = int(rate); mode = int(mode); scale = int(scale); mpeg1 = rate >= 32000
        order = ordering(bands[str(rate)], shape, mpeg1)
        dsp = Equations(window)
        long_subbands = 32 if shape == 'long' else (4 if rate == 8000 else 2) if shape == 'mixed' else 0
        expected = []
        for case in entry['cases']+[(7, None, None)]*2:
            expected.append(dsp.granule(spectrum(case, order, shape, mpeg1, mode, scale), long_subbands))
            if mpeg1:
                expected.append(dsp.granule(np.zeros((2, 576)), long_subbands))
        expected = np.concatenate(expected)
        target = args.output/(source.stem+'.bin')
        command = prefix+[args.native, 'native', toolpath(source), toolpath(target)]
        run = subprocess.run(command, capture_output=True, text=True); assert not run.returncode, (command, run.stderr)
        actual = frames(target)
        assert len(actual) == entry['frames']
        assert all(h == (rate, 2, 1152 if mpeg1 else 576) for h, _ in actual)
        actual = np.concatenate([pcm for _, pcm in actual]); assert len(actual) == len(expected)
        delta = actual-expected
        record = {'file': source.name, 'input_sha256': sha(source), 'samples': len(actual),
                  'rms': float(np.sqrt(np.mean(delta*delta))), 'max': float(np.max(np.abs(delta))),
                  'command': command, 'pcm_sha256': sha(target)}
        record['passed'] = record['rms'] <= limits['rms'] and record['max'] <= limits['max']
        records.append(record)
        (args.output/'results.json').write_text(json.dumps({'method': __doc__, 'limits': limits, 'records': records}, indent=2)+'\n')
        print(source.name, record['passed'], record['rms'], record['max'], flush=True)
        assert record['passed'], record
    binary_sha = subprocess.check_output(prefix+['sha256sum', args.native], text=True).split()[0] if prefix else sha(Path(args.native))
    proof = {'passed': bool(records) and all(r['passed'] for r in records), 'files': len(records),
             'sample_values': sum(r['samples'] for r in records), 'native_sha256': binary_sha,
             'script_sha256': sha(Path(__file__)), 'numpy': np.__version__}
    (args.output/'provenance.json').write_text(json.dumps(proof, indent=2)+'\n'); assert proof['passed']


if __name__ == '__main__':
    main()
