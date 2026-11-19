"""Build the benchmark as it runs on an x86-64 CPU without AVX2.

The repository stays untouched: a throwaway workspace under
target/jpeg-no-avx2 holds a copy of this benchmark whose path dependencies
point back at the repository, and a copy of the locked zune-core release
whose `DecoderOptions::use_avx2` returns false, exactly what its runtime
check reports on such a CPU. zune-jpeg checks no other x86 feature, so it
and image (its backend) take their scalar paths. Native decoding is built
with `--cfg deadlib_image_jpeg_portable`, compiling out AVX2 dispatch.
jpeg-decoder keeps its SSSE3 path, present on pre-AVX2 x86-64 CPUs.

Run from the repository root; prints the directory holding the binaries.
"""
from pathlib import Path
import os
import shutil
import subprocess

ZUNE_CORE = 'zune-core-0.5.3'


def main():
    repo = Path.cwd()
    bench = repo / 'crates/deadlib-image/jpeg/bench'
    assert (bench / 'Cargo.toml').exists(), 'run from the repository root'
    root = repo / 'target/jpeg-no-avx2'
    workspace = root / 'bench'
    if workspace.exists():
        shutil.rmtree(workspace)
    shutil.copytree(bench / 'src', workspace / 'src')
    shutil.copy2(bench / 'Cargo.lock', workspace / 'Cargo.lock')

    home = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    sources = sorted(home.glob(f'registry/src/*/{ZUNE_CORE}'))
    assert sources, f'{ZUNE_CORE} is not in the cargo registry; build the benchmark once first'
    patched = root / ZUNE_CORE
    if patched.exists():
        shutil.rmtree(patched)
    shutil.copytree(sources[0], patched)
    options = patched / 'src/options/decoder.rs'
    text = options.read_text(encoding='utf-8')
    signature = '    pub fn use_avx2(&self) -> bool {\n'
    assert text.count(signature) == 1, 'unexpected zune-core source'
    options.write_text(
        text.replace(signature, signature + '        return false; // CPU without AVX2\n'),
        encoding='utf-8')

    manifest = (bench / 'Cargo.toml').read_text(encoding='utf-8')
    jpeg = (bench / '..').resolve().as_posix()
    image = (bench / '../..').resolve().as_posix()
    for old, new in [('path = ".."', f'path = "{jpeg}"'), ('path = "../.."', f'path = "{image}"')]:
        assert manifest.count(old) == 1, old
        manifest = manifest.replace(old, new)
    manifest += f'\n[patch.crates-io]\nzune-core = {{ path = "{patched.as_posix()}" }}\n'
    (workspace / 'Cargo.toml').write_text(manifest, encoding='utf-8')

    env = os.environ.copy()
    env['RUSTFLAGS'] = '--cfg deadlib_image_jpeg_portable'
    subprocess.run(['cargo', 'build', '--release', '--offline', '--bins',
                    '--manifest-path', str(workspace / 'Cargo.toml'),
                    '--target-dir', str(root / 'target')], env=env, check=True)
    print(root / 'target/release')


if __name__ == '__main__':
    main()
