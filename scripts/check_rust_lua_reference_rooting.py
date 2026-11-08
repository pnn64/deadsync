"""Verify the lifetime guard used by the package debug-stack comparison.

The negative control intentionally reproduces a Lua 5.1.5 reference use-after-free.
Linux GCC/Clang with AddressSanitizer is required; no Rust code is sanitized here.
"""
from pathlib import Path
import argparse
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile
import time
from build_lua_cli_reference import SHA256


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--observed-audit', type=Path)
    parser.add_argument('--rust-cli', type=Path)
    args = parser.parse_args()
    if os.name == 'nt':
        parser.error('run this sanitizer control on Linux')
    root = Path(__file__).resolve().parents[1]
    work = args.work.resolve() / str(time.time_ns())
    work.mkdir(parents=True)
    archive = root / 'target/lua-cli-reference/lua-5.1.5.tar.gz'
    data = archive.read_bytes()
    assert hashlib.sha256(data).hexdigest() == SHA256
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as reference:
        assert all((work / m.name).resolve().is_relative_to(work) for m in reference.getmembers())
        reference.extractall(work, filter='data')
    source = work / 'lua-5.1.5/src'
    binary = work / 'lua-asan'
    command = ['cc', '-g', '-O1', '-fsanitize=address', '-fno-omit-frame-pointer',
               '-D_DEFAULT_SOURCE', '-D_LARGEFILE64_SOURCE', '-D_POSIX_C_SOURCE=200809L',
               '-DLUA_USE_POSIX', '-DLUA_USE_DLOPEN', '-DLUA_USE_ISATTY', '-I' + str(source)]
    command += [str(p) for p in sorted(source.glob('*.c')) if p.name not in ['luac.c', 'print.c']]
    command += ['-lm', '-ldl', '-o', str(binary)]
    build = subprocess.run(command, capture_output=True)
    log = work / 'build.log'
    log.write_bytes(build.stdout + build.stderr)
    assert build.returncode == 0, (build.stdout + build.stderr).decode(errors='replace')
    fixture = root / 'crates/deadlib-lua/tests/reference/package-rooting.lua'
    environment = os.environ.copy()
    for key in ['LUA_INIT', 'LUA_PATH', 'LUA_CPATH']:
        environment.pop(key, None)
    environment['ASAN_OPTIONS'] = 'detect_leaks=0:halt_on_error=1'
    rows = {}
    for mode in ['unrooted', 'rooted']:
        invocation = [str(binary), '-e', 'ROOT_REFERENCE_STRINGS=' + str(mode == 'rooted').lower(), str(fixture)]
        process = subprocess.run(invocation, cwd=work, env=environment, capture_output=True, timeout=30)
        rows[mode] = dict(command=invocation, returncode=process.returncode,
                          stdout=process.stdout.decode(errors='replace'), stderr=process.stderr.decode(errors='replace'))
    assert rows['unrooted']['returncode'] != 0 and 'heap-use-after-free' in rows['unrooted']['stderr'], rows['unrooted']
    assert 'findfile' in rows['unrooted']['stderr'] and 'luaL_gsub' in rows['unrooted']['stderr']
    assert rows['rooted']['returncode'] == 0 and rows['rooted']['stdout'] == 'package-rooting-complete\n' and not rows['rooted']['stderr'], rows['rooted']
    artifacts = {str(p): sha(p) for p in [binary, log]}
    if args.rust_cli:
        cli = args.rust_cli.resolve()
        artifacts[str(cli)] = sha(cli)
        rows['rust_rooted'] = {}
        invocation = [str(cli), '-e', 'ROOT_REFERENCE_STRINGS=true', str(fixture)]
        process = subprocess.run(invocation, cwd=work, env=environment, capture_output=True, timeout=30)
        rows['rust_rooted'] = dict(command=invocation, returncode=process.returncode,
                                   stdout=process.stdout.decode(errors='replace'), stderr=process.stderr.decode(errors='replace'))
        assert process.returncode == 0 and rows['rust_rooted']['stdout'] == rows['rooted']['stdout'] and not process.stderr, rows['rust_rooted']
    observation = None
    if args.observed_audit:
        copied = work / 'observed-audit.json'
        shutil.copyfile(args.observed_audit, copied)
        artifacts[str(copied)] = sha(copied)
        audit = json.loads(copied.read_text())
        samples = audit['native_body_stacks_probe']['runs']
        observation = dict(fixture_sha256=audit['source_hashes']['crates/deadlib-lua/tests/native-boundary/package-stacks.lua'], samples={})
        for engine in ['reference', 'rust']:
            row = samples[engine]['package-stacks.lua']
            line = next(line for line in bytes.fromhex(row['stdout_hex']).decode().splitlines() if line.startswith('LIVE\tlua-path\t'))
            observation['samples'][engine] = dict(returncode=row['returncode'], line=line)
        assert observation['samples']['reference']['line'] != observation['samples']['rust']['line']
    result = dict(reference_archive_sha256=SHA256, work=str(work), command=command,
                  reference_source_hashes={p.name: sha(p) for p in source.iterdir() if p.suffix in {'.c', '.h'}},
                  source_hashes={fixture.relative_to(root).as_posix(): sha(fixture), Path(__file__).relative_to(root).as_posix(): sha(Path(__file__))},
                  fixture_source=fixture.read_text(), artifacts=artifacts, runs=rows,
                  unrooted_windows_observation=observation, reference_use_after_free_confirmed=True,
                  rooted_control_passed=True, asan_options=environment['ASAN_OPTIONS'],
                  compiler=subprocess.check_output(['cc', '--version'], text=True),
                  pointer_lifetime_contract='https://www.lua.org/manual/5.1/manual.html#lua_tolstring')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print('Reference ASan use-after-free reproduced; rooted reference and Rust controls pass.')


if __name__ == '__main__':
    main()
