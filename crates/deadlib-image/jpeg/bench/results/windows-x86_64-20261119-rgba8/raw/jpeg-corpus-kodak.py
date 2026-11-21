"""Generate the predeclared Kodak JPEG supplement with installed Pillow only.

No download, package installation, native decoder run or benchmark is performed.
Source PNG hashes must match the repository's already pinned Kodak manifest.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import importlib.util
import inspect
import io
import json
import pathlib
import platform
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(__file__).resolve().parent
PHOTO_DIR = OUT / 'photos'
SOURCE_DIR = ROOT / 'target/png-kodak-20261117'
SOURCE_MANIFEST = ROOT / 'crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8/raw/kodak.json'
SPECIAL = {1, 6, 13, 23}

def sha256(data):
    return hashlib.sha256(data).hexdigest()

def relative(path):
    return path.relative_to(ROOT).as_posix()

def pillow_environment():
    try:
        import PIL
        from PIL import Image, JpegImagePlugin, features
    except ImportError as error:
        raise SystemExit(f'Pillow unavailable; no installation attempted: {error}')
    source = inspect.getsource(JpegImagePlugin._save)
    environment = dict(python=platform.python_version(), platform=platform.platform(),
                       pillow=PIL.__version__, libjpeg=features.version_codec('jpg'),
                       libjpeg_turbo=features.version_feature('libjpeg_turbo') if features.check_feature('libjpeg_turbo') else None,
                       keep_rgb_option_present='keep_rgb' in source,
                       jpeg_plugin_sha256=sha256(pathlib.Path(JpegImagePlugin.__file__).read_bytes()),
                       pillow_imaging_sha256=sha256(pathlib.Path(Image.core.__file__).read_bytes()))
    environment['jpeg_save_option_lines'] = [line.strip() for line in source.splitlines()
                                             if any(word in line for word in ('quality', 'subsampling', 'progressive', 'optimize', 'keep_rgb'))]
    return Image, environment

def inspect_module():
    spec = importlib.util.spec_from_file_location('jpeg_corpus_inventory', OUT / 'jpeg-corpus-inventory.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--inspect', action='store_true', help='Print installed encoder details; do not generate files.')
    args = parser.parse_args()
    Image, environment = pillow_environment()
    if args.inspect:
        print(json.dumps(environment, indent=2))
        return
    manifest_bytes = SOURCE_MANIFEST.read_bytes()
    manifest = json.loads(manifest_bytes)
    originals = {entry['file']: entry for entry in manifest['files']}
    expected_names = [f'kodim{index:02d}.png' for index in range(1, 25)]
    if set(expected_names) != set(originals):
        raise SystemExit('Source manifest is not exactly the expected 24 Kodak PNG files.')
    PHOTO_DIR.mkdir(exist_ok=True)
    inventory = inspect_module()
    outputs = []
    sources = []
    unavailable = []
    if not environment['keep_rgb_option_present']:
        unavailable.append('Installed Pillow has no keep_rgb option; the four encoded-RGB cases were not generated.')
    for index, filename in enumerate(expected_names, start=1):
        path = SOURCE_DIR / filename
        source_bytes = path.read_bytes()
        original = originals[filename]
        if len(source_bytes) != original['bytes'] or sha256(source_bytes) != original['sha256']:
            raise SystemExit(f'Source hash/length mismatch: {path}')
        with Image.open(io.BytesIO(source_bytes)) as opened:
            opened.load()
            if opened.mode != 'RGB':
                raise SystemExit(f'Unexpected source mode for {path}: {opened.mode}')
            rgb = Image.frombytes('RGB', opened.size, opened.tobytes())
        sources.append(dict(path=relative(path), url=original['url'], sha256=original['sha256'],
                            bytes=len(source_bytes), mode='RGB', width=rgb.width, height=rgb.height))
        variants = [('baseline-420-q85', 'RGB', dict(subsampling=2, progressive=False)),
                    ('progressive-420-q85', 'RGB', dict(subsampling=2, progressive=True))]
        if index in SPECIAL:
            variants += [('baseline-gray-q85', 'L', dict(subsampling=0, progressive=False)),
                         ('baseline-cmyk-444-q85', 'CMYK', dict(subsampling=0, progressive=False))]
            if environment['keep_rgb_option_present']:
                variants.append(('baseline-rgb-444-q85', 'RGB', dict(subsampling=0, progressive=False, keep_rgb=True)))
        for label, mode, settings in variants:
            settings = dict(quality=85, optimize=False, exif=b'', icc_profile=b'', **settings)
            image = rgb if mode == 'RGB' else rgb.convert(mode)
            output = PHOTO_DIR / f'kodim{index:02d}-{label}.jpg'
            encoded = io.BytesIO()
            image.save(encoded, format='JPEG', **settings)
            data = encoded.getvalue()
            if output.exists() and output.read_bytes() != data:
                raise SystemExit(f'Existing generated bytes differ; retained without overwriting: {output}')
            output.write_bytes(data)
            item = inventory.inspect(output)
            if item.get('header_error') or item.get('precision') != 8 or not item['eoi']:
                raise SystemExit(f'Unexpected generated JPEG structure: {output}')
            expected_color = 'RGB' if 'keep_rgb' in settings else ('gray' if mode == 'L' else 'CMYK' if mode == 'CMYK' else 'YCbCr')
            expected_sampling = '1x1' if mode == 'L' else ','.join(['1x1'] * (4 if mode == 'CMYK' else 3)) if settings['subsampling'] == 0 else '2x2,1x1,1x1'
            expected_process = 'progressive' if settings['progressive'] else 'baseline'
            if (item.get('color_hint'), item.get('sampling'), item.get('process')) != (expected_color, expected_sampling, expected_process):
                raise SystemExit(f'Encoder did not honor declared color/sampling/process: {output}')
            item.update(group='kodak-' + label, source_path=relative(path), source_sha256=original['sha256'],
                        source_url=original['url'], source_mode='RGB', encoder_mode=mode,
                        conversion='Pillow RGB.convert(' + repr(mode) + ')' if mode != 'RGB' else 'none',
                        encoder_options={key: (value.hex() if isinstance(value, bytes) else value) for key, value in settings.items()},
                        metadata='Source metadata discarded; explicit empty EXIF and ICC; generated codec headers only.')
            outputs.append(item)
    outputs.sort(key=lambda item: item['path'])
    supplement = dict(schema=1, status='Generated before decoder performance measurements; no timing-based filtering.',
                       baseline_commit='9a6f4ebabe71934202b55df110dae03883ca512c',
                       policy='All 24 pinned Kodak PNGs at quality 85 in baseline and progressive 4:2:0; photos 01,06,13,23 additionally baseline gray, CMYK and supported keep-RGB 4:4:4.',
                       environment=environment, source_manifest=dict(path=relative(SOURCE_MANIFEST), sha256=sha256(manifest_bytes),
                                                                      source=manifest['source'], retrieved=manifest['retrieved']),
                       generator=dict(path=relative(pathlib.Path(__file__)), sha256=sha256(pathlib.Path(__file__).read_bytes())),
                       sources=sources, inputs=outputs, unavailable=unavailable,
                       generated_bytes=sum(item['bytes'] for item in outputs),
                       groups=dict(collections.Counter(item['group'] for item in outputs)))
    (OUT / 'jpeg-corpus-kodak.json').write_text(json.dumps(supplement, indent=2) + '\n', encoding='utf-8')
    (OUT / 'jpeg-corpus-kodak.txt').write_text(''.join(item['path'] + '\n' for item in outputs), encoding='utf-8')
    print(json.dumps(dict(files=len(outputs), generated_bytes=supplement['generated_bytes'], groups=supplement['groups'],
                          unavailable=unavailable, environment=environment), indent=2))

if __name__ == '__main__':
    main()
