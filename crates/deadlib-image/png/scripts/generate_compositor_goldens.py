"""Independent libpng/Fraction RGBA16 answers for generated precision APNGs.

Decode original frame bytes through C libpng, then apply exact rational OVER,
full-canvas snapshots and immediate disposal in the existing independent model.
No native Rust compositor or encoder is invoked. Output must be fresh.
"""
from argparse import ArgumentParser
from pathlib import Path
import hashlib
import json
import tempfile
import zlib
import generate_apng_render_goldens as reference


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--oracle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir()
    paths = sorted(args.input.glob('format-*.png'))
    assert len(paths) == 240
    manifest = {
        'version': 1,
        'oracle_sha256': hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
        'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'reference_model_sha256': hashlib.sha256(Path(reference.__file__).read_bytes()).hexdigest(),
        'rounding': 'Exact Python Fraction straight-alpha OVER, nearest integer RGBA16 after every display; full-canvas snapshots with immediate disposal',
        'files': [],
    }
    with tempfile.TemporaryDirectory(prefix='deadlib-compositor-reference-') as directory:
        assert Path(directory).resolve().parent.samefile(tempfile.gettempdir())
        scratch = Path(directory) / 'frame.png'
        for path in paths:
            data, info = reference.generate(path, args.oracle.resolve(), scratch)
            output = args.output / (path.stem + '.rgba16.z')
            output.write_bytes(zlib.compress(data, 9))
            manifest['files'].append({
                'input': path.name, 'input_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                'golden': output.name, 'sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
                'uncompressed_bytes': len(data), **info,
            })
    assert sum(row['frames'] for row in manifest['files']) == 1920
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8', newline='\n')
    print('Generated 240 independent precision answers / 1920 displayed RGBA16 canvases', flush=True)


if __name__ == '__main__':
    main()
