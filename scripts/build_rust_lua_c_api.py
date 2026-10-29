#!/usr/bin/env python3
"""Build and stage the standalone Rust Lua 5.1 C library and public headers."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', help='Rust target triple; defaults to the host')
    parser.add_argument('--debug', action='store_true')
    parser.add_argument('--target-dir', type=Path, default=Path(os.environ.get('CARGO_TARGET_DIR', 'target/lua-capi')))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    target = args.target
    if target is None:
        verbose = subprocess.check_output(['rustc', '-vV'], text=True)
        target = next(line[6:] for line in verbose.splitlines() if line.startswith('host: '))
    windows = 'windows-msvc' in target
    if not windows and 'linux-gnu' not in target:
        parser.error('the standalone C library currently supports Windows MSVC and Linux GNU targets')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    target_dir = args.target_dir.resolve()
    command = ['cargo', 'build', '--locked', '-p', 'deadlib-lua-capi', '--target-dir', str(target_dir), '--message-format=json-render-diagnostics']
    if not args.debug:
        command.append('--release')
    if args.target:
        command += ['--target', args.target]
    process = subprocess.run(command, cwd=root, capture_output=True)
    (output / 'build.log').write_bytes(process.stdout + process.stderr)
    if process.returncode:
        sys.stderr.buffer.write(process.stdout + process.stderr)
        raise SystemExit(process.returncode)
    artifacts = []
    build_folder = None
    for line in process.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if event.get('reason') == 'compiler-artifact' and event.get('target', {}).get('name') == 'lua51':
            artifacts += [Path(name) for name in event['filenames']]
        if event.get('reason') == 'build-script-executed' and 'deadlib-lua-capi' in event.get('package_id', ''):
            build_folder = Path(event['out_dir'])
    suffix = '.dll' if windows else '.so'
    runtime = next((path for path in artifacts if path.suffix == suffix), None)
    if runtime is None or build_folder is None:
        raise RuntimeError('Cargo did not report the standalone C library and its export manifest')
    include = output / 'include'
    include.mkdir(exist_ok=True)
    for name in ['lua.h', 'lauxlib.h', 'lualib.h', 'luaconf.h']:
        shutil.copy2(root / 'vendor/lua-src/lua-5.1.5' / name, include / name)
    library = output / ('bin' if windows else 'lib')
    library.mkdir(exist_ok=True)
    runtime_copy = library / ('lua51.dll' if windows else 'liblua5.1.so.0')
    shutil.copy2(runtime, runtime_copy)
    libdir = output / 'lib'
    libdir.mkdir(exist_ok=True)
    if windows:
        import_library = runtime.with_suffix('.dll.lib')
        if not import_library.exists():
            raise RuntimeError(f'missing MSVC import library: {import_library}')
        shutil.copy2(import_library, libdir / 'lua51.lib')
    else:
        link = libdir / 'liblua5.1.so'
        if link.is_symlink():
            link.unlink()
        elif link.exists():
            raise RuntimeError(f'refusing to replace a non-symlink development library: {link}')
        link.symlink_to(runtime_copy.name)
    package_config = libdir / 'pkgconfig'
    package_config.mkdir(exist_ok=True)
    (package_config / 'lua5.1.pc').write_text(
        'prefix=${pcfiledir}/../..\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\n\n'
        'Name: deadlib-lua\nDescription: Rust Lua 5.1 C API (partial parity)\nVersion: 5.1.5\n'
        + ('Libs: -L${libdir} -llua51\nCflags: -I${includedir} -DLUA_BUILD_AS_DLL\n' if windows else 'Libs: -L${libdir} -llua5.1\nCflags: -I${includedir}\n')
    )
    exports = (build_folder / 'exports.txt').read_text().splitlines()
    metadata = output / 'metadata'
    metadata.mkdir(exist_ok=True)
    shutil.copy2(build_folder / 'exports.txt', metadata / 'exports.txt')
    if windows:
        shutil.copy2(build_folder / 'lua51.map', metadata / 'forwarding.map')
    sources = [root / 'Cargo.toml', root / 'Cargo.lock', Path(__file__).resolve()]
    sources += [path for path in (root / 'crates/deadlib-lua').rglob('*') if path.suffix in {'.rs', '.c', '.h', '.toml'}]
    payload = [include / name for name in ['lua.h', 'lauxlib.h', 'lualib.h', 'luaconf.h']]
    payload += [runtime_copy, package_config / 'lua5.1.pc', metadata / 'exports.txt']
    if windows:
        payload += [libdir / 'lua51.lib', metadata / 'forwarding.map']
    result = dict(
        target=target,
        profile='debug' if args.debug else 'release',
        command=command,
        runtime=str(runtime_copy),
        runtime_sha256=sha(runtime_copy),
        cargo_runtime=str(runtime),
        generated_exports=exports,
        generated_export_manifest_sha256=sha(build_folder / 'exports.txt'),
        source_hashes={path.relative_to(root).as_posix(): sha(path) for path in sorted(set(sources))},
        payload_hashes={path.relative_to(output).as_posix(): sha(path) for path in sorted(payload)},
        symlinks={} if windows else {'lib/liblua5.1.so': 'liblua5.1.so.0'},
        log_sha256=sha(output / 'build.log'),
        full_lua51_parity=False,
        missing_core_exports=['lua_getallocf', 'lua_newstate', 'lua_setallocf'],
        limitations=['Default C-owned states only; custom allocator APIs remain absent.', 'Fallible userdata payload errors report LUA_ERRMEM; other allocations still lack recoverable OOM. Exact GC/heap accounting and general continuation/resource parity remain open.'],
    )
    (output / 'manifest.json').write_text(json.dumps(result, indent=2) + '\n')
    print(f'{runtime_copy}: {result["runtime_sha256"]}')


if __name__ == '__main__':
    main()
